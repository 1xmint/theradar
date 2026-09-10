// SPDX-License-Identifier: Apache-2.0
//! One event, and the chain it belongs to.

use serde::{Deserialize, Serialize};

/// The schema this build writes.
///
/// On every event rather than once per file: a journal is appended to across
/// restarts and upgrades, so two versions legitimately sit in one file and a
/// reader has to be told which line is which.
pub const SCHEMA_VERSION: u32 = 1;

/// The longest a caller-supplied diagnostic may be.
///
/// A bound rather than a warning, because the thing most likely to arrive here
/// is a provider's error body, and the thing most likely to be in a provider's
/// error body is the request that caused it — headers included. Truncating is
/// not a defence against a credential in the first hundred bytes; the defence is
/// that callers pass a reason, not a response. The bound keeps one bad day from
/// filling the disk that the next event has to be written to.
pub const MAX_REDACTED: usize = 512;

/// Which part of the loop an event belongs to.
///
/// Named stages rather than free text, so a replay can find every event of a
/// kind without matching strings, and so adding one is a change a reader of this
/// enum meets rather than a new spelling appearing in the file.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// A mention arrived.
    Received,
    /// It was parsed into something the loop understands, or was not.
    Parsed,
    /// The admission gate ruled on it.
    Admitted,
    /// The chain and the store were read for it.
    InputFetched,
    /// A fact sheet was built from what came back, or withheld.
    FactBuilt,
    /// The model was asked, and its answer was used or refused.
    ModelAnswered,
    /// A public statement was prepared, submitted, or settled.
    Publication,
    /// One contest candidate was scored.
    CandidateScored,
    /// The week's scoring mode was chosen.
    ScoringMode,
    /// A winner was selected.
    WinnerSelected,
    /// A claim was accepted or refused.
    Claim,
    /// A payout was prepared, submitted, confirmed or reconciled.
    Payout,
    /// An operation against the account moved from one state to the next.
    ///
    /// The money path's own stage. Every event carrying it also carries an
    /// [`OperationEntry`](crate::OperationEntry), which is what makes the record
    /// something a restart can be rebuilt from rather than something a person
    /// can read.
    Operation,
}

/// How a stage came out.
///
/// # Why `Uncertain` is not a failure
///
/// It is the whole point. A post the platform accepted whose response was lost,
/// and a transaction broadcast whose confirmation never arrived, are not
/// failures — and treating them as one is how one payout becomes two. A caller
/// seeing this must reconcile, never retry.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The stage did what it set out to do.
    Ok,
    /// It deliberately did not: a refusal, a withheld fact, a fallback taken.
    ///
    /// A decision, not a fault. The reason belongs in `public_reason`.
    Refused,
    /// It failed, and the failure is established.
    Failed,
    /// **Nobody knows.** The effect may or may not have happened.
    Uncertain,
}

/// The ids that tie one event to the others about the same thing.
///
/// All optional and all skipped when absent, so a line stays readable and an
/// event about a payout does not carry six nulls about mentions. The
/// `correlation` on [`Event`] is what groups a run; these are what let a reader
/// find that run from a mint, a week, or a transaction they are holding.
#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Correlation {
    /// The mention that started it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mention: Option<String>,
    /// The receipt this concerns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<String>,
    /// The contest nomination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nomination: Option<String>,
    /// The contest week, as the ledger spells it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub week: Option<String>,
    /// The claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<String>,
    /// The payout's transaction signature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payout: Option<String>,
    /// The mint everything here is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mint: Option<String>,
    /// The operation this event advances.
    ///
    /// The id of the event that proposed it, so every later event about the
    /// same operation is findable from the one that opened it. Absent on the
    /// proposal itself, which *is* that event and cannot name its own id
    /// without the digest covering a field derived from the digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
}

impl Correlation {
    /// Whether this names nothing at all.
    ///
    /// An event with no correlation is findable only by sequence, which is
    /// usually a caller that forgot rather than a fact about the event — so
    /// [`Journal::record`](crate::Journal::record) can say so.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.mention.is_none()
            && self.receipt.is_none()
            && self.nomination.is_none()
            && self.week.is_none()
            && self.claim.is_none()
            && self.payout.is_none()
            && self.mint.is_none()
            && self.operation.is_none()
    }
}

