// SPDX-License-Identifier: Apache-2.0
//! Actual command configuration refusals; no broker request or credential echo.

use std::process::Command;

#[test]
fn activities_command_requires_bounded_pagination_and_private_paper_credentials() {
    for (args, expected) in [
        (
            vec!["alpaca-activities-read"],
            "usage: radar alpaca-activities-read",
        ),
        (
            vec![
                "alpaca-activities-read",
                "--account-id",
                "paper-id",
                "--max-pages",
                "2",
            ],
            "configure both Alpaca paper credentials privately",
        ),
        (
            vec![
                "alpaca-activities-read",
                "--account-id",
                "paper-id",
                "--max-pages",
                "0",
            ],
            "activity page limit must be a positive u16",
        ),
        (
            vec![
                "alpaca-activities-read",
                "--account-id",
                "paper-id",
                "--max-pages",
                "65536",
            ],
            "activity page limit must be a positive u16",
        ),
        (
            vec![
                "alpaca-activities-read",
                "--account-id",
                "paper-id",
                "--max-pages",
                "invalid",
            ],
            "activity page limit must be a positive u16",
        ),
        (
            vec![
                "alpaca-activities-read",
                "--account-id",
                "paper-id",
                "--max-pages",
                "2",
                "--url",
                "https://api.alpaca.markets",
            ],
            "usage: radar alpaca-activities-read",
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
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("sentinel-private-secret"));
    }
}
