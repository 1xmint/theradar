// SPDX-License-Identifier: Apache-2.0
//! What one decision session saw, refused, cost and could not see.
//!
//! # Why a record and not a printout
//!
//! `radar consider` printed a funnel and exited. The numbers were true when they
//! were on screen and gone afterwards, which makes a morning question — *what
//! did it do overnight, and how much of what it did not do was the market rather
//! than the instrument* — unanswerable without re-running the pass against a
//! world that has moved. [`crate::portfolio`] made the account say what it cannot
//! say; this makes the **run** say it.
//!
//! # The one rule this type enforces
//!
//! **Every count here is a denominator, and no absence is written as a zero.**
//! Three states that print identically as `0` are kept apart in the types:
//!
//! - *measured empty* — collection ran, finished, and observed nothing.
//!   [`WindowCoverage::MeasuredEmpty`].
//! - *never attested* — nothing says anybody collected these slots at all.
//!   [`WindowCoverage::Unattested`], and a report over it says so instead of
//!   reporting zero opportunities.
//! - *excluded* — considered and then dropped, by a cap, an unbuildable
//!   candidate or a refusal. [`Funnel`] keeps each one and
//!   [`Funnel::unaccounted`] prints whatever still does not add up rather than
//!   letting the arithmetic close over a leak.
//!
//! The same rule on the money side: [`EquityTotal::of`] is `Unknown` as soon as
//! either half is, because a total that quietly omits what nobody could price is
//! the smaller number, and the smaller number is the one that gets permission.
//! That is [`Portfolio::results`](crate::Portfolio::results)' rule, carried into
//! the report so a renderer cannot undo it.
//!
//! # What this is built toward
//!
//! An offline replay needs the **inputs** to a decision, not its output; that is
//! why `radar audit replay` is deliberately absent. This record is the run-level
//! half of those inputs — the watermark, the versions, the coverage the decision
//! rested on, the spend it took and the clocks it ran against — kept apart from
//! the per-candidate [`Decision`] rows the store already holds.
//!
//! [`Decision`]: https://github.com/hey-vera/radar

use serde::{Deserialize, Serialize};

use crate::{SignedMicroUsd, Slot, Unvaluable};

/// The schema this build writes.
///
/// On the record rather than in the filename, for [`radar_journal`]'s reason: a
/// directory accumulates records across upgrades, and a reader has to be told
/// which one it is holding.
///
/// [`radar_journal`]: https://github.com/hey-vera/radar
pub const SESSION_SCHEMA: u32 = 1;

/// One `radar consider` run, as it happened.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SessionRecord {
    /// Which schema wrote this.
    pub schema: u32,
    /// The key a later reader finds this by.
    ///
    /// Derived from the watermark and the wall clock the run started at, so two
    /// runs at the same watermark are distinguishable and a run is orderable
    /// without opening it. See [`SessionRecord::key`].
    pub run_id: String,
    /// UTC seconds when the pass started.
    pub started_at_unix: u64,
    /// UTC seconds when it finished.
    pub finished_at_unix: u64,
    /// The store watermark the whole pass was taken as of.
    ///
    /// This is *when the evidence was ready*. Not when the tokens launched, and
    /// not when the command ran.
    pub decided_at: Slot,
    /// The store that was read.
    pub store: String,
    /// How far back from the watermark a launch could be and still be considered.
    pub window_slots: u64,
    /// The cap on how many candidates the paid tier was allowed.
    ///
    /// Recorded because it is an **exclusion**: candidates past it were worth
    /// paying for and were not looked at, and [`Funnel::deferred_by_cap`] counts
    /// them rather than dropping them.
    pub paid_tier_cap: usize,
    /// Which strategy decided, and under which version.
    pub strategy: String,
    /// Its version.
    pub strategy_version: String,
    /// What the round trip was assumed to cost, per ten thousand.
    pub assumed_round_trip_bps: u64,
    /// Which instrument priced the exits.
    pub pricing: String,
    /// Whether the policy in force could authorise anything at all.
    ///
    /// `true` is the shipped default and means no action was eligible at any
    /// slot — which is why [`EarliestEntry::Unknown`] carries
    /// [`NoEntryTime::PolicyRefused`] rather than a fabricated one.
    pub policy_closed: bool,
    /// The commit this binary came from, when it knows.
    ///
    /// `None` is "built outside release CI", never a plausible-looking hash.
    pub build: Option<String>,
    /// What the store can attest was collected.
    pub coverage: CoverageReport,
    /// Every stage's count, including the stages that produced nothing.
    pub funnel: Funnel,
    /// Every refusal, by reason.
    pub refusals: Refusals,
    /// Every call, including the ones that bought nothing.
    pub spend: Spend,
    /// The clocks that make a fill honest.
    pub timings: Timings,
    /// What the account is worth, or why that cannot be said.
    pub equity: AccountView,
}

