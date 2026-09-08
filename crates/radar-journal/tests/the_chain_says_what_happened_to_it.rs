// SPDX-License-Identifier: Apache-2.0
//! What a journal file can be asked, against real files on disk.
//!
//! # Why this file exists
//!
//! The journal's whole value is that reading it back tells you something true
//! about a machine you were not watching. There are three answers it has to keep
//! apart, and an implementation that collapses any two of them is worse than no
//! journal at all — because it looks like evidence:
//!
//! - **nothing was ever written**, which is a machine that never started;
//! - **the last write was interrupted**, which is the ordinary shape of a crash
//!   and leaves everything before it standing;
//! - **something is wrong with the file**, which means something wrote to it
//!   that was not this journal.
//!
//! An operator sent to the wrong one of those three loses the outage.

use radar_journal::{Correlation, Journal, Outcome, Stage, Verified};

fn about(mint: &str) -> Correlation {
    Correlation {
        mint: Some(mint.to_owned()),
        ..Correlation::default()
    }
}

/// Records `n` events into a fresh journal and returns its path.
fn journal_of(dir: &std::path::Path, n: u64) -> std::path::PathBuf {
    let path = dir.join("journal.jsonl");
    let mut journal = Journal::open(&path).expect("open");
    for i in 0..n {
        journal
            .record(
                Stage::Received,
                Outcome::Ok,
                1_000 + i,
                about("So11111111111111111111111111111111111111112"),
                Some("abc1234".to_owned()),
                Vec::new(),
                None,
                None,
            )
            .expect("record");
    }
    path
}

#[test]
fn an_empty_journal_and_a_torn_one_are_not_the_same_answer() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Never written to at all.
    let empty = dir.path().join("never.jsonl");
    assert_eq!(
        Journal::open(&empty)
            .expect("open")
            .verify()
            .expect("verify"),
        Verified::Intact { events: 0 },
        "a machine that never started"
    );

    // Three events, then a write that did not finish.
    let path = journal_of(dir.path(), 3);
    let mut text = std::fs::read_to_string(&path).expect("read");
    text.push_str("{\"schema\":1,\"sequence\":4,\"id\":\"aa");
    std::fs::write(&path, text).expect("write");

    assert_eq!(
        Journal::open(&path)
            .expect("open")
            .verify()
            .expect("verify"),
        Verified::Torn { events: 3 },
        "a crash mid-write, with everything before it standing"
    );
}

#[test]
fn a_torn_journal_is_resumed_from_rather_than_repaired() {
    // The append-only rule applied to this crate's own file. Removing the torn
    // line would be the journal mutating its own record, and the next event
    // continuing from the *torn* line would chain onto something incomplete.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = journal_of(dir.path(), 2);

    let before = std::fs::read_to_string(&path).expect("read");
    let mut text = before.clone();
    text.push_str("{\"schema\":1,\"sequ");
    std::fs::write(&path, &text).expect("write");

    let mut journal = Journal::open(&path).expect("open");
    assert_eq!(
        journal.next_sequence(),
        3,
        "the torn line is not an event, so the next one takes its number"
    );
    journal
        .record(
            Stage::Publication,
            Outcome::Ok,
            2_000,
            about("mint"),
            None,
            Vec::new(),
            None,
            None,
        )
        .expect("record");

    let after = std::fs::read_to_string(&path).expect("read");
    assert!(
        after.starts_with(&before),
        "the events before the tear are byte-identical; nothing was rewritten"
    );
    let events = Journal::open(&path)
        .expect("open")
        .events()
        .expect("events");
    assert_eq!(events.len(), 3);
    assert_eq!(
        events[2].previous, events[1].id,
        "chained onto the last complete event, not onto the torn line"
    );
}

#[test]
fn a_gap_in_the_sequence_is_a_visible_fault() {
    // Something removed a line, or something that was not this journal wrote to
    // the file. Either way it is not a crash and must not read as one.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = journal_of(dir.path(), 4);

    let text = std::fs::read_to_string(&path).expect("read");
    let kept: Vec<&str> = text
        .lines()
        .enumerate()
        .filter(|(i, _)| *i != 2)
        .map(|(_, l)| l)
        .collect();
    std::fs::write(&path, kept.join("\n") + "\n").expect("write");

    let verdict = Journal::open(&path)
        .expect("open")
        .verify()
        .expect("verify");
    assert!(
        matches!(verdict, Verified::Broken { at: 3, .. }),
        "a missing line is a fault at the sequence it should have held: {verdict:?}"
    );
}

#[test]
fn an_altered_event_is_a_visible_fault_at_the_event_after_it() {
    // The chain's actual job. Editing an event changes its contents, so its own
    // hash stops matching -- and even an attacker who recomputes that hash is
    // then caught by the *next* event, whose `previous` still names the old one.
    //
    // Re-apply by dropping either check in `verify`: the altered line reads as
    // sound and the journal reports `Intact`.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = journal_of(dir.path(), 3);

    let text = std::fs::read_to_string(&path).expect("read");
    let mut lines: Vec<String> = text.lines().map(ToOwned::to_owned).collect();
    lines[1] = lines[1].replace("\"at\":1001", "\"at\":9999");
    std::fs::write(&path, lines.join("\n") + "\n").expect("write");

    let verdict = Journal::open(&path)
        .expect("open")
        .verify()
        .expect("verify");
    assert!(
        matches!(verdict, Verified::Broken { at: 2, .. }),
        "the altered event is where the fault is reported: {verdict:?}"
    );
}

