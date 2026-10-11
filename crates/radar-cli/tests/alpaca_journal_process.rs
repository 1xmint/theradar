// SPDX-License-Identifier: Apache-2.0
//! The operator's durable claims survive separate CLI processes without broker keys.

use serde_json::{Value, json};
use std::process::Command;

fn run(
    journal: &std::path::Path,
    intent: Option<&std::path::Path>,
    account: &str,
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_radar"));
    command
        .arg(if intent.is_some() {
            "alpaca-order-reserve"
        } else {
            "alpaca-order-journal"
        })
        .arg("--journal")
        .arg(journal)
        .args(["--account-id", account])
        .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
        .env_remove("RADAR_ALPACA_PAPER_SECRET_KEY");
    if let Some(path) = intent {
        command.arg("--intent").arg(path);
    }
    command.output().unwrap()
}

fn intent() -> Value {
    json!({"account_id":"paper-id","client_order_id":"radar_1","symbol":"SPY","side":"buy",
        "quantity_nanoshares":10000001,"limit_microusd_per_share":100000001,"fee_bound_microusd":7})
}

#[test]
fn reservation_is_idempotent_and_account_bound_across_processes() {
    let dir = tempfile::tempdir().unwrap();
    let journal = dir.path().join("paper.jsonl");
    let input = dir.path().join("intent.json");
    std::fs::write(&input, intent().to_string()).unwrap();
    for written in [true, false] {
        let output = run(&journal, Some(&input), "paper-id");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["new_reservation_written"], written);
        assert_eq!(report["claims"].as_array().unwrap().len(), 1);
        assert_eq!(report["claims"][0]["cash_claim_microusd"], "1000008");
        assert_eq!(report["claims"][0]["share_claim_nanoshares"], "0");
        assert_eq!(report["claims"][0]["state"], "reserved");
        assert_eq!(report["claims"][0]["intent"], intent());
        for flag in [
            "broker_reconciled",
            "retry_authorized",
            "claim_release_authorized",
            "execution_authority",
        ] {
            assert_eq!(report[flag], false);
        }
    }
    let read = run(&journal, None, "paper-id");
    assert!(read.status.success());
    let report: Value = serde_json::from_slice(&read.stdout).unwrap();
    assert_eq!(report["new_reservation_written"], false);
    assert_eq!(report["claims"][0]["intent"], intent());
    let wrong = run(&journal, None, "other");
    assert!(!wrong.status.success());
    assert_eq!(wrong.stdout, Vec::<u8>::new());
}

#[test]
fn missing_journal_bad_or_conflicting_intent_never_prints_empty_safe_claims() {
    let dir = tempfile::tempdir().unwrap();
    let journal = dir.path().join("paper.jsonl");
    let input = dir.path().join("intent.json");
    let missing = run(&journal, None, "paper-id");
    assert!(!missing.status.success());
    assert_eq!(missing.stdout, Vec::<u8>::new());
    assert!(
        String::from_utf8(missing.stderr)
            .unwrap()
            .contains("paper order journal is missing")
    );
    let valid = intent().to_string();
    // A valid object ending at the bound must not lose its closing brace.
    let at_bound = format!("{}{valid}", " ".repeat(65_536 - valid.len()));
    std::fs::write(&input, at_bound).unwrap();
    assert!(run(&journal, Some(&input), "paper-id").status.success());
    // Valid JSON plus whitespace remains parseable if a short read hides excess
    // bytes. Refusal must come from the byte bound, not a JSON syntax failure.
    std::fs::write(
        &input,
        format!("{valid}{}", " ".repeat(65_537 - valid.len())),
    )
    .unwrap();
    let oversized = run(&journal, Some(&input), "paper-id");
    assert!(!oversized.status.success());
    assert_eq!(oversized.stdout, Vec::<u8>::new());
    assert!(
        String::from_utf8(oversized.stderr)
            .unwrap()
            .contains("paper intent exceeds 64 KiB")
    );
    for body in [
        "null".to_owned(),
        "x".repeat(65_537),
        intent().to_string().replace(
            "\"symbol\":\"SPY\"",
            "\"symbol\":\"SPY\",\"secret\":\"sentinel-private-secret\"",
        ),
    ] {
        std::fs::write(&input, body).unwrap();
        let refused = run(&journal, Some(&input), "paper-id");
        assert!(!refused.status.success());
        assert_eq!(refused.stdout, Vec::<u8>::new());
        assert!(
            !String::from_utf8(refused.stderr)
                .unwrap()
                .contains("sentinel-private-secret")
        );
    }
    std::fs::write(&input, intent().to_string()).unwrap();
    assert!(run(&journal, Some(&input), "paper-id").status.success());
    let mut changed = intent();
    changed["quantity_nanoshares"] = json!(20000000);
    std::fs::write(&input, changed.to_string()).unwrap();
    let refused = run(&journal, Some(&input), "paper-id");
    assert!(!refused.status.success());
    assert_eq!(refused.stdout, Vec::<u8>::new());
    assert!(
        String::from_utf8(refused.stderr)
            .unwrap()
            .contains("client order ID already binds another intent")
    );
}
