// SPDX-License-Identifier: Apache-2.0
//! Exercises the offline issuer's actual process, not a caller-built proof.

use serde_json::{Value, json};
use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Child, Command, Stdio};

const SEED: [u8; 32] = [0x6B; 32];

#[test]
fn issued_history_retains_the_typed_reviewed_proposal_without_unreviewed_fields() {
    let mut fixture = Fixture::new();
    let expected = fixture.snapshot["proposal"].clone();
    fixture.snapshot["proposal"]["provider_body"] = json!("UNREVIEWED_PROPOSAL_MUST_NOT_PERSIST");
    fixture.save();
    let mut issuer = fixture.start();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
    drop(issuer);
    let history = fixture.dir.path().join("operations.jsonl");
    let log = radar_journal::OperationLog::open(&history).expect("replay");
    let id = log.outstanding().next().expect("operation").0;
    assert_eq!(
        log.execution(id).expect("binding").reviewed_proposal,
        Some(expected)
    );
    assert!(
        !std::fs::read_to_string(&history)
            .expect("history")
            .contains("UNREVIEWED_PROPOSAL_MUST_NOT_PERSIST")
    );
    drop(log);
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
}

#[test]
fn accounting_checkpoint_is_required_and_cannot_be_supplied_by_the_candidate() {
    let mut fixture = Fixture::new();
    let mut issuer = fixture.start();
    for value in [Value::Null, json!(1), json!([])] {
        fixture.snapshot["accounting_checkpoint"] = value;
        fixture.save();
        assert_eq!(
            issuer.ask(&fixture.candidate)["reason"],
            "invalid trusted snapshot"
        );
    }
    fixture
        .snapshot
        .as_object_mut()
        .expect("object")
        .remove("accounting_checkpoint");
    fixture.save();
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "invalid trusted snapshot"
    );
    fixture.snapshot["accounting_checkpoint"] = json!("invented history");
    fixture.save();
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "snapshot accounting does not cover current journal history"
    );
    fixture.snapshot["accounting_checkpoint"] = json!("");
    fixture.save();
    let mut candidate = fixture.candidate.clone();
    candidate["accounting_checkpoint"] = json!("");
    assert_eq!(issuer.ask(&candidate)["reason"], "invalid candidate");
    assert_eq!(
        std::fs::read(fixture.dir.path().join("operations.jsonl")).expect("history"),
        Vec::<u8>::new()
    );
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
}

#[test]
fn terminal_history_requires_a_new_protected_accounting_checkpoint_before_issuance() {
    use radar_types::{
        Asset, AssetRole, Balance, Holding, Portfolio, Slot, TokenQuantity, Unvaluable, Valuation,
    };
    for completed in [false, true] {
        let mut fixture = Fixture::new();
        let history = fixture.dir.path().join("operations.jsonl");
        let wallet = serde_json::from_value(fixture.snapshot["wallet"].clone()).expect("wallet");
        let mut portfolio = Portfolio::at(wallet, Slot(1000));
        portfolio
            .hold(
                Asset::Sol,
                Holding::new(
                    AssetRole::Cash,
                    Balance::Counted(TokenQuantity::lamports(300_000_000)),
                    Valuation::Unknown(Unvaluable::NoPrice),
                    Valuation::Unknown(Unvaluable::NoPrice),
                ),
            )
            .expect("balance");
        let mut log = radar_journal::OperationLog::open(&history).expect("log");
        let id = log
            .propose(
                radar_journal::Intent {
                    asset: Asset::Sol,
                    amount: TokenQuantity::lamports(5000),
                    at: Slot(1000),
                },
                unix_now(),
                radar_journal::Correlation {
                    receipt: Some("accounting fixture".into()),
                    ..radar_journal::Correlation::default()
                },
            )
            .expect("proposal");
        if completed {
            log.reserve(&id, &mut portfolio, unix_now())
                .expect("reserve");
            log.submit(&id, unix_now(), |_| Ok::<_, ()>(()))
                .expect("submit")
                .expect("effect");
            // Generic caller fixture, not a protected economic reconciliation command.
            log.reconcile(
                &id,
                radar_types::Settlement::Completed(TokenQuantity::lamports(5000)),
                &mut portfolio,
                unix_now(),
            )
            .expect("complete");
        } else {
            log.fail(&id, &mut portfolio, unix_now())
                .expect("abort before effect");
        }
        let checkpoint = log.checkpoint().to_owned();
        assert_ne!(checkpoint, id.as_str());
        assert_eq!(log.outstanding().count(), 0);
        drop(log);
        let before = std::fs::read(&history).expect("history");
        for stale in ["", id.as_str(), "different journal head"] {
            fixture.snapshot["accounting_checkpoint"] = json!(stale);
            fixture.save();
            assert_eq!(
                fixture.start().ask(&fixture.candidate)["reason"],
                "snapshot accounting does not cover current journal history"
            );
            assert_eq!(std::fs::read(&history).expect("unchanged"), before);
        }
        // Updating coverage never overrides the independent risk kernel.
        fixture.snapshot["accounting_checkpoint"] = json!(checkpoint);
        fixture.snapshot["state"]["realised_loss_today"] = json!(10_000_000);
        fixture.save();
        let mut issuer = fixture.start();
        assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "refused");
        assert_eq!(std::fs::read(&history).expect("no issuance"), before);
        // Deliberate protected operator provision in this fixture, not inferred PnL.
        fixture.snapshot["state"]["realised_loss_today"] = json!(0);
        fixture.save();
        assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
        assert_eq!(
            issuer.ask(&fixture.candidate)["reason"],
            "outstanding operation requires reconciliation"
        );
    }
}

#[test]
fn settlement_review_reverifies_the_protected_artifact_signature() {
    let key = ed25519_dalek::SigningKey::from_bytes(&[0x42; 32]);
    let (fixture, unsigned) = fixture_for_wallet(&key);
    let mut issuer = fixture.start();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
    drop(issuer);
    let history = fixture.dir.path().join("operations.jsonl");
    let mut log = radar_journal::OperationLog::open(&history).expect("history");
    let id = log.outstanding().next().expect("operation").0.clone();
    // Generic journal callers do not authenticate signatures. Deliberately
    // provision an invalid artifact to prove review enforces that boundary.
    log.record_signed(&id, radar_types::b64::encode(&unsigned), unix_now())
        .expect("unverified fixture");
    let time = unix_now();
    let reserved = log
        .entry(&id)
        .expect("entry")
        .reserved
        .expect("reserve")
        .raw();
    log.record_settlement(
        &id,
        radar_journal::SettlementRecord {
            signed_transaction: radar_types::b64::encode(&unsigned),
            review: json!({"version":1,"authority":"protected_file_review","operation":id.as_str(),
                "wallet":fixture.config["wallet"],"signature_verified_locally":true,
                "reserved_lamports":reserved.to_string(),"slot":"1001","block_time_unix_secs":time.to_string(),
                "wallet_net_change_lamports":"-5000","network_fee_lamports":"5000"}),
        },
        unix_now(),
    )
    .expect("unverified generic facts");
    drop(log);
    let path = fixture.dir.path().join("evidence.json");
    write(
        &path,
        &json!({"version":1,"asset":"sol","micro_usd_per_sol":"200000000",
        "as_of_slot":"1000","as_of_unix_secs":time.to_string()}),
    );
    let saved = std::fs::read(&history).expect("history bytes");
    for mode in [
        "--review-settlement",
        "--record-settlement",
        "--review-valuation",
        "--record-valuation",
    ] {
        let result = fixture
            .command()
            .args([mode, id.as_str()])
            .arg(&path)
            .output()
            .expect("review");
        assert!(!result.status.success());
        assert_eq!(result.stdout, Vec::<u8>::new());
        assert!(
            String::from_utf8_lossy(&result.stderr)
                .contains("wallet signature verification failed")
        );
        assert_eq!(std::fs::read(&history).expect("history bytes"), saved);
    }
    assert_eq!(
        radar_journal::OperationLog::open(&history)
            .expect("history")
            .outstanding()
            .count(),
        1
    );
}

#[test]
fn protected_valuation_prices_retained_effects_without_changing_history_or_claims() {
    let (fixture, _, id, mut evidence) = finalized_fixture();
    evidence["outcome"] = json!("succeeded");
    let time = evidence["read_completed_at_unix_secs"]
        .as_u64()
        .expect("time")
        - 1;
    evidence["block_time_unix_secs"] = json!(time.to_string());
    let path = fixture.dir.path().join("price.json");
    let price = json!({"version":1,"asset":"sol","micro_usd_per_sol":"200000000",
        "as_of_slot":"1000","as_of_unix_secs":time.to_string()});
    write(&path, &price);
    let review = || {
        fixture
            .command()
            .args(["--review-valuation", id.as_str()])
            .arg(&path)
            .output()
            .expect("valuation process")
    };
    let missing = review();
    assert!(!missing.status.success());
    assert_eq!(missing.stdout, Vec::<u8>::new());
    let evidence_path = fixture.dir.path().join("effects.json");
    write(&evidence_path, &evidence);
    let record = fixture
        .command()
        .args(["--record-settlement", id.as_str()])
        .arg(&evidence_path)
        .output()
        .expect("record");
    assert!(
        record.status.success(),
        "{}",
        String::from_utf8_lossy(&record.stderr)
    );
    let history = fixture.dir.path().join("operations.jsonl");
    let saved = std::fs::read(&history).expect("history");
    for _ in 0..2 {
        let result = review();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: Value = serde_json::from_slice(&result.stdout).expect("report");
        assert_eq!(report["operation"], id.as_str());
        assert_eq!(report["wallet_net_debit_micro_usd"], "1000");
        assert_eq!(report["network_fee_micro_usd"], "1000");
        assert_eq!(report["valuation_as_of_slot"], "1000");
        assert_eq!(report["realised_pnl_micro_usd"], Value::Null);
        assert_eq!(report["portfolio_state_updated"], false);
        assert_eq!(report["operation_reconciled"], false);
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    }
    for (field, bad) in [
        ("micro_usd_per_sol", json!("0")),
        ("as_of_slot", json!("1002")),
        ("as_of_unix_secs", json!((time + 1).to_string())),
        ("asset", json!("usdc")),
    ] {
        let mut bad_price = price.clone();
        bad_price[field] = bad;
        write(&path, &bad_price);
        let result = review();
        assert!(!result.status.success());
        assert_eq!(result.stdout, Vec::<u8>::new());
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    }
    write(&path, &price);
    let incomplete = fixture
        .command()
        .args(["--record-valuation", id.as_str()])
        .arg(&path)
        .output()
        .expect("incomplete costs");
    assert!(!incomplete.status.success());
    assert_eq!(incomplete.stdout, Vec::<u8>::new());
    assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    let log = radar_journal::OperationLog::open(&history).expect("replay");
    assert_eq!(log.outstanding().count(), 1);
    drop(log);
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
}

fn finalized_fixture() -> (Fixture, Vec<u8>, radar_journal::OperationId, Value) {
    finalized_fixture_with_opening(false)
}

