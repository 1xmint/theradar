// SPDX-License-Identifier: Apache-2.0
//! The operator command refuses missing configuration and endpoint overrides.

use std::process::Command;

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
