// SPDX-License-Identifier: Apache-2.0
//! What the recorder actually collected, and over what.
//!
//! # Why a file is not evidence
//!
//! Coverage was read from partition **filenames**: a trades partition existing
//! meant the window was covered, so a launch inside it got trade features. A
//! partition file is written when the first row lands in it and says nothing
//! about whether the rest of the range was ever queried — so a run that died a
//! quarter of the way through a window produced a file, and the file made the
//! remaining three quarters read as a quiet market. That is
//! [LEARNINGS] entry 10 in its exact form: a zero that is a measurement about
//! the instrument.
//!
//! A completed query that returned nothing and a query nobody ran are different
//! facts, and only one of them is a measurement. This table is where the
//! difference is written down.
//!
//! # Point-in-time, like everything else
//!
//! `recorded_at` is the slot column, and it is the **collection watermark** —
//! the moment the range was established complete. A replay at an earlier
//! watermark does not see it, which is the property that matters: a coverage
//! record written today must not make a decision taken last week look
//! better-informed than it was. AGENTS.md rule 3, applied to the evidence about
//! the evidence.
//!
//! # What a time window can honestly attest about slots
//!
//! The backfill queries CryptoHouse by `block_timestamp` and the store is keyed
//! by `block_slot`. There is no conversion between them here and there must not
//! be one: the "~2.5 slots a second" figures in this tree are bucketing
//! heuristics, and using one to name a boundary would fabricate the very number
//! this table exists to make honest.
//!
//! So a completed *time* window attests a slot range only through the events it
//! returned — the lowest and highest slot among them, which under-claims at the
//! edges, and under-claiming is the safe direction. **A completed window that
//! returned nothing has no slot range at all**, which is the exact case this
//! table exists to express. That is [`ObservedSlots::Nothing`], and it is a
//! recorded fact rather than a missing row: the difference between *we looked
//! and the market was quiet* and *nobody looked* is the whole point of the
//! table, and it cannot survive being written as slots zero to zero.
//!
//! [`radar-backfill`] writes these records — one per table a window collected,
//! on both the historical and the follow path.
//!
//! [LEARNINGS]: https://github.com/hey-vera/radar/blob/main/LEARNINGS.md
//! [`radar-backfill`]: https://github.com/hey-vera/radar/blob/main/crates/radar-backfill/src/main.rs

use radar_types::Slot;
use serde::{Deserialize, Serialize};

use crate::event::Table;

/// Whether a recorded ingestion range finished.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Completion {
    /// The query ran to the end of the range. Zero rows here is a measurement.
    Complete,
    /// It did not. Rows from this range exist and the range is not covered.
    Partial,
}

impl Completion {
    /// The name used in the stored column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
        }
    }

    /// The stored name, back.
    ///
    /// An unrecognised value reads as [`Partial`](Self::Partial): a status this
    /// build does not understand is not a promise this build may rely on, and
    /// the safe direction for coverage is to claim less of it.
    #[must_use]
    pub fn from_str_or_partial(s: &str) -> Self {
        match s {
            "complete" => Self::Complete,
            _ => Self::Partial,
        }
    }
}

/// The slots a recorded range actually touched.
///
/// # Why this is not two `Slot` fields
///
/// It was, and the pair could not say the one thing the table is for. A window
/// that ran to the end and returned no rows observed **no slot**, and every
/// value available to fill a `from`/`to` pair with is a fabrication: zero is
/// rule 9's forbidden default, and the window's own time bounds are not slots.
/// Writing no record at all is worse still — it is indistinguishable from a
/// window nobody ran, which is the failure this table exists to close.
///
/// So the range is one field with two states, and "ran, saw nothing" is
/// [`Nothing`](Self::Nothing): a fact the reader can act on rather than a
/// silence it has to guess at.
///
/// # Why not two `Option<Slot>` fields
///
/// Because `(Some(from), None)` would compile, and it means nothing. One field
/// holding both bounds together makes a half-known range unrepresentable rather
/// than merely undocumented — AGENTS.md §5, enforced at level 1.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedSlots {
    /// The rows the range returned, bounded by the lowest and highest slot
    /// among them. Inclusive at both ends.
    ///
    /// This under-claims deliberately: the queried window may reach past the
    /// first and last row it found, and the amount it reaches by is not
    /// knowable from the rows. Claiming less coverage than was collected is
    /// recoverable; claiming more is the quiet-market bug.
    Span {
        /// The lowest slot observed.
        from: Slot,
        /// The highest slot observed.
        to: Slot,
    },
    /// The range ran and returned no rows, so it observed no slot at all.
    ///
    /// Read with [`Completion::Complete`] this is a **measurement**: the
    /// collection finished and there was nothing there. Read with
    /// [`Completion::Partial`] it is an attempt that did not finish.
    Nothing,
}

impl ObservedSlots {
    /// The span of the slots a window's rows landed at, or [`Nothing`] when it
    /// landed none.
    ///
    /// The one place the min/max is taken, so both the historical and the
    /// follow path get the same answer and the empty case cannot be handled
    /// differently in one of them.
    ///
    /// [`Nothing`]: Self::Nothing
    #[must_use]
    pub fn over(slots: impl IntoIterator<Item = Slot>) -> Self {
        let mut slots = slots.into_iter();
        let Some(first) = slots.next() else {
            return Self::Nothing;
        };
        let (from, to) = slots.fold((first, first), |(lo, hi), s| (lo.min(s), hi.max(s)));
        Self::Span { from, to }
    }

    /// The bounds, when there are any.
    ///
    /// `None` is "this range covers no slots", which is what a caller mapping
    /// coverage to covered slot ranges wants: a window that saw nothing extends
    /// no range, however complete it was.
    #[must_use]
    pub const fn span(self) -> Option<(Slot, Slot)> {
        match self {
            Self::Span { from, to } => Some((from, to)),
            Self::Nothing => None,
        }
    }
}

/// One ingestion range, as the recorder actually ran it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Coverage {
    /// When the range was established — the collection watermark, and the slot
    /// this row is read point-in-time by.
    pub recorded_at: Slot,
    /// Which table the range is about.
    pub table: Table,
    /// The filter the collection ran under, when it was narrower than the whole
    /// table.
    ///
    /// `None` is the whole table over the interval. `Some(mint)` is one mint
    /// and covers **nothing else** in the interval — a cohort capture is not a
    /// statement about the venue.
    pub filter: Option<String>,
    /// The slots this range actually touched, or that it touched none.
    ///
    /// Bounded by the rows the collection returned, never derived from the
    /// window it was queried over. See [`ObservedSlots`].
    pub observed: ObservedSlots,
    /// Where the rows came from, so a range can be attributed to the query that
    /// produced it.
    pub source: String,
    /// The decoder that produced them. A range collected by a decoder that has
    /// since learned an instruction is complete for what that decoder knew.
    pub decoder_version: String,
    /// Whether the query finished.
    pub status: Completion,
}
