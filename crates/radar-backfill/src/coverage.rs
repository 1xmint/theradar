// SPDX-License-Identifier: Apache-2.0
//! What each window actually collected, written down as the window finishes.
//!
//! The store has had a coverage table since 2026-09-07 and nothing in
//! production wrote to it, so every recorded range read as *unvisited* and
//! every trade-derived feature was absent. This module is the producer.
//!
//! # The one hard case
//!
//! A window is queried in **epoch seconds** and the store is keyed by **slot**,
//! and there is no conversion between them — the "~2.5 slots a second" figures
//! in this tree are bucketing heuristics, and using one to name a boundary
//! would invent the number the table exists to make honest.
//!
//! So a window attests only the slots its own rows landed at. A window that
//! finished and returned nothing attests **no slots at all**, and that is the
//! case the whole table is for: it is written as
//! [`ObservedSlots::Nothing`] rather than as slots zero to zero, and it is
//! written rather than skipped, because a skipped record is indistinguishable
//! from a window nobody ran.
//!
//! # Which tables, and what `filter` means here
//!
//! One record per table the scope collects into, because a reader asks about a
//! table: a `Lifecycle` window covers launches and graduations and says nothing
//! about trades, and one record naming one of those tables would have to be
//! wrong about the others.
//!
//! `filter` is `None` on every record these paths write, and it is relative to
//! `source`. The query narrows by program and instruction, and `source` names
//! that: what `None` claims is *nothing narrower was asked for within this
//! source* — not that the record covers every trade on Solana. `filter` carries
//! a cohort, a mint, a subset asked for inside the source.
//!
//! **This is exact only while these tables hold one venue.** The store's trades
//! table holds pump.fun trades and nothing else today, so a complete pump.fun
//! capture is complete for it. The moment a second venue writes into
//! `Table::Trades` — plan 0011's P1, which is a separate task — these records
//! become overclaims, and this field is where the venue has to go.

use radar_store::{Completion, Coverage, Event, ObservedSlots};
use radar_types::Slot;

use crate::extract::Scope;

/// The decoder that produced the rows.
///
/// This crate's version, because this crate is what decoded them: a range
/// collected by a decoder that has since learned an instruction is complete for
/// what that decoder knew, and the record has to say which decoder that was.
const DECODER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The historical path's source.
pub const SOURCE_BACKFILL: &str = "cryptohouse:pumpfun:backfill";

/// The follow path's source.
pub const SOURCE_FOLLOW: &str = "cryptohouse:pumpfun:follow";

/// The collection watermark to stamp a window's records with.
///
/// `recorded_at` is a [`Slot`] and the only slots available are the ones the
/// rows carried, so this is the highest slot the run has seen: this window's,
/// or the store's own before it when this window saw none.
///
/// It **understates** the true moment — the window finished some seconds after
/// its last row's slot, and by more than that when the window was empty. The
/// error is bounded by how long the market stays quiet, and the alternative is
/// converting a wall clock to a slot, which is the fabrication this whole
/// change exists to avoid.
///
/// `Slot(0)` when neither is known, which is a store holding nothing at all: a
/// record at genesis is visible to every replay, and that is tolerable only
/// because a record written in that state bounds no slots and so grants no
/// coverage of any row to anyone.
#[must_use]
pub fn established_at(window_high: Option<Slot>, store_high: Option<Slot>) -> Slot {
    match (window_high, store_high) {
        (Some(a), Some(b)) => a.max(b),
        (Some(s), None) | (None, Some(s)) => s,
        (None, None) => Slot(0),
    }
}

/// The highest slot among a window's events, or `None` when it produced none.
#[must_use]
pub fn highest_slot(events: &[Event]) -> Option<Slot> {
    events.iter().map(Event::slot).max()
}