impl SessionRecord {
    /// The key a record is stored and found under.
    ///
    /// Watermark first so a directory listing sorts by the point in the store's
    /// history the run was taken at, which is the order a reader wants. The
    /// start time breaks the tie between two runs at one watermark — the hourly
    /// cron produces exactly that whenever the recorder is behind.
    #[must_use]
    pub fn key(decided_at: Slot, started_at_unix: u64) -> String {
        format!("{:020}-{started_at_unix:010}", decided_at.get())
    }

    /// How long the pass took, in whole seconds.
    ///
    /// Saturating: a clock that went backwards between the two reads is a fact
    /// about the host, and reporting it as a negative duration cast to a huge
    /// positive one would be worse than reporting nothing happened.
    #[must_use]
    pub const fn elapsed_secs(&self) -> u64 {
        self.finished_at_unix.saturating_sub(self.started_at_unix)
    }
}

/// What the store can attest about its own collection, at the run's watermark.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CoverageReport {
    /// One entry per table this build knows.
    pub tables: Vec<TableCoverage>,
    /// Whether anything attests collection over the slots this run considered.
    pub window: WindowCoverage,
}

/// What one table's coverage records say.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TableCoverage {
    /// The table, as the store names its directory.
    pub table: String,
    /// What is attested about it.
    pub state: CoverageState,
}

/// Whether anybody attested collecting a table, and what they found.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    /// No coverage record exists at this watermark.
    ///
    /// **Not zero rows.** Nobody has said whether this table was ever
    /// collected, so anything derived from its emptiness is a fact about the
    /// recorder. This is the state the coverage table was built to make
    /// expressible.
    NeverAttested,
    /// Records exist, and this is what they say.
    Attested {
        /// Ranges that ran to the end of their window and observed slots.
        complete_spans: usize,
        /// Ranges that ran to the end and observed nothing. A **measurement**.
        measured_empty: usize,
        /// Ranges that did not finish. An attempt, not a measurement.
        unfinished: usize,
    },
}

/// Whether the slots this run considered were ever collected.
///
/// The distinction the whole report rests on. A `consider` pass over a window
/// nobody collected finds nothing, and the sentence "0 launches in window" is
/// then a statement about the recorder wearing a statement about the market's
/// clothes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowCoverage {
    /// A completed collection observed slots inside the considered window.
    Attested {
        /// The lowest attested slot inside the window.
        from: Slot,
        /// The highest.
        to: Slot,
    },
    /// Collection over these slots finished and observed nothing.
    ///
    /// Zero candidates here is a measurement and may be reported as one.
    MeasuredEmpty,
    /// Nothing attests these slots.
    ///
    /// Zero candidates here is **not** a measurement, and a report must say so
    /// rather than print the zero.
    Unattested,
}

impl WindowCoverage {
    /// Whether a count of zero over this window may be read as a measurement.
    ///
    /// The one question a renderer asks. `false` for [`Unattested`]: the count
    /// describes the instrument.
    ///
    /// [`Unattested`]: Self::Unattested
    #[must_use]
    pub const fn zero_is_a_measurement(self) -> bool {
        match self {
            Self::Attested { .. } | Self::MeasuredEmpty => true,
            Self::Unattested => false,
        }
    }
}

/// Every stage of the pass, counted — including the stages that produced
/// nothing and the candidates that were excluded.
///
/// Design 0017 §6: *"Do not discard excluded candidates from measurement."* A
/// run that considered two hundred candidates and proposed three reports two
/// hundred.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Funnel {
    /// Launches in the store at the watermark.
    pub launches_recorded: usize,
    /// Creators in the store at the watermark.
    pub creators_recorded: usize,
    /// Launches inside the considered window. **The denominator.**
    pub in_window: usize,
    /// In-window mints no candidate could be assembled for.
    ///
    /// Counted rather than skipped: the free tier used to `continue` past these
    /// silently, so a store missing launch facts and a market with nothing in it
    /// produced the same tally.
    pub unbuildable: usize,
    /// Candidates the free tier refused outright — no paid look could change
    /// the answer.
    pub refused_free: usize,
    /// Candidates whose only remaining objection a paid look could remove.
    pub worth_paying_for: usize,
    /// How many of those the paid tier actually examined.
    pub paid_examined: usize,
    /// How many it did not, because the cap ran out.
    ///
    /// An exclusion, not an absence. These were worth paying for and nobody
    /// looked, and a proposal rate computed without them measures the cap.
    pub deferred_by_cap: usize,
    /// Examined candidates refused on launch-block shape before any exit probe.
    pub refused_on_shape: usize,
    /// Examined candidates whose launch block could not be read at all.
    ///
    /// The gate was silently off for these, rather than clean.
    pub look_failed: usize,
    /// Examined candidates that vanished between the exit probe and the
    /// strategy, because no candidate could be assembled from them.
    pub dropped_after_probe: usize,
    /// Examined candidates the strategy passed over after the paid look.
    pub passed_paid: usize,
    /// Proposals raised.
    pub proposed: usize,
    /// Proposals the kernel authorised.
    pub kernel_authorised: usize,
    /// Proposals the kernel refused.
    pub kernel_refused: usize,
}

