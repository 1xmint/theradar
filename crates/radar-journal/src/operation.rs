// SPDX-License-Identifier: Apache-2.0
//! An operation against the account, and the durable record that outlives the
//! process running it.
//!
//! # The failure this module exists to prevent
//!
//! [`Portfolio`] holds reservations in memory. A reservation is capital claimed
//! by an operation that has not finished, and the whole point of it is that a
//! second proposal cannot spend the same dollars. Kill the process and every
//! claim in it disappears, while the transaction the claim was held for is
//! still on its way to a validator. The next run reads a clean balance and
//! commits the same capital again.
//!
//! So a claim has to survive a restart, and surviving means it was written down
//! **before** anything outward-facing happened. That is the rule the crate
//! already states; this module is the first caller of it on the money path.
//!
//! # The six states, and the one that matters
//!
//! [`OperationState`] is [`Proposed`], [`Reserved`], [`SubmissionUnknown`],
//! [`Confirmed`], [`Failed`] and [`Reconciled`].
//!
//! [`SubmissionUnknown`] is the state a process is in from the instant *before*
//! it releases an effect until a response comes back. It is not a state a
//! caller reaches by having something go wrong; it is the honest description of
//! *we let go of it and we do not yet know*. AGENTS.md rule 9 in its most
//! expensive form: **unknown is not safe, and unknown is not failed.**
//!
//! An implementation that treats it as failed frees the claim, and freeing the
//! claim is what lets the next pass size a second trade against capital the
//! first one may already have spent. So the transition is not merely
//! discouraged — [`OperationState::advance`] refuses it, and
//! [`OperationError::UnknownIsNotFailed`] is what a caller gets instead. The
//! only ways out of [`SubmissionUnknown`] are [`Confirmed`] and [`Reconciled`],
//! and **both carry a [`Settlement`] somebody had to establish.** There is no
//! exit that costs no evidence.
//!
//! This is the gap `radar-analyst`'s `Publisher` trait names and cannot express:
//! *"accepted, response lost"*. [`Outcome::Uncertain`] was already the right
//! word for it in the journal; this is the state machine that will not let a
//! caller round it off.
//!
//! # Ordering, and why a crash between two steps is safe
//!
//! Two different rules, because the two steps are not the same kind of step:
//!
//! - **A reservation is taken first and recorded second.** It is a change to a
//!   `BTreeMap` in this process. Nothing outside can observe it, and a crash
//!   between the two loses a claim nothing acted on — because acting requires
//!   the [`SubmissionUnknown`] record, which requires the [`Reserved`] one. If
//!   the record fails the claim is rolled back, so the file never says
//!   `Reserved` about a reservation that was refused.
//! - **A submission is recorded first and released second**, by
//!   [`OperationLog::submit`], which takes the effect as a closure and calls it
//!   only after the write returned. A crash between the two leaves an operation
//!   in [`SubmissionUnknown`] whose effect never happened — the account holds
//!   capital it did not need to. That is the safe direction, and it is the same
//!   reasoning `radar_store`'s writer flushes its coverage claim *after* the
//!   rows it claims: lose the claim, keep the truth.
//!
//! The unsafe orderings are the mirror images, and neither is reachable through
//! this type: an effect released before its record, and a claim freed by
//! anything short of an established outcome.
//!
//! # Identity a caller cannot choose
//!
//! An [`OperationId`] is the id of the journal event that proposed it — a
//! digest over the intent and the entire chain before it, filled in by
//! [`Journal::record`] rather than by whoever called it. Two consequences carry
//! weight:
//!
//! - You cannot hold an id without the proposal having reached disk, so
//!   "recorded before acted on" is a thing the compiler asks about rather than
//!   a thing a reviewer remembers.
//! - It is unique across restarts. A counter reset by a crash would hand a new
//!   operation the id of one still outstanding, and the second would settle the
//!   first's claim.
//!
//! Everything downstream keys on it: replay is idempotent **by operation
//! identity**, never by timestamp and never by position in the file.
//!
//! [`Proposed`]: OperationState::Proposed
//! [`Reserved`]: OperationState::Reserved
//! [`SubmissionUnknown`]: OperationState::SubmissionUnknown
//! [`Confirmed`]: OperationState::Confirmed
//! [`Failed`]: OperationState::Failed
//! [`Reconciled`]: OperationState::Reconciled
//! [`Portfolio`]: radar_types::Portfolio

