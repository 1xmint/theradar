// SPDX-License-Identifier: Apache-2.0
//! Actual cash command refuses invalid private inputs before broker access.

use radar_journal::{BrokerIntent, BrokerLog, BrokerSide};
use serde_json::{Value, json};
use std::process::Command;

fn baseline() -> Value {
    let account = json!({"id":"paper-id","status":"ACTIVE","currency":"USD","cash":"100"});
    json!({"version":1,"venue":"alpaca","environment":"paper","source":"https://paper-api.alpaca.markets",
        "expected_account_id":"paper-id","pagination_exhausted":true,"selected_account_fields_unchanged":true,
        "pages_read":1,"account_before":account,"account_after":account,"activities":[],
        "read_started_unix_secs":1,"read_completed_unix_secs":2})
}

fn refused(args: &[&str], expected: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_radar"))
        .args(args)
        .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
        .env("RADAR_ALPACA_PAPER_SECRET_KEY", "sentinel-private-secret")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.stdout, Vec::<u8>::new());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains(expected), "{error}");
    assert!(!error.contains("sentinel-private-secret"));
}

#[test]
fn durable_cash_command_validates_setup_without_echoing_or_writing_a_baseline() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cash.jsonl");
    let path = path.to_str().unwrap();
    refused(&["alpaca-cash-checkpoint"], "usage:");
    for pages in ["0", "65536", "-1", "one"] {
        refused(
            &[
                "alpaca-cash-checkpoint",
                "--journal",
                path,
                "--account-id",
                "paper-id",
                "--max-pages",
                pages,
            ],
            "positive",
        );
    }
    refused(
        &[
            "alpaca-cash-checkpoint",
            "--journal",
            path,
            "--account-id",
            "paper-id",
            "--max-pages",
            "1",
        ],
        "configure both Alpaca paper credentials privately",
    );
    assert!(!std::path::Path::new(path).exists());
    refused(
        &[
            "alpaca-cash-checkpoint",
            "--journal",
            path,
            "--account-id",
            " ",
            "--max-pages",
            "1",
        ],
        "invalid",
    );
}

#[test]
fn cash_command_requires_existing_claims_valid_baseline_and_private_paper_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("claims.jsonl");
    let input = directory.path().join("baseline.json");
    let missing_path = directory.path().join("missing.jsonl");
    {
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        log.reserve(
            BrokerIntent {
                account_id: "paper-id".into(),
                client_order_id: "retained_1".into(),
                symbol: "SPY".into(),
                side: BrokerSide::Buy,
                quantity_nanoshares: 1_000_000_000,
                limit_microusd_per_share: 100_000_000,
                fee_bound_microusd: 7,
            },
            1,
        )
        .unwrap();
    }
    std::fs::write(&input, baseline().to_string()).unwrap();
    let base = vec![
        "alpaca-cash-reconcile",
        "--journal",
        path.to_str().unwrap(),
        "--account-id",
        "paper-id",
        "--baseline",
        input.to_str().unwrap(),
        "--max-pages",
        "1",
    ];
    for (index, replacement, expected) in [
        (
            2,
            missing_path.to_str().unwrap(),
            "paper order journal is missing",
        ),
        (4, "other", "invalid or mismatched paper broker intent"),
        (
            6,
            missing_path.to_str().unwrap(),
            "paper cash baseline unavailable",
        ),
        (8, "0", "activity page limit must be a positive u16"),
        (8, "65536", "activity page limit must be a positive u16"),
        (8, "invalid", "activity page limit must be a positive u16"),
        (8, "1", "configure both Alpaca paper credentials privately"),
    ] {
        let mut args = base.clone();
        args[index] = replacement;
        refused(&args, expected);
    }
    let mut future = baseline();
    future["read_completed_unix_secs"] = json!(u64::MAX);
    for (body, expected) in [
        (
            "sentinel-private-secret".to_owned(),
            "invalid paper cash baseline JSON",
        ),
        (future.to_string(), "paper cash baseline is in the future"),
        (
            baseline().to_string().replace("paper-id", "other"),
            "invalid or changing paper cash observation",
        ),
    ] {
        std::fs::write(&input, body).unwrap();
        refused(&base, expected);
    }
    for args in [vec!["alpaca-cash-reconcile"], {
        let mut args = base;
        args.extend(["--url", "https://api.alpaca.markets"]);
        args
    }] {
        refused(&args, "usage: radar alpaca-cash-reconcile");
    }
    assert!(!missing_path.exists());
}