#[test]
fn protected_failed_fee_costs_are_durable_without_releasing_or_closing_claims() {
    let (fixture, _, id, mut evidence) = finalized_fixture();
    let time = evidence["read_completed_at_unix_secs"]
        .as_u64()
        .expect("time")
        - 1;
    evidence["block_time_unix_secs"] = json!(time.to_string());
    let effects_path = fixture.dir.path().join("effects.json");
    write(&effects_path, &evidence);
    assert!(
        fixture
            .command()
            .args(["--record-settlement", id.as_str()])
            .arg(&effects_path)
            .output()
            .expect("settlement")
            .status
            .success()
    );
    let price_path = fixture.dir.path().join("price.json");
    let price = json!({"version":1,"asset":"sol","micro_usd_per_sol":"200000003",
        "as_of_slot":"1000","as_of_unix_secs":time.to_string()});
    write(&price_path, &price);
    let run = |mode| {
        fixture
            .command()
            .args([mode, id.as_str()])
            .arg(&price_path)
            .output()
            .expect("valuation")
    };
    let history = fixture.dir.path().join("operations.jsonl");
    let before = std::fs::read(&history).expect("history");
    let reviewed = run("--review-valuation");
    assert!(
        reviewed.status.success(),
        "{}",
        String::from_utf8_lossy(&reviewed.stderr)
    );
    let report: Value = serde_json::from_slice(&reviewed.stdout).expect("report");
    assert_eq!(
        report["failed_execution_costs"]["network_fee_micro_usd"],
        "1001"
    );
    assert_eq!(
        report["failed_execution_costs"]["network_fee_lamports"],
        "5000"
    );
    assert_eq!(report["acquisition_costs"], Value::Null);
    assert_eq!(report["position_cost_basis_micro_usd"], Value::Null);
    assert_eq!(report["realised_pnl_micro_usd"], Value::Null);
    assert_eq!(report["portfolio_state_updated"], false);
    assert_eq!(report["reservation_released"], false);
    assert_eq!(std::fs::read(&history).expect("unchanged"), before);
    assert!(run("--record-valuation").status.success());
    let saved = std::fs::read(&history).expect("recorded");
    assert_ne!(saved, before);
    for _ in 0..2 {
        assert!(run("--record-valuation").status.success());
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
        let log = radar_journal::OperationLog::open(&history).expect("replay");
        assert_eq!(log.valuation(&id).expect("fee record").review, report);
        assert_eq!(log.outstanding().count(), 1);
    }
    let mut changed = price;
    changed["micro_usd_per_sol"] = json!("200000004");
    write(&price_path, &changed);
    assert!(!run("--record-valuation").status.success());
    assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
    // The acquisition-only history reader still refuses this separate cost category.
    assert!(
        !fixture
            .command()
            .arg("--review-acquisitions")
            .output()
            .expect("history review")
            .status
            .success()
    );
}

#[test]
fn protected_failed_fee_record_refuses_unexplained_native_or_token_changes() {
    for case in [
        "extra_debit",
        "other_native",
        "token_change",
        "missing_pair",
    ] {
        let (fixture, _, id, mut evidence) = finalized_fixture();
        let time = evidence["read_completed_at_unix_secs"]
            .as_u64()
            .expect("time")
            - 1;
        evidence["block_time_unix_secs"] = json!(time.to_string());
        match case {
            "extra_debit" => evidence["post_balances_lamports"][0] = json!("299994999"),
            "other_native" => evidence["post_balances_lamports"][1] = json!("1"),
            _ => {
                let pre = json!({"account_index":1,"mint":fixture.snapshot["proposal"]["mint"],
                    "owner":fixture.config["wallet"],"program_id":address(0x44),"decimals":6,"raw_amount":"10"});
                let mut post = pre.clone();
                post["raw_amount"] = json!("11");
                evidence["pre_token_balances"] = json!([pre]);
                evidence["post_token_balances"] = if case == "missing_pair" {
                    json!([])
                } else {
                    json!([post])
                };
            }
        }
        let effects_path = fixture.dir.path().join("effects.json");
        write(&effects_path, &evidence);
        assert!(
            fixture
                .command()
                .args(["--record-settlement", id.as_str()])
                .arg(&effects_path)
                .output()
                .expect("settlement")
                .status
                .success()
        );
        let price_path = fixture.dir.path().join("price.json");
        write(
            &price_path,
            &json!({"version":1,"asset":"sol","micro_usd_per_sol":"200000000",
            "as_of_slot":"1000","as_of_unix_secs":time.to_string()}),
        );
        let history = fixture.dir.path().join("operations.jsonl");
        let saved = std::fs::read(&history).expect("history");
        for mode in ["--review-valuation", "--record-valuation"] {
            let output = fixture
                .command()
                .args([mode, id.as_str()])
                .arg(&price_path)
                .output()
                .expect("refusal");
            assert!(!output.status.success(), "{case}");
            assert_eq!(output.stdout, Vec::<u8>::new());
            assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
        }
    }
}

fn finalized_fixture_with_opening(
    with_opening: bool,
) -> (Fixture, Vec<u8>, radar_journal::OperationId, Value) {
    use ed25519_dalek::Signer as _;
    let key = ed25519_dalek::SigningKey::from_bytes(&[0x42; 32]);
    let (mut fixture, mut signed) = fixture_for_wallet(&key);
    if with_opening {
        set_inventory(&mut fixture, 10, 1000);
        assert!(record_opening(&fixture).status.success());
        let log = radar_journal::OperationLog::open(fixture.dir.path().join("operations.jsonl"))
            .expect("opening history");
        fixture.snapshot["accounting_checkpoint"] = json!(log.checkpoint());
        drop(log);
        fixture.save();
    }
    let mut issuer = fixture.start();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
    drop(issuer);
    let signature = key.sign(&signed[65..]).to_bytes();
    signed[1..65].copy_from_slice(&signature);
    let history = fixture.dir.path().join("operations.jsonl");
    let mut log = radar_journal::OperationLog::open(&history).expect("history");
    let id = log.outstanding().next().expect("operation").0.clone();
    log.record_signed(&id, radar_types::b64::encode(&signed), unix_now())
        .expect("fixture signed binding");
    drop(log);
    let evidence = json!({"version":1,"authority":"read_only","commitment":"finalized","wallet":fixture.config["wallet"],
        "transaction_base64":radar_types::b64::encode(&signed),"signature":radar_types::Signature::new(signature).to_string(),
        "signature_verified_locally":false,"operation_reconciled":false,"usd_value":null,"realised_pnl":null,
        "outcome":"failed","slot":"1001","minimum_slot":"1000","read_started_at_unix_secs":unix_now(),"read_completed_at_unix_secs":unix_now(),
        "account_keys":[fixture.config["wallet"],address(0x22),address(0x11)],"pre_balances_lamports":["300000000","0","0"],
        "post_balances_lamports":["299995000","0","0"],"network_fee_lamports":"5000","pre_token_balances":[],"post_token_balances":[],
        "provider_response":"UNREVIEWED_RESPONSE_MUST_NOT_PERSIST"});
    (fixture, signed, id, evidence)
}

fn acquisition_cost_fixture() -> (
    Fixture,
    radar_journal::OperationId,
    std::path::PathBuf,
    Value,
) {
    acquisition_cost_fixture_with_opening(false)
}

fn acquisition_cost_fixture_with_opening(
    with_opening: bool,
) -> (
    Fixture,
    radar_journal::OperationId,
    std::path::PathBuf,
    Value,
) {
    let (fixture, signed, id, mut evidence) = finalized_fixture_with_opening(with_opening);
    let time = evidence["read_completed_at_unix_secs"]
        .as_u64()
        .expect("read")
        - 1;
    evidence["block_time_unix_secs"] = json!(time.to_string());
    evidence["outcome"] = json!("succeeded");
    evidence["post_balances_lamports"] = json!(["284844000", "150000", "15001000"]);
    let pre = json!({"account_index":1,"mint":fixture.snapshot["proposal"]["mint"],
        "owner":fixture.config["wallet"],"program_id":address(0x44),"decimals":6,"raw_amount":"10"});
    let mut post = pre.clone();
    post["raw_amount"] = json!("25");
    evidence["pre_token_balances"] = json!([pre]);
    evidence["post_token_balances"] = json!([post]);
    let evidence_path = fixture.dir.path().join("effects.json");
    write(&evidence_path, &evidence);
    let record = fixture
        .command()
        .args(["--record-settlement", id.as_str()])
        .arg(&evidence_path)
        .output()
        .expect("record");
    assert!(
        record.status.success(),
        "{}",
        String::from_utf8_lossy(&record.stderr)
    );
    let price_path = fixture.dir.path().join("price.json");
    let price = json!({"version":1,"asset":"sol","micro_usd_per_sol":"200000000",
        "as_of_slot":"1000","as_of_unix_secs":time.to_string(),"acquisition_costs":{
            "version":1,"operation":id.as_str(),"signed_transaction":radar_types::b64::encode(&signed),
            "wallet":fixture.config["wallet"],"mint":fixture.snapshot["proposal"]["mint"],
            "token_program":address(0x44),"decimals":6,"net_acquired_raw":"15",
            "swap_lamports":"15000000","rent_lamports":"150000","tip_lamports":"1000",
            "other_cash_flows_absent":true}});
    write(&price_path, &price);
    (fixture, id, price_path, price)
}

#[test]
fn protected_acquisition_cost_review_accounts_for_exact_outlay_without_settling() {
    let (fixture, id, price_path, price) = acquisition_cost_fixture();
    let history = fixture.dir.path().join("operations.jsonl");
    let saved = std::fs::read(&history).expect("history");
    let review = || {
        fixture
            .command()
            .args(["--review-valuation", id.as_str()])
            .arg(&price_path)
            .output()
            .expect("valuation")
    };
    for _ in 0..2 {
        let result = review();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: Value = serde_json::from_slice(&result.stdout).expect("report");
        assert_eq!(report["trade_notional_micro_usd"], "3000000");
        assert_eq!(report["position_cost_basis_micro_usd"], "3001200");
        assert_eq!(report["acquisition_costs"]["rent_micro_usd"], "30000");
        assert_eq!(report["wallet_net_debit_micro_usd"], "3031200");
        assert_eq!(report["realised_pnl_micro_usd"], Value::Null);
        assert_eq!(report["portfolio_state_updated"], false);
        assert_eq!(report["operation_reconciled"], false);
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    }
    for (field, value) in [
        ("operation", json!("foreign")),
        ("net_acquired_raw", json!("14")),
        ("other_cash_flows_absent", json!(false)),
        ("swap_lamports", json!("15000001")),
    ] {
        let mut bad = price.clone();
        bad["acquisition_costs"][field] = value;
        write(&price_path, &bad);
        let result = review();
        assert!(!result.status.success());
        assert_eq!(result.stdout, Vec::<u8>::new());
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    }
    let log = radar_journal::OperationLog::open(&history).expect("replay");
    assert_eq!(log.outstanding().count(), 1);
    drop(log);
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
}

#[test]
fn protected_cost_record_is_durable_idempotent_and_refuses_changed_economics() {
    let (fixture, id, price_path, price) = acquisition_cost_fixture();
    let run = |mode| {
        fixture
            .command()
            .args([mode, id.as_str()])
            .arg(&price_path)
            .output()
            .expect("issuer mode")
    };
    let reviewed = run("--review-valuation");
    assert!(reviewed.status.success());
    let expected: Value = serde_json::from_slice(&reviewed.stdout).expect("review");
    let history = fixture.dir.path().join("operations.jsonl");
    let mut log = radar_journal::OperationLog::open(&history).expect("log");
    assert_eq!(log.valuation(&id), None);
    let checkpoint = log.checkpoint().to_owned();
    let settlement = log.settlement(&id).expect("facts").clone();
    drop(log);
    let result = run("--record-valuation");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: Value = serde_json::from_slice(&result.stdout).expect("record result");
    assert_eq!(result["valuation_recorded"], true);
    assert_eq!(result["portfolio_state_updated"], false);
    assert_eq!(result["reconciled"], false);
    assert_eq!(result["reservation_released"], false);
    log = radar_journal::OperationLog::open(&history).expect("replay");
    assert_ne!(log.checkpoint(), checkpoint);
    assert_eq!(
        log.valuation(&id),
        Some(&radar_journal::ValuationRecord {
            settlement,
            review: expected
        })
    );
    assert_eq!(log.outstanding().count(), 1);
    drop(log);
    let saved = std::fs::read(&history).expect("history");
    assert!(run("--record-valuation").status.success());
    assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    let text = String::from_utf8(saved.clone()).expect("text");
    assert!(!text.contains("UNREVIEWED_RESPONSE_MUST_NOT_PERSIST"));
    assert!(!text.contains("other_cash_flows_absent"));
    for price_changed in [false, true] {
        let mut conflict = price.clone();
        if price_changed {
            conflict["micro_usd_per_sol"] = json!("201000000");
        } else {
            conflict["acquisition_costs"]["swap_lamports"] = json!("14999999");
            conflict["acquisition_costs"]["rent_lamports"] = json!("150001");
        }
        write(&price_path, &conflict);
        assert!(
            run("--review-valuation").status.success(),
            "valid conflicting review"
        );
        let result = run("--record-valuation");
        assert!(!result.status.success());
        assert_eq!(result.stdout, Vec::<u8>::new());
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    }
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
}

