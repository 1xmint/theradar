// SPDX-License-Identifier: Apache-2.0
//! What survives a restart, and what a lost response is allowed to do.
//!
//! # Why this file exists
//!
//! A reservation is capital claimed by an operation that has not finished. It
//! lives in a `BTreeMap` inside one process, and the four ways that becomes
//! money are all in here:
//!
//! - the process dies and the claim does not come back, so the next pass sizes
//!   a second trade against capital the first one is still holding;
//! - one recorded change is applied twice and one claim becomes two;
//! - a submission whose answer never arrived is written off as a failure, which
//!   frees the claim while the transaction is still landing;
//! - the effect is released and the record of intending it is written
//!   afterwards, so a crash in between leaves an effect nobody can account for.
//!
//! Each test below puts one of those back and says what fails when it does.

use radar_journal::{
    Applied, Correlation, Intent, OperationError, OperationId, OperationLog, OperationState,
};
use radar_types::{
    Address, Asset, AssetRole, Balance, Holding, Portfolio, Settlement, Slot, TokenQuantity,
    Unvaluable, Valuation,
};

const WALLET: Address = Address::new([7u8; 32]);
const HELD: u64 = 10_000_000;
const CLAIM: u64 = 4_000_000;
const NOW: Slot = Slot(500);

/// A wallet with ten million lamports in it, as a fresh chain read would give
/// it: the balance says nothing about what any process has claimed.
fn account() -> Portfolio {
    let mut portfolio = Portfolio::at(WALLET, NOW);
    portfolio
        .hold(
            Asset::Sol,
            Holding::new(
                AssetRole::Cash,
                Balance::Counted(TokenQuantity::lamports(HELD)),
                Valuation::Unknown(Unvaluable::NoPrice),
                Valuation::Unknown(Unvaluable::NoPrice),
            ),
        )
        .expect("hold");
    portfolio
}

fn intent() -> Intent {
    Intent {
        asset: Asset::Sol,
        amount: TokenQuantity::lamports(CLAIM),
        at: NOW,
    }
}

fn about() -> Correlation {
    Correlation {
        mint: Some("So11111111111111111111111111111111111111112".to_owned()),
        ..Correlation::default()
    }
}

fn free(portfolio: &Portfolio) -> u64 {
    portfolio.free(Asset::Sol).expect("free").raw()
}

/// A journal path inside a directory of its own, so a test can take the
/// directory away and make the next write fail.
fn somewhere(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let live = dir.path().join("live");
    std::fs::create_dir(&live).expect("mkdir");
    live.join("operations.jsonl")
}

/// Opens the log, proposes and reserves, and hands back the id.
fn reserved(path: &std::path::Path, portfolio: &mut Portfolio) -> (OperationLog, OperationId) {
    let mut log = OperationLog::open(path).expect("open");
    let id = log.propose(intent(), 1_000, about()).expect("propose");
    assert_eq!(
        log.reserve(&id, portfolio, 1_001).expect("reserve"),
        Applied::Advanced
    );
    (log, id)
}

#[test]
fn a_claim_outstanding_when_the_process_died_is_outstanding_when_it_comes_back() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = somewhere(&dir);

    let mut portfolio = account();
    let (log, id) = reserved(&path, &mut portfolio);
    assert_eq!(free(&portfolio), HELD - CLAIM);

    // The process dies. Every reservation was a map entry inside it.
    drop(log);
    drop(portfolio);

    // It comes back. The balances are read fresh, and a balance says nothing
    // about what is claimed against it -- the wallet still holds every lamport,
    // because the transaction has not landed.
    let mut portfolio = account();
    assert_eq!(
        free(&portfolio),
        HELD,
        "the chain read shows the full balance"
    );

    let mut log = OperationLog::open(&path).expect("reopen");
    assert_eq!(log.outstanding().count(), 1);
    log.rehold(&mut portfolio).expect("rehold");

    // Re-apply the bug: have `rehold` skip the operation, or have
    // `OperationState::is_outstanding` answer `false` for `Reserved`. Either
    // way this reads HELD, the account offers the same lamports to the next
    // proposal, and the same capital is committed twice.
    assert_eq!(
        free(&portfolio),
        HELD - CLAIM,
        "the claim came back with the process"
    );
    assert_eq!(portfolio.reservations().count(), 1);
    assert_eq!(
        log.entry(&id).map(|e| e.state),
        Some(OperationState::Reserved)
    );
}

