// SPDX-License-Identifier: Apache-2.0
//! Three states, and a reader that can tell them apart.
//!
//! *Nobody collected this*, *somebody collected this and it was empty*, and
//! *somebody collected this and here is the range* are three different
//! sentences, and until 2026-09-09 the store could write only two of them. A
//! completed window that returned no rows has no slot range to record, so the
//! only shapes available were a fabricated `0..0` — a coverage claim starting
//! at genesis — or no row at all, which is the first sentence, not the second.
//!
//! That collapse is the whole failure this table exists to prevent: LEARNINGS
//! 10, a zero that is a measurement about the instrument. So the range is one
//! field with two states and the empty case is recorded rather than skipped.
//!
//! Re-apply the bug by having [`what_is_known`] treat `ObservedSlots::Nothing`
//! as `Unvisited`, or by making the writer drop a record with no span. Both
//! make the middle case indistinguishable from the first, and both fail here.

use radar_asof::AsOf;
use radar_store::{Completion, Coverage, ObservedSlots, Reader, Table, Writer};
use radar_types::Slot;

/// What a reader can say about a table's collection, from coverage alone.
#[derive(PartialEq, Eq, Debug)]
enum Known {
    /// No record names this table. Nobody looked, and nothing may be concluded
    /// about what is or is not there.
    Unvisited,
    /// A collection finished over this table and returned no rows. A
    /// measurement: the market was quiet, or the decoder understood nothing in
    /// it, and either way it is a fact rather than a silence.
    RanAndFoundNothing,
    /// A collection finished and the rows it wrote span these slots.
    Collected { from: Slot, to: Slot },
    /// A collection was attempted and did not finish. It covers nothing, and it
    /// is not the same as nobody having tried.
    Attempted,
}

/// The strongest thing coverage supports saying about `table` at `as_of`.
fn what_is_known(reader: &Reader, table: Table, as_of: AsOf) -> Known {
    let records: Vec<Coverage> = reader
        .read_coverage(as_of)
        .expect("read")
        .into_iter()
        .filter(|c| c.table == table && c.filter.is_none())
        .collect();
    if records.is_empty() {
        return Known::Unvisited;
    }
    let complete: Vec<&Coverage> = records
        .iter()
        .filter(|c| c.status == Completion::Complete)
        .collect();
    if complete.is_empty() {
        return Known::Attempted;
    }
    complete.iter().find_map(|c| c.observed.span()).map_or(
        Known::RanAndFoundNothing,
        |(from, to)| Known::Collected { from, to },
    )
}

fn record(table: Table, observed: ObservedSlots, status: Completion) -> Coverage {
    Coverage {
        recorded_at: Slot(500),
        table,
        filter: None,
        observed,
        source: "test".to_owned(),
        decoder_version: "test".to_owned(),
        status,
    }
}

#[test]
fn a_reader_tells_unvisited_from_empty_from_collected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut w = Writer::open(dir.path(), 1_000).expect("open");
    // Launches: a window ran and the market was quiet.
    w.append_coverage(record(
        Table::Launches,
        ObservedSlots::Nothing,
        Completion::Complete,
    ))
    .expect("append");
    // Trades: a window ran and its rows landed between these slots.
    w.append_coverage(record(
        Table::Trades,
        ObservedSlots::Span {
            from: Slot(10),
            to: Slot(400),
        },
        Completion::Complete,
    ))
    .expect("append");
    // Graduations: nothing at all. Deliberately no record.
    w.flush().expect("flush");

    let reader = Reader::open(dir.path());
    let as_of = AsOf::at(Slot(9_999));

    assert_eq!(
        what_is_known(&reader, Table::Graduations, as_of),
        Known::Unvisited,
        "no record is no knowledge, and must never read as an empty market"
    );
    assert_eq!(
        what_is_known(&reader, Table::Launches, as_of),
        Known::RanAndFoundNothing,
        "a completed window that returned nothing is a measurement, and it \
         survives the disk"
    );
    assert_eq!(
        what_is_known(&reader, Table::Trades, as_of),
        Known::Collected {
            from: Slot(10),
            to: Slot(400),
        },
    );
}

#[test]
fn an_unfinished_window_is_not_a_collected_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut w = Writer::open(dir.path(), 1_000).expect("open");
    // The shape a run that died leaves: it got as far as some rows, and the
    // range was never established.
    w.append_coverage(record(
        Table::Trades,
        ObservedSlots::Span {
            from: Slot(10),
            to: Slot(400),
        },
        Completion::Partial,
    ))
    .expect("append");
    w.flush().expect("flush");

    assert_eq!(
        what_is_known(
            &Reader::open(dir.path()),
            Table::Trades,
            AsOf::at(Slot(9_999))
        ),
        Known::Attempted,
        "a partial record carries a span and still covers nothing: the rows it \
         names exist and the rest of the window was never queried"
    );
}

#[test]
fn an_empty_record_is_still_gated_on_when_it_was_established() {
    // The three states are point-in-time like everything else. A window that
    // ran today and found nothing must not tell a replay of last week that
    // somebody had already looked.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut w = Writer::open(dir.path(), 1_000).expect("open");
    w.append_coverage(record(
        Table::Launches,
        ObservedSlots::Nothing,
        Completion::Complete,
    ))
    .expect("append");
    w.flush().expect("flush");

    let reader = Reader::open(dir.path());
    assert_eq!(
        what_is_known(&reader, Table::Launches, AsOf::at(Slot(499))),
        Known::Unvisited,
        "before the record was established, nobody had looked"
    );
    assert_eq!(
        what_is_known(&reader, Table::Launches, AsOf::at(Slot(500))),
        Known::RanAndFoundNothing,
        "and at it, somebody had"
    );
}