fn acquisition_report(fixture: &Fixture) -> std::process::Output {
    fixture
        .command()
        .arg("--review-acquisitions")
        .output()
        .expect("acquisition report")
}

fn inventory_fixture() -> Fixture {
    let (mut fixture, id, price_path, _) = acquisition_cost_fixture();
    assert!(
        fixture
            .command()
            .args(["--record-valuation", id.as_str()])
            .arg(price_path)
            .output()
            .expect("record costs")
            .status
            .success()
    );
    let log = radar_journal::OperationLog::open(fixture.dir.path().join("operations.jsonl"))
        .expect("history");
    fixture.snapshot["accounting_checkpoint"] = json!(log.checkpoint());
    drop(log);
    fixture.snapshot["state"]["now"] = json!(1003);
    let evidence = &mut fixture.snapshot["wallet_evidence"];
    evidence["native_sol"]["slot"] = json!("1002");
    evidence["token_program"]["slot"] = json!("1002");
    evidence["token_2022"]["slot"] = json!("1003");
    let account = json!({"address":address(0x66),"mint":address(0x22),"program":address(0x44),
        "decimals":6,"state":"frozen","raw_amount":"10","owner":fixture.config["wallet"],"spendable":null});
    let mut second = account.clone();
    second["address"] = json!(address(0x67));
    second["raw_amount"] = json!("5");
    evidence["token_program"]["accounts"] = json!([account, second]);
    evidence["raw_token_verification"] = json!({"authority":"read_only","inventory_complete":false,
        "slot":"1003","accounts":[account,second]});
    fixture.save();
    fixture
}

fn inventory_report(fixture: &Fixture) -> std::process::Output {
    fixture
        .command()
        .arg("--review-inventory")
        .output()
        .expect("inventory report")
}

fn record_opening(fixture: &Fixture) -> std::process::Output {
    fixture
        .command()
        .arg("--record-opening-inventory")
        .output()
        .expect("record opening")
}

fn set_inventory(fixture: &mut Fixture, raw: u64, slot: u64) {
    let mint = fixture.snapshot["proposal"]["mint"].clone();
    let evidence = &mut fixture.snapshot["wallet_evidence"];
    for read in ["native_sol", "token_program", "token_2022"] {
        evidence[read]["slot"] = json!(slot.to_string());
    }
    let account = json!({"address":address(0x66),"mint":mint,"program":address(0x44),
        "decimals":6,"state":"initialized","raw_amount":raw.to_string(),"owner":fixture.config["wallet"]});
    evidence["token_program"]["accounts"] = json!([account]);
    evidence["raw_token_verification"] = json!({"authority":"read_only","inventory_complete":false,"slot":slot.to_string(),"accounts":[account]});
    fixture.snapshot["state"]["now"] = json!(slot);
    fixture.save();
}

#[test]
fn protected_opening_record_is_normalized_immutable_and_never_infers_cost_basis() {
    let mut fixture = Fixture::new();
    let missing = record_opening(&fixture);
    assert!(!missing.status.success());
    assert_eq!(missing.stdout, Vec::<u8>::new());
    set_inventory(&mut fixture, 10, 1000);
    fixture.snapshot["wallet_evidence"]["extra"] = json!("MUST_NOT_PERSIST");
    fixture.save();
    let result = record_opening(&fixture);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let history = fixture.dir.path().join("operations.jsonl");
    let saved = std::fs::read(&history).expect("opening history");
    assert!(!String::from_utf8_lossy(&saved).contains("MUST_NOT_PERSIST"));
    assert!(record_opening(&fixture).status.success());
    assert_eq!(std::fs::read(&history).expect("same"), saved);
    let log = radar_journal::OperationLog::open(&history).expect("replay");
    let opening = log.opening_inventory().expect("opening");
    assert_eq!(opening.wallet.to_string(), fixture.config["wallet"]);
    assert_eq!(opening.native_lamports, 300_000_000);
    assert_eq!(opening.holdings[0].raw_amount, 10);
    assert_eq!(opening.raw_token_slot, Some(radar_types::Slot(1000)));
    let checkpoint = log.checkpoint().to_owned();
    drop(log);
    set_inventory(&mut fixture, 11, 1000);
    let conflict = record_opening(&fixture);
    assert!(!conflict.status.success());
    assert_eq!(conflict.stdout, Vec::<u8>::new());
    assert_eq!(std::fs::read(&history).expect("same"), saved);
    set_inventory(&mut fixture, 10, 1000);
    fixture.snapshot["accounting_checkpoint"] = json!(checkpoint);
    fixture.save();
    let output = inventory_report(&fixture);
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("report");
    assert_eq!(report["tokens_by_mint"][0]["opening_raw"], "10");
    assert_eq!(report["tokens_by_mint"][0]["retained_acquired_raw"], "0");
    assert_eq!(report["tokens_by_mint"][0]["expected_raw"], "10");
    assert!(report["opening_cost_basis_micro_usd"].is_null());
    assert_eq!(report["wallet_inventory_complete"], false);
    assert_eq!(report["portfolio_state_updated"], false);
    assert_eq!(std::fs::read(&history).expect("same"), saved);
}

#[test]
fn opening_plus_retained_buys_compare_once_without_claiming_loss_or_coverage() {
    let (mut fixture, id, price_path, _) = acquisition_cost_fixture_with_opening(true);
    assert!(
        fixture
            .command()
            .args(["--record-valuation", id.as_str()])
            .arg(price_path)
            .output()
            .expect("record costs")
            .status
            .success()
    );
    let history = fixture.dir.path().join("operations.jsonl");
    let saved = std::fs::read(&history).expect("history");
    let log = radar_journal::OperationLog::open(&history).expect("log");
    fixture.snapshot["accounting_checkpoint"] = json!(log.checkpoint());
    drop(log);
    for raw in [20u64, 25, 30] {
        set_inventory(&mut fixture, raw, 1003);
        let output = inventory_report(&fixture);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).expect("report");
        let row = &report["tokens_by_mint"][0];
        assert_eq!(row["opening_raw"], "10");
        assert_eq!(row["retained_acquired_raw"], "15");
        assert_eq!(row["expected_raw"], "25");
        assert_eq!(row["observed_raw"], raw.to_string());
        assert_eq!(
            row["unexplained_excess_raw"],
            raw.saturating_sub(25).to_string()
        );
        assert_eq!(
            row["unaccounted_reduction_raw"],
            25u64.saturating_sub(raw).to_string()
        );
        assert_eq!(
            report["acquisition_history"]["lots"]
                .as_array()
                .expect("lots")
                .len(),
            1
        );
        assert!(report["opening_cost_basis_micro_usd"].is_null());
        assert!(report["current_exposure_micro_usd"].is_null());
        assert!(report["realised_loss_today_micro_usd"].is_null());
        assert_eq!(report["economic_reconciliation_complete"], false);
        assert_eq!(report["reservation_released"], false);
        assert_eq!(inventory_report(&fixture).stdout, output.stdout, "restart");
        assert_eq!(std::fs::read(&history).expect("same"), saved);
    }
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
    fixture.snapshot["accounting_checkpoint"] = json!("");
    fixture.save();
    assert!(
        !record_opening(&fixture).status.success(),
        "cannot replace opening after trade"
    );
    assert_eq!(std::fs::read(&history).expect("same"), saved);
}

#[test]
fn protected_inventory_comparison_reports_differences_without_releasing_or_flattening() {
    let mut fixture = inventory_fixture();
    let history = fixture.dir.path().join("operations.jsonl");
    let saved = std::fs::read(&history).expect("history");
    for amount in [5u64, 10, 15] {
        for read in ["token_program", "raw_token_verification"] {
            fixture.snapshot["wallet_evidence"][read]["accounts"][0]["raw_amount"] =
                json!(amount.to_string());
        }
        fixture.snapshot["wallet_evidence"]["provider_detail"] = json!("MUST_NOT_APPEAR");
        fixture.save();
        let output = inventory_report(&fixture);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("MUST_NOT_APPEAR"));
        let report: Value = serde_json::from_slice(&output.stdout).expect("report");
        let row = &report["tokens_by_mint"][0];
        assert_eq!(row["retained_acquired_raw"], "15");
        assert_eq!(row["observed_raw"], (amount + 5).to_string());
        assert_eq!(
            row["unexplained_excess_raw"],
            (amount + 5).saturating_sub(15).to_string()
        );
        assert_eq!(
            row["unaccounted_reduction_raw"],
            15u64.saturating_sub(amount + 5).to_string()
        );
        assert_eq!(row["quantity_matches"], amount == 10);
        assert_eq!(report["native_sol"]["raw_amount"], "300000000");
        assert_eq!(report["token_program_slot"], "1002");
        assert_eq!(report["token_2022_slot"], "1003");
        assert_eq!(report["raw_token_slot"], "1003");
        assert_eq!(
            report["accounting_checkpoint"],
            fixture.snapshot["accounting_checkpoint"]
        );
        for field in [
            "opening_inventory",
            "current_exposure_micro_usd",
            "realised_loss_today_micro_usd",
        ] {
            assert!(report[field].is_null(), "{field}");
        }
        for field in [
            "wallet_inventory_complete",
            "portfolio_state_updated",
            "economic_reconciliation_complete",
            "reservation_released",
        ] {
            assert_eq!(report[field], false, "{field}");
        }
        assert_eq!(
            inventory_report(&fixture).stdout,
            output.stdout,
            "restart/repeat"
        );
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    }
    for read in ["token_program", "token_2022", "raw_token_verification"] {
        fixture.snapshot["wallet_evidence"][read]["accounts"] = json!([]);
    }
    fixture.snapshot["wallet_evidence"]["raw_token_verification"]["slot"] = Value::Null;
    fixture.save();
    let output = inventory_report(&fixture);
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("missing holdings");
    assert_eq!(
        report["tokens_by_mint"][0]["unaccounted_reduction_raw"],
        "15"
    );
    assert_eq!(report["wallet_inventory_complete"], false);
    let mut empty = Fixture::new();
    empty.snapshot["wallet_evidence"]["raw_token_verification"] = json!({"authority":"read_only",
        "inventory_complete":false,"slot":null,"accounts":[]});
    empty.save();
    let output = inventory_report(&empty);
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("empty history/wallet");
    assert_eq!(report["tokens_by_mint"], json!([]));
    assert_eq!(report["wallet_inventory_complete"], false);
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
}

