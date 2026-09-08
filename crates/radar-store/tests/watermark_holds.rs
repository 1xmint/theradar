// SPDX-License-Identifier: Apache-2.0
//! The watermark guarantee, tested adversarially rather than asserted.
//!
//! `AGENTS.md` rule 3 says nothing reads past its watermark. That is the
//! property every research result rests on, and until these tests existed it was
//! held up by reading the code and agreeing with it.
//!
//! The interesting case is **a file that straddles the watermark**. `Reader` skips
//! whole files whose slot range starts after `as_of`, which is a real and worthwhile
//! optimisation — and it means a file starting *before* the watermark is opened and
//! read in full, so every row in it past the watermark has to be caught one at a
//! time. A test that only puts future events in future files would pass against a
//! reader with no per-row filter at all.

use radar_asof::AsOf;
use radar_store::{
    Envelope, Event, Launch, Origin, Outcome, Position, Reader, SLOTS_PER_PARTITION, Table, Writer,
};
use radar_types::{Address, Signature, Slot};

fn launch_at(slot: u64) -> Event {
    Event::Launch(Box::new(Launch {
        envelope: Envelope {
            slot: Slot(slot),
            signature: Signature::new([u8::try_from(slot % 251).unwrap_or(1); 64]),
            tx_index: Some(1),
            instruction_index: 0,
            parent_index: None,
            success: Some(true),
        },
        origin: Origin::known(Address::new([9; 32]), "create_v2"),
        mint: Address::new([u8::try_from(slot % 251).unwrap_or(1); 32]),
        creator: Address::new([7; 32]),
        name: format!("T{slot}"),
        symbol: "TKN".to_owned(),
        uri: String::new(),
        dev_buy_lamports: None,
    }))
}

fn outcome_at(measured_at: u64) -> Outcome {
    Outcome {
        mint: Address::new([u8::try_from(measured_at % 251).unwrap_or(1); 32]),
        measured_at: Slot(measured_at),
        launch_slot: Slot(1_000),
        first_transfer_slot: Some(Slot(1_001)),
        last_transfer_slot: Some(Slot(1_500)),
        transfers: 20,
        unique_senders: 5,
        unique_receivers: 5,
        graduated_at: None,
        first_price: None,
        last_price: None,
        peak_price: None,
        trough_price: None,
        window_peak_price: None,
        window_trough_price: None,
        vwap: None,
        fills: 0,
    }
}

/// Slots that all fall inside one partition, so they land in one file.
///
/// That is the arrangement the per-row filter has to survive: the file starts
/// before the watermark, so it is opened, and half its contents are from after it.
fn slots_in_one_partition() -> [u64; 4] {
    let base = SLOTS_PER_PARTITION;
    [base, base + 2_000, base + 7_000, base + 12_000]
}

#[test]
fn a_file_that_straddles_the_watermark_yields_only_the_admissible_half() {
    let dir = tempfile::tempdir().expect("tempdir");
    let slots = slots_in_one_partition();

    let mut w = Writer::open(dir.path(), 10_000).expect("open");
    for slot in slots {
        w.append(launch_at(slot)).expect("append");
    }
    w.flush().expect("flush");

    // One file, so the whole-file skip cannot be what does the filtering.
    let files: Vec<_> = std::fs::read_dir(dir.path().join(Table::Launches.dir()))
        .expect("dir")
        .filter_map(Result::ok)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "the fixture must be a single straddling file"
    );

    // Between the second and third slot.
    let watermark = Slot(slots[1] + 1);
    let read = Reader::open(dir.path())
        .read(Table::Launches, AsOf::at(watermark))
        .expect("read");

    assert_eq!(read.len(), 2, "expected only the two admissible events");
    for event in &read {
        assert!(
            event.envelope().slot <= watermark,
            "leaked slot {} past watermark {watermark}",
            event.envelope().slot
        );
    }
}

#[test]
fn no_read_at_any_watermark_ever_returns_a_later_event() {
    // Swept rather than spot-checked: an off-by-one at a partition or file
    // boundary would pass a single well-chosen watermark.
    let dir = tempfile::tempdir().expect("tempdir");
    let slots = slots_in_one_partition();
    let mut w = Writer::open(dir.path(), 10_000).expect("open");
    for slot in slots {
        w.append(launch_at(slot)).expect("append");
    }
    w.flush().expect("flush");
    let reader = Reader::open(dir.path());

    for probe in slots {
        for watermark in [probe - 1, probe, probe + 1] {
            let as_of = AsOf::at(Slot(watermark));
            let read = reader.read(Table::Launches, as_of).expect("read");
            for event in &read {
                assert!(
                    event.envelope().slot.get() <= watermark,
                    "at watermark {watermark}, leaked slot {}",
                    event.envelope().slot
                );
            }
            let expected = slots.iter().filter(|s| **s <= watermark).count();
            assert_eq!(read.len(), expected, "at watermark {watermark}");
        }
    }
}