use std::collections::BTreeMap;

use radar_types::{
    Asset, Portfolio, PortfolioError, ReservationId, Settlement, Slot, TokenQuantity,
};
use serde::{Deserialize, Serialize};

use crate::event::{Correlation, Event, Outcome};
use crate::file::{Journal, JournalError};

/// Which operation an event is about.
///
/// The id of the event that proposed it. Opaque, and issued only by
/// [`OperationLog::propose`] after the proposal reached disk — see the module
/// documentation on why a counter would not do.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct OperationId(String);

impl OperationId {
    /// The id, as it is written in a later event's correlation.
    ///
    /// The join between an operation and the journal lines about it: every
    /// event after the proposal carries this string in
    /// `Correlation::operation`, which is what `radar audit explain` finds an
    /// operation's history by.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What an operation set out to do.
///
/// The asset and the amount it means to claim, and the slot it was decided at.
/// Not a price and not a verdict: the risk kernel decides whether an operation
/// may happen and this records that one was started, which is AGENTS.md rule 1's
/// division — nothing here creates authority.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Intent {
    /// What the operation means to spend.
    pub asset: Asset,
    /// How much of it.
    pub amount: TokenQuantity,
    /// The watermark the decision was taken at.
    pub at: Slot,
}

/// How far an operation got.
///
/// # Why the terminal states carry a [`Settlement`]
///
/// So that leaving [`SubmissionUnknown`] costs evidence. A variant with no
/// payload could be reached by a `match` arm on an error, and the arm that maps
/// "no response" to "it did not happen" is the one that frees a claim while the
/// transaction is still landing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    /// Decided on, nothing claimed yet.
    Proposed,
    /// Capital is claimed in the portfolio and the effect has not been
    /// released.
    Reserved,
    /// **An effect was released and the answer is not in.** The claim stands.
    SubmissionUnknown,
    /// The answer came back and said what happened.
    Confirmed(Settlement),
    /// It did not happen, and that is established rather than assumed.
    ///
    /// Reachable only from [`Proposed`](Self::Proposed) and
    /// [`Reserved`](Self::Reserved) — that is, only while nothing has been
    /// released. See [`OperationError::UnknownIsNotFailed`].
    Failed,
    /// An unknown submission was resolved by looking at what actually happened.
    Reconciled(Settlement),
}

impl OperationState {
    /// Whether an operation in this state still holds a claim on capital.
    ///
    /// [`SubmissionUnknown`](Self::SubmissionUnknown) does. That single `true`
    /// is the money in this file: a restart re-holds every outstanding claim,
    /// and an unknown submission is outstanding until somebody establishes
    /// otherwise.
    #[must_use]
    pub const fn is_outstanding(self) -> bool {
        matches!(self, Self::Reserved | Self::SubmissionUnknown)
    }

    /// The state after `next`, or why the move is refused.
    ///
    /// # Errors
    ///
    /// [`OperationError::UnknownIsNotFailed`] for the one transition this
    /// module exists to forbid, and [`OperationError::IllegalTransition`] for
    /// every other move that is not in the table.
    pub fn advance(self, next: Self) -> Result<Self, OperationError> {
        let allowed = match (self, next) {
            (Self::Proposed, Self::Reserved | Self::Failed)
            | (Self::Reserved, Self::SubmissionUnknown | Self::Failed)
            | (Self::SubmissionUnknown, Self::Confirmed(_) | Self::Reconciled(_)) => true,
            // Named separately from the catch-all so the caller is told *which*
            // rule refused it. "Illegal transition" would be true and useless:
            // the thing a reader needs to learn here is that an unknown
            // submission is not a failure, and an error that says so is where
            // they learn it.
            (Self::SubmissionUnknown, Self::Failed) => {
                return Err(OperationError::UnknownIsNotFailed);
            }
            // Named separately from the catch-all so the caller is told *which*
            // rule refused it. "Illegal transition" would be true and useless:
            // the thing a reader needs to learn here is that an unknown
            // submission is not a failure, and an error that says so is where
            // they learn it.
            _ => false,
        };
        if allowed {
            Ok(next)
        } else {
            Err(OperationError::IllegalTransition {
                from: self,
                to: next,
            })
        }
    }
}

