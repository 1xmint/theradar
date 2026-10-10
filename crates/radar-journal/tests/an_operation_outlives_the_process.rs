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

fn opening() -> radar_journal::OpeningInventoryRecord {
    radar_journal::OpeningInventoryRecord {
        wallet: WALLET,
        native_lamports: HELD,
        native_slot: NOW,
        token_program_slot: NOW,
        token_2022_slot: NOW,
        raw_token_slot: None,
        read_started_at_unix_secs: 1000,
        read_completed_at_unix_secs: 1001,
        accounts: None,
        holdings: vec![],
    }
}

#[test]
fn opening_inventory_is_immutable_genesis_and_survives_restart_without_operations() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("opening.jsonl");
    let mut log = OperationLog::open(&path).expect("open");
    assert!(log.opening_inventory().is_none());
    assert_eq!(
        log.record_opening_inventory(opening(), 1001)
            .expect("record"),
        Applied::Advanced
    );
    let checkpoint = log.checkpoint().to_owned();
    assert_ne!(checkpoint, "");
    assert_eq!(log.opening_inventory(), Some(&opening()));
    assert_eq!(log.entries().count(), 0);
    let saved = std::fs::read(&path).expect("recorded");
    assert_eq!(
        log.record_opening_inventory(opening(), 1002)
            .expect("repeat"),
        Applied::AlreadySeen
    );
    let mut changed = opening();
    changed.native_lamports += 1;
    assert!(log.record_opening_inventory(changed, 1002).is_err());
    assert_eq!(std::fs::read(&path).expect("unchanged"), saved);
    drop(log);
    let mut log = OperationLog::open(&path).expect("restart");
    assert_eq!(log.opening_inventory(), Some(&opening()));
    assert_eq!(log.checkpoint(), checkpoint);
    log.propose(intent(), 1002, about())
        .expect("later operation");
    assert_eq!(
        log.record_opening_inventory(opening(), 1003)
            .expect("same after trade"),
        Applied::AlreadySeen
    );
    drop(log);
    assert_eq!(
        OperationLog::open(&path)
            .expect("later replay")
            .opening_inventory(),
        Some(&opening())
    );
    let path = dir.path().join("old.jsonl");
    let mut log = OperationLog::open(&path).expect("legacy");
    log.propose(intent(), 1000, about())
        .expect("existing trade");
    let saved = std::fs::read(&path).expect("history");
    assert!(log.record_opening_inventory(opening(), 1001).is_err());
    assert!(log.opening_inventory().is_none());
    assert_eq!(std::fs::read(&path).expect("unchanged"), saved);
}

#[test]
fn failed_opening_write_never_updates_memory_or_checkpoint() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("opening.jsonl");
    let mut log = OperationLog::open(&path).expect("open");
    std::fs::create_dir(&path).expect("obstruct file");
    assert!(log.record_opening_inventory(opening(), 1001).is_err());
    assert!(log.opening_inventory().is_none());
    assert_eq!(log.checkpoint(), "");
}

#[test]
fn replay_refuses_duplicate_late_mixed_or_misstaged_opening_records() {
    use radar_journal::{Journal, Outcome, Stage};
    for case in [
        "stage",
        "outcome",
        "mixed",
        "missing",
        "late",
        "duplicate",
        "operation",
        "inventory_operation",
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("opening.jsonl");
        let mut journal = Journal::open(&path).expect("journal");
        let mut correlation = Correlation {
            opening_inventory: Some(opening()),
            ..Correlation::default()
        };
        if case == "late" {
            journal
                .record(
                    Stage::Received,
                    Outcome::Ok,
                    1000,
                    about(),
                    None,
                    vec![],
                    None,
                    None,
                )
                .expect("earlier history");
        }
        if case == "mixed" {
            correlation.mention = Some("unexpected".into());
        }
        if case == "missing" {
            correlation = about();
        }
        if case == "operation" {
            journal
                .record_operation(
                    Outcome::Ok,
                    1001,
                    correlation,
                    radar_journal::OperationEntry {
                        intent: intent(),
                        reserved: None,
                        state: OperationState::Proposed,
                    },
                    None,
                    None,
                )
                .expect("mixed operation");
        } else {
            let stage = if case == "stage" {
                Stage::Received
            } else {
                Stage::Inventory
            };
            let outcome = if case == "outcome" {
                Outcome::Failed
            } else {
                Outcome::Ok
            };
            journal
                .record(
                    stage,
                    outcome,
                    1001,
                    correlation.clone(),
                    None,
                    vec![],
                    None,
                    None,
                )
                .expect("event");
            if case == "duplicate" {
                journal
                    .record(stage, outcome, 1002, correlation, None, vec![], None, None)
                    .expect("duplicate");
            }
        }
        assert!(matches!(
            journal.verify().expect("chain"),
            radar_journal::Verified::Intact { .. }
        ));
        if case == "inventory_operation" {
            mix_opening_operation(&journal, &path);
            assert!(matches!(
                journal.verify().expect("valid hashes"),
                radar_journal::Verified::Intact { .. }
            ));
        }
        drop(journal);
        assert!(OperationLog::open(&path).is_err(), "{case}");
    }
}

fn mix_opening_operation(journal: &radar_journal::Journal, path: &std::path::Path) {
    let mut event = journal.events().expect("event").remove(0);
    event.operation = Some(radar_journal::OperationEntry {
        intent: intent(),
        reserved: None,
        state: OperationState::Proposed,
    });
    event.id = event.digest();
    std::fs::write(
        path,
        serde_json::to_string(&event).expect("event JSON") + "\n",
    )
    .expect("rehashed mixed event");
}