#[test]
fn one_change_recorded_twice_is_one_claim_and_not_two() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = somewhere(&dir);

    let mut portfolio = account();
    let (log, _) = reserved(&path, &mut portfolio);
    drop(log);

    // A write that landed, timed out on the way back, and was retried. The
    // same change is now in the file twice.
    let text = std::fs::read_to_string(&path).expect("read");
    let again = text.lines().last().expect("a line").to_owned();
    std::fs::write(&path, format!("{text}{again}\n")).expect("append");

    // Re-apply the bug: drop the `live.entry.state == entry.state` guard in
    // `replay` and the second copy is a move from `Reserved` to `Reserved`,
    // which is not in the table -- the log refuses to open at all and the
    // process cannot start. Key the map on position in the file rather than on
    // the operation's identity and it opens with two claims for one operation,
    // which is the same lamports claimed twice.
    let mut log = OperationLog::open(&path).expect("reopen");
    assert_eq!(log.outstanding().count(), 1);

    let mut portfolio = account();
    log.rehold(&mut portfolio).expect("rehold");
    assert_eq!(free(&portfolio), HELD - CLAIM);
    assert_eq!(portfolio.reservations().count(), 1);
}

#[test]
fn identity_comes_from_the_record_and_not_from_the_intent_or_the_clock() {
    // Two operations that intend exactly the same thing at exactly the same
    // moment are two operations. An identity derived from the intent, or from
    // the timestamp, would fold them into one -- and the second `reserve`
    // would then settle the first one's claim.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = somewhere(&dir);
    let mut log = OperationLog::open(&path).expect("open");

    let first = log.propose(intent(), 1_000, about()).expect("propose");
    let second = log.propose(intent(), 1_000, about()).expect("propose");
    assert_ne!(first, second);

    let mut portfolio = account();
    log.reserve(&first, &mut portfolio, 1_001).expect("first");
    log.reserve(&second, &mut portfolio, 1_001).expect("second");
    assert_eq!(free(&portfolio), HELD - CLAIM - CLAIM);
    assert_eq!(portfolio.reservations().count(), 2);
}

#[test]
fn a_submission_with_no_answer_is_not_a_failure_and_does_not_free_its_claim() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = somewhere(&dir);

    let mut portfolio = account();
    let (mut log, id) = reserved(&path, &mut portfolio);

    // The effect goes out and the response is lost. What the closure returns is
    // the transport's answer, not the operation's: it says the reply did not
    // arrive, and it says nothing about whether the transaction did.
    let answer: Result<(), &str> = log
        .submit(&id, 1_002, |_| Err("the connection dropped"))
        .expect("the intent was recorded");
    assert!(answer.is_err());
    assert_eq!(
        log.entry(&id).map(|e| e.state),
        Some(OperationState::SubmissionUnknown)
    );
    assert_eq!(free(&portfolio), HELD - CLAIM, "the claim still stands");

    // Re-apply the bug: add `Self::Failed` to the `SubmissionUnknown` arm of
    // `OperationState::advance`. This call then succeeds, the claim is
    // abandoned, `free` returns HELD, and the next proposal is sized against
    // lamports the released transaction may already have spent.
    let refused = log.fail(&id, &mut portfolio, 1_003);
    assert!(
        matches!(refused, Err(OperationError::UnknownIsNotFailed)),
        "a lost response is not an established failure, got {refused:?}"
    );
    assert_eq!(free(&portfolio), HELD - CLAIM, "and nothing was freed");

    // A restart does not resolve it either. Unknown stays unknown across a
    // process boundary, and it stays claimed.
    drop(log);
    drop(portfolio);
    let mut portfolio = account();
    let mut log = OperationLog::open(&path).expect("reopen");
    log.rehold(&mut portfolio).expect("rehold");
    assert_eq!(free(&portfolio), HELD - CLAIM);

    // Only an observation of what actually happened moves it, and the
    // observation is what frees the capital.
    assert_eq!(
        log.reconcile(&id, Settlement::Abandoned, &mut portfolio, 1_004)
            .expect("reconcile"),
        Applied::Advanced
    );
    assert_eq!(free(&portfolio), HELD, "nothing landed, so nothing is held");
    assert_eq!(log.outstanding().count(), 0);
}