impl Funnel {
    /// In-window candidates this funnel cannot account for.
    ///
    /// `0` when every launch in the window ends up in exactly one bucket. Any
    /// other number is a leak, and it is **printed** rather than absorbed: a
    /// funnel whose arithmetic closes by construction cannot report that a
    /// stage lost rows.
    ///
    /// Signed, because both directions are faults and they are different ones.
    /// Positive means candidates went missing; negative means a stage counted
    /// more than reached it.
    #[must_use]
    pub const fn unaccounted(&self) -> i64 {
        let placed = self
            .unbuildable
            .saturating_add(self.refused_free)
            .saturating_add(self.worth_paying_for);
        #[expect(
            clippy::cast_possible_wrap,
            reason = "both sides are candidate counts bounded by the store's launch \
                      count; a store with 2^63 launches in one window is not a \
                      state this arithmetic is the problem in"
        )]
        let residual = self.in_window as i64 - placed as i64;
        residual
    }

    /// Whether the paid tier's own buckets account for everything it examined.
    ///
    /// The same property one level down, kept separate because the two stages
    /// fail for different reasons and a single number would hide which.
    #[must_use]
    pub const fn paid_unaccounted(&self) -> i64 {
        let placed = self
            .refused_on_shape
            .saturating_add(self.dropped_after_probe)
            .saturating_add(self.passed_paid)
            .saturating_add(self.proposed);
        #[expect(
            clippy::cast_possible_wrap,
            reason = "bounded by the paid-tier cap, which is an operator flag in the \
                      tens"
        )]
        let residual = self.paid_examined as i64 - placed as i64;
        residual
    }

    /// Proposals the kernel never judged.
    ///
    /// A proposal the kernel never saw is a different state from one it refused
    /// — [`KernelOutcome`] says so per row, and this says so per run.
    ///
    /// [`KernelOutcome`]: https://github.com/hey-vera/radar
    #[must_use]
    pub const fn kernel_unseen(&self) -> usize {
        self.proposed
            .saturating_sub(self.kernel_authorised)
            .saturating_sub(self.kernel_refused)
    }
}

/// Every refusal the pass raised, by reason.
///
/// Kept as strings for [`Decision`]'s reason: this record outlives the code, and
/// a reason retired from the strategy must still read correctly from a file
/// written a month ago.
///
/// [`Decision`]: https://github.com/hey-vera/radar
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Refusals {
    /// Strategy refusals raised in the free tier, worst-first.
    ///
    /// One candidate raises several, so these sum past the candidate count. The
    /// renderer says so rather than letting a reader add them up.
    pub free_tier: Vec<(String, usize)>,
    /// Strategy refusals raised after the paid look.
    pub paid_tier: Vec<(String, usize)>,
    /// Risk-kernel refusals, by reason.
    pub kernel: Vec<(String, usize)>,
    /// Holdings the account could not place at all, by reason.
    pub portfolio: Vec<(String, usize)>,
}

impl Refusals {
    /// How many refusal *raisings* are recorded here.
    ///
    /// Deliberately not "how many candidates were refused": a candidate raising
    /// three reasons contributes three. Named for what it counts so nobody
    /// reads it as a population.
    #[must_use]
    pub fn raisings(&self) -> usize {
        let sum = |v: &Vec<(String, usize)>| v.iter().map(|(_, n)| *n).sum::<usize>();
        sum(&self.free_tier) + sum(&self.paid_tier) + sum(&self.kernel) + sum(&self.portfolio)
    }
}

/// What the pass cost, including the work that produced nothing.
///
/// Design 0018 §7: *"Rejected candidates, abandoned investigations, timed-out
/// jobs, retries and every child call in the decision tree are charged to the
/// arm that requested them. An arm cannot improve its number by discarding what
/// it wasted."*
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Spend {
    /// One entry per kind of call the pass makes.
    pub calls: Vec<CallTally>,
    /// What it cost in money.
    pub money: MoneySpent,
}

impl Spend {
    /// Calls attempted, across every kind.
    #[must_use]
    pub fn attempted(&self) -> usize {
        self.calls.iter().map(|c| c.attempted).sum()
    }