#[test]
fn inventory_review_refuses_stale_foreign_unbound_or_changed_protected_evidence() {
    let mut fixture = inventory_fixture();
    let original = fixture.snapshot.clone();
    let history = fixture.dir.path().join("operations.jsonl");
    let saved = std::fs::read(&history).expect("history");
    for case in [
        "checkpoint",
        "wallet",
        "snapshot_age",
        "snapshot_future",
        "read_age",
        "read_future",
        "raw_missing",
        "raw_old",
        "raw_future",
        "raw_owner",
        "before_acquisition",
        "quantity",
        "native",
    ] {
        fixture.snapshot = original.clone();
        match case {
            "checkpoint" => fixture.snapshot["accounting_checkpoint"] = json!(""),
            "wallet" => fixture.snapshot["wallet"] = json!(address(9)),
            "snapshot_age" => fixture.snapshot["observed_at_unix_secs"] = json!(unix_now() - 121),
            "snapshot_future" => {
                fixture.snapshot["observed_at_unix_secs"] = json!(unix_now() + 120);
            }
            "read_age" => {
                fixture.snapshot["wallet_evidence"]["read_started_at_unix_secs"] =
                    json!(unix_now() - 121);
            }
            "read_future" => {
                fixture.snapshot["wallet_evidence"]["read_completed_at_unix_secs"] =
                    json!(unix_now() + 120);
            }
            "raw_missing" => {
                fixture.snapshot["wallet_evidence"]["raw_token_verification"] = Value::Null;
            }
            "raw_old" => {
                fixture.snapshot["wallet_evidence"]["raw_token_verification"]["slot"] =
                    json!("1002");
            }
            "raw_future" => {
                fixture.snapshot["wallet_evidence"]["raw_token_verification"]["slot"] =
                    json!("1004");
            }
            "raw_owner" => {
                fixture.snapshot["wallet_evidence"]["raw_token_verification"]["accounts"][0]["owner"] =
                    json!(address(9));
            }
            "before_acquisition" => {
                fixture.snapshot["wallet_evidence"]["token_program"]["slot"] = json!("1000");
            }
            "quantity" => {
                fixture.snapshot["wallet_evidence"]["raw_token_verification"]["accounts"][0]["raw_amount"] =
                    json!("11");
            }
            "native" => {
                fixture.snapshot["wallet_evidence"]["native_sol"]["raw_amount"] = json!("1");
            }
            _ => unreachable!("case"),
        }
        fixture.save();
        let output = inventory_report(&fixture);
        assert!(!output.status.success(), "{case}");
        assert_eq!(output.stdout, Vec::<u8>::new(), "{case}");
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    }
}

fn native_portfolio(fixture: &Fixture) -> radar_types::Portfolio {
    use radar_types::{
        Asset, AssetRole, Balance, Holding, Portfolio, Slot, TokenQuantity, Unvaluable, Valuation,
    };
    let wallet = serde_json::from_value(fixture.config["wallet"].clone()).expect("wallet");
    let mut portfolio = Portfolio::at(wallet, Slot(1000));
    portfolio
        .hold(
            Asset::Sol,
            Holding::new(
                AssetRole::Cash,
                Balance::Counted(TokenQuantity::lamports(300_000_000)),
                Valuation::Unknown(Unvaluable::NoPrice),
                Valuation::Unknown(Unvaluable::NoPrice),
            ),
        )
        .expect("cash");
    portfolio
}

#[test]
fn acquisition_history_includes_completed_lots_once_and_keeps_wallet_risk_unknown() {
    let (fixture, id, price_path, _) = acquisition_cost_fixture();
    assert!(
        fixture
            .command()
            .args(["--record-valuation", id.as_str()])
            .arg(price_path)
            .output()
            .expect("record costs")
            .status
            .success()
    );
    let history = fixture.dir.path().join("operations.jsonl");
    let saved = std::fs::read(&history).expect("history");
    let result = acquisition_report(&fixture);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let first: Value = serde_json::from_slice(&result.stdout).expect("report");
    assert_eq!(first["lots"].as_array().expect("lots").len(), 1);
    assert_eq!(first["lots"][0]["operation"], id.as_str());
    assert_eq!(
        first["lots"][0]["creator"],
        fixture.snapshot["proposal"]["creator"]
    );
    let group = &first["acquisitions_by_mint_and_creator"][0];
    assert_eq!(group["net_acquired_raw"], "15");
    assert_eq!(group["position_cost_basis_micro_usd"], "3001200");
    assert_eq!(group["rent_micro_usd"], "30000");
    assert_eq!(group["oldest_valuation_slot"], "1000");
    assert_eq!(first["wallet_inventory_complete"], false);
    assert_eq!(first["current_exposure_micro_usd"], Value::Null);
    assert_eq!(first["realised_loss_today_micro_usd"], Value::Null);
    assert_eq!(first["portfolio_state_updated"], false);
    assert_eq!(first["economic_reconciliation_complete"], false);
    assert_eq!(first["reservation_released"], false);
    assert_eq!(
        serde_json::from_slice::<Value>(&acquisition_report(&fixture).stdout).expect("restart"),
        first
    );
    assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
    let mut log = radar_journal::OperationLog::open(&history).expect("log");
    let mut portfolio = native_portfolio(&fixture);
    log.rehold(&mut portfolio).expect("rehold");
    log.reconcile(
        &id,
        radar_types::Settlement::Completed(radar_types::TokenQuantity::lamports(15_156_000)),
        &mut portfolio,
        1_010,
    )
    .expect("generic completion");
    let checkpoint = log.checkpoint().to_owned();
    assert_eq!(log.outstanding().count(), 0);
    drop(log);
    let saved = std::fs::read(&history).expect("terminal history");
    let result = acquisition_report(&fixture);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let after: Value = serde_json::from_slice(&result.stdout).expect("terminal report");
    assert_eq!(after["lots"], first["lots"]);
    assert_eq!(
        after["acquisitions_by_mint_and_creator"],
        first["acquisitions_by_mint_and_creator"]
    );
    assert_eq!(after["accounting_checkpoint"], checkpoint);
    assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
}

#[test]
fn acquisition_report_refuses_missing_changed_duplicate_or_invalid_retained_economics() {
    use radar_journal::{Correlation, OperationLog};
    for case in [
        "missing",
        "duplicate",
        "signature",
        "normalized",
        "facts_signature",
        "terminal",
    ] {
        let (fixture, id, price_path, _) = acquisition_cost_fixture();
        if case != "missing" {
            assert!(
                fixture
                    .command()
                    .args(["--record-valuation", id.as_str()])
                    .arg(price_path)
                    .output()
                    .expect("costs")
                    .status
                    .success()
            );
        }
        let history = fixture.dir.path().join("operations.jsonl");
        let mut log = OperationLog::open(&history).expect("log");
        if case == "terminal" {
            let mut portfolio = native_portfolio(&fixture);
            log.rehold(&mut portfolio).expect("rehold");
            log.reconcile(
                &id,
                radar_types::Settlement::Completed(radar_types::TokenQuantity::lamports(
                    15_000_000,
                )),
                &mut portfolio,
                1_010,
            )
            .expect("generic mismatched spend");
        } else if case != "missing" {
            let intent = log.entry(&id).expect("entry").intent;
            let mut binding = log.execution(&id).expect("binding").clone();
            let mut value = log.valuation(&id).expect("value").clone();
            if case == "normalized" || case == "facts_signature" {
                let (unsigned, signed) = another_fixture_transaction(&binding);
                binding.transaction = unsigned;
                binding.signed_transaction = Some(signed.clone());
                value.settlement.signed_transaction = signed;
                value.settlement.review["signature"] =
                    fixture_signature(&value.settlement.signed_transaction);
            }
            if case == "signature" {
                let mut bytes =
                    radar_types::b64::decode(binding.signed_transaction.as_ref().expect("signed"))
                        .expect("bytes");
                bytes[1] ^= 1;
                binding.signed_transaction = Some(radar_types::b64::encode(&bytes));
                value.settlement.signed_transaction =
                    binding.signed_transaction.clone().expect("signed");
            }
            let signed = binding.signed_transaction.take().expect("signed");
            let second = log
                .propose(
                    intent,
                    1_011,
                    Correlation {
                        execution: Some(binding),
                        ..Correlation::default()
                    },
                )
                .expect("second operation");
            let mut portfolio = native_portfolio(&fixture);
            log.reserve(&second, &mut portfolio, 1_012)
                .expect("reserve");
            log.submit(&second, 1_013, |_| Ok::<(), ()>(()))
                .expect("submit")
                .expect("effect");
            log.record_signed(&second, signed, 1_014).expect("signed");
            value.settlement.review["operation"] = json!(second.as_str());
            value.review["operation"] = json!(second.as_str());
            if case == "normalized" {
                value.review["position_cost_basis_micro_usd"] = json!("3001201");
            }
            if case == "facts_signature" {
                value.settlement.review["signature"] = json!("another signature");
            }
            log.record_settlement(&second, value.settlement.clone(), 1_015)
                .expect("facts");
            log.record_valuation(&second, value, 1_016)
                .expect("generic costs");
        }
        drop(log);
        let saved = std::fs::read(&history).expect("history");
        let result = acquisition_report(&fixture);
        assert!(!result.status.success(), "{case}");
        assert_eq!(result.stdout, Vec::<u8>::new(), "{case}");
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    }
}

fn another_fixture_transaction(binding: &radar_journal::ExecutionBinding) -> (String, String) {
    use ed25519_dalek::Signer as _;
    let mut unsigned = radar_types::b64::decode(&binding.transaction).expect("unsigned");
    // This fixture has three legacy account keys; change only its blockhash.
    unsigned[69 + 3 * 32] ^= 1;
    let mut signed = unsigned.clone();
    let signature = ed25519_dalek::SigningKey::from_bytes(&[0x42; 32])
        .sign(&unsigned[65..])
        .to_bytes();
    signed[1..65].copy_from_slice(&signature);
    (
        radar_types::b64::encode(&unsigned),
        radar_types::b64::encode(&signed),
    )
}

fn fixture_signature(signed: &str) -> Value {
    let bytes = radar_types::b64::decode(signed).expect("bytes");
    json!(radar_types::Signature::new(bytes[1..65].try_into().expect("signature")).to_string())
}

#[test]
fn distinct_signed_acquisitions_are_both_counted_after_restart() {
    let (fixture, id, price_path, _) = acquisition_cost_fixture();
    assert!(
        fixture
            .command()
            .args(["--record-valuation", id.as_str()])
            .arg(price_path)
            .output()
            .expect("costs")
            .status
            .success()
    );
    let history = fixture.dir.path().join("operations.jsonl");
    let mut log = radar_journal::OperationLog::open(&history).expect("log");
    let intent = log.entry(&id).expect("entry").intent;
    let mut binding = log.execution(&id).expect("binding").clone();
    let mut value = log.valuation(&id).expect("value").clone();
    let (unsigned, signed) = another_fixture_transaction(&binding);
    binding.transaction = unsigned;
    binding.signed_transaction = None;
    value.settlement.signed_transaction = signed;
    value.settlement.review["signature"] = fixture_signature(&value.settlement.signed_transaction);
    let second = log
        .propose(
            intent,
            1_011,
            radar_journal::Correlation {
                execution: Some(binding),
                ..Default::default()
            },
        )
        .expect("second");
    let mut portfolio = native_portfolio(&fixture);
    log.reserve(&second, &mut portfolio, 1_012)
        .expect("reserve");
    log.submit(&second, 1_013, |_| Ok::<(), ()>(()))
        .expect("submit")
        .expect("effect");
    log.record_signed(&second, value.settlement.signed_transaction.clone(), 1_014)
        .expect("signed");
    value.settlement.review["operation"] = json!(second.as_str());
    value.review["operation"] = json!(second.as_str());
    log.record_settlement(&second, value.settlement.clone(), 1_015)
        .expect("facts");
    log.record_valuation(&second, value, 1_016).expect("costs");
    drop(log);
    let saved = std::fs::read(&history).expect("history");
    let result = acquisition_report(&fixture);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).expect("report");
    assert_eq!(report["lots"].as_array().expect("lots").len(), 2);
    assert_eq!(
        report["acquisitions_by_mint_and_creator"][0]["net_acquired_raw"],
        "30"
    );
    assert_eq!(
        report["acquisitions_by_mint_and_creator"][0]["position_cost_basis_micro_usd"],
        "6002400"
    );
    assert_eq!(
        report["acquisitions_by_mint_and_creator"][0]["rent_micro_usd"],
        "60000"
    );
    assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
}