#[test]
fn a_relinked_chain_is_still_caught_by_the_event_after_it() {
    // The harder half. An attacker who edits an event *and* recomputes its own
    // id defeats the self-hash check -- and the next event's `previous` still
    // names the id the old event had, so the chain breaks there. This is what
    // "hash-chained" actually buys, and it is worth pinning because the
    // self-hash check alone would pass this file.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = journal_of(dir.path(), 3);

    let text = std::fs::read_to_string(&path).expect("read");
    let mut lines: Vec<String> = text.lines().map(ToOwned::to_owned).collect();
    let mut altered: radar_journal::Event =
        serde_json::from_str(&lines[1]).expect("the second event");
    altered.at = 9_999;
    altered.id = altered.digest();
    lines[1] = serde_json::to_string(&altered).expect("serialise");
    std::fs::write(&path, lines.join("\n") + "\n").expect("write");

    let verdict = Journal::open(&path)
        .expect("open")
        .verify()
        .expect("verify");
    assert!(
        matches!(verdict, Verified::Broken { at: 3, .. }),
        "the third event still points at the id the second one used to have: {verdict:?}"
    );
}

#[test]
fn an_intact_journal_chains_every_event_to_the_one_before_it() {
    // The other direction, so the three faults above cannot be satisfied by a
    // `verify` that returns `Broken` for everything.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = journal_of(dir.path(), 5);

    assert_eq!(
        Journal::open(&path)
            .expect("open")
            .verify()
            .expect("verify"),
        Verified::Intact { events: 5 }
    );

    let events = Journal::open(&path)
        .expect("open")
        .events()
        .expect("events");
    assert_eq!(
        events[0].previous, "",
        "the first event chains onto nothing"
    );
    for pair in events.windows(2) {
        assert_eq!(pair[1].previous, pair[0].id);
        assert_eq!(pair[1].sequence, pair[0].sequence + 1);
    }
}

#[test]
fn a_reopened_journal_continues_the_chain_rather_than_starting_one() {
    // A restart is the ordinary case, not the exception: the daemon this serves
    // is restarted by deploys. A journal that began a new chain on every start
    // would make every restart look like a fault.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = journal_of(dir.path(), 2);

    let mut reopened = Journal::open(&path).expect("open");
    assert_eq!(reopened.next_sequence(), 3);
    reopened
        .record(
            Stage::Payout,
            Outcome::Uncertain,
            3_000,
            Correlation {
                week: Some("2026-W37".to_owned()),
                ..Correlation::default()
            },
            None,
            vec![("payout".to_owned(), "0.1.0".to_owned())],
            Some("a transaction was broadcast and its confirmation never arrived".to_owned()),
            None,
        )
        .expect("record");

    assert_eq!(
        Journal::open(&path)
            .expect("open")
            .verify()
            .expect("verify"),
        Verified::Intact { events: 3 }
    );
}

#[test]
fn an_event_that_names_nothing_is_refused_rather_than_written() {
    // Findable only by sequence, which is a caller that forgot rather than a
    // property of the event. Refusing costs the caller a line of code; admitting
    // it costs whoever is trying to explain an outage at three in the morning.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("journal.jsonl");
    let mut journal = Journal::open(&path).expect("open");

    let refused = journal.record(
        Stage::Received,
        Outcome::Ok,
        1,
        Correlation::default(),
        None,
        Vec::new(),
        None,
        None,
    );
    assert!(refused.is_err());
    assert!(
        !path.exists() || std::fs::read_to_string(&path).expect("read").is_empty(),
        "a refused event is not half-written"
    );
}

#[test]
fn an_oversized_diagnostic_is_refused_rather_than_truncated() {
    // The thing most likely to arrive here is a provider's error body, and the
    // thing most likely to be in one is the request that caused it, headers
    // included. Truncating would keep the first five hundred bytes, which is
    // where a credential would be. The rule is that callers pass a reason.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("journal.jsonl");
    let mut journal = Journal::open(&path).expect("open");

    let refused = journal.record(
        Stage::Publication,
        Outcome::Failed,
        1,
        about("mint"),
        None,
        Vec::new(),
        None,
        Some("x".repeat(600)),
    );
    assert!(matches!(
        refused,
        Err(radar_journal::JournalError::RedactedTooLong { length: 600 })
    ));
}

#[test]
fn the_same_history_gives_an_event_the_same_id_twice() {
    // What makes a replay able to compare by id rather than by position. Two
    // journals built the same way are byte-identical, so a difference between
    // two runs is a difference in what happened rather than in when it was
    // written down.
    let a = tempfile::tempdir().expect("tempdir");
    let b = tempfile::tempdir().expect("tempdir");

    let one = std::fs::read_to_string(journal_of(a.path(), 4)).expect("read");
    let two = std::fs::read_to_string(journal_of(b.path(), 4)).expect("read");
    assert_eq!(one, two);
}