fn signed_operation(
    path: &std::path::Path,
) -> (
    OperationLog,
    OperationId,
    Portfolio,
    radar_journal::SettlementRecord,
) {
    let mut log = OperationLog::open(path).expect("open");
    let mut portfolio = account();
    let id = log
        .propose(
            intent(),
            1_000,
            Correlation {
                execution: Some(radar_journal::ExecutionBinding {
                    wallet: WALLET,
                    transaction: "approved".into(),
                    signed_transaction: None,
                    reviewed_proposal: None,
                }),
                ..about()
            },
        )
        .expect("propose");
    log.reserve(&id, &mut portfolio, 1_001).expect("reserve");
    log.submit(&id, 1_002, |_| Ok::<(), ()>(()))
        .expect("record")
        .expect("effect");
    log.record_signed(&id, "signed".into(), 1_003)
        .expect("signed");
    let facts = radar_journal::SettlementRecord {
        signed_transaction: "signed".into(),
        review: serde_json::json!({"network_fee_lamports":"5000"}),
    };
    (log, id, portfolio, facts)
}

fn cost_record(facts: &radar_journal::SettlementRecord) -> radar_journal::ValuationRecord {
    radar_journal::ValuationRecord {
        settlement: facts.clone(),
        review: serde_json::json!({"position_cost_basis_micro_usd":"1000"}),
    }
}

#[test]
fn cost_records_append_before_memory_and_survive_repeats_restart_and_completion() {
    let dir = tempfile::tempdir().expect("dir");
    let path = somewhere(&dir);
    let (mut log, id, _portfolio, facts) = signed_operation(&path);
    log.record_settlement(&id, facts.clone(), 1_004)
        .expect("facts");
    let value = cost_record(&facts);
    let checkpoint = log.checkpoint().to_owned();
    let moved = dir.path().join("moved");
    std::fs::rename(&path, &moved).expect("move");
    std::fs::create_dir(&path).expect("obstruct");
    assert!(log.record_valuation(&id, value.clone(), 1_005).is_err());
    assert_eq!(log.valuation(&id), None);
    assert_eq!(log.checkpoint(), checkpoint);
    std::fs::remove_dir(&path).expect("remove obstruction");
    std::fs::rename(&moved, &path).expect("restore");
    assert_eq!(
        log.record_valuation(&id, value.clone(), 1_005)
            .expect("costs"),
        Applied::Advanced
    );
    assert_eq!(log.valuation(&id), Some(&value));
    assert_ne!(log.checkpoint(), checkpoint);
    let checkpoint = log.checkpoint().to_owned();
    let saved = std::fs::read(&path).expect("history");
    assert_eq!(
        log.record_valuation(&id, value.clone(), 1_006)
            .expect("repeat"),
        Applied::AlreadySeen
    );
    assert_eq!(log.checkpoint(), checkpoint);
    assert_eq!(std::fs::read(&path).expect("history"), saved);
    let mut conflict = value.clone();
    conflict.review["position_cost_basis_micro_usd"] = serde_json::json!("1001");
    assert!(matches!(
        log.record_valuation(&id, conflict, 1_006),
        Err(OperationError::ValuationBinding)
    ));
    assert_eq!(std::fs::read(&path).expect("history"), saved);
    assert_eq!(log.outstanding().count(), 1);
    drop(log);
    let mut log = OperationLog::open(&path).expect("replay");
    assert_eq!(log.valuation(&id), Some(&value));
    let mut fresh = account();
    log.rehold(&mut fresh).expect("rehold");
    assert_eq!(free(&fresh), HELD - CLAIM);
    log.reconcile(
        &id,
        Settlement::Completed(TokenQuantity::lamports(5000)),
        &mut fresh,
        1_007,
    )
    .expect("generic caller completion");
    assert!(matches!(
        log.record_valuation(&id, value.clone(), 1_008),
        Err(OperationError::ValuationBinding)
    ));
    drop(log);
    let log = OperationLog::open(&path).expect("terminal replay");
    assert_eq!(log.valuation(&id), Some(&value));
    assert_eq!(log.outstanding().count(), 0);
    assert_eq!(
        log.entries()
            .map(|(id, entry)| (id.clone(), entry.state))
            .collect::<Vec<_>>(),
        vec![(
            id,
            OperationState::Reconciled(Settlement::Completed(TokenQuantity::lamports(5000)))
        )]
    );
}

#[test]
fn cost_records_require_exact_preexisting_settlement_and_cannot_name_a_proposal() {
    for case in 0..4 {
        let dir = tempfile::tempdir().expect("dir");
        let path = somewhere(&dir);
        let (mut log, id, mut portfolio, facts) = signed_operation(&path);
        let mut value = cost_record(&facts);
        if case != 0 {
            log.record_settlement(&id, facts, 1_004).expect("facts");
        }
        match case {
            1 => value.settlement.signed_transaction = "foreign".into(),
            2 => value.settlement.review = serde_json::json!({"network_fee_lamports":"6000"}),
            3 => {
                log.reconcile(
                    &id,
                    Settlement::Completed(TokenQuantity::lamports(5000)),
                    &mut portfolio,
                    1_005,
                )
                .expect("terminal");
            }
            _ => {}
        }
        let saved = std::fs::read(&path).expect("history");
        assert!(matches!(
            log.record_valuation(&id, value.clone(), 1_006),
            Err(OperationError::ValuationBinding)
        ));
        assert_eq!(log.valuation(&id), None);
        assert!(matches!(
            log.propose(
                intent(),
                1_006,
                Correlation {
                    valuation: Some(value),
                    ..about()
                }
            ),
            Err(OperationError::ValuationBinding)
        ));
        assert_eq!(std::fs::read(&path).expect("history"), saved);
    }
}