#[test]
fn empty_or_unsubmitted_history_does_not_claim_flat_wallet_inventory() {
    for stage in ["empty", "proposed", "reserved", "failed"] {
        let fixture = Fixture::new();
        let history = fixture.dir.path().join("operations.jsonl");
        let mut log = radar_journal::OperationLog::open(&history).expect("log");
        if stage != "empty" {
            let id = log
                .propose(
                    radar_journal::Intent {
                        asset: radar_types::Asset::Sol,
                        amount: radar_types::TokenQuantity::lamports(1_000),
                        at: radar_types::Slot(1000),
                    },
                    1_000,
                    radar_journal::Correlation {
                        mention: Some("unsubmitted".into()),
                        ..Default::default()
                    },
                )
                .expect("propose");
            let mut portfolio = native_portfolio(&fixture);
            if stage == "reserved" {
                log.reserve(&id, &mut portfolio, 1_001).expect("reserve");
            }
            if stage == "failed" {
                log.fail(&id, &mut portfolio, 1_002).expect("fail");
            }
        }
        let checkpoint = log.checkpoint().to_owned();
        drop(log);
        let saved = std::fs::read(&history).expect("history");
        let result = acquisition_report(&fixture);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result: Value = serde_json::from_slice(&result.stdout).expect("report");
        assert_eq!(result["lots"], json!([]));
        assert_eq!(
            result["unsubmitted_operations"]
                .as_array()
                .expect("unsubmitted")
                .len(),
            usize::from(stage != "empty")
        );
        assert_eq!(result["accounting_checkpoint"], checkpoint);
        assert_eq!(result["wallet_inventory_complete"], false);
        assert_eq!(result["current_exposure_micro_usd"], Value::Null);
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    }
}

#[test]
fn recorded_token_acquisition_is_measured_or_unknown_and_keeps_claims_outstanding() {
    for case in ["paired", "missing_pre", "failed"] {
        let (fixture, _, id, mut evidence) = finalized_fixture();
        evidence["outcome"] = json!(if case == "failed" {
            "failed"
        } else {
            "succeeded"
        });
        let pre = json!({"account_index":1,"mint":fixture.snapshot["proposal"]["mint"],
            "owner":fixture.config["wallet"],"program_id":address(0x44),"decimals":6,"raw_amount":"10"});
        let mut post = pre.clone();
        post["raw_amount"] = json!("25");
        evidence["pre_token_balances"] = if case == "missing_pre" {
            json!([])
        } else {
            json!([pre])
        };
        evidence["post_token_balances"] = json!([post]);
        let path = fixture.dir.path().join("effects.json");
        write(&path, &evidence);
        let run = |mode| {
            fixture
                .command()
                .args([mode, id.as_str()])
                .arg(&path)
                .output()
                .expect("issuer mode")
        };
        let result = run("--review-settlement");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: Value = serde_json::from_slice(&result.stdout).expect("review");
        if case == "paired" {
            assert_eq!(report["wallet_token_acquisition"]["net_acquired_raw"], "15");
            assert_eq!(
                report["wallet_token_acquisition"]["mint"],
                fixture.snapshot["proposal"]["mint"]
            );
            assert_eq!(report["wallet_token_acquisition"]["decimals"], 6);
        } else {
            assert_eq!(report["wallet_token_acquisition"], Value::Null);
        }
        assert_eq!(report["usd_value"], Value::Null);
        assert_eq!(report["realised_pnl"], Value::Null);
        assert!(run("--record-settlement").status.success());
        let history = fixture.dir.path().join("operations.jsonl");
        let saved = std::fs::read(&history).expect("history");
        assert!(run("--record-settlement").status.success());
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
        let log = radar_journal::OperationLog::open(&history).expect("replay");
        assert_eq!(log.settlement(&id).expect("facts").review, report);
        assert_eq!(log.outstanding().count(), 1);
        drop(log);
        assert_eq!(
            fixture.start().ask(&fixture.candidate)["reason"],
            "outstanding operation requires reconciliation"
        );
    }
}

#[test]
fn settlement_review_binds_exact_evidence_and_keeps_the_journal_outstanding() {
    let (fixture, signed, id, evidence) = finalized_fixture();
    let history = fixture.dir.path().join("operations.jsonl");
    let path = fixture.dir.path().join("settlement.json");
    let saved = std::fs::read(&history).expect("history bytes");
    write(&path, &evidence);
    let result = fixture
        .command()
        .args(["--review-settlement", id.as_str()])
        .arg(&path)
        .output()
        .expect("review");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).expect("report");
    assert_eq!(report["operation"], id.as_str());
    assert_eq!(report["wallet_net_change_lamports"], "-5000");
    assert_eq!(report["network_fee_lamports"], "5000");
    assert_eq!(
        report["native_settlement_candidate"],
        serde_json::json!(radar_types::Settlement::Completed(
            radar_types::TokenQuantity::lamports(5_000)
        ))
    );
    assert_eq!(report["reservation_released"], false);
    assert_eq!(report["signature_verified_locally"], true);
    assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    for (field, bad) in [
        ("transaction_base64", json!("different")),
        ("read_completed_at_unix_secs", json!(unix_now() + 600)),
    ] {
        let mut value = evidence.clone();
        value[field] = bad;
        write(&path, &value);
        let result = fixture
            .command()
            .args(["--review-settlement", id.as_str()])
            .arg(&path)
            .output()
            .expect("refused review");
        assert!(!result.status.success());
        assert_eq!(result.stdout, Vec::<u8>::new());
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    }
    let mut log = radar_journal::OperationLog::open(&history).expect("restart");
    assert_eq!(log.outstanding().count(), 1);
    let invalid_id = log
        .propose(
            radar_journal::Intent {
                asset: radar_types::Asset::Sol,
                amount: radar_types::TokenQuantity::lamports(1),
                at: radar_types::Slot(1000),
            },
            unix_now(),
            radar_journal::Correlation {
                execution: Some(radar_journal::ExecutionBinding {
                    wallet: serde_json::from_value(fixture.config["wallet"].clone())
                        .expect("wallet"),
                    transaction: radar_types::b64::encode(&signed),
                    signed_transaction: None,
                    reviewed_proposal: None,
                }),
                ..radar_journal::Correlation::default()
            },
        )
        .expect("another protected test operation");
    drop(log);
    let result = fixture
        .command()
        .args(["--review-settlement", invalid_id.as_str()])
        .arg(&path)
        .output()
        .expect("unknown refusal");
    assert!(!result.status.success());
    assert_eq!(result.stdout, Vec::<u8>::new());
}

#[test]
fn finalized_record_persists_only_normalized_facts_and_keeps_issuance_blocked() {
    let (fixture, signed, id, mut evidence) = finalized_fixture();
    evidence["block_time_unix_secs"] = json!(
        (evidence["read_completed_at_unix_secs"]
            .as_u64()
            .expect("read completion")
            - 1)
        .to_string()
    );
    let history = fixture.dir.path().join("operations.jsonl");
    let path = fixture.dir.path().join("settlement.json");
    let mut saved = std::fs::read(&history).expect("history bytes");
    write(&path, &evidence);
    for iteration in 0..2 {
        let result = fixture
            .command()
            .args(["--record-settlement", id.as_str()])
            .arg(&path)
            .output()
            .expect("record");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result: Value = serde_json::from_slice(&result.stdout).expect("result");
        assert_eq!(result["settlement_evidence_recorded"], true);
        assert_eq!(result["reconciled"], false);
        assert_eq!(result["reservation_released"], false);
        let history_bytes = std::fs::read(&history).expect("history");
        if iteration == 0 {
            assert_ne!(history_bytes, saved);
            let log = radar_journal::OperationLog::open(&history).expect("replay");
            let retained = log.settlement(&id).expect("retained evidence");
            assert_eq!(
                retained.signed_transaction,
                radar_types::b64::encode(&signed)
            );
            assert_eq!(retained.review["network_fee_lamports"], "5000");
            assert_eq!(retained.review["signature_verified_locally"], true);
            assert_eq!(retained.review["minimum_slot"], "1000");
            assert_eq!(
                retained.review["block_time_unix_secs"],
                evidence["block_time_unix_secs"]
            );
            assert_eq!(
                retained.review["read_started_at_unix_secs"],
                evidence["read_started_at_unix_secs"]
            );
            assert_eq!(log.outstanding().count(), 1);
            assert!(
                !String::from_utf8_lossy(&history_bytes)
                    .contains("UNREVIEWED_RESPONSE_MUST_NOT_PERSIST")
            );
            saved = history_bytes;
        } else {
            assert_eq!(history_bytes, saved);
        }
    }
    let mut issuer = fixture.start();
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
    drop(issuer);
    for (field, bad) in [
        ("transaction_base64", json!("different")),
        ("read_completed_at_unix_secs", json!(unix_now() + 600)),
        ("slot", json!("1002")),
        ("block_time_unix_secs", json!("0")),
    ] {
        let mut value = evidence.clone();
        value[field] = bad;
        write(&path, &value);
        let result = fixture
            .command()
            .args(["--record-settlement", id.as_str()])
            .arg(&path)
            .output()
            .expect("refused review");
        assert!(!result.status.success());
        assert_eq!(result.stdout, Vec::<u8>::new());
        assert_eq!(std::fs::read(&history).expect("unchanged"), saved);
    }
}

#[test]
fn signed_binding_mode_requires_the_exact_flag_and_argument_count() {
    let fixture = Fixture::new();
    for args in [
        vec!["--bind-signed"],
        vec!["--bind-signed", "id"],
        vec!["--other", "id", "absent"],
        vec!["--bind-signed", "id", "absent", "extra"],
    ] {
        let result = fixture.command().args(args).output().expect("issuer");
        assert!(!result.status.success());
        assert_eq!(result.stdout, Vec::<u8>::new());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("usage: radar-issuer --bind-signed")
        );
    }
}

fn fixture_for_wallet(wallet_key: &ed25519_dalek::SigningKey) -> (Fixture, Vec<u8>) {
    let mut fixture = Fixture::new();
    let wallet = radar_types::Address::new(wallet_key.verifying_key().to_bytes());
    let mut unsigned =
        radar_types::b64::decode(fixture.candidate["transaction"].as_str().expect("bytes"))
            .expect("decode");
    unsigned[69..101].copy_from_slice(wallet.as_bytes());
    fixture.candidate["transaction"] = json!(radar_types::b64::encode(&unsigned));
    fixture.config["wallet"] = json!(wallet.to_string());
    fixture.snapshot["wallet"] = json!(wallet.to_string());
    fixture.snapshot["wallet_evidence"]["wallet"] = json!(wallet.to_string());
    fixture.snapshot["transaction"] = fixture.candidate["transaction"].clone();
    fixture.snapshot["transaction_evidence"]["transaction_base64"] =
        fixture.candidate["transaction"].clone();
    fixture.snapshot["transaction_evidence"]["message_base64"] =
        json!(radar_types::b64::encode(&unsigned[65..]));
    fixture.save();
    (fixture, unsigned)
}