    /// Calls that failed, across every kind.
    #[must_use]
    pub fn failed(&self) -> usize {
        self.calls.iter().map(|c| c.failed).sum()
    }

    /// Calls that succeeded and changed no answer.
    ///
    /// Charged, and reported. This is the number a cheaper tiering would move.
    #[must_use]
    pub fn wasted(&self) -> usize {
        self.calls.iter().map(|c| c.produced_nothing).sum()
    }
}

/// One kind of call, and how it came out.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct CallTally {
    /// What was called.
    pub kind: String,
    /// How many times.
    pub attempted: usize,
    /// How many of those failed.
    ///
    /// A failed call is still a call and is still charged. It also leaves the
    /// gate it fed silently off, which is why it is counted apart from a call
    /// that answered.
    pub failed: usize,
    /// How many succeeded and changed nothing.
    pub produced_nothing: usize,
}

/// What the pass cost in money.
///
/// An enum rather than a `u64`, because the honest answer on this instance is
/// that nobody knows: design 0018 §11 leaves provider and model selection unset,
/// so no cost-rate table exists to price a call with. A zero here would read as
/// "free", and free is the one thing it is not.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoneySpent {
    /// Priced, against a named rate table.
    Measured {
        /// The bill.
        micro_usd: u64,
        /// Which rate table priced it, so two runs under different rates are
        /// distinguishable without the rates being in the file.
        rate_table: String,
    },
    /// Not priced, and here is why.
    Unmeasured(UnmeasuredCost),
}

impl Default for MoneySpent {
    /// Unmeasured, for want of a rate table. The default has to be the honest
    /// one: a `Measured { micro_usd: 0 }` default would put "free" on every run
    /// nobody priced.
    fn default() -> Self {
        Self::Unmeasured(UnmeasuredCost::NoCostRateTable)
    }
}

impl MoneySpent {
    /// The bill, when there is one. Never zero as a stand-in.
    #[must_use]
    pub const fn micro_usd(&self) -> Option<u64> {
        match self {
            Self::Measured { micro_usd, .. } => Some(*micro_usd),
            Self::Unmeasured(_) => None,
        }
    }
}

/// Why a pass could not be priced.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnmeasuredCost {
    /// No provider or model is selected on this instance, so no rate exists to
    /// multiply the call counts by. Design 0018 §11.
    NoCostRateTable,
}

/// The clocks that decide whether a fill is honest.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Timings {
    /// The watermark the evidence was ready at.
    pub evidence_ready_at: Slot,
    /// Wall clock over the whole pass, in milliseconds.
    pub reasoning_ms: u64,
    /// One entry per candidate the paid tier examined.
    ///
    /// Only the paid tier, for [`record_of`]'s reason: a free-tier refusal is a
    /// pure function of data already in the store and can be re-derived, and
    /// forty thousand rows an hour to store a derivation is storage, not
    /// evidence.
    ///
    /// [`record_of`]: https://github.com/hey-vera/radar
    pub candidates: Vec<CandidateTiming>,
}

/// When one candidate could have been acted on, and when it could not.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CandidateTiming {
    /// The mint.
    pub mint: String,
    /// The slot it launched at — when the opportunity **happened**.
    pub launch_slot: Slot,
    /// When Radar could first have seen it — when the opportunity became
    /// **visible**.
    pub visible_at: Visibility,
    /// How long reasoning about this candidate took, in milliseconds.
    pub reasoning_ms: u64,
    /// The earliest slot an action could have been taken.
    pub earliest_entry: EarliestEntry,
}

/// When a launch became available to be reasoned about.
///
/// Availability time, not event time — design 0017 §6, and the reason the
/// coverage table records a collection watermark rather than only a range. A
/// launch that happened at slot 100 and was collected at slot 900 was not
/// actionable at slot 100, and filling at slot 100's price would be a return
/// nobody could have had.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// A completed collection whose observed span contains the launch was
    /// established at this slot. The launch was visible no earlier.
    At(Slot),
    /// No coverage record attests when this launch became visible.
    ///
    /// The launch slot is **not** substituted. Doing so is exactly the fill at
    /// the launch price for a decision taken forty minutes later.
    Unattested,
}

impl Visibility {
    /// The slot, when one is attested. Never the launch slot as a stand-in.
    #[must_use]
    pub const fn slot(self) -> Option<Slot> {
        match self {
            Self::At(slot) => Some(slot),
            Self::Unattested => None,
        }
    }
}

