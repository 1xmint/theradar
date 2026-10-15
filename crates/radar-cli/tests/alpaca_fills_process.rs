// SPDX-License-Identifier: Apache-2.0
//! Command refuses invalid recovery configuration before credentials or broker IO.

use radar_journal::{BrokerIntent, BrokerLog, BrokerSide};
use std::process::Command;

#[test]
fn reconciliation_requires_existing_account_bound_intent_and_private_paper_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("claims.jsonl");
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
    let journal = path.to_str().unwrap();
    let missing_path = directory.path().join("missing.jsonl");
    let missing = missing_path.to_str().unwrap();
    let base = vec![
        "alpaca-order-reconcile",
        "--journal",
        journal,
        "--account-id",
        "paper-id",
        "--client-order-id",
        "retained_1",
        "--max-pages",
        "1",
    ];
    for (index, replacement, expected) in [
        (2, missing, "paper order journal is missing"),
        (4, "other", "invalid or mismatched paper broker intent"),
        (6, "missing", "no retained journal intent"),
        (6, "bad&id", "client order ID must contain"),
        (8, "0", "activity page limit must be a positive u16"),
        (8, "65536", "activity page limit must be a positive u16"),
        (8, "invalid", "activity page limit must be a positive u16"),
        (8, "1", "configure both Alpaca paper credentials privately"),
    ] {
        let mut args = base.clone();
        args[index] = replacement;
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
    for args in [vec!["alpaca-order-reconcile"], {
        let mut override_args = base;
        override_args.extend(["--url", "https://api.alpaca.markets"]);
        override_args
    }] {
        let output = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("usage: radar alpaca-order-reconcile")
        );
    }
    assert!(!missing_path.exists());
}