#[test]
fn an_effect_whose_intent_could_not_be_recorded_never_happens() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = somewhere(&dir);

    let mut portfolio = account();
    let (mut log, id) = reserved(&path, &mut portfolio);

    // The disk goes away between the reservation and the submission.
    std::fs::remove_dir_all(path.parent().expect("parent")).expect("remove");

    let released = std::cell::Cell::new(false);
    let refused = log.submit(&id, 1_002, |_| {
        released.set(true);
        Ok::<(), ()>(())
    });

    // Re-apply the bug: call `effect` before `self.write` in
    // `OperationLog::submit`. The transaction goes out, the machine has no
    // record that it did, and the next run reads an account with an unexplained
    // hole in it.
    assert!(
        matches!(refused, Err(OperationError::Journal(_))),
        "got {refused:?}"
    );
    assert!(
        !released.get(),
        "the effect ran even though its intent was never recorded"
    );
    assert_eq!(
        log.entry(&id).map(|e| e.state),
        Some(OperationState::Reserved),
        "and the operation did not move"
    );
}

#[test]
fn a_confirmation_delivered_twice_settles_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = somewhere(&dir);

    let mut portfolio = account();
    let (mut log, id) = reserved(&path, &mut portfolio);
    log.submit(&id, 1_002, |_| Ok::<(), ()>(()))
        .expect("record")
        .expect("effect");

    assert_eq!(
        log.confirm(&id, Settlement::Filled, &mut portfolio, 1_003)
            .expect("confirm"),
        Applied::Advanced
    );
    let after = free(&portfolio);
    assert_eq!(after, HELD - CLAIM, "the fill came out of the balance");

    // Re-apply the bug: drop the `live.entry.state == next` guard in `close`.
    // The second delivery either debits the balance again or blows up on a
    // reservation that is already gone; either way the account no longer
    // matches what happened.
    assert_eq!(
        log.confirm(&id, Settlement::Filled, &mut portfolio, 1_004)
            .expect("confirm again"),
        Applied::AlreadySeen
    );
    assert_eq!(
        free(&portfolio),
        after,
        "and the balance did not move again"
    );
}

#[test]
fn a_journal_the_operations_were_never_written_to_opens_empty() {
    // The state every deployment is in today, and it has to read as "nothing is
    // outstanding" rather than as a failure to start.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut log = OperationLog::open(dir.path().join("nothing.jsonl")).expect("open");
    assert_eq!(log.outstanding().count(), 0);

    let mut portfolio = Portfolio::unattributed(NOW);
    log.rehold(&mut portfolio).expect("rehold holds nothing");
}

#[test]
fn the_operation_lines_are_a_chain_like_every_other_line() {
    // An operation event is an ordinary journal event and has to obey the
    // ordinary guarantee: a sequence from one with no gaps, each line hashed to
    // the one before it. `record_operation` fills the sequence itself, and CI's
    // mutation gate found this exact shape once already -- `next_id += 1`
    // mutated to `*= 1` in `Portfolio::reserve`.
    //
    // Re-apply the bug by replacing `self.sequence + 1` with `self.sequence * 1`
    // in `Journal::record_operation`: every operation line carries sequence
    // zero, and this reads `Broken` at sequence one.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = somewhere(&dir);

    let mut portfolio = account();
    let (mut log, id) = reserved(&path, &mut portfolio);
    log.submit(&id, 1_002, |_| Ok::<(), ()>(()))
        .expect("record")
        .expect("effect");

    let journal = radar_journal::Journal::open(&path).expect("open");
    assert_eq!(
        journal.verify().expect("verify"),
        radar_journal::Verified::Intact { events: 3 }
    );

    // And every line after the proposal names the operation it advances, which
    // is the only way `audit explain` can gather one operation's history. The
    // proposal names nothing, because it *is* the identity.
    let events = journal.events().expect("events");
    assert_eq!(events[0].correlation.operation, None);
    for event in &events[1..] {
        assert_eq!(
            event.correlation.operation.as_deref(),
            Some(id.as_str()),
            "a line about the operation that does not name it"
        );
    }
}