#[test]
fn replay_refuses_changed_unbound_or_misstaged_cost_records() {
    for case in 0..10 {
        let dir = tempfile::tempdir().expect("dir");
        let path = somewhere(&dir);
        let (mut log, id, mut portfolio, facts) = signed_operation(&path);
        let mut value = cost_record(&facts);
        if case != 0 && case != 9 {
            log.record_settlement(&id, facts.clone(), 1_004)
                .expect("facts");
        }
        if case == 2 || case == 8 {
            log.record_valuation(&id, value.clone(), 1_005)
                .expect("first costs");
        }
        if case == 7 {
            log.reconcile(
                &id,
                Settlement::Completed(TokenQuantity::lamports(5000)),
                &mut portfolio,
                1_005,
            )
            .expect("terminal");
        }
        let mut entry = *log.entry(&id).expect("entry");
        match case {
            1 => value.settlement.signed_transaction = "foreign".into(),
            2 => value.review["position_cost_basis_micro_usd"] = serde_json::json!("1001"),
            3 => entry.state = OperationState::Reserved,
            4 => entry.intent.amount = TokenQuantity::lamports(CLAIM + 1),
            5 => entry.reserved = Some(TokenQuantity::lamports(CLAIM + 1)),
            6 => entry.state = OperationState::Proposed,
            _ => {}
        }
        drop(log);
        radar_journal::Journal::open(&path)
            .expect("journal")
            .record_operation(
                radar_journal::Outcome::Uncertain,
                1_006,
                Correlation {
                    operation: Some(id.as_str().into()),
                    valuation: Some(value.clone()),
                    settlement: if case == 9 { Some(facts) } else { None },
                    ..Correlation::default()
                },
                entry,
                None,
                None,
            )
            .expect("hashed record");
        let opened = OperationLog::open(&path);
        if case == 8 {
            assert_eq!(
                opened.expect("identical replay").valuation(&id),
                Some(&value)
            );
        } else {
            assert!(
                matches!(opened, Err(OperationError::ValuationBinding)),
                "case {case}"
            );
        }
    }
}

#[test]
fn checkpoint_tracks_durable_history_including_terminal_and_non_operation_events() {
    let dir = tempfile::tempdir().expect("dir");
    let path = somewhere(&dir);
    let mut log = OperationLog::open(&path).expect("log");
    assert_eq!(log.checkpoint(), "");
    let id = log
        .propose(
            intent(),
            1,
            Correlation {
                receipt: Some("fixture".into()),
                ..Correlation::default()
            },
        )
        .expect("proposal");
    assert_eq!(log.checkpoint(), id.as_str());
    let proposed = log.checkpoint().to_owned();
    let moved = dir.path().join("moved");
    std::fs::rename(&path, &moved).expect("move history");
    std::fs::create_dir(&path).expect("obstruct writes");
    let mut portfolio = account();
    assert!(log.fail(&id, &mut portfolio, 2).is_err());
    assert_eq!(log.checkpoint(), proposed);
    std::fs::remove_dir(&path).expect("remove empty obstruction");
    std::fs::rename(&moved, &path).expect("restore history");
    log.fail(&id, &mut portfolio, 2).expect("terminal");
    let terminal = log.checkpoint().to_owned();
    assert_ne!(terminal, proposed);
    log.fail(&id, &mut portfolio, 3).expect("repeat");
    assert_eq!(log.checkpoint(), terminal);
    drop(log);
    let reopened = OperationLog::open(&path).expect("replay");
    assert_eq!(reopened.checkpoint(), terminal);
    assert_eq!(reopened.outstanding().count(), 0);
    drop(reopened);
    let mut journal = radar_journal::Journal::open(&path).expect("journal");
    journal
        .record(
            radar_journal::Stage::Received,
            radar_journal::Outcome::Ok,
            4,
            Correlation {
                mention: Some("operator note".into()),
                ..Correlation::default()
            },
            None,
            Vec::new(),
            None,
            None,
        )
        .expect("non-operation event");
    let events = journal.events().expect("events");
    let last = &events.last().expect("last event").id;
    assert_ne!(last, &terminal);
    assert_eq!(journal.checkpoint(), last);
    let reopened = OperationLog::open(&path).expect("replay note");
    assert_eq!(reopened.checkpoint(), last);
}

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

#[test]
fn signed_binding_survives_restart_without_releasing_or_replacing_a_claim() {
    let dir = tempfile::tempdir().expect("dir");
    let path = somewhere(&dir);
    let mut log = OperationLog::open(&path).expect("log");
    let binding = radar_journal::ExecutionBinding {
        wallet: WALLET,
        transaction: "approved bytes".into(),
        signed_transaction: None,
        reviewed_proposal: Some(serde_json::json!({"creator":"reviewed creator","action":"buy"})),
    };
    let correlation = Correlation {
        execution: Some(binding.clone()),
        ..Correlation::default()
    };
    assert!(!correlation.is_empty());
    let id = log.propose(intent(), 1, correlation).expect("propose");
    assert_eq!(log.execution(&id), Some(&binding));
    assert!(log.record_signed(&id, "signed".into(), 2).is_err());
    let mut portfolio = account();
    log.reserve(&id, &mut portfolio, 2).expect("reserve");
    assert!(log.record_signed(&id, "signed".into(), 3).is_err());
    log.submit(&id, 3, |_| Ok::<_, ()>(()))
        .expect("submit")
        .expect("effect");
    let moved = dir.path().join("moved");
    std::fs::rename(&path, &moved).expect("temporarily unavailable journal");
    std::fs::create_dir(&path).expect("journal path is not writable as a file");
    assert!(log.record_signed(&id, "signed".into(), 4).is_err());
    assert_eq!(log.execution(&id), Some(&binding));
    std::fs::remove_dir(&path).expect("remove empty obstruction");
    std::fs::rename(&moved, &path).expect("restore journal");
    assert_eq!(
        log.record_signed(&id, "signed".into(), 4).expect("record"),
        Applied::Advanced
    );
    assert_eq!(
        log.record_signed(&id, "signed".into(), 5).expect("repeat"),
        Applied::AlreadySeen
    );
    assert!(log.record_signed(&id, "other".into(), 6).is_err());
    assert_eq!(free(&portfolio), HELD - CLAIM);
    drop(log);
    let mut reopened = OperationLog::open(&path).expect("replay");
    assert_eq!(
        reopened.execution(&id),
        Some(&radar_journal::ExecutionBinding {
            signed_transaction: Some("signed".into()),
            ..binding
        })
    );
    assert_eq!(reopened.outstanding().count(), 1);
    assert_eq!(
        reopened
            .record_signed(&id, "signed".into(), 7)
            .expect("replayed repeat"),
        Applied::AlreadySeen
    );
    assert!(reopened.record_signed(&id, "other".into(), 8).is_err());
    let mut fresh = account();
    reopened.rehold(&mut fresh).expect("rehold");
    assert_eq!(free(&fresh), HELD - CLAIM);
}

