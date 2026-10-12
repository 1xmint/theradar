// SPDX-License-Identifier: Apache-2.0
//! Cancellation is private, paper-only and requires an existing owned journal.

#[test]
fn cancellation_dispatch_refuses_missing_journal_and_endpoint_override_without_secrets() {
    let directory = tempfile::tempdir().unwrap();
    let journal = directory.path().join("missing.jsonl");
    for override_endpoint in [false, true] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_radar"));
        command.args(["alpaca-paper-cancel", "--journal"]).arg(&journal)
            .args(["--account-id", "paper", "--client-order-id", "radar_1"]);
        if override_endpoint {
            command.args(["--url", "https://api.alpaca.markets"]);
        }
        let output = command.env_remove("RADAR_ALPACA_PAPER_KEY_ID")
            .env("RADAR_ALPACA_PAPER_SECRET_KEY", "private-secret-sentinel")
            .output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(!error.contains("private-secret-sentinel"));
        assert!(error.contains(if override_endpoint {
            "usage: radar alpaca-paper-cancel"
        } else {
            "paper order journal is missing"
        }), "{error}");
        assert!(!journal.exists());
    }
}