/// One coverage record per table this scope collects into.
///
/// `status` is the caller's: [`Completion::Complete`] for a window that ran to
/// the end, [`Completion::Partial`] for one that did not. A partial window is
/// recorded rather than dropped — an attempt that failed is a fact about the
/// range, and the reader that treats `Partial` as covering nothing needs the
/// row to be there to treat it that way.
#[must_use]
pub fn records_for_window(
    scope: Scope,
    status: Completion,
    events: &[Event],
    recorded_at: Slot,
    source: &str,
) -> Vec<Coverage> {
    scope
        .tables()
        .iter()
        .map(|&table| Coverage {
            recorded_at,
            table,
            // See the module docs: relative to `source`, and exact only while
            // these tables hold one venue.
            filter: None,
            observed: ObservedSlots::over(
                events
                    .iter()
                    .filter(|e| e.table() == table)
                    .map(Event::slot),
            ),
            source: source.to_owned(),
            decoder_version: DECODER_VERSION.to_owned(),
            status,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use radar_store::{Envelope, Launch, Origin, Table};
    use radar_types::{Address, Signature};

    use super::*;

    fn launch(slot: u64) -> Event {
        Event::Launch(Box::new(Launch {
            envelope: Envelope {
                slot: Slot(slot),
                signature: Signature::new([0u8; 64]),
                tx_index: Some(0),
                instruction_index: 0,
                parent_index: None,
                success: Some(true),
            },
            origin: Origin::known(Address::new([1u8; 32]), "create"),
            mint: Address::new([2u8; 32]),
            creator: Address::new([3u8; 32]),
            name: String::new(),
            symbol: String::new(),
            uri: String::new(),
            dev_buy_lamports: None,
        }))
    }

    #[test]
    fn a_window_that_returned_nothing_is_recorded_and_bounds_no_slots() {
        let records = records_for_window(
            Scope::Lifecycle,
            Completion::Complete,
            &[],
            Slot(500),
            SOURCE_FOLLOW,
        );
        assert_eq!(
            records.len(),
            2,
            "a lifecycle window covers launches and graduations, whatever it found"
        );
        for record in &records {
            assert_eq!(
                record.observed,
                ObservedSlots::Nothing,
                "no row, no slot -- and never slot zero, which is a coverage claim"
            );
            assert_eq!(record.status, Completion::Complete);
        }
    }

    #[test]
    fn a_table_the_scope_did_not_collect_gets_no_record() {
        let records = records_for_window(
            Scope::Lifecycle,
            Completion::Complete,
            &[launch(10)],
            Slot(500),
            SOURCE_BACKFILL,
        );
        assert!(
            records.iter().all(|c| c.table != Table::Trades),
            "a lifecycle window says nothing about trades and must not claim to"
        );
    }

    #[test]
    fn a_records_span_covers_only_the_table_it_names() {
        let records = records_for_window(
            Scope::Lifecycle,
            Completion::Complete,
            &[launch(40), launch(10), launch(25)],
            Slot(500),
            SOURCE_BACKFILL,
        );
        let launches = records
            .iter()
            .find(|c| c.table == Table::Launches)
            .expect("a launches record");
        assert_eq!(
            launches.observed,
            ObservedSlots::Span {
                from: Slot(10),
                to: Slot(40),
            },
            "the span is the lowest and highest slot the rows landed at"
        );
        let graduations = records
            .iter()
            .find(|c| c.table == Table::Graduations)
            .expect("a graduations record");
        assert_eq!(
            graduations.observed,
            ObservedSlots::Nothing,
            "the same window found no graduation, and launches do not bound one"
        );
    }

    #[test]
    fn the_watermark_never_goes_backwards_and_never_invents_one() {
        assert_eq!(
            established_at(Some(Slot(10)), Some(Slot(90))),
            Slot(90),
            "an empty window keeps the store's own watermark"
        );
        assert_eq!(established_at(Some(Slot(90)), Some(Slot(10))), Slot(90));
        assert_eq!(established_at(None, Some(Slot(10))), Slot(10));
        assert_eq!(established_at(Some(Slot(10)), None), Slot(10));
        assert_eq!(
            established_at(None, None),
            Slot(0),
            "a store holding nothing has no slot to stamp with"
        );
    }
}
