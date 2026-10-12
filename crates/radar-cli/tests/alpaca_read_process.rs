// SPDX-License-Identifier: Apache-2.0
//! The operator command refuses missing configuration and endpoint overrides.

use std::process::Command;

#[test]
fn bound_recovery_command_requires_existing_verified_retained_intent_before_credentials() {
    use radar_journal::{BrokerIntent, BrokerLog, BrokerSide};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("paper.jsonl");
    let mut log = BrokerLog::open(&path, "paper-id").unwrap();
    log.reserve(
        BrokerIntent {
            account_id: "paper-id".into(),
            client_order_id: "retained_1".into(),
            symbol: "SPY".into(),
            side: BrokerSide::Buy,
            quantity_nanoshares: 10_000_000,
            limit_microusd_per_share: 100_000_000,
            fee_bound_microusd: 1,
        },
        1,
    )
    .unwrap();
    drop(log);
    let missing = directory.path().join("missing.jsonl");
    for (journal, account, client, expected) in [
        (
            &missing,
            "paper-id",
            "retained_1",
            "paper order journal is missing",
        ),
        (
            &path,
            "paper-id",
            "missing",
            "client order ID has no retained journal intent",
        ),
        (
            &path,
            "other",
            "retained_1",
            "invalid or mismatched paper broker intent",
        ),
        (
            &path,
            "paper-id",
            "retained_1",
            "configure both Alpaca paper credentials privately",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args([
                "alpaca-order-read",
                "--account-id",
                account,
                "--client-order-id",
                client,
                "--journal",
            ])
            .arg(journal)
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
    assert!(
        !missing.exists(),
        "recovery must not initialize a missing journal"
    );
}

#[test]
fn paper_reader_refuses_missing_credentials_and_live_override_without_output() {
    for args in [
        vec!["alpaca-read"],
        vec!["alpaca-read", "--url", "https://api.alpaca.markets"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args(&args)
            .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
            .env("RADAR_ALPACA_PAPER_SECRET_KEY", "sentinel-private-secret")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(!error.contains("sentinel-private-secret"));
        if args.len() == 1 {
            assert!(error.contains("configure both Alpaca paper credentials privately"));
        } else {
            assert!(error.contains("usage: radar alpaca-read"));
        }
    }
}

#[test]
fn market_replay_enforces_watermark_in_the_operator_command() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capture.json");
    let capture = serde_json::json!({"format":"alpaca-iex-snapshot-v1",
        "read_started_unix_ms":99,"available_at_unix_ms":100,
        "symbols":["SPY"],"raw_response":"{\"SPY\":null}"});
    std::fs::write(&path, capture.to_string()).unwrap();
    for watermark in ["99", "100", "invalid"] {
        let output = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args(["alpaca-market-replay", "--input"])
            .arg(&path)
            .args(["--as-of-unix-ms", watermark])
            .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
            .env_remove("RADAR_ALPACA_PAPER_SECRET_KEY")
            .output()
            .unwrap();
        if watermark == "100" {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["feed"], "iex");
            assert_eq!(report["snapshots"], serde_json::json!({"SPY":null}));
            assert_eq!(report["as_of_unix_ms"], 100);
        } else {
            assert!(!output.status.success());
            assert_eq!(output.stdout, Vec::<u8>::new());
            let error = String::from_utf8(output.stderr).unwrap();
            if watermark == "99" {
                assert!(error.contains("market capture was unavailable at the replay watermark"));
            } else {
                assert!(error.contains("replay watermark must be u64 milliseconds"));
            }
        }
    }
}

#[test]
fn quote_command_uses_capture_universe_and_availability_before_validating_prices() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capture.json");
    let at = 1_792_540_800_000_u64;
    let quote =
        r#"{"latestQuote":{"t":"2026-10-21T00:00:00Z","bp":100,"ap":100.01,"bs":20,"as":30,"bx":"V","ax":"V","c":["R"],"z":"B"}}"#;
    let capture = serde_json::json!({"format":"alpaca-iex-snapshot-v1",
        "read_started_unix_ms":at-1,"available_at_unix_ms":at,"symbols":["SPY"],
        "raw_response":format!("{{\"SPY\":{quote},\"QQQ\":{quote}}}")});
    std::fs::write(&path, capture.to_string()).unwrap();
    for (symbol, watermark, success, error) in [
        ("SPY", at, true, ""),
        (
            "QQQ",
            at,
            false,
            "quote symbol was not requested in this capture",
        ),
        (
            "SPY",
            at - 1,
            false,
            "market capture was unavailable at the replay watermark",
        ),
        ("SPY", at + 2, false, "quote exceeds configured age"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args(["alpaca-quote", "--input"])
            .arg(&path)
            .args([
                "--symbol",
                symbol,
                "--as-of-unix-ms",
                &watermark.to_string(),
                "--max-age-ms",
                "1",
                "--max-spread-bps",
                "1",
            ])
            .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
            .env_remove("RADAR_ALPACA_PAPER_SECRET_KEY")
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if success {
            let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["ask_microusd_per_share"], "100010000");
            assert_eq!(report["symbol"], "SPY");
            assert_eq!(report["execution_authority"], false);
        } else {
            assert_eq!(output.stdout, Vec::<u8>::new());
            assert!(String::from_utf8(output.stderr).unwrap().contains(error));
        }
    }
}

#[test]
fn reconciliation_command_refuses_missing_credentials_and_endpoint_overrides() {
    for (args, expected) in [
        (
            vec!["alpaca-reconcile", "--account-id", "paper-id"],
            "configure both Alpaca paper credentials privately",
        ),
        (vec!["alpaca-reconcile"], "usage: radar alpaca-reconcile"),
        (
            vec![
                "alpaca-reconcile",
                "--account-id",
                "paper-id",
                "--url",
                "https://api.alpaca.markets",
            ],
            "usage: radar alpaca-reconcile",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args(args)
            .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
            .env("RADAR_ALPACA_PAPER_SECRET_KEY", "sentinel-private-secret")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains(expected));
        assert!(!error.contains("sentinel-private-secret"));
    }
}

#[test]
fn recovery_lookup_command_refuses_missing_credentials_and_injected_identifiers() {
    for (args, expected) in [
        (
            vec![
                "alpaca-order-read",
                "--account-id",
                "paper-id",
                "--client-order-id",
                "retained_1",
            ],
            "configure both Alpaca paper credentials privately",
        ),
        (vec!["alpaca-order-read"], "usage: radar alpaca-order-read"),
        (
            vec![
                "alpaca-order-read",
                "--account-id",
                "paper-id",
                "--client-order-id",
                "a&x=1",
            ],
            "client order ID must contain ASCII letters",
        ),
        (
            vec![
                "alpaca-order-read",
                "--account-id",
                "paper-id",
                "--client-order-id",
                "retained_1",
                "--url",
                "https://api.alpaca.markets",
            ],
            "usage: radar alpaca-order-read",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args(args)
            .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
            .env("RADAR_ALPACA_PAPER_SECRET_KEY", "sentinel-private-secret")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains(expected));
        assert!(!error.contains("sentinel-private-secret"));
    }
}