#[test]
fn signed_bytes_are_verified_and_persisted_without_releasing_the_operation() {
    use ed25519_dalek::Signer as _;
    let wallet_key = ed25519_dalek::SigningKey::from_bytes(&[0x42; 32]);
    let (fixture, unsigned) = fixture_for_wallet(&wallet_key);
    let mut issuer = fixture.start();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
    drop(issuer);
    let history = fixture.dir.path().join("operations.jsonl");
    let log = radar_journal::OperationLog::open(&history).expect("log");
    let id = log.outstanding().next().expect("operation").0.clone();
    assert_eq!(
        log.execution(&id).expect("binding").transaction,
        radar_types::b64::encode(&unsigned)
    );
    drop(log);
    let original_history = std::fs::read(&history).expect("history bytes");
    let mut signed = unsigned.clone();
    let signature = wallet_key.sign(&signed[65..]).to_bytes();
    signed[1..65].copy_from_slice(&signature);
    let path = fixture.dir.path().join("signed.bin");
    for bytes in [
        unsigned.clone(),
        {
            let mut wrong = signed.clone();
            wrong[1] ^= 1;
            wrong
        },
        {
            let mut wrong = signed.clone();
            wrong[133] ^= 1;
            wrong
        },
        {
            let mut wrong = signed.clone();
            wrong[165] ^= 1;
            let signature = wallet_key.sign(&wrong[65..]).to_bytes();
            wrong[1..65].copy_from_slice(&signature);
            wrong
        },
        vec![0; 1233],
    ] {
        std::fs::write(&path, bytes).expect("signed fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("private");
        }
        let result = fixture
            .command()
            .args(["--bind-signed", id.as_str()])
            .arg(&path)
            .output()
            .expect("bind");
        assert!(!result.status.success());
        assert_eq!(result.stdout, Vec::<u8>::new());
        assert_eq!(
            std::fs::read(&history).expect("history unchanged"),
            original_history
        );
    }
    std::fs::write(&path, &signed).expect("signed");
    for _ in 0..2 {
        let result = fixture
            .command()
            .args(["--bind-signed", id.as_str()])
            .arg(&path)
            .output()
            .expect("bind");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let packet: Value = serde_json::from_slice(&result.stdout).expect("packet");
        assert_eq!(
            packet,
            json!({"outcome":"recorded","operation":id.as_str(),"broadcast":false,"reconciled":false})
        );
    }
    let log = radar_journal::OperationLog::open(&history).expect("replay signed");
    assert_eq!(
        log.execution(&id).expect("binding").signed_transaction,
        Some(radar_types::b64::encode(&signed))
    );
    assert_eq!(
        log.execution(&id).expect("binding").reviewed_proposal,
        Some(fixture.snapshot["proposal"].clone())
    );
    assert_eq!(log.outstanding().count(), 1);
    drop(log);
    let mut restarted = fixture.start();
    assert_eq!(
        restarted.ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

fn address(seed: u8) -> String {
    radar_types::Address::new([seed; 32]).to_string()
}

fn write(path: &std::path::Path, value: &Value) {
    std::fs::write(path, value.to_string()).expect("fixture write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("private fixture");
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    config: Value,
    snapshot: Value,
    candidate: Value,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let wallet = address(0x55);
        let mint = address(0x22);
        let mut transaction = vec![1];
        transaction.extend_from_slice(&[0; 64]);
        transaction.extend_from_slice(&[1, 0, 0, 3]);
        transaction.extend_from_slice(&[0x55; 32]);
        transaction.extend_from_slice(&[0x22; 32]);
        transaction.extend_from_slice(&[0x11; 32]);
        transaction.extend_from_slice(&[0xAA; 32]);
        transaction.extend_from_slice(&[1, 2, 2, 0, 1, 2, 0xAB, 0xCD]);
        let proposal = json!({
            "mint":mint, "market":radar_types::Market::PUMP_FUN_BONDING_CURVE,
            "quote":radar_types::Asset::Sol, "creator":address(0x33), "action":"buy",
            "notional":50_000_000, "estimated_round_trip_cost":100_000,
            "oldest_input_slot":1000, "simulated_exit_capacity":100_000_000
        });
        let candidate =
            json!({"proposal":proposal, "transaction":radar_types::b64::encode(&transaction)});
        let snapshot = json!({
            "accounting_checkpoint":"",
            "wallet":wallet, "observed_at_unix_secs":unix_now(), "sol_lamports":300_000_000,
            "sol_upper_micro_usd":200_000_000, "fee_upper_lamports":5000, "state":radar_risk::PortfolioState::flat(radar_types::Slot(1000)),
            "proposal":candidate["proposal"], "transaction":candidate["transaction"],
            "wallet_evidence":{
                "version":1,"authority":"read_only","commitment":"finalized","wallet":wallet,
                "native_sol":{"slot":"1000","raw_amount":"300000000","decimals":9},
                "token_program":{"slot":"1000","accounts":[]},
                "token_2022":{"slot":"1000","accounts":[]},
                "usd_value":null,"realised_pnl":null,"common_reported_slot":"1000",
                "read_started_at_unix_secs":unix_now(),"read_completed_at_unix_secs":unix_now()
            },
            "transaction_evidence":{
                "version":1,"authority":"read_only","commitment":"finalized",
                "minimum_context_slot":"1000","transaction_base64":candidate["transaction"],
                "message_base64":radar_types::b64::encode(&transaction[65..]),
                "simulation":{"slot":"1000","err":null,"signature_verified":false,
                    "blockhash_replaced":false,"units_consumed":null},
                "network_fee":{"slot":"1000","lamports":"5000"},
                "execution_guaranteed":false,"rent_and_other_instruction_costs":null,"usd_value":null,
                "read_started_at_unix_secs":unix_now(),"read_completed_at_unix_secs":unix_now()
            }
        });
        let config = json!({
            "active":true, "wallet":wallet, "app_id":"test-app", "wallet_id":"test-wallet",
            "policy":radar_risk::Policy {
                autonomy:radar_risk::Autonomy::Capped,
                max_position:radar_types::MicroUsd(50_000_000),
                max_deployed:radar_types::MicroUsd(100_000_000),
                max_per_creator:radar_types::MicroUsd(100_000_000),
                max_daily_loss:radar_types::MicroUsd(10_000_000),
                max_round_trip_cost_bps:100, max_canary:radar_types::MicroUsd::ZERO,
                max_input_staleness:radar_types::SlotDelta(10), max_consecutive_failures:2
            },
            "valid_until_unix_secs":unix_now()+600, "snapshot_path":dir.path().join("snapshot.json"),
            "history_path":dir.path().join("operations.jsonl"), "key_path":dir.path().join("issuer.json"),
            "max_snapshot_age_secs":120, "intent_lifetime_secs":60, "fee_reserve_lamports":5000,
            "programs":[address(0x11)]
        });
        let fixture = Self {
            dir,
            config,
            snapshot,
            candidate,
        };
        fixture.save();
        write(&fixture.dir.path().join("issuer.json"), &json!(SEED));
        std::fs::write(fixture.dir.path().join("operations.jsonl"), b"")
            .expect("provision history");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(
                fixture.dir.path().join("operations.jsonl"),
                std::fs::Permissions::from_mode(0o600),
            )
            .expect("private history");
        }
        fixture
    }
    fn save(&self) {
        write(&self.dir.path().join("config.json"), &self.config);
        write(&self.dir.path().join("snapshot.json"), &self.snapshot);
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_radar-issuer"));
        command.env("RADAR_ISSUER_CONFIG", self.dir.path().join("config.json"));
        command
    }
    fn start(&self) -> Process {
        let mut child = self
            .command()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("issuer starts");
        let reader = BufReader::new(child.stdout.take().expect("stdout"));
        let mut process = Process { child, reader };
        let mut ready = String::new();
        process.reader.read_line(&mut ready).expect("ready");
        assert_eq!(
            serde_json::from_str::<Value>(&ready).expect("ready JSON")["outcome"],
            "ready"
        );
        process
    }
}

struct Process {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
}
impl Process {
    fn ask(&mut self, value: &Value) -> Value {
        let stdin = self.child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{value}").expect("request");
        stdin.flush().expect("flush");
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("answer");
        serde_json::from_str(&line).expect("JSON answer")
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.wait();
    }
}

fn raw_response(fixture: &Fixture, raw: &[u8]) -> (String, bool) {
    let mut process = fixture.start();
    let mut stdin = process.child.stdin.take().expect("stdin");
    let _ = stdin.write_all(raw);
    drop(stdin);
    let mut answer = String::new();
    process.reader.read_line(&mut answer).expect("response");
    (answer, process.child.wait().expect("exit").success())
}

#[test]
fn wallet_read_identity_balance_and_complete_contexts_are_required_before_reservation() {
    let mut fixture = Fixture::new();
    let original = fixture.snapshot.clone();
    let mut issuer = fixture.start();
    for pointer in [
        "/version",
        "/authority",
        "/commitment",
        "/wallet",
        "/native_sol/decimals",
        "/native_sol/raw_amount",
        "/native_sol/slot",
        "/token_program/slot",
        "/token_2022/slot",
        "/token_program/accounts",
        "/token_2022/accounts",
        "/read_started_at_unix_secs",
        "/read_completed_at_unix_secs",
    ] {
        fixture.snapshot = original.clone();
        let (parent, key) = pointer.rsplit_once('/').expect("pointer");
        fixture.snapshot["wallet_evidence"]
            .pointer_mut(parent)
            .expect("parent")
            .as_object_mut()
            .expect("object")
            .remove(key);
        fixture.save();
        assert_eq!(
            issuer.ask(&fixture.candidate)["outcome"],
            "refused",
            "missing {pointer}"
        );
    }
    for (pointer, value) in [
        ("/version", json!(2)),
        ("/authority", json!("issued")),
        ("/commitment", json!("processed")),
        ("/wallet", json!(address(0x44))),
        ("/native_sol/decimals", json!(6)),
        ("/native_sol/raw_amount", json!("300000001")),
        ("/native_sol/raw_amount", json!(300_000_000)),
        ("/native_sol/raw_amount", json!("unknown")),
        ("/native_sol/raw_amount", json!("18446744073709551616")),
        ("/token_program/accounts", Value::Null),
        ("/token_2022/accounts", json!({})),
    ] {
        fixture.snapshot = original.clone();
        *fixture.snapshot["wallet_evidence"]
            .pointer_mut(pointer)
            .expect("field") = value;
        fixture.save();
        assert_eq!(
            issuer.ask(&fixture.candidate)["outcome"],
            "refused",
            "bad {pointer}"
        );
    }
    fixture.snapshot = original.clone();
    fixture
        .snapshot
        .as_object_mut()
        .expect("snapshot")
        .remove("wallet_evidence");
    fixture.save();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "refused");
    assert_eq!(
        std::fs::read(fixture.dir.path().join("operations.jsonl")).expect("history"),
        Vec::<u8>::new()
    );
    fixture.snapshot = original;
    fixture.save();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
}