/// One operation, as one line of the journal describes it.
///
/// Every line repeats the intent rather than referring back to the proposal, so
/// a single line is a complete description of the operation at that moment.
/// That is what a future `audit replay` needs and what `audit explain` can
/// print without holding the whole file.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct OperationEntry {
    /// What it set out to do.
    pub intent: Intent,
    /// What it actually claimed, once it claimed anything.
    ///
    /// `None` while [`Proposed`](OperationState::Proposed). Not zero: a
    /// reservation of nothing and no reservation at all are different facts,
    /// and only one of them is a measurement.
    pub reserved: Option<TokenQuantity>,
    /// Where it got to.
    pub state: OperationState,
}

/// What stopped an operation.
#[derive(Debug, thiserror::Error)]
pub enum OperationError {
    /// The durable record could not be written, so nothing was done.
    #[error("the operation could not be recorded, so it did not happen: {0}")]
    Journal(#[from] JournalError),
    /// The account refused the claim.
    #[error("the account refused the claim: {0}")]
    Portfolio(#[from] PortfolioError),
    /// A submission whose answer never arrived was about to be written off.
    ///
    /// The refusal this module exists for. An effect was released; nobody
    /// established what became of it; and freeing the claim would let the next
    /// pass spend capital the released effect may already have spent.
    #[error(
        "a submission with no answer is not a failure; reconcile it against what happened rather than writing it off"
    )]
    UnknownIsNotFailed,
    /// A move that is not in the table.
    #[error("an operation cannot go from {from:?} to {to:?}")]
    IllegalTransition {
        /// Where it was.
        from: OperationState,
        /// Where the caller tried to take it.
        to: OperationState,
    },
    /// No operation with that id.
    #[error("no operation {0}")]
    NoSuchOperation(OperationId),
    /// A recorded event advances an operation no recorded event proposed.
    ///
    /// A visible fault rather than a skipped line: an operation whose proposal
    /// is missing is one whose intent nobody can read, and replaying the rest of
    /// it would rebuild a claim with no idea what it was for.
    #[error("event {event} advances operation {operation}, which nothing in this journal proposed")]
    Orphan {
        /// The event's own id.
        event: String,
        /// The operation it names.
        operation: String,
    },
    /// An event carries an operation entry and does not say which operation.
    #[error("event {0} carries an operation entry but names no operation")]
    Unnamed(String),
    /// A terminal state was asked for on a settlement that leaves capital
    /// claimed.
    ///
    /// [`Settlement::PartiallyFilled`] for less than the outstanding claim
    /// leaves the remainder reserved, and a finished operation is one
    /// [`OperationLog::rehold`] will not re-take. Closing on one would drop the
    /// remaining claim at the next restart — the same dollars, offered to the
    /// next proposal, while the first operation's remainder is still out there.
    ///
    /// Partial fills are not expressible through this type yet, and that is a
    /// refusal rather than an omission.
    #[error(
        "settling {settlement:?} would leave part of the claim on {outstanding} outstanding, and a finished operation is not re-held after a restart"
    )]
    RemainderWouldBeLost {
        /// What is still claimed.
        outstanding: TokenQuantity,
        /// What the caller offered to close it with.
        settlement: Settlement,
    },
}

/// Whether a recorded change had already been applied.
///
/// The return of every transition, so a caller can tell *it moved* from
/// *it was already there* without comparing states itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Applied {
    /// The operation moved, and the move was recorded.
    Advanced,
    /// The operation was already in exactly this state. Nothing was written and
    /// nothing was settled.
    AlreadySeen,
}

/// Permission to release an effect.
///
/// Cannot be built outside this crate and is handed to the closure
/// [`OperationLog::submit`] takes, which is called only after the durable
/// record returned. So "no external effect before the record of it is durable"
/// is a property of the control flow rather than of a comment above a call.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Released {
    operation: OperationId,
}