#[test]
fn outcomes_are_gated_on_when_they_were_measured() {
    // An outcome describes a token's past but is *known* only from the slot it
    // was measured at. Gating it on the launch slot instead would hand a decision
    // a measurement taken after it — the exact leak this crate exists to stop.
    //
    // Measured inside one partition on purpose. Spread across partitions, the
    // whole-file skip does all the work and the per-row filter is never reached —
    // which is exactly what an earlier version of this test did, and it passed
    // with the per-row filter deleted.
    let dir = tempfile::tempdir().expect("tempdir");
    let measured = slots_in_one_partition();
    let watermark = Slot(measured[1] + 1);

    let mut w = Writer::open(dir.path(), 10_000).expect("open");
    for measured_at in measured {
        w.append_outcome(outcome_at(measured_at)).expect("append");
    }
    w.flush().expect("flush");

    let read = Reader::open(dir.path())
        .read_outcomes(AsOf::at(watermark))
        .expect("read");

    assert_eq!(
        read.len(),
        2,
        "expected only the two admissible measurements"
    );
    for outcome in &read {
        assert!(
            outcome.measured_at <= watermark,
            "leaked a measurement from slot {}",
            outcome.measured_at
        );
        // Every one of these has launch_slot 1_000, well before the watermark, so
        // a reader gating on the launch slot would have returned all three.
        assert!(outcome.launch_slot < outcome.measured_at);
    }
}

#[test]
fn a_watermark_before_everything_returns_nothing_rather_than_everything() {
    // The failure direction that matters: an inverted comparison returns the
    // whole store, and every count downstream still looks plausible.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut w = Writer::open(dir.path(), 10_000).expect("open");
    for slot in slots_in_one_partition() {
        w.append(launch_at(slot)).expect("append");
    }
    w.append_outcome(outcome_at(500_000)).expect("append");
    w.flush().expect("flush");

    let reader = Reader::open(dir.path());
    let ancient = AsOf::at(Slot(1));
    assert!(
        reader
            .read(Table::Launches, ancient)
            .expect("read")
            .is_empty()
    );
    assert!(reader.read_outcomes(ancient).expect("read").is_empty());
}

#[test]
fn a_close_from_after_the_watermark_does_not_shut_a_position_that_was_open() {
    // The one leak that ran in the *permissive* direction, and the reason it is
    // worth its own test rather than a row in the one above.
    //
    // A position row is written once and updated in place when the position
    // closes, so a row read at an earlier watermark carries a `closed_at` from
    // the future. `read_positions` filtered on `opened_at` alone, so that close
    // was admitted and the position read as shut at a moment when it was open.
    //
    // Every other watermark violation shows a replay something it should not
    // have seen, which produces an answer that is too good and is caught by
    // being too good. This one shows a replay *less* than the live run had:
    // a closed position counts against no exposure limit and an open one counts
    // against every limit, so the kernel's deployment limits were judged against
    // a portfolio smaller than the real one. A backtest that permits what
    // production refused, and nothing about the number looks wrong.
    let dir = tempfile::tempdir().expect("tempdir");
    let opened = Slot(SLOTS_PER_PARTITION);
    let watermark = Slot(opened.get() + 1_000);
    let closed = Slot(opened.get() + 5_000);

    let mut w = Writer::open(dir.path(), 10_000).expect("open");
    w.append_position(Position {
        mint: Address::new([3; 32]),
        creator: Address::new([7; 32]),
        opened_at: opened,
        notional_micro_usd: 50_000_000,
        entry_price: Some(1_000),
        // Closed after the watermark. As of the watermark this position is open.
        closed_at: Some(closed),
        exit_price: Some(2_000),
        realised_micro_usd: Some(25_000_000),
    })
    .expect("append");
    w.flush().expect("flush");

    let read = Reader::open(dir.path())
        .read_positions(AsOf::at(watermark))
        .expect("read");

    assert_eq!(
        read.len(),
        1,
        "the position was opened before the watermark"
    );
    let p = &read[0];
    assert_eq!(
        p.closed_at, None,
        "a close at {closed} is not visible at {watermark}"
    );
    assert_eq!(
        p.exit_price, None,
        "the exit price belongs to the close and is equally from the future"
    );
    assert_eq!(
        p.realised_micro_usd, None,
        "and so does the realised figure -- reading it as recorded would let a \
         realised-loss limit be judged against a trip that had not happened"
    );
    // The open half is untouched: this must not become a filter that drops the
    // row, which would understate exposure in the same direction.
    assert_eq!(p.opened_at, opened);
    assert_eq!(p.notional_micro_usd, 50_000_000);
    assert_eq!(p.entry_price, Some(1_000));
}

#[test]
fn a_close_at_or_before_the_watermark_is_still_a_close() {
    // The other half. A filter that dropped every close would pass the test
    // above and make every position in every replay look permanently open.
    let dir = tempfile::tempdir().expect("tempdir");
    let opened = Slot(SLOTS_PER_PARTITION);
    let closed = Slot(opened.get() + 1_000);

    let mut w = Writer::open(dir.path(), 10_000).expect("open");
    w.append_position(Position {
        mint: Address::new([3; 32]),
        creator: Address::new([7; 32]),
        opened_at: opened,
        notional_micro_usd: 50_000_000,
        entry_price: Some(1_000),
        closed_at: Some(closed),
        exit_price: Some(2_000),
        realised_micro_usd: Some(25_000_000),
    })
    .expect("append");
    w.flush().expect("flush");

    for watermark in [closed, Slot(closed.get() + 1)] {
        let read = Reader::open(dir.path())
            .read_positions(AsOf::at(watermark))
            .expect("read");
        let p = &read[0];
        assert_eq!(p.closed_at, Some(closed), "at watermark {watermark}");
        assert_eq!(p.exit_price, Some(2_000));
        assert_eq!(p.realised_micro_usd, Some(25_000_000));
    }
}