#[test]
fn replay_refuses_replacing_removing_or_inventing_the_reviewed_proposal() {
    for change in 0..4 {
        let dir = tempfile::tempdir().expect("dir");
        let path = somewhere(&dir);
        let mut log = OperationLog::open(&path).expect("log");
        let mut binding = radar_journal::ExecutionBinding {
            wallet: WALLET,
            transaction: "approved".into(),
            signed_transaction: None,
            reviewed_proposal: (change != 3)
                .then(|| serde_json::json!({"creator":"original","action":"buy"})),
        };
        let id = log
            .propose(
                intent(),
                1,
                Correlation {
                    execution: Some(binding.clone()),
                    ..about()
                },
            )
            .expect("proposal");
        log.reserve(&id, &mut account(), 2).expect("reserve");
        log.submit(&id, 3, |_| Ok::<_, ()>(()))
            .expect("submit")
            .expect("effect");
        let entry = *log.entry(&id).expect("entry");
        binding.signed_transaction = Some("signed".into());
        match change {
            0 => {
                binding.reviewed_proposal.as_mut().expect("proposal")["creator"] =
                    serde_json::json!("changed");
            }
            1 => {
                binding.reviewed_proposal.as_mut().expect("proposal")["action"] =
                    serde_json::json!("exit");
            }
            2 => binding.reviewed_proposal = None,
            _ => {
                binding.reviewed_proposal =
                    Some(serde_json::json!({"creator":"invented","action":"buy"}));
            }
        }
        drop(log);
        radar_journal::Journal::open(&path)
            .expect("journal")
            .record_operation(
                radar_journal::Outcome::Uncertain,
                4,
                Correlation {
                    operation: Some(id.as_str().into()),
                    execution: Some(binding),
                    ..Correlation::default()
                },
                entry,
                None,
                None,
            )
            .expect("valid hashes but changed attribution");
        assert!(matches!(
            OperationLog::open(&path),
            Err(OperationError::ExecutionBinding)
        ));
    }
}

#[test]
fn replay_refuses_changed_execution_identity_missing_bindings_and_changed_claims() {
    for change in 0..7 {
        let dir = tempfile::tempdir().expect("dir");
        let path = somewhere(&dir);
        let mut log = OperationLog::open(&path).expect("log");
        let binding = radar_journal::ExecutionBinding {
            wallet: WALLET,
            transaction: "approved".into(),
            signed_transaction: None,
            reviewed_proposal: None,
        };
        let id = log
            .propose(
                intent(),
                1,
                Correlation {
                    execution: (change != 6).then_some(binding),
                    ..about()
                },
            )
            .expect("propose");
        assert!(log.record_signed(&id, "signed".into(), 1).is_err());
        log.reserve(&id, &mut account(), 2).expect("reserve");
        if change != 4 {
            log.submit(&id, 3, |_| Ok::<_, ()>(()))
                .expect("submit")
                .expect("effect");
        }
        if change == 3 {
            log.record_signed(&id, "first".into(), 4)
                .expect("first binding");
        }
        let mut entry = *log.entry(&id).expect("entry");
        let mut binding = radar_journal::ExecutionBinding {
            wallet: WALLET,
            transaction: "approved".into(),
            signed_transaction: Some("signed".into()),
            reviewed_proposal: None,
        };
        match change {
            0 => binding.wallet = radar_types::Address::SYSTEM_PROGRAM,
            1 => binding.transaction = "changed".into(),
            2 => binding.signed_transaction = None,
            5 => entry.intent.amount = TokenQuantity::lamports(1),
            _ => {}
        }
        drop(log);
        radar_journal::Journal::open(&path)
            .expect("journal")
            .record_operation(
                radar_journal::Outcome::Uncertain,
                5,
                Correlation {
                    operation: Some(id.as_str().into()),
                    execution: Some(binding),
                    ..Correlation::default()
                },
                entry,
                None,
                None,
            )
            .expect("valid hash chain with invalid execution metadata");
        assert!(
            OperationLog::open(&path).is_err(),
            "invalid binding {change}"
        );
    }
    let dir = tempfile::tempdir().expect("dir");
    let path = somewhere(&dir);
    let (mut log, id) = reserved(&path, &mut account());
    log.submit(&id, 3, |_| Ok::<_, ()>(()))
        .expect("submit")
        .expect("effect");
    assert!(log.record_signed(&id, "signed".into(), 4).is_err());
    assert_eq!(log.execution(&id), None);
    assert_eq!(log.outstanding().count(), 1);
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
    // same operation change is now recorded twice, each with a valid event
    // sequence/hash. Repeating raw JSONL bytes would corrupt the chain rather
    // than represent a second recorded change, and now refuses before replay.
    let mut journal = radar_journal::Journal::open(&path).expect("journal");
    let last = journal.events().expect("events").pop().expect("last");
    journal
        .record_operation(
            last.outcome,
            last.at,
            last.correlation,
            last.operation.expect("operation"),
            last.build,
            last.redacted,
        )
        .expect("duplicate change");

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
fn finalized_facts_survive_restart_and_terminal_completion_without_releasing_early() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = somewhere(&dir);
    let mut portfolio = account();
    let mut log = OperationLog::open(&path).expect("open");
    let reviewed = serde_json::json!({"creator":"reviewed","action":"buy"});
    let id = log
        .propose(
            intent(),
            1_000,
            Correlation {
                execution: Some(radar_journal::ExecutionBinding {
                    wallet: WALLET,
                    transaction: "approved".into(),
                    signed_transaction: None,
                    reviewed_proposal: Some(reviewed.clone()),
                }),
                ..about()
            },
        )
        .expect("propose");
    log.reserve(&id, &mut portfolio, 1_001).expect("reserve");
    log.submit(&id, 1_002, |_| Ok::<(), ()>(()))
        .expect("record")
        .expect("effect");
    log.record_signed(&id, "signed".into(), 1_003)
        .expect("signed");
    let evidence = radar_journal::SettlementRecord {
        signed_transaction: "signed".into(),
        review: serde_json::json!({"network_fee_lamports":"5000"}),
    };
    let before = portfolio.clone();
    let history = std::fs::read(&path).expect("history");
    let moved = dir.path().join("moved");
    std::fs::rename(&path, &moved).expect("move history");
    std::fs::create_dir(&path).expect("obstruct write");
    assert!(log.record_settlement(&id, evidence.clone(), 1_004).is_err());
    assert_eq!(log.settlement(&id), None);
    assert_eq!(portfolio, before);
    std::fs::remove_dir(&path).expect("remove empty obstruction");
    std::fs::rename(&moved, &path).expect("restore history");
    assert_eq!(std::fs::read(&path).expect("history"), history);
    assert_eq!(
        log.record_settlement(&id, evidence.clone(), 1_004)
            .expect("record"),
        Applied::Advanced
    );
    assert_eq!(log.settlement(&id), Some(&evidence));
    assert_eq!(portfolio, before);
    assert_eq!(log.outstanding().count(), 1);
    let recorded = std::fs::read(&path).expect("history");
    assert_eq!(
        log.record_settlement(&id, evidence.clone(), 1_005)
            .expect("repeat"),
        Applied::AlreadySeen
    );
    assert_eq!(std::fs::read(&path).expect("history"), recorded);
    let mut changed = evidence.clone();
    changed.review["network_fee_lamports"] = serde_json::json!("6000");
    assert!(matches!(
        log.record_settlement(&id, changed, 1_005),
        Err(OperationError::SettlementBinding)
    ));
    assert_eq!(std::fs::read(&path).expect("history"), recorded);
    drop(log);
    let mut log = OperationLog::open(&path).expect("replay");
    assert_eq!(log.settlement(&id), Some(&evidence));
    let mut fresh = account();
    log.rehold(&mut fresh).expect("rehold");
    assert_eq!(free(&fresh), HELD - CLAIM);
    log.reconcile(
        &id,
        Settlement::Completed(TokenQuantity::lamports(5_000)),
        &mut fresh,
        1_006,
    )
    .expect("generic caller established completion");
    assert!(matches!(
        log.record_settlement(&id, evidence.clone(), 1_007),
        Err(OperationError::SettlementBinding)
    ));
    drop(log);
    let log = OperationLog::open(&path).expect("terminal replay");
    assert_eq!(log.settlement(&id), Some(&evidence));
    assert_eq!(
        log.execution(&id).expect("binding").reviewed_proposal,
        Some(reviewed)
    );
    assert_eq!(log.outstanding().count(), 0);
}