impl Released {
    /// Which operation the effect belongs to.
    #[must_use]
    pub const fn operation(&self) -> &OperationId {
        &self.operation
    }
}

/// One operation as this process is holding it.
#[derive(Clone, Debug)]
struct Live {
    entry: OperationEntry,
    /// The portfolio handle for its claim, when this process is holding one.
    ///
    /// `None` after a replay and before [`OperationLog::rehold`]: a
    /// [`ReservationId`] is a position in one `Portfolio`'s map and means
    /// nothing to a different process, so it is deliberately not written down.
    /// The durable half is the *amount*, which is what a new claim is rebuilt
    /// from.
    claim: Option<ReservationId>,
}

/// Operations, and the journal they are recorded in.
///
/// The caller is `radar consider`, which reopens the log before the risk kernel
/// sizes anything so that capital claimed by an operation still in flight is not
/// offered to a second one.
pub struct OperationLog {
    journal: Journal,
    operations: BTreeMap<OperationId, Live>,
    build: Option<String>,
}

impl OperationLog {
    /// Opens the log at `path` and rebuilds every operation the file records.
    ///
    /// The portfolio is untouched here. Rebuilding *state* and re-taking
    /// *claims* are separate steps because the terminal ones must not be
    /// replayed against balances: a confirmed fill was already debited from the
    /// wallet the balances were read from, and settling it again would take the
    /// same units out twice. [`rehold`](Self::rehold) does the second half, and
    /// only for what is still outstanding.
    ///
    /// # Errors
    ///
    /// [`OperationError::Journal`] if the file cannot be read, and
    /// [`OperationError::Orphan`] or [`OperationError::IllegalTransition`] if
    /// what it records is not a history an operation could have had.
    pub fn open(path: impl Into<std::path::PathBuf>) -> Result<Self, OperationError> {
        let journal = Journal::open(path)?;
        let events = journal.events()?;
        let operations = replay(&events)?;
        Ok(Self {
            journal,
            operations,
            build: radar_types::build_sha().map(str::to_owned),
        })
    }

    /// Every operation that still holds a claim, oldest first.
    pub fn outstanding(&self) -> impl Iterator<Item = (&OperationId, &OperationEntry)> {
        self.operations
            .iter()
            .filter(|(_, live)| live.entry.state.is_outstanding())
            .map(|(id, live)| (id, &live.entry))
    }

    /// What the log holds about one operation.
    #[must_use]
    pub fn entry(&self, id: &OperationId) -> Option<&OperationEntry> {
        self.operations.get(id).map(|live| &live.entry)
    }

    /// Re-takes every outstanding claim against `portfolio`.
    ///
    /// The half of a restart that costs money if it is skipped. Each claim goes
    /// back through [`Portfolio::reserve`], so the account checks it against the
    /// balance rather than being told what to believe.
    ///
    /// A claim that cannot be re-taken is an **error**, never a claim quietly
    /// dropped. The two ways it happens are an operation whose spend already
    /// landed — so the balance no longer covers it — and a wallet that is not
    /// the one the operation was opened against. Both need somebody to look;
    /// neither is a reason to hand the next pass more capital than it should
    /// have.
    ///
    /// # Errors
    ///
    /// [`OperationError::Portfolio`] with the account's own refusal.
    ///
    /// [`Portfolio::reserve`]: radar_types::Portfolio::reserve
    pub fn rehold(&mut self, portfolio: &mut Portfolio) -> Result<(), OperationError> {
        for live in self.operations.values_mut() {
            if !live.entry.state.is_outstanding() {
                continue;
            }
            let Some(amount) = live.entry.reserved else {
                continue;
            };
            let id = portfolio.reserve(live.entry.intent.asset, amount, live.entry.intent.at)?;
            live.claim = Some(id);
        }
        Ok(())
    }