/// The earliest slot at which an action could have been taken.
///
/// Design 0017 §6: *"The earliest eligible entry is after the evidence arrives,
/// reasoning completes, policy approves and a transaction could be built/landed.
/// Never fill at the launch price for a decision made forty minutes later."*
/// All four conditions, and the type refuses to name a slot unless all four are
/// answerable.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EarliestEntry {
    /// Evidence had arrived, reasoning had finished, the policy approved, and a
    /// transaction could have been built and landed by this slot.
    At(Slot),
    /// One of the four could not be answered, and this is which.
    Unknown(NoEntryTime),
}

impl EarliestEntry {
    /// The slot, when there is one.
    #[must_use]
    pub const fn slot(self) -> Option<Slot> {
        match self {
            Self::At(slot) => Some(slot),
            Self::Unknown(_) => None,
        }
    }
}

/// Why no earliest eligible entry exists.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoEntryTime {
    /// The policy in force authorises nothing, so no slot was eligible.
    ///
    /// The true answer for every run under `Policy::CLOSED`, which is what
    /// ships. It is not "we could have entered at the watermark but chose not
    /// to"; there was no eligible slot at all.
    PolicyRefused,
    /// The policy could authorise, but no build-and-land latency has been
    /// measured on this instance.
    ///
    /// A slot cannot be named without one, and it must not be guessed: the
    /// "~2.5 slots a second" figures in this tree are bucketing heuristics, and
    /// using one to name an entry boundary would fabricate the number the whole
    /// fill rests on. Design 0018 §11 lists per-arm latency among the
    /// measurements nobody has taken.
    LandingLatencyUnmeasured,
    /// Nothing attests when the candidate became visible, so no clock has a
    /// start.
    VisibilityUnattested,
}

/// Whether the account could be read at all.
///
/// An account that could not be read is not an empty one, and the two would
/// print identically as a row of zeros. `radar consider` already refuses to size
/// against a portfolio it could not read; this is the same refusal on the
/// reporting side, where the convenient default is a clean sheet.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountView {
    /// It was read, and this is what it said.
    Read(EquityReport),
    /// It was not, and nothing about it is known — not even that it is empty.
    Unreadable {
        /// What the read said, as the operator would have been told.
        because: String,
    },
}

impl AccountView {
    /// The equity, when an account was read and could state one.
    ///
    /// Two absences fold into `None` here and they are kept apart everywhere a
    /// reader looks: an account nobody could read, and one holding something
    /// nobody could price. This method exists for the callers that only need to
    /// know there is no number.
    #[must_use]
    pub fn equity_micro_usd(&self) -> Option<i64> {
        match self {
            Self::Read(report) => report.equity.micro_usd(),
            Self::Unreadable { .. } => None,
        }
    }
}

/// What the account is worth, or why that cannot be said.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EquityReport {
    /// Open holdings.
    pub holdings: usize,
    /// Recorded exposure the account could not place as a holding at all.
    pub unaccounted: usize,
    /// Round trips that closed, netted, signed.
    pub realised_micro_usd: i64,
    /// Open holdings, valued — or why they are not.
    pub unrealised: UnrealisedReport,
    /// Realised plus unrealised, or why that total cannot be stated.
    pub equity: EquityTotal,
    /// Lamports paid out, by kind.
    ///
    /// Kept in lamports and kept apart from the dollar figures, because
    /// converting them needs a SOL price this record does not hold and a run
    /// that could not fetch one would otherwise silently convert at nothing.
    pub costs: CostsReport,
}

/// Lamports paid out, by kind.
///
/// Three fields rather than one total, for [`Costs`](crate::Costs)' reason: rent
/// is recoverable, a priority fee is not, and a tip is discretionary.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct CostsReport {
    /// Base and priority fees paid to validators.
    pub network_fee_lamports: u64,
    /// Lamports locked in account rent.
    pub rent_lamports: u64,
    /// Tips to block-building services.
    pub tip_lamports: u64,
}

/// The paper result, or why there is not one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnrealisedReport {
    /// This much, as of the stalest slot contributing to it.
    Known {
        /// The signed difference between value and cost.
        micro_usd: i64,
        /// The stalest slot behind it.
        as_of: Slot,
    },
    /// Not computable, and here is why.
    Unknown(Unvaluable),
}

impl UnrealisedReport {
    /// Reads a [`Unrealised`](crate::Unrealised) into the reportable form.
    #[must_use]
    pub const fn of(unrealised: crate::Unrealised) -> Self {
        match unrealised {
            crate::Unrealised::Known { amount, as_of } => Self::Known {
                micro_usd: amount.0,
                as_of,
            },
            crate::Unrealised::Unknown(why) => Self::Unknown(why),
        }
    }

    /// The amount, or `None`. Never zero as a stand-in.
    #[must_use]
    pub const fn micro_usd(self) -> Option<i64> {
        match self {
            Self::Known { micro_usd, .. } => Some(micro_usd),
            Self::Unknown(_) => None,
        }
    }
}

