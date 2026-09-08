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
//! # Nothing in production writes one yet, and that is deliberate
//!
//! The reader is real: [`crate::Reader::read_coverage`] serves it and
//! `radar-research`'s feature pass is its caller. The **producer** is not, and
//! it is a harder question than it looks, so it is a change of its own rather
//! than a guess bolted onto this one.
//!
//! The backfill queries CryptoHouse by `block_timestamp` and the store is keyed
//! by `block_slot`. A completed *time* window therefore attests a slot range
//! only through the events it returned — and a completed window that returned
//! nothing, which is the exact case this table exists to express, returns no
//! slots to bound it with. The workable answer is per-run rather than
//! per-window: a contiguous chain of completed windows attests the slot span
//! its events cover, under-claiming at the edges, which is the safe direction.
//! Writing that down is the next item, with its own regression.
//!
//! Until then every trade-derived feature is **absent**. On the production
//! store that changes nothing today: the trades directory was created
//! 2026-08-23 and has never been written to, so those features were already
//! absent, by the older and weaker rule this table replaces.
//!
//! [LEARNINGS]: https://github.com/hey-vera/radar/blob/main/LEARNINGS.md

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
    /// First slot of the range, inclusive.
    pub from_slot: Slot,
    /// Last slot of the range, inclusive.
    pub to_slot: Slot,
    /// Where the rows came from, so a range can be attributed to the query that
    /// produced it.
    pub source: String,
    /// The decoder that produced them. A range collected by a decoder that has
    /// since learned an instruction is complete for what that decoder knew.
    pub decoder_version: String,
    /// Whether the query finished.
    pub status: Completion,
}