    /// Records an intention to operate, and returns its identity.
    ///
    /// Nothing is claimed and nothing is released. This is the line a later
    /// event points back at.
    ///
    /// # Errors
    ///
    /// [`OperationError::Journal`] when the write fails, in which case no
    /// operation exists.
    pub fn propose(
        &mut self,
        intent: Intent,
        at: u64,
        correlation: Correlation,
    ) -> Result<OperationId, OperationError> {
        let entry = OperationEntry {
            intent,
            reserved: None,
            state: OperationState::Proposed,
        };
        let recorded = self.journal.record_operation(
            Outcome::Ok,
            at,
            correlation,
            entry,
            self.build.clone(),
            None,
        )?;
        let id = OperationId(recorded.id().to_owned());
        self.operations
            .insert(id.clone(), Live { entry, claim: None });
        Ok(id)
    }

    /// Claims `amount` of the intent's asset and records the claim.
    ///
    /// The account is asked first, because asking is a change to this process
    /// and nothing else can see it. If the record then fails the claim is
    /// released again, so the file never says `Reserved` about capital that was
    /// never held. See the module documentation.
    ///
    /// # Errors
    ///
    /// [`OperationError::NoSuchOperation`], whatever the account refuses with,
    /// and [`OperationError::Journal`] — after which the claim is released and
    /// the operation is still `Proposed`.
    pub fn reserve(
        &mut self,
        id: &OperationId,
        portfolio: &mut Portfolio,
        at: u64,
    ) -> Result<Applied, OperationError> {
        let live = self
            .operations
            .get(id)
            .ok_or_else(|| OperationError::NoSuchOperation(id.clone()))?;
        if live.entry.state == OperationState::Reserved {
            return Ok(Applied::AlreadySeen);
        }
        let next = live.entry.state.advance(OperationState::Reserved)?;
        let intent = live.entry.intent;

        let claim = portfolio.reserve(intent.asset, intent.amount, intent.at)?;
        let entry = OperationEntry {
            intent,
            reserved: Some(intent.amount),
            state: next,
        };
        match self.write(id, entry, Outcome::Ok, at) {
            Ok(()) => {}
            Err(error) => {
                // The claim exists only in this process and nothing has acted
                // on it, so releasing it is free. Leaving it held would starve
                // the account of capital nothing is using, with no record
                // anywhere saying why.
                portfolio.settle(claim, Settlement::Abandoned)?;
                return Err(error);
            }
        }
        self.set(id, entry, Some(claim));
        Ok(Applied::Advanced)
    }

    /// Records that an effect is about to be released, then releases it.
    ///
    /// `effect` runs **only if** the write returned. The operation is in
    /// [`SubmissionUnknown`](OperationState::SubmissionUnknown) before the
    /// closure is called and stays there whatever the closure returns — the
    /// answer to "did it land" is the caller's next call, not this one's return
    /// value.
    ///
    /// The nested result is deliberate. The outer one says whether the effect
    /// was **allowed to happen**; the inner is the effect's own answer. A caller
    /// mapping an inner `Err` straight to [`fail`](Self::fail) is refused by
    /// [`OperationError::UnknownIsNotFailed`], which is the point.
    ///
    /// # Errors
    ///
    /// [`OperationError::NoSuchOperation`],
    /// [`OperationError::IllegalTransition`] for an operation that has not
    /// reserved, and [`OperationError::Journal`] — in which case **the closure
    /// was never called.**
    pub fn submit<T, E>(
        &mut self,
        id: &OperationId,
        at: u64,
        effect: impl FnOnce(&Released) -> Result<T, E>,
    ) -> Result<Result<T, E>, OperationError> {
        let live = self
            .operations
            .get(id)
            .ok_or_else(|| OperationError::NoSuchOperation(id.clone()))?;
        let next = live
            .entry
            .state
            .advance(OperationState::SubmissionUnknown)?;
        let entry = OperationEntry {
            state: next,
            ..live.entry
        };
        let claim = live.claim;

        // `Outcome::Uncertain` on the line itself, because that is what is true
        // at the moment it is written: the effect has not been released and its
        // result is not knowable. An `Ok` here would be a claim about something
        // that has not happened yet.
        self.write(id, entry, Outcome::Uncertain, at)?;
        self.set(id, entry, claim);

        Ok(effect(&Released {
            operation: id.clone(),
        }))
    }