#[test]
fn finalized_record_requires_an_unknown_operation_with_the_recorded_signed_artifact() {
    for change in 0..4 {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = somewhere(&dir);
        let mut portfolio = account();
        let mut log = OperationLog::open(&path).expect("open");
        let id = log
            .propose(
                intent(),
                1_000,
                if change == 0 {
                    about()
                } else {
                    Correlation {
                        execution: Some(radar_journal::ExecutionBinding {
                            wallet: WALLET,
                            transaction: "approved".into(),
                            signed_transaction: None,
                            reviewed_proposal: None,
                        }),
                        ..about()
                    }
                },
            )
            .expect("propose");
        log.reserve(&id, &mut portfolio, 1_001).expect("reserve");
        log.submit(&id, 1_002, |_| Ok::<(), ()>(()))
            .expect("record")
            .expect("effect");
        if change >= 2 {
            log.record_signed(&id, "signed".into(), 1_003)
                .expect("signed");
        }
        if change == 3 {
            log.reconcile(
                &id,
                Settlement::Completed(TokenQuantity::lamports(5_000)),
                &mut portfolio,
                1_004,
            )
            .expect("complete");
        }
        let evidence = radar_journal::SettlementRecord {
            signed_transaction: if change == 2 { "different" } else { "signed" }.into(),
            review: serde_json::json!({}),
        };
        let history = std::fs::read(&path).expect("history");
        let before = portfolio.clone();
        assert!(matches!(
            log.record_settlement(&id, evidence.clone(), 1_005),
            Err(OperationError::SettlementBinding)
        ));
        assert_eq!(std::fs::read(&path).expect("history"), history);
        assert_eq!(portfolio, before);
        assert_eq!(log.settlement(&id), None);
        assert!(matches!(
            log.propose(
                intent(),
                1_006,
                Correlation {
                    settlement: Some(evidence),
                    ..about()
                }
            ),
            Err(OperationError::SettlementBinding)
        ));
        assert_eq!(std::fs::read(&path).expect("history"), history);
    }
}