/// What the account is worth in total.
///
/// The rule [`Portfolio::results`](crate::Portfolio::results) enforces, carried
/// into the report: **one holding nobody can price makes the whole total
/// unknown, not smaller.** A renderer holding this type cannot print a number
/// unless one exists.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquityTotal {
    /// Realised plus unrealised, both known.
    Known {
        /// The total.
        micro_usd: i64,
        /// The stalest slot behind the unrealised half.
        as_of: Slot,
    },
    /// One half could not be valued, so no total exists.
    ///
    /// Carries the same reason the unrealised half gave, so a reader is told
    /// *what* could not be priced rather than only that something could not.
    Unknown(Unvaluable),
}

impl EquityTotal {
    /// The only constructor.
    ///
    /// A free function rather than struct literals at call sites, because the
    /// rule is the construction: an `Unknown` unrealised half can only produce
    /// an `Unknown` total, and there is no code path that adds a realised figure
    /// to nothing and calls the result equity.
    #[must_use]
    pub const fn of(realised: SignedMicroUsd, unrealised: UnrealisedReport) -> Self {
        match unrealised {
            UnrealisedReport::Known { micro_usd, as_of } => Self::Known {
                micro_usd: realised.0.saturating_add(micro_usd),
                as_of,
            },
            UnrealisedReport::Unknown(why) => Self::Unknown(why),
        }
    }