#[test]
fn wallet_contexts_must_fit_the_reviewed_window_without_claiming_atomicity() {
    let mut fixture = Fixture::new();
    let original = fixture.snapshot.clone();
    let mut issuer = fixture.start();
    for read in ["native_sol", "token_program", "token_2022"] {
        for slot in [json!("999"), json!("1001"), json!(1000), json!("unknown")] {
            fixture.snapshot = original.clone();
            fixture.snapshot["wallet_evidence"][read]["slot"] = slot;
            fixture.save();
            assert_eq!(
                issuer.ask(&fixture.candidate)["outcome"],
                "refused",
                "{read}"
            );
        }
    }
    assert_eq!(
        std::fs::read(fixture.dir.path().join("operations.jsonl")).expect("history"),
        Vec::<u8>::new()
    );
    fixture.snapshot = original;
    fixture.snapshot["state"]["now"] = json!(1002);
    fixture.snapshot["wallet_evidence"]["token_program"]["slot"] = json!("1001");
    fixture.snapshot["wallet_evidence"]["token_2022"]["slot"] = json!("1002");
    fixture.snapshot["wallet_evidence"]["common_reported_slot"] = Value::Null;
    fixture.save();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
}

#[test]
fn wallet_read_start_caps_the_proof_even_when_its_completion_is_recent() {
    let mut fixture = Fixture::new();
    fixture.config["intent_lifetime_secs"] = json!(300);
    fixture.save();
    let original = fixture.snapshot.clone();
    let mut issuer = fixture.start();
    let now = unix_now();
    for (start, end) in [
        (now - 121, now),
        (now + 600, now + 600),
        (now, now + 600),
        (now, now - 1),
    ] {
        fixture.snapshot = original.clone();
        fixture.snapshot["wallet_evidence"]["read_started_at_unix_secs"] = json!(start);
        fixture.snapshot["wallet_evidence"]["read_completed_at_unix_secs"] = json!(end);
        fixture.save();
        assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "refused");
    }
    assert_eq!(
        std::fs::read(fixture.dir.path().join("operations.jsonl")).expect("history"),
        Vec::<u8>::new()
    );
    fixture.snapshot = original;
    fixture.snapshot["wallet_evidence"]["read_started_at_unix_secs"] = json!(now - 90);
    fixture.save();
    let answer = issuer.ask(&fixture.candidate);
    assert_eq!(answer["outcome"], "issued", "{answer}");
    assert_eq!(
        answer["intent"]["proof"]["expires_at_unix_secs"],
        now - 90 + 120
    );
}

#[test]
fn exact_simulation_evidence_is_required_and_cannot_be_replaced_by_stdin() {
    let mut fixture = Fixture::new();
    let original = fixture.snapshot.clone();
    let mut issuer = fixture.start();
    for pointer in [
        "/version",
        "/authority",
        "/commitment",
        "/transaction_base64",
        "/message_base64",
        "/execution_guaranteed",
        "/simulation/err",
        "/simulation/signature_verified",
        "/simulation/blockhash_replaced",
        "/minimum_context_slot",
        "/simulation/slot",
        "/network_fee/slot",
        "/network_fee/lamports",
        "/read_started_at_unix_secs",
        "/read_completed_at_unix_secs",
    ] {
        fixture.snapshot = original.clone();
        let evidence = &mut fixture.snapshot["transaction_evidence"];
        let (parent, key) = pointer.rsplit_once('/').expect("pointer");
        evidence
            .pointer_mut(parent)
            .expect("parent")
            .as_object_mut()
            .expect("object")
            .remove(key);
        fixture.save();
        assert_eq!(
            issuer.ask(&fixture.candidate)["outcome"],
            "refused",
            "missing {pointer}"
        );
    }
    for (pointer, value) in [
        ("/version", json!(2)),
        ("/authority", json!("issued")),
        ("/commitment", json!("processed")),
        ("/transaction_base64", json!("different bytes")),
        ("/message_base64", json!("different message")),
        ("/execution_guaranteed", json!(true)),
        ("/simulation/err", json!({"InstructionError":[0,1]})),
        ("/simulation/signature_verified", json!(true)),
        ("/simulation/blockhash_replaced", json!(true)),
    ] {
        fixture.snapshot = original.clone();
        *fixture.snapshot["transaction_evidence"]
            .pointer_mut(pointer)
            .expect("field") = value;
        fixture.save();
        assert_eq!(
            issuer.ask(&fixture.candidate)["outcome"],
            "refused",
            "bad {pointer}"
        );
    }
    fixture.snapshot = original.clone();
    fixture
        .snapshot
        .as_object_mut()
        .expect("snapshot")
        .remove("transaction_evidence");
    fixture.save();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "refused");
    assert_eq!(
        std::fs::read(fixture.dir.path().join("operations.jsonl")).expect("history"),
        Vec::<u8>::new()
    );
    fixture.snapshot = original;
    fixture.save();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
}

#[test]
fn evidence_contexts_and_network_cost_must_fit_the_reviewed_snapshot() {
    let mut fixture = Fixture::new();
    let original = fixture.snapshot.clone();
    let mut issuer = fixture.start();
    for (pointer, value) in [
        ("/minimum_context_slot", json!("999")),
        ("/minimum_context_slot", json!("1001")),
        ("/simulation/slot", json!("999")),
        ("/simulation/slot", json!("1001")),
        ("/network_fee/slot", json!("999")),
        ("/network_fee/slot", json!("1001")),
        ("/network_fee/lamports", json!("5001")),
        ("/network_fee/lamports", Value::Null),
        ("/network_fee/lamports", json!("-1")),
        ("/network_fee/lamports", json!(5000)),
    ] {
        fixture.snapshot = original.clone();
        *fixture.snapshot["transaction_evidence"]
            .pointer_mut(pointer)
            .expect("field") = value;
        fixture.save();
        assert_eq!(
            issuer.ask(&fixture.candidate)["outcome"],
            "refused",
            "{pointer}"
        );
    }
    fixture.snapshot = original.clone();
    fixture.snapshot["fee_upper_lamports"] = json!(4999);
    fixture.save();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "refused");
    assert_eq!(
        std::fs::read(fixture.dir.path().join("operations.jsonl")).expect("history"),
        Vec::<u8>::new()
    );
    fixture.snapshot = original;
    // Separate slots are accepted independently, not declared one atomic bank.
    fixture.snapshot["state"]["now"] = json!(1001);
    fixture.snapshot["transaction_evidence"]["network_fee"]["slot"] = json!("1001");
    fixture.snapshot["transaction_evidence"]["network_fee"]["lamports"] = json!("0");
    fixture.save();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
}

#[test]
fn the_oldest_transaction_read_bounds_proof_lifetime_and_future_or_stale_reads_refuse() {
    let mut fixture = Fixture::new();
    fixture.config["intent_lifetime_secs"] = json!(300);
    fixture.save();
    let original = fixture.snapshot.clone();
    let mut issuer = fixture.start();
    let now = unix_now();
    for (started, completed) in [
        (now - 121, now),
        (now + 600, now + 600),
        (now, now + 600),
        (now, now - 1),
    ] {
        fixture.snapshot = original.clone();
        fixture.snapshot["transaction_evidence"]["read_started_at_unix_secs"] = json!(started);
        fixture.snapshot["transaction_evidence"]["read_completed_at_unix_secs"] = json!(completed);
        fixture.save();
        assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "refused");
    }
    fixture.snapshot = original;
    fixture.snapshot["transaction_evidence"]["read_started_at_unix_secs"] = json!(now - 90);
    fixture.snapshot["transaction_evidence"]["read_completed_at_unix_secs"] = json!(now - 80);
    fixture.save();
    let answer = issuer.ask(&fixture.candidate);
    assert_eq!(answer["outcome"], "issued", "{answer}");
    assert_eq!(
        answer["intent"]["proof"]["expires_at_unix_secs"],
        now - 90 + 120
    );
}

#[test]
fn an_overflowing_evidence_expiry_refuses_without_reserving_capital() {
    let mut fixture = Fixture::new();
    fixture.config["max_snapshot_age_secs"] = json!(u64::MAX);
    fixture.save();
    let mut issuer = fixture.start();
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "evidence expiry overflow"
    );
    assert_eq!(
        std::fs::read(fixture.dir.path().join("operations.jsonl")).expect("history"),
        Vec::<u8>::new()
    );
}

#[test]
fn the_privy_request_encodes_checked_bytes_even_when_the_provisioned_spelling_is_lenient() {
    let mut fixture = Fixture::new();
    let canonical = fixture.candidate["transaction"]
        .as_str()
        .expect("encoding")
        .to_owned();
    let alias = format!("{canonical} ignored suffix");
    assert_eq!(
        radar_types::b64::decode(&alias),
        radar_types::b64::decode(&canonical)
    );
    fixture.candidate["transaction"] = json!(alias);
    fixture.snapshot["transaction"] = fixture.candidate["transaction"].clone();
    fixture.save();
    let mut issuer = fixture.start();
    let answer = issuer.ask(&fixture.candidate);
    assert_eq!(answer["outcome"], "issued", "{answer}");
    assert_eq!(
        answer["intent"]["request"]["body"]["params"]["transaction"],
        canonical
    );
    let intent: radar_signer::protocol::PrivyAuthorization =
        serde_json::from_value(answer["intent"].clone()).expect("intent");
    let public = radar_types::Address::new(
        ed25519_dalek::SigningKey::from_bytes(&SEED)
            .verifying_key()
            .to_bytes(),
    );
    radar_signer::attestation::Issuer::new(&public, 60)
        .expect("trust")
        .check(&intent, unix_now())
        .expect("proof binds canonical request");
}

#[test]
fn issuance_runs_the_kernel_and_reserves_before_a_verifiable_proof_leaves() {
    let fixture = Fixture::new();
    let mut issuer = fixture.start();
    let answer = issuer.ask(&fixture.candidate);
    assert_eq!(answer["outcome"], "issued", "{answer}");
    let intent: radar_signer::protocol::PrivyAuthorization =
        serde_json::from_value(answer["intent"].clone()).expect("intent");
    let proposal = serde_json::from_value(fixture.candidate["proposal"].clone()).expect("proposal");
    let state = serde_json::from_value(fixture.snapshot["state"].clone()).expect("state");
    let policy = serde_json::from_value(fixture.config["policy"].clone()).expect("policy");
    let mut expected = radar_risk::evaluate(&proposal, &state, &policy)
        .authorisation()
        .expect("kernel decision")
        .clone();
    expected.expires_after = radar_types::Slot(1010);
    assert_eq!(intent.authorization, expected);
    assert_eq!(intent.max_lamports, 250_000_000);
    assert_eq!(intent.now_slot, 1000);
    assert_eq!(intent.wallet, address(0x55));
    assert_eq!(
        intent.request.transaction(),
        fixture.candidate["transaction"].as_str()
    );
    let public = radar_types::Address::new(
        ed25519_dalek::SigningKey::from_bytes(&SEED)
            .verifying_key()
            .to_bytes(),
    );
    radar_signer::attestation::Issuer::new(&public, 60)
        .expect("trust")
        .check(&intent, unix_now())
        .expect("proof");
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
    drop(issuer);
    let operations = radar_journal::OperationLog::open(fixture.dir.path().join("operations.jsonl"))
        .expect("history");
    let entries: Vec<_> = operations.outstanding().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].1.state,
        radar_journal::OperationState::SubmissionUnknown
    );
    assert_eq!(entries[0].1.intent.amount.raw(), 250_005_000);
    let events = radar_journal::Journal::open(fixture.dir.path().join("operations.jsonl"))
        .expect("audit")
        .events()
        .expect("events");
    assert_eq!(events.len(), 3);
    assert_eq!(
        events[0].correlation.mint,
        Some(intent.authorization.mint.to_string())
    );
    assert_eq!(
        events[0].correlation.receipt.as_deref(),
        Some(intent.authorization.nonce.as_str())
    );
    for event in &events[1..] {
        assert_eq!(
            event.correlation.operation.as_deref(),
            Some(entries[0].0.as_str())
        );
    }
    drop(operations);
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
}