#[test]
fn replay_refuses_unbound_changed_or_misstaged_finalized_facts() {
    for change in 0..7 {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = somewhere(&dir);
        let mut portfolio = account();
        let mut log = OperationLog::open(&path).expect("open");
        let id = log
            .propose(
                intent(),
                1_000,
                Correlation {
                    execution: Some(radar_journal::ExecutionBinding {
                        wallet: WALLET,
                        transaction: "approved".into(),
                        signed_transaction: None,
                        reviewed_proposal: None,
                    }),
                    ..about()
                },
            )
            .expect("propose");
        log.reserve(&id, &mut portfolio, 1_001).expect("reserve");
        log.submit(&id, 1_002, |_| Ok::<(), ()>(()))
            .expect("record")
            .expect("effect");
        if change != 4 {
            log.record_signed(&id, "signed".into(), 1_003)
                .expect("signed");
        }
        if change == 6 {
            log.reconcile(
                &id,
                Settlement::Completed(TokenQuantity::lamports(5_000)),
                &mut portfolio,
                1_004,
            )
            .expect("terminal operation");
        }
        let mut evidence = radar_journal::SettlementRecord {
            signed_transaction: "signed".into(),
            review: serde_json::json!({"network_fee_lamports":"5000"}),
        };
        if change == 3 {
            log.record_settlement(&id, evidence.clone(), 1_004)
                .expect("first");
            evidence.review = serde_json::json!({"network_fee_lamports":"6000"});
        }
        let mut entry = *log.entry(&id).expect("entry");
        match change {
            0 => evidence.signed_transaction = "different".into(),
            1 => entry.state = OperationState::Reserved,
            2 => entry.reserved = Some(TokenQuantity::lamports(CLAIM + 1)),
            5 => entry.state = OperationState::Proposed,
            _ => {}
        }
        drop(log);
        radar_journal::Journal::open(&path)
            .expect("journal")
            .record_operation(
                radar_journal::Outcome::Uncertain,
                1_005,
                Correlation {
                    operation: Some(id.as_str().into()),
                    settlement: Some(evidence),
                    ..Correlation::default()
                },
                entry,
                None,
                None,
            )
            .expect("hashed but invalid record");
        assert!(matches!(
            OperationLog::open(&path),
            Err(OperationError::SettlementBinding)
        ));
    }
}

#[test]
fn completed_spends_close_durably_once_without_redebiting_fresh_balances() {
    for spent in [0, 5_000, CLAIM] {
        for confirmed in [true, false] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = somewhere(&dir);
            let mut portfolio = account();
            let (mut log, id) = reserved(&path, &mut portfolio);
            log.submit(&id, 1_002, |_| Ok::<(), ()>(()))
                .expect("record")
                .expect("effect");
            let settlement = Settlement::Completed(TokenQuantity::lamports(spent));
            let apply = |log: &mut OperationLog, portfolio: &mut Portfolio| {
                if confirmed {
                    log.confirm(&id, settlement, portfolio, 1_003)
                } else {
                    log.reconcile(&id, settlement, portfolio, 1_003)
                }
            };
            assert_eq!(
                apply(&mut log, &mut portfolio).expect("complete"),
                Applied::Advanced
            );
            assert_eq!(free(&portfolio), HELD - spent);
            assert_eq!(portfolio.reservations().count(), 0);
            assert_eq!(log.outstanding().count(), 0);
            let history = std::fs::read(&path).expect("history");
            assert_eq!(
                apply(&mut log, &mut portfolio).expect("repeat"),
                Applied::AlreadySeen
            );
            assert_eq!(std::fs::read(&path).expect("history"), history);
            assert_eq!(free(&portfolio), HELD - spent);
            drop(log);
            let mut log = OperationLog::open(&path).expect("replay");
            let mut fresh = account();
            fresh
                .hold(
                    Asset::Sol,
                    Holding::new(
                        AssetRole::Cash,
                        Balance::Counted(TokenQuantity::lamports(HELD - spent)),
                        Valuation::Unknown(Unvaluable::NoPrice),
                        Valuation::Unknown(Unvaluable::NoPrice),
                    ),
                )
                .expect("fresh balance");
            log.rehold(&mut fresh).expect("rehold");
            assert_eq!(fresh.reservations().count(), 0);
            assert_eq!(
                apply(&mut log, &mut fresh).expect("replayed repeat"),
                Applied::AlreadySeen
            );
            assert_eq!(free(&fresh), HELD - spent);
            assert_eq!(
                log.entry(&id).expect("entry").state,
                if confirmed {
                    OperationState::Confirmed(settlement)
                } else {
                    OperationState::Reconciled(settlement)
                }
            );
        }
    }
}

#[test]
fn rejected_or_unwritten_completion_changes_neither_history_nor_portfolio() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = somewhere(&dir);
    let mut portfolio = account();
    let (mut log, id) = reserved(&path, &mut portfolio);
    log.submit(&id, 1_002, |_| Ok::<(), ()>(()))
        .expect("record")
        .expect("effect");
    let history = std::fs::read(&path).expect("history");
    let before = portfolio.clone();
    for spent in [
        TokenQuantity::lamports(CLAIM + 1),
        TokenQuantity::new(
            1,
            radar_types::Decimals::from_mint_account(6).expect("fixture decimals"),
        ),
    ] {
        assert!(
            log.reconcile(&id, Settlement::Completed(spent), &mut portfolio, 1_003)
                .is_err()
        );
        assert_eq!(portfolio, before);
        assert_eq!(std::fs::read(&path).expect("history"), history);
        assert_eq!(
            log.entry(&id).expect("entry").state,
            OperationState::SubmissionUnknown
        );
    }
    let moved = dir.path().join("moved");
    std::fs::rename(&path, &moved).expect("move journal");
    std::fs::create_dir(&path).expect("obstruct journal");
    let settlement = Settlement::Completed(TokenQuantity::lamports(5_000));
    assert!(
        log.reconcile(&id, settlement, &mut portfolio, 1_003)
            .is_err()
    );
    assert_eq!(portfolio, before);
    assert_eq!(
        log.entry(&id).expect("entry").state,
        OperationState::SubmissionUnknown
    );
    std::fs::remove_dir(&path).expect("remove empty obstruction");
    std::fs::rename(&moved, &path).expect("restore journal");
    assert_eq!(std::fs::read(&path).expect("history"), history);
    drop(log);
    let mut log = OperationLog::open(&path).expect("replay");
    assert!(matches!(
        log.reconcile(&id, settlement, &mut portfolio, 1_004),
        Err(OperationError::ClaimNotReheld)
    ));
    assert_eq!(std::fs::read(&path).expect("history"), history);
    assert_eq!(portfolio, before);
    let mut fresh = account();
    log.rehold(&mut fresh).expect("rehold");
    log.reconcile(&id, settlement, &mut fresh, 1_005)
        .expect("complete after rehold");
    assert_eq!(free(&fresh), HELD - 5_000);
}