    /// The total, when there is one. Never zero, never the realised half alone.
    #[must_use]
    pub const fn micro_usd(self) -> Option<i64> {
        match self {
            Self::Known { micro_usd, .. } => Some(micro_usd),
            Self::Unknown(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AccountView, CostsReport, CoverageState, EarliestEntry, EquityReport, EquityTotal, Funnel,
        MoneySpent, NoEntryTime, Refusals, SessionRecord, Spend, UnmeasuredCost, UnrealisedReport,
        Visibility, WindowCoverage,
    };
    use crate::{SignedMicroUsd, Slot, Unrealised, Unvaluable};

    #[test]
    fn one_unvaluable_holding_makes_the_whole_total_unknown() {
        // The rule #240 put in `Portfolio::results`, and the one a report is
        // likeliest to undo. Re-apply the bug -- return the realised half when
        // the unrealised one is unknown -- and this fails.
        let realised = SignedMicroUsd(4_000_000);
        let total = EquityTotal::of(realised, UnrealisedReport::Unknown(Unvaluable::NoPrice));

        assert_eq!(
            total,
            EquityTotal::Unknown(Unvaluable::NoPrice),
            "an unpriceable holding removes the total, it does not shrink it"
        );
        assert_eq!(
            total.micro_usd(),
            None,
            "there is no number here, and $4.00 is the number that would get permission"
        );
    }

    #[test]
    fn a_known_total_is_the_two_halves_added_and_dated() {
        let total = EquityTotal::of(
            SignedMicroUsd(-1_500_000),
            UnrealisedReport::Known {
                micro_usd: 2_000_000,
                as_of: Slot(900),
            },
        );
        assert_eq!(
            total,
            EquityTotal::Known {
                micro_usd: 500_000,
                as_of: Slot(900),
            }
        );
    }

    #[test]
    fn an_account_holding_nothing_is_a_measured_zero_and_not_an_unknown() {
        // The other direction, and it matters as much: refusing to state a
        // total for an empty account would make "nothing is open" unreportable,
        // and a report that says "unknown" whatever happened is not a report.
        let empty = UnrealisedReport::of(Unrealised::Known {
            amount: SignedMicroUsd::ZERO,
            as_of: Slot(10),
        });
        assert_eq!(
            EquityTotal::of(SignedMicroUsd::ZERO, empty).micro_usd(),
            Some(0)
        );
    }

    #[test]
    fn an_unknown_unrealised_half_carries_its_reason_across() {
        for why in [
            Unvaluable::NoPrice,
            Unvaluable::QuoteUnpriced,
            Unvaluable::BalanceUncounted,
            Unvaluable::NotDated,
        ] {
            let reported = UnrealisedReport::of(Unrealised::Unknown(why));
            assert_eq!(reported, UnrealisedReport::Unknown(why));
            assert_eq!(reported.micro_usd(), None);
            assert_eq!(
                UnrealisedReport::Known {
                    micro_usd: -3,
                    as_of: Slot(4),
                }
                .micro_usd(),
                Some(-3),
                "and a figure that exists is returned rather than withheld"
            );
            assert_eq!(
                EquityTotal::of(SignedMicroUsd(1), reported),
                EquityTotal::Unknown(why),
                "a reader is told what could not be priced, not only that something was not"
            );
        }
    }

    #[test]
    fn a_zero_over_a_window_nobody_collected_is_not_a_measurement() {
        // The data-side half of the same rule. Re-apply the bug -- let
        // `Unattested` answer true -- and this fails.
        assert!(!WindowCoverage::Unattested.zero_is_a_measurement());
        assert!(WindowCoverage::MeasuredEmpty.zero_is_a_measurement());
        assert!(
            WindowCoverage::Attested {
                from: Slot(1),
                to: Slot(2)
            }
            .zero_is_a_measurement()
        );
    }

    #[test]
    fn a_funnel_that_loses_candidates_says_by_how_many() {
        // Two hundred considered, three proposed, and the other hundred and
        // ninety-seven have to be somewhere.
        let leaky = Funnel {
            in_window: 200,
            unbuildable: 0,
            refused_free: 100,
            worth_paying_for: 3,
            ..Funnel::default()
        };
        assert_eq!(
            leaky.unaccounted(),
            97,
            "ninety-seven candidates are in no bucket and the report has to say so"
        );

        let closed = Funnel {
            in_window: 200,
            unbuildable: 7,
            refused_free: 190,
            worth_paying_for: 3,
            ..Funnel::default()
        };
        assert_eq!(closed.unaccounted(), 0);
    }

    #[test]
    fn a_stage_counting_more_than_reached_it_reports_the_other_sign() {
        // Both directions are faults and they are different ones. A single
        // `usize` would saturate the overcount to zero and read as balanced.
        let over = Funnel {
            in_window: 10,
            refused_free: 12,
            ..Funnel::default()
        };
        assert_eq!(over.unaccounted(), -2);
    }

    #[test]
    fn the_paid_tier_accounts_for_what_it_examined_separately() {
        let f = Funnel {
            paid_examined: 25,
            refused_on_shape: 4,
            dropped_after_probe: 1,
            passed_paid: 18,
            proposed: 2,
            ..Funnel::default()
        };
        assert_eq!(f.paid_unaccounted(), 0);
        assert_eq!(
            f.unaccounted(),
            0,
            "an empty outer funnel is still balanced"
        );

        // And a paid tier that lost rows says by how many, in both directions.
        let leaky = Funnel {
            passed_paid: 10,
            ..f
        };
        assert_eq!(leaky.paid_unaccounted(), 8);
        let over = Funnel {
            passed_paid: 30,
            ..f
        };
        assert_eq!(over.paid_unaccounted(), -12);
    }

    #[test]
    fn a_pass_reports_how_long_it_took_and_never_a_negative_duration() {
        let record = |started: u64, finished: u64| -> u64 {
            let base = super::SessionRecord {
                schema: super::SESSION_SCHEMA,
                run_id: SessionRecord::key(Slot(0), started),
                started_at_unix: started,
                finished_at_unix: finished,
                decided_at: Slot(0),
                store: String::new(),
                window_slots: 0,
                paid_tier_cap: 0,
                strategy: String::new(),
                strategy_version: String::new(),
                assumed_round_trip_bps: 0,
                pricing: String::new(),
                policy_closed: true,
                build: None,
                coverage: super::CoverageReport {
                    tables: Vec::new(),
                    window: WindowCoverage::Unattested,
                },
                funnel: Funnel::default(),
                refusals: Refusals::default(),
                spend: Spend::default(),
                timings: super::Timings {
                    evidence_ready_at: Slot(0),
                    reasoning_ms: 0,
                    candidates: Vec::new(),
                },
                equity: AccountView::Unreadable {
                    because: String::new(),
                },
            };
            base.elapsed_secs()
        };

        assert_eq!(record(100, 147), 47);
        // A clock that went backwards is a fact about the host. Zero rather
        // than a duration that wrapped into eighteen quintillion seconds.
        assert_eq!(record(147, 100), 0);
    }

    #[test]
    fn a_proposal_the_kernel_never_judged_is_counted_apart_from_a_refusal() {
        let f = Funnel {
            proposed: 5,
            kernel_authorised: 1,
            kernel_refused: 2,
            ..Funnel::default()
        };
        assert_eq!(f.kernel_unseen(), 2);

        // And a kernel that saw everything leaves none.
        let seen = Funnel {
            proposed: 3,
            kernel_refused: 3,
            ..Funnel::default()
        };
        assert_eq!(seen.kernel_unseen(), 0);
    }

    #[test]
    fn a_pass_nobody_priced_reports_no_bill_rather_than_a_free_one() {
        // The default has to be the honest one. `Measured { micro_usd: 0 }`
        // would put "free" on every run on this instance, and free is the one
        // thing an inference call is not.
        assert_eq!(
            MoneySpent::default(),
            MoneySpent::Unmeasured(UnmeasuredCost::NoCostRateTable)
        );
        assert_eq!(MoneySpent::default().micro_usd(), None);
        assert_eq!(
            MoneySpent::Measured {
                micro_usd: 0,
                rate_table: "t".to_owned()
            }
            .micro_usd(),
            Some(0),
            "a rate table that priced a pass at nothing is a different claim and is kept"
        );
    }

    #[test]
    fn an_unattested_launch_never_borrows_the_launch_slot() {
        // Substituting the launch slot here is the forty-minutes-later fill,
        // exactly. Re-apply it -- have `slot()` fall back -- and this fails.
        assert_eq!(Visibility::Unattested.slot(), None);
        assert_eq!(Visibility::At(Slot(77)).slot(), Some(Slot(77)));
    }

    #[test]
    fn a_closed_policy_has_no_earliest_entry_at_all() {
        // Not "the watermark, but we declined". There was no eligible slot.
        for why in [
            NoEntryTime::PolicyRefused,
            NoEntryTime::LandingLatencyUnmeasured,
            NoEntryTime::VisibilityUnattested,
        ] {
            assert_eq!(EarliestEntry::Unknown(why).slot(), None);
        }
        assert_eq!(EarliestEntry::At(Slot(5)).slot(), Some(Slot(5)));
    }

    #[test]
    fn refusal_raisings_are_named_for_what_they_count() {
        // Three reasons on one candidate is three raisings and one candidate,
        // and the report must never present the first number as the second.
        let r = Refusals {
            free_tier: vec![
                ("NoExitSimulated".to_owned(), 40),
                ("NoRoute".to_owned(), 2),
            ],
            paid_tier: vec![("CreatorUnproven".to_owned(), 3)],
            kernel: vec![("NoAutonomy".to_owned(), 1)],
            // Every group non-empty, and no two of them equal. A zero anywhere
            // lets one of the three additions be a subtraction and still land
            // on the same total.
            portfolio: vec![("NotRecorded".to_owned(), 7)],
        };
        assert_eq!(r.raisings(), 53);
    }

    #[test]
    fn a_spend_reports_the_calls_that_bought_nothing() {
        // An arm cannot improve its number by discarding what it wasted.
        let s = Spend {
            calls: vec![
                super::CallTally {
                    kind: "launch_block".to_owned(),
                    attempted: 25,
                    failed: 3,
                    produced_nothing: 20,
                },
                super::CallTally {
                    kind: "exit_depth".to_owned(),
                    attempted: 22,
                    failed: 0,
                    produced_nothing: 19,
                },
            ],
            money: MoneySpent::default(),
        };
        assert_eq!(s.attempted(), 47);
        assert_eq!(s.failed(), 3);
        assert_eq!(s.wasted(), 39);
    }

    #[test]
    fn a_key_sorts_by_watermark_and_then_by_start() {
        // A directory listing has to come back in the order a reader wants
        // without opening anything, and two runs at one watermark is what the
        // hourly cron produces whenever the recorder is behind.
        let mut keys = [
            SessionRecord::key(Slot(9), 100),
            SessionRecord::key(Slot(10), 5),
            SessionRecord::key(Slot(9), 50),
        ];
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                SessionRecord::key(Slot(9), 50),
                SessionRecord::key(Slot(9), 100),
                SessionRecord::key(Slot(10), 5),
            ]
        );
    }