/// One line of the journal.
///
/// Written by [`Journal::record`](crate::Journal::record), which fills the
/// fields a caller must not choose: the sequence, the previous hash, and this
/// event's own id.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Event {
    /// Which schema wrote this line.
    pub schema: u32,
    /// Position in the chain, from one, with no gaps.
    ///
    /// A gap is a visible fault rather than a missing line: something wrote to
    /// this file that was not this journal, or something removed a line.
    pub sequence: u64,
    /// This event's stable id — the hash of everything above and below it.
    ///
    /// Stable because it is derived rather than drawn: two runs that recorded
    /// the same event after the same history give it the same id, which is what
    /// lets a replay compare by id rather than by position.
    pub id: String,
    /// The id of the event before it, or the empty string for the first.
    pub previous: String,
    /// What this event is about.
    pub correlation: Correlation,
    /// Which part of the loop.
    pub stage: Stage,
    /// How it came out.
    pub outcome: Outcome,
    /// UTC seconds since the epoch.
    ///
    /// Supplied by the caller rather than read here, for the reason the risk
    /// kernel is pure: a clock inside makes a replay disagree with itself.
    pub at: u64,
    /// How long the stage took, in milliseconds, where that was measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub took_ms: Option<u64>,
    /// The commit this build came from, when the binary knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    /// The versions of the rules that decided this — scoring, decoder, model.
    ///
    /// A map rather than three fields, because which of them matters depends on
    /// the stage and a payout event carrying a null model version is noise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<(String, String)>,
    /// A hash of the configuration in force, so two runs under different
    /// settings are distinguishable without the settings being in the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    /// What a person is told, when they are told anything.
    ///
    /// Kept apart from [`redacted`](Self::redacted) deliberately: this is the
    /// sentence that may be published, and a developer diagnostic that drifted
    /// into a public position is how an internal error becomes a reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_reason: Option<String>,
    /// Bounded developer detail. Never a credential, never a header, never a
    /// chain of thought — see [`MAX_REDACTED`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redacted: Option<String>,
    /// What an operation intended, what it reserved, and where it got to.
    ///
    /// Structured rather than folded into [`redacted`](Self::redacted), because
    /// this is the field a restart is rebuilt from. A diagnostic string is
    /// something a person reads; this is something the process parses before it
    /// is allowed to believe its own balances.
    ///
    /// `None` on every event that is not about an operation, and **skipped
    /// entirely** when it is `None`. The digest hashes it only when it is
    /// present, so every chain written before this field existed still
    /// verifies — an id that moved because a field was added would break every
    /// journal already on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<crate::operation::OperationEntry>,
}

impl Event {
    /// The bytes an event's id is taken over.
    ///
    /// Everything except the id itself, in a fixed order. `serde_json` of the
    /// whole struct would be simpler and wrong: field order in the serialised
    /// form is a serde implementation detail, and an id that moved when a field
    /// was reordered would break every chain already on disk.
    fn digest_input(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.schema.to_le_bytes());
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(self.previous.as_bytes());
        out.extend_from_slice(&self.at.to_le_bytes());
        // Through `serde_json` for the two structured fields, whose own shapes
        // are stable because they are this crate's types and their `Serialize`
        // is derived from the declaration order above.
        out.extend_from_slice(
            serde_json::to_string(&self.correlation)
                .unwrap_or_default()
                .as_bytes(),
        );
        out.extend_from_slice(
            serde_json::to_string(&self.stage)
                .unwrap_or_default()
                .as_bytes(),
        );
        out.extend_from_slice(
            serde_json::to_string(&self.outcome)
                .unwrap_or_default()
                .as_bytes(),
        );
        for (name, version) in &self.versions {
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(version.as_bytes());
        }
        for field in [
            self.build.as_deref(),
            self.config.as_deref(),
            self.public_reason.as_deref(),
            self.redacted.as_deref(),
        ] {
            // The separator matters: without it, `Some("ab") + None` and
            // `Some("a") + Some("b")` hash the same, and two different events
            // would share an id.
            out.push(0);
            out.extend_from_slice(field.unwrap_or("").as_bytes());
        }
        // Appended **only when present**, and with a marker byte of its own.
        //
        // Nothing is pushed for `None`, which is what keeps every journal
        // written before this field existed hashing to the same ids it already
        // has on disk. Writing an unconditional separator here instead would
        // change the digest of every historical event and turn every intact
        // chain into `Verified::Broken` at sequence one.
        if let Some(operation) = &self.operation {
            out.push(1);
            out.extend_from_slice(
                serde_json::to_string(operation)
                    .unwrap_or_default()
                    .as_bytes(),
            );
        }
        out
    }

    /// This event's id, from its contents and its place in the chain.
    #[must_use]
    pub fn digest(&self) -> String {
        blake3::hash(&self.digest_input()).to_hex().to_string()
    }
}

/// The receipt for an event that reached disk.
///
/// Held by every call site that then does something outward-facing, so "the
/// durable intent exists before the effect" is a thing the compiler asks about
/// rather than a thing a reviewer remembers. It cannot be built outside this
/// crate.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Recorded {
    event: Event,
}

impl Recorded {
    /// Only [`Journal`](crate::Journal) makes one, and only after the write
    /// returned.
    pub(crate) const fn new(event: Event) -> Self {
        Self { event }
    }

    /// The event as written.
    #[must_use]
    pub const fn event(&self) -> &Event {
        &self.event
    }

    /// Its id, which is what a later event about the same effect refers to.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.event.id
    }
}