#[test]
fn replay_refuses_completed_spends_that_change_or_exceed_the_recorded_reservation() {
    for change in 0..6 {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = somewhere(&dir);
        let mut portfolio = account();
        let (mut log, id) = reserved(&path, &mut portfolio);
        log.submit(&id, 1_002, |_| Ok::<(), ()>(()))
            .expect("record")
            .expect("effect");
        let mut entry = *log.entry(&id).expect("entry");
        entry.state =
            OperationState::Reconciled(Settlement::Completed(TokenQuantity::lamports(5_000)));
        match change {
            0 => entry.intent.amount = TokenQuantity::lamports(CLAIM + 1),
            1 => entry.reserved = Some(TokenQuantity::lamports(CLAIM + 1)),
            2 => {
                entry.state = OperationState::Confirmed(Settlement::Completed(
                    TokenQuantity::lamports(CLAIM + 1),
                ));
            }
            3 => {
                entry.state =
                    OperationState::Reconciled(Settlement::Completed(TokenQuantity::new(
                        1,
                        radar_types::Decimals::from_mint_account(6).expect("fixture decimals"),
                    )));
            }
            4 => entry.reserved = None,
            _ => entry.intent.asset = Asset::Usdc,
        }
        drop(log);
        radar_journal::Journal::open(&path)
            .expect("journal")
            .record_operation(
                radar_journal::Outcome::Ok,
                1_003,
                Correlation {
                    operation: Some(id.as_str().into()),
                    ..Correlation::default()
                },
                entry,
                None,
                None,
            )
            .expect("hashed but invalid completion");
        assert!(matches!(
            OperationLog::open(&path),
            Err(OperationError::InvalidCompletedSettlement)
        ));
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

fn external_transfer() -> radar_journal::NativeTransferRecord {
    let mut bytes = vec![1];
    bytes.extend([7; 64]);
    bytes.extend([1, 0, 1]);
    let signed = radar_types::b64::encode(&bytes);
    radar_journal::NativeTransferRecord {
        wallet: WALLET,
        signed_transaction: signed.clone(),
        evidence: serde_json::json!({"transaction_base64":signed}),
        review: serde_json::json!({"signature":radar_types::Signature::new([7;64]),"net_change_lamports":"-2"}),
    }
}

#[test]
fn native_transfer_storage_is_immutable_idempotent_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native.jsonl");
    let mut log = OperationLog::open(&path).unwrap();
    assert_eq!(log.native_transfers().count(), 0);
    assert_eq!(
        log.record_native_transfer(external_transfer(), 1).unwrap(),
        Applied::Advanced
    );
    let checkpoint = log.checkpoint().to_owned();
    let saved = std::fs::read(&path).unwrap();
    assert_eq!(
        log.record_native_transfer(external_transfer(), 2).unwrap(),
        Applied::AlreadySeen
    );
    let mut changed = external_transfer();
    changed.review["net_change_lamports"] = serde_json::json!("-3");
    assert!(log.record_native_transfer(changed, 2).is_err());
    let mut changed = external_transfer();
    changed.wallet = Address::new([8; 32]);
    assert!(log.record_native_transfer(changed, 2).is_err());
    let mut changed = external_transfer();
    changed.evidence["transaction_base64"] = serde_json::json!("different");
    assert!(log.record_native_transfer(changed, 2).is_err());
    let mut changed = external_transfer();
    changed.review["signature"] = serde_json::json!(radar_types::Signature::new([8; 64]));
    assert!(log.record_native_transfer(changed, 2).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert_eq!(log.entries().count(), 0);
    assert_eq!(log.outstanding().count(), 0);
    drop(log);
    let mut log = OperationLog::open(&path).unwrap();
    assert_eq!(
        log.native_transfers().cloned().collect::<Vec<_>>(),
        vec![external_transfer()]
    );
    assert_eq!(log.checkpoint(), checkpoint);
    assert_eq!(
        log.record_native_transfer(external_transfer(), 3).unwrap(),
        Applied::AlreadySeen
    );
    assert_eq!(std::fs::read(&path).unwrap(), saved);
}

#[test]
fn native_transfer_write_failure_preserves_memory_and_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native.jsonl");
    let mut log = OperationLog::open(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(log.record_native_transfer(external_transfer(), 1).is_err());
    assert_eq!(log.native_transfers().count(), 0);
    assert_eq!(log.checkpoint(), "");
}

fn record_native_collision(journal: &mut radar_journal::Journal) {
    use radar_journal::Outcome;
    journal
        .record_operation(
            Outcome::Ok,
            0,
            Correlation {
                execution: Some(radar_journal::ExecutionBinding {
                    wallet: WALLET,
                    transaction: "reviewed".into(),
                    signed_transaction: Some(external_transfer().signed_transaction),
                    reviewed_proposal: None,
                }),
                ..Correlation::default()
            },
            radar_journal::OperationEntry {
                intent: intent(),
                reserved: None,
                state: OperationState::Proposed,
            },
            None,
            None,
        )
        .unwrap();
}

fn native_correlation(case: &str) -> Correlation {
    let mut correlation = Correlation {
        native_transfer: Some(external_transfer()),
        ..Correlation::default()
    };
    if case == "mixed" {
        correlation.mention = Some("unrelated".into());
    }
    if case == "missing" {
        correlation.native_transfer = None;
        correlation.mention = Some("unrelated".into());
    }
    correlation
}

#[test]
fn native_transfer_replay_checks_association_and_conflicts_even_with_an_intact_chain() {
    use radar_journal::{Journal, Outcome, Stage};
    for case in [
        "stage",
        "outcome",
        "mixed",
        "missing",
        "conflict",
        "duplicate",
        "operation",
        "collision",
        "native_operation",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("native.jsonl");
        let mut journal = Journal::open(&path).unwrap();
        if case == "collision" {
            record_native_collision(&mut journal);
        }
        let mut correlation = native_correlation(case);
        if case == "operation" {
            journal
                .record_operation(
                    Outcome::Ok,
                    1,
                    correlation,
                    radar_journal::OperationEntry {
                        intent: intent(),
                        reserved: None,
                        state: OperationState::Proposed,
                    },
                    None,
                    None,
                )
                .unwrap();
        } else {
            journal
                .record(
                    if case == "stage" {
                        Stage::Received
                    } else {
                        Stage::NativeTransfer
                    },
                    if case == "outcome" {
                        Outcome::Uncertain
                    } else {
                        Outcome::Ok
                    },
                    1,
                    correlation.clone(),
                    None,
                    vec![],
                    None,
                    None,
                )
                .unwrap();
            if case == "conflict" || case == "duplicate" {
                if case == "conflict" {
                    correlation.native_transfer.as_mut().unwrap().review["net_change_lamports"] =
                        serde_json::json!("-3");
                }
                journal
                    .record(
                        Stage::NativeTransfer,
                        Outcome::Ok,
                        2,
                        correlation,
                        None,
                        vec![],
                        None,
                        None,
                    )
                    .unwrap();
            }
        }
        if case == "native_operation" {
            mix_opening_operation(&journal, &path);
        }
        assert!(matches!(
            journal.verify().unwrap(),
            radar_journal::Verified::Intact { .. }
        ));
        if case == "duplicate" {
            assert_eq!(
                OperationLog::open(&path)
                    .unwrap()
                    .native_transfers()
                    .count(),
                1
            );
        } else {
            assert!(OperationLog::open(&path).is_err(), "{case}");
        }
    }
}

#[test]
fn native_transfers_cannot_be_recounted_as_operations_in_either_direction() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native.jsonl");
    let mut log = OperationLog::open(&path).unwrap();
    let record = external_transfer();
    let binding = radar_journal::ExecutionBinding {
        wallet: WALLET,
        transaction: "reviewed".into(),
        signed_transaction: Some(record.signed_transaction.clone()),
        reviewed_proposal: None,
    };
    log.propose(
        intent(),
        1,
        Correlation {
            execution: Some(binding.clone()),
            ..Correlation::default()
        },
    )
    .unwrap();
    assert!(log.record_native_transfer(record.clone(), 2).is_err());
    let other = dir.path().join("other.jsonl");
    let mut log = OperationLog::open(&other).unwrap();
    log.record_native_transfer(record.clone(), 1).unwrap();
    assert!(
        log.propose(
            intent(),
            2,
            Correlation {
                native_transfer: Some(record.clone()),
                ..Correlation::default()
            }
        )
        .is_err()
    );
    assert!(
        log.propose(
            intent(),
            2,
            Correlation {
                execution: Some(binding.clone()),
                ..Correlation::default()
            }
        )
        .is_err()
    );
    let mut unsigned = binding;
    unsigned.signed_transaction = None;
    let id = log
        .propose(
            intent(),
            2,
            Correlation {
                execution: Some(unsigned),
                ..Correlation::default()
            },
        )
        .unwrap();
    let mut portfolio = account();
    log.reserve(&id, &mut portfolio, 3).unwrap();
    log.submit(&id, 4, |_| Ok::<_, ()>(())).unwrap().unwrap();
    assert!(
        log.record_signed(&id, record.signed_transaction, 5)
            .is_err()
    );
    assert_eq!(log.outstanding().count(), 1);
}

#[test]
fn native_transfer_storage_checks_exact_wire_extent_and_canonical_identity() {
    for (size, accepted) in [(64, false), (65, true), (1232, true), (1233, false)] {
        let dir = tempfile::tempdir().unwrap();
        let mut log = OperationLog::open(dir.path().join("native.jsonl")).unwrap();
        let mut record = external_transfer();
        let mut bytes = vec![7; size];
        bytes[0] = 1;
        record.signed_transaction = radar_types::b64::encode(&bytes);
        record.evidence["transaction_base64"] = serde_json::json!(record.signed_transaction);
        assert_eq!(
            log.record_native_transfer(record, 1).is_ok(),
            accepted,
            "{size}"
        );
    }
    for case in ["count", "base64", "canonical", "artifact", "signature"] {
        let dir = tempfile::tempdir().unwrap();
        let mut log = OperationLog::open(dir.path().join("native.jsonl")).unwrap();
        let mut record = external_transfer();
        if case == "count" {
            let mut bytes = radar_types::b64::decode(&record.signed_transaction).unwrap();
            bytes[0] = 2;
            record.signed_transaction = radar_types::b64::encode(&bytes);
        }
        if case == "base64" {
            record.signed_transaction = "%%%".into();
        }
        if case == "canonical" {
            record.signed_transaction.push(' ');
        }
        record.evidence["transaction_base64"] = serde_json::json!(record.signed_transaction);
        if case == "artifact" {
            record.evidence["transaction_base64"] = serde_json::json!("changed");
        }
        if case == "signature" {
            record.review["signature"] = serde_json::json!(radar_types::Signature::new([8; 64]));
        }
        assert!(log.record_native_transfer(record, 1).is_err(), "{case}");
    }
}

#[test]
fn opening_account_extension_preserves_legacy_serialization_and_replays_known_empty() {
    let old = opening();
    let value = serde_json::to_value(&old).unwrap();
    assert!(value.get("accounts").is_none());
    let roundtrip: radar_journal::OpeningInventoryRecord =
        serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(roundtrip).unwrap(), value);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("known-empty.jsonl");
    let mut log = OperationLog::open(&path).unwrap();
    let mut known = old.clone();
    known.accounts = Some(vec![]);
    log.record_opening_inventory(known.clone(), 1001).unwrap();
    let checkpoint = log.checkpoint().to_owned();
    drop(log);
    let mut log = OperationLog::open(&path).unwrap();
    assert_eq!(log.opening_inventory(), Some(&known));
    assert_eq!(log.checkpoint(), checkpoint);
    assert!(log.record_opening_inventory(old, 1002).is_err());
}