    /// Records the answer that came back, and settles the claim against it.
    ///
    /// # Errors
    ///
    /// [`OperationError::NoSuchOperation`],
    /// [`OperationError::IllegalTransition`] for an operation nothing was
    /// submitted for, whatever the account refuses the settlement with, and
    /// [`OperationError::Journal`].
    pub fn confirm(
        &mut self,
        id: &OperationId,
        settlement: Settlement,
        portfolio: &mut Portfolio,
        at: u64,
    ) -> Result<Applied, OperationError> {
        self.close(
            id,
            OperationState::Confirmed(settlement),
            settlement,
            portfolio,
            at,
        )
    }

    /// Records what an unknown submission turned out to have done.
    ///
    /// The only other way out of
    /// [`SubmissionUnknown`](OperationState::SubmissionUnknown), and the one
    /// that takes an observation rather than a response.
    /// [`Settlement::Abandoned`] here frees the claim, and it is the honest
    /// value **only** once somebody has established that nothing landed.
    ///
    /// # Errors
    ///
    /// The same as [`confirm`](Self::confirm).
    pub fn reconcile(
        &mut self,
        id: &OperationId,
        settlement: Settlement,
        portfolio: &mut Portfolio,
        at: u64,
    ) -> Result<Applied, OperationError> {
        self.close(
            id,
            OperationState::Reconciled(settlement),
            settlement,
            portfolio,
            at,
        )
    }

    /// Records an established failure and frees the claim.
    ///
    /// # Errors
    ///
    /// [`OperationError::UnknownIsNotFailed`] when the operation released an
    /// effect, plus [`OperationError::NoSuchOperation`] and
    /// [`OperationError::Journal`].
    pub fn fail(
        &mut self,
        id: &OperationId,
        portfolio: &mut Portfolio,
        at: u64,
    ) -> Result<Applied, OperationError> {
        self.close(
            id,
            OperationState::Failed,
            Settlement::Abandoned,
            portfolio,
            at,
        )
    }

    /// The shared body of the three terminal moves.
    fn close(
        &mut self,
        id: &OperationId,
        next: OperationState,
        settlement: Settlement,
        portfolio: &mut Portfolio,
        at: u64,
    ) -> Result<Applied, OperationError> {
        let live = self
            .operations
            .get(id)
            .ok_or_else(|| OperationError::NoSuchOperation(id.clone()))?;
        // The idempotence guard, and it is keyed on the operation's identity
        // and its state -- never on a timestamp and never on where the event
        // sat in the file. A confirmation delivered twice settles once.
        if live.entry.state == next {
            return Ok(Applied::AlreadySeen);
        }
        let next = live.entry.state.advance(next)?;
        let entry = OperationEntry {
            state: next,
            ..live.entry
        };
        let claim = live.claim;

        // Checked before the write, so a refusal leaves no line saying the
        // operation finished. See `RemainderWouldBeLost`.
        if let (Some(claim), Settlement::PartiallyFilled(filled)) = (claim, settlement) {
            let outstanding = portfolio
                .reservations()
                .find(|r| r.id() == claim)
                .map(radar_types::Reservation::outstanding);
            if let Some(outstanding) = outstanding
                && outstanding != filled
            {
                return Err(OperationError::RemainderWouldBeLost {
                    outstanding,
                    settlement,
                });
            }
        }

        let outcome = match next {
            OperationState::Failed => Outcome::Failed,
            _ => Outcome::Ok,
        };
        self.write(id, entry, outcome, at)?;
        if let Some(claim) = claim {
            portfolio.settle(claim, settlement)?;
        }
        self.set(id, entry, None);
        Ok(Applied::Advanced)
    }

    /// Writes one line about an operation that already exists.
    fn write(
        &mut self,
        id: &OperationId,
        entry: OperationEntry,
        outcome: Outcome,
        at: u64,
    ) -> Result<(), OperationError> {
        let correlation = Correlation {
            operation: Some(id.0.clone()),
            ..Correlation::default()
        };
        self.journal
            .record_operation(outcome, at, correlation, entry, self.build.clone(), None)?;
        Ok(())
    }

    /// Puts the new state in the map. Called only after the write returned.
    fn set(&mut self, id: &OperationId, entry: OperationEntry, claim: Option<ReservationId>) {
        if let Some(live) = self.operations.get_mut(id) {
            live.entry = entry;
            live.claim = claim;
        }
    }
}

