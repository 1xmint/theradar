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
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(!error.contains("sentinel-private-secret"));
        if args.len() == 1 {
            assert!(error.contains("configure both Alpaca paper credentials privately"));
        } else {
            assert!(error.contains("usage: radar alpaca-read"));
        }
    }
}
