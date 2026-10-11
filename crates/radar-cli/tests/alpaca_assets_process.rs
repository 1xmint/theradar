// SPDX-License-Identifier: Apache-2.0
//! Paper asset observations cannot be redirected to live through CLI arguments.

use std::process::Command;

#[test]
fn assets_reader_refuses_missing_keys_and_endpoint_overrides_without_output() {
    for extra in [false, true] {
        let mut args = vec![
            "alpaca-assets-read",
            "--account-id",
            "paper-id",
            "--symbols",
            "SPY,QQQ",
        ];
        if extra {
            args.extend(["--url", "https://api.alpaca.markets"]);
        }
        let output = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args(args)
            .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
            .env("RADAR_ALPACA_PAPER_SECRET_KEY", "sentinel-private-secret")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(!error.contains("sentinel-private-secret"));
        assert!(error.contains(if extra {
            "usage: radar alpaca-assets-read"
        } else {
            "configure both Alpaca paper credentials privately"
        }));
    }
}