/// Rebuilds operations from the events a journal holds.
///
/// Idempotent by operation identity: a line that says what the operation
/// already says is skipped, and a proposal whose id is already known does not
/// open a second operation. A file with a line duplicated therefore rebuilds
/// one claim rather than two — which is the difference between an account that
/// knows what it is holding and one holding twice as much as it thinks.
fn replay(events: &[Event]) -> Result<BTreeMap<OperationId, Live>, OperationError> {
    let mut operations: BTreeMap<OperationId, Live> = BTreeMap::new();
    for event in events {
        let Some(entry) = event.operation else {
            continue;
        };
        if entry.state == OperationState::Proposed {
            // The proposal *is* the identity, so it needs no correlation to
            // find itself by.
            let id = OperationId(event.id.clone());
            operations.entry(id).or_insert(Live { entry, claim: None });
            continue;
        }
        let named = event
            .correlation
            .operation
            .clone()
            .ok_or_else(|| OperationError::Unnamed(event.id.clone()))?;
        let id = OperationId(named);
        let live = operations
            .get_mut(&id)
            .ok_or_else(|| OperationError::Orphan {
                event: event.id.clone(),
                operation: id.0.clone(),
            })?;
        if live.entry.state == entry.state {
            continue;
        }
        live.entry.state = live.entry.state.advance(entry.state)?;
        live.entry.reserved = entry.reserved;
    }
    Ok(operations)
}

#[cfg(test)]
mod tests {
    use super::{OperationError, OperationState};
    use radar_types::{Decimals, Settlement, TokenQuantity};

    fn some() -> Settlement {
        Settlement::PartiallyFilled(TokenQuantity::new(1, Decimals::NATIVE_SOL))
    }

    #[test]
    fn an_unknown_submission_cannot_be_written_off_as_a_failure() {
        // The transition this module exists to refuse, at the level that can
        // hold it: the state machine, not a comment above a call site.
        //
        // Re-apply the bug by adding `Self::Failed` to the
        // `SubmissionUnknown` arm of `advance` and this returns `Ok`.
        let refused = OperationState::SubmissionUnknown.advance(OperationState::Failed);
        assert!(matches!(refused, Err(OperationError::UnknownIsNotFailed),));

        // And the two exits that do exist both carry a settlement somebody had
        // to establish.
        assert!(
            OperationState::SubmissionUnknown
                .advance(OperationState::Confirmed(some()))
                .is_ok()
        );
        assert!(
            OperationState::SubmissionUnknown
                .advance(OperationState::Reconciled(some()))
                .is_ok()
        );
    }

    #[test]
    fn an_unknown_submission_still_holds_its_claim() {
        // The consequence of the state above, and the reason it matters. If
        // this returned false a restart would not re-take the claim, and the
        // next pass would size a second trade against capital the released
        // effect may already have spent.
        assert!(OperationState::SubmissionUnknown.is_outstanding());
        assert!(OperationState::Reserved.is_outstanding());
        assert!(!OperationState::Proposed.is_outstanding());
        assert!(!OperationState::Failed.is_outstanding());
        assert!(!OperationState::Confirmed(some()).is_outstanding());
        assert!(!OperationState::Reconciled(some()).is_outstanding());
    }

    #[test]
    fn nothing_may_be_confirmed_that_was_never_submitted() {
        // A confirmation is a statement about a response, and there is no
        // response to an effect nobody released. Allowing it would let a
        // caller debit a balance for a transaction that never left.
        for from in [OperationState::Proposed, OperationState::Reserved] {
            assert!(matches!(
                from.advance(OperationState::Confirmed(some())),
                Err(OperationError::IllegalTransition { .. })
            ));
            assert!(matches!(
                from.advance(OperationState::Reconciled(some())),
                Err(OperationError::IllegalTransition { .. })
            ));
        }
    }

    #[test]
    fn a_finished_operation_does_not_start_again() {
        for from in [
            OperationState::Failed,
            OperationState::Confirmed(some()),
            OperationState::Reconciled(some()),
        ] {
            assert!(from.advance(OperationState::Reserved).is_err());
            assert!(from.advance(OperationState::SubmissionUnknown).is_err());
        }
    }
}