#[test]
fn stdin_cannot_choose_authority_or_invent_the_market_evidence() {
    let fixture = Fixture::new();
    let mut issuer = fixture.start();
    for field in [
        "authorization",
        "proof",
        "state",
        "max_lamports",
        "wallet",
        "policy",
        "transaction_evidence",
        "wallet_evidence",
        "accounting_checkpoint",
    ] {
        let mut candidate = fixture.candidate.clone();
        candidate[field] = json!("caller chooses");
        assert_eq!(
            issuer.ask(&candidate)["reason"],
            "invalid candidate",
            "{field}"
        );
    }
    for (pointer, value) in [
        ("/proposal/creator", json!(address(0x44))),
        ("/proposal/estimated_round_trip_cost", json!(0)),
        ("/proposal/simulated_exit_capacity", json!(999_000_000)),
        ("/transaction", json!("different bytes")),
    ] {
        let mut candidate = fixture.candidate.clone();
        *candidate.pointer_mut(pointer).expect("field") = value;
        assert_eq!(
            issuer.ask(&candidate)["reason"],
            "candidate does not match independently provisioned evidence",
            "{pointer}"
        );
    }
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
}

#[test]
fn unreadable_stale_future_wrong_wallet_and_unpriced_snapshots_refuse() {
    let mut fixture = Fixture::new();
    let original = fixture.snapshot.clone();
    let mut issuer = fixture.start();
    for (field, value) in [
        ("wallet", json!(address(0x66))),
        ("sol_upper_micro_usd", json!(0)),
        ("observed_at_unix_secs", json!(unix_now() - 121)),
        ("observed_at_unix_secs", json!(unix_now() + 600)),
    ] {
        fixture.snapshot = original.clone();
        fixture.snapshot[field] = value;
        fixture.save();
        assert_eq!(
            issuer.ask(&fixture.candidate)["reason"],
            "snapshot is not current for the configured wallet",
            "{field}"
        );
    }
    std::fs::write(fixture.dir.path().join("snapshot.json"), b"invalid").expect("damage");
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "invalid trusted snapshot"
    );
    fixture.snapshot = original;
    fixture.save();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
}

#[test]
fn kernel_refusals_and_insufficient_cash_never_produce_a_proof() {
    let mut fixture = Fixture::new();
    let original = fixture.snapshot.clone();
    let mut issuer = fixture.start();
    for (pointer, value) in [
        ("/state/halted", json!(true)),
        ("/state/realised_loss_today", json!(10_000_000)),
        ("/state/deployed", json!(50_000_001)),
        ("/state/consecutive_failures", json!(2)),
        ("/proposal/oldest_input_slot", json!(989)),
        ("/proposal/simulated_exit_capacity", Value::Null),
        ("/proposal/estimated_round_trip_cost", json!(500_001)),
        ("/proposal/notional", json!(50_000_001)),
    ] {
        fixture.snapshot = original.clone();
        *fixture.snapshot.pointer_mut(pointer).expect("field") = value;
        fixture.save();
        let mut candidate = fixture.candidate.clone();
        candidate["proposal"] = fixture.snapshot["proposal"].clone();
        assert_eq!(
            issuer.ask(&candidate)["reason"],
            "risk kernel refused",
            "{pointer}"
        );
    }
    for (field, value) in [
        ("action", json!("exit")),
        ("quote", json!("wrapped_sol")),
        ("notional", json!(0)),
        ("oldest_input_slot", json!(1001)),
    ] {
        fixture.snapshot = original.clone();
        fixture.snapshot["proposal"][field] = value;
        fixture.save();
        let mut candidate = fixture.candidate.clone();
        candidate["proposal"] = fixture.snapshot["proposal"].clone();
        assert_eq!(
            issuer.ask(&candidate)["reason"],
            "issuer currently accepts only current native-SOL buys",
            "{field}"
        );
    }
    fixture.snapshot = original;
    fixture.snapshot["sol_lamports"] = json!(250_004_999);
    fixture.snapshot["wallet_evidence"]["native_sol"]["raw_amount"] = json!("250004999");
    fixture.save();
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "capital reservation refused"
    );
    fixture.snapshot["sol_lamports"] = json!(250_005_000);
    fixture.snapshot["wallet_evidence"]["native_sol"]["raw_amount"] = json!("250005000");
    fixture.save();
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "snapshot accounting does not cover current journal history"
    );
    // A refused reservation still recorded a proposal: cover that history too.
    let journal =
        radar_journal::Journal::open(fixture.dir.path().join("operations.jsonl")).expect("history");
    fixture.snapshot["accounting_checkpoint"] = json!(journal.checkpoint());
    fixture.save();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
}

#[test]
fn startup_refuses_missing_history_and_unbounded_config_and_runtime_revocation() {
    for (pointer, value) in [
        ("/active", json!(false)),
        ("/intent_lifetime_secs", json!(0)),
        ("/max_snapshot_age_secs", json!(0)),
        ("/fee_reserve_lamports", json!(0)),
        ("/programs", json!([])),
        ("/policy/autonomy", json!("observe")),
        ("/history_path", json!("missing-history.jsonl")),
        ("/key_path", json!("missing-issuer-key.json")),
    ] {
        let mut fixture = Fixture::new();
        *fixture.config.pointer_mut(pointer).expect("field") = value;
        fixture.save();
        let output = fixture
            .command()
            .stdin(Stdio::null())
            .output()
            .expect("startup");
        assert!(!output.status.success(), "{pointer}");
        assert_eq!(output.stdout.len(), 0);
    }
    let mut fixture = Fixture::new();
    let mut issuer = fixture.start();
    fixture.config["active"] = json!(false);
    fixture.save();
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "mandate changed; restart issuer against current configuration"
    );
}

#[test]
fn conversion_rounds_down_and_fee_and_time_evidence_bound_the_proof() {
    let mut fixture = Fixture::new();
    fixture.snapshot["sol_upper_micro_usd"] = json!(200_000_003);
    fixture.snapshot["observed_at_unix_secs"] = json!(unix_now() - 60);
    fixture.config["intent_lifetime_secs"] = json!(300);
    fixture.save();
    let mut issuer = fixture.start();
    for value in [0, 5001] {
        fixture.snapshot["fee_upper_lamports"] = json!(value);
        fixture.save();
        assert_eq!(
            issuer.ask(&fixture.candidate)["reason"],
            "reviewed transaction fee is not covered by the reservation"
        );
    }
    fixture.snapshot["fee_upper_lamports"] = json!(5000);
    fixture.save();
    let answer = issuer.ask(&fixture.candidate);
    assert_eq!(answer["outcome"], "issued", "{answer}");
    assert_eq!(answer["intent"]["max_lamports"], 249_999_996);
    assert_eq!(
        answer["intent"]["proof"]["expires_at_unix_secs"],
        fixture.snapshot["observed_at_unix_secs"]
            .as_u64()
            .expect("observed")
            + 120
    );
    drop(issuer);
    let operations = radar_journal::OperationLog::open(fixture.dir.path().join("operations.jsonl"))
        .expect("history");
    assert_eq!(
        operations
            .outstanding()
            .next()
            .expect("claim")
            .1
            .intent
            .amount
            .raw(),
        250_004_996
    );
    drop(operations);

    for (price, fee, reason) in [
        (u64::MAX, 5000, "lamport ceiling is zero"),
        (1, 5000, "capital reservation refused"),
        (200_000_000, u64::MAX, "fee reservation overflow"),
    ] {
        let mut fixture = Fixture::new();
        fixture.snapshot["sol_upper_micro_usd"] = json!(price);
        fixture.config["fee_reserve_lamports"] = json!(fee);
        fixture.save();
        assert_eq!(fixture.start().ask(&fixture.candidate)["reason"], reason);
    }
    let mut fixture = Fixture::new();
    fixture.config["valid_until_unix_secs"] = json!(unix_now() - 1);
    fixture.save();
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "mandate expired"
    );
}

#[test]
fn unusable_private_files_and_overlong_input_stop_without_issuing() {
    let fixture = Fixture::new();
    let config_path = fixture.dir.path().join("config.json");
    let mut exact = fixture.config.to_string();
    exact.extend(std::iter::repeat_n(' ', 1_048_576 - exact.len()));
    std::fs::write(&config_path, exact).expect("maximum-size valid JSON");
    let output = fixture
        .command()
        .stdin(Stdio::null())
        .output()
        .expect("startup at file boundary");
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).expect("ready")["outcome"],
        "ready"
    );
    let mut large = fixture.config.to_string();
    large.extend(std::iter::repeat_n(' ', 1_048_577));
    std::fs::write(&config_path, large).expect("oversize valid JSON");
    let output = fixture
        .command()
        .stdin(Stdio::null())
        .output()
        .expect("startup");
    assert!(!output.status.success());
    assert_eq!(output.stdout.len(), 0);
    fixture.save();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o644))
            .expect("insecure config");
        let output = fixture
            .command()
            .stdin(Stdio::null())
            .output()
            .expect("startup");
        assert!(!output.status.success());
        assert_eq!(output.stdout.len(), 0);
        fixture.save();
    }
    let mut issuer = fixture.start();
    let mut large = fixture.candidate.to_string();
    large.extend(std::iter::repeat_n(' ', 70_000));
    large.push('\n');
    let mut stdin = issuer.child.stdin.take().expect("stdin");
    let _ = stdin.write_all(large.as_bytes());
    drop(stdin);
    let mut answer = String::new();
    issuer.reader.read_line(&mut answer).expect("EOF");
    assert!(
        answer.is_empty(),
        "no proof from a truncated input: {answer}"
    );
    assert!(!issuer.child.wait().expect("exit").success());
}

#[test]
fn the_input_size_boundary_accepts_a_complete_line_and_refuses_truncation() {
    for (length, newline, accepted) in [
        (65_536, true, true),
        (65_537, true, false),
        (1000, false, false),
    ] {
        let fixture = Fixture::new();
        let mut raw = fixture.candidate.to_string();
        raw.extend(std::iter::repeat_n(
            ' ',
            length - usize::from(newline) - raw.len(),
        ));
        if newline {
            raw.push('\n');
        }
        let (answer, success) = raw_response(&fixture, raw.as_bytes());
        assert_eq!(success, accepted, "{length}/{newline}: {answer}");
        if accepted {
            assert_eq!(
                serde_json::from_str::<Value>(&answer).expect("proof JSON")["outcome"],
                "issued"
            );
        } else {
            assert!(answer.is_empty(), "{answer}");
        }
    }
}

#[test]
fn a_broken_output_pipe_keeps_the_claim_unknown_after_restart() {
    let fixture = Fixture::new();
    let mut child = fixture
        .command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("issuer");
    let mut reader = BufReader::new(child.stdout.take().expect("stdout"));
    let mut ready = String::new();
    reader.read_line(&mut ready).expect("ready");
    assert_eq!(
        serde_json::from_str::<Value>(&ready).expect("ready JSON")["outcome"],
        "ready"
    );
    drop(reader);
    let mut stdin = child.stdin.take().expect("stdin");
    writeln!(stdin, "{}", fixture.candidate).expect("candidate");
    stdin.flush().expect("flush");
    drop(stdin);
    assert!(!child.wait().expect("exit").success());
    let operations = radar_journal::OperationLog::open(fixture.dir.path().join("operations.jsonl"))
        .expect("history");
    let entries: Vec<_> = operations.outstanding().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].1.state,
        radar_journal::OperationState::SubmissionUnknown
    );
    drop(operations);
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
}