#[test]
fn a_settlement_that_would_strand_the_remainder_is_refused() {
    // A partial fill leaves the rest of the claim reserved. Closing the
    // operation on one puts it in a state `rehold` does not re-take, so the
    // remainder is lost at the next restart -- the same lamports handed back to
    // the next proposal while the first operation's remainder is still out
    // there.
    //
    // Re-apply the bug two ways, both of which CI found: make the reservation
    // lookup `r.id() != claim` and nothing is found, so the guard never fires
    // and the short fill is accepted; flip `outstanding != filled` to `==` and
    // the guard fires on the fill that *does* close the claim and refuses it.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = somewhere(&dir);

    let mut portfolio = account();
    let (mut log, id) = reserved(&path, &mut portfolio);
    log.submit(&id, 1_002, |_| Ok::<(), ()>(()))
        .expect("record")
        .expect("effect");

    let short = Settlement::PartiallyFilled(TokenQuantity::lamports(CLAIM / 2));
    let refused = log.confirm(&id, short, &mut portfolio, 1_003);
    assert!(
        matches!(refused, Err(OperationError::RemainderWouldBeLost { .. })),
        "got {refused:?}"
    );
    assert_eq!(free(&portfolio), HELD - CLAIM, "the claim is untouched");
    assert_eq!(
        log.entry(&id).map(|e| e.state),
        Some(OperationState::SubmissionUnknown),
        "and no line says the operation finished"
    );

    // A fill for exactly what was outstanding closes the claim, so it is a fill
    // by another name and is accepted.
    let whole = Settlement::PartiallyFilled(TokenQuantity::lamports(CLAIM));
    assert_eq!(
        log.confirm(&id, whole, &mut portfolio, 1_004)
            .expect("whole"),
        Applied::Advanced
    );
    assert_eq!(free(&portfolio), HELD - CLAIM);
    assert_eq!(portfolio.reservations().count(), 0);
}

#[test]
fn an_established_failure_is_recorded_as_a_failure() {
    // `radar audit verify` reads the outcome column, and an operation that
    // failed reading as `ok` is a machine reporting a clean run over a
    // reservation it threw away.
    //
    // Re-apply the bug by deleting the `OperationState::Failed` arm of the
    // outcome match in `close`: the line lands as `Outcome::Ok`.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = somewhere(&dir);

    let mut portfolio = account();
    let (mut log, id) = reserved(&path, &mut portfolio);
    assert_eq!(
        log.fail(&id, &mut portfolio, 1_002).expect("fail"),
        Applied::Advanced
    );
    assert_eq!(
        free(&portfolio),
        HELD,
        "nothing was released, so nothing is held"
    );

    let events = radar_journal::Journal::open(&path)
        .expect("open")
        .events()
        .expect("events");
    let last = events.last().expect("a line");
    assert_eq!(last.outcome, radar_journal::Outcome::Failed);
    assert_eq!(last.correlation.operation.as_deref(), Some(id.as_str()));
}

#[test]
fn an_event_about_nothing_operational_is_written_exactly_as_it_always_was() {
    // The operation entry is a new field on `Event`. Every journal already on
    // disk was written without it, and a field that serialised as `null` -- or
    // that joined the digest unconditionally -- would change the id of every
    // historical event and turn an intact chain into `Verified::Broken` at
    // sequence one.
    //
    // Re-apply the bug by dropping `skip_serializing_if` from
    // `Event::operation` and this finds the key; drop the `if let Some` guard
    // in `digest_input` and every file written before today stops verifying.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("mixed.jsonl");
    let mut journal = radar_journal::Journal::open(&path).expect("open");
    journal
        .record(
            radar_journal::Stage::Received,
            radar_journal::Outcome::Ok,
            1,
            about(),
            None,
            Vec::new(),
            None,
            None,
        )
        .expect("record");

    let line = std::fs::read_to_string(&path).expect("read");
    assert!(
        !line.contains("operation"),
        "an event about nothing operational gained a field: {line}"
    );
    assert_eq!(
        journal.verify().expect("verify"),
        radar_journal::Verified::Intact { events: 1 }
    );
}