    #[test]
    fn an_account_nobody_could_read_is_not_an_empty_one() {
        // A row of zeros is what an unreadable account and a flat one both look
        // like once a report has flattened them, and only one of them is a
        // measurement. Re-apply the bug -- report an unreadable account as a
        // read one holding nothing -- and this fails.
        let unreadable = AccountView::Unreadable {
            because: "positions unreadable".to_owned(),
        };
        assert_eq!(unreadable.equity_micro_usd(), None);

        let flat = AccountView::Read(EquityReport {
            holdings: 0,
            unaccounted: 0,
            realised_micro_usd: 0,
            unrealised: UnrealisedReport::Known {
                micro_usd: 0,
                as_of: Slot(1),
            },
            equity: EquityTotal::Known {
                micro_usd: 0,
                as_of: Slot(1),
            },
            costs: CostsReport::default(),
        });
        assert_eq!(
            flat.equity_micro_usd(),
            Some(0),
            "an account that was read and holds nothing states its zero"
        );
        assert_ne!(unreadable, flat);
    }

    #[test]
    fn coverage_keeps_ran_and_saw_nothing_apart_from_nobody_ran() {
        let never = CoverageState::NeverAttested;
        let empty = CoverageState::Attested {
            complete_spans: 0,
            measured_empty: 4,
            unfinished: 0,
        };
        assert_ne!(
            never, empty,
            "a table nobody collected and one collected four times to find nothing \
             are different facts"
        );
    }
}
