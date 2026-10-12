// SPDX-License-Identifier: Apache-2.0
//! Actual maintenance dispatch rejects missing state and arbitrary endpoints.

#[test]
fn maintenance_requires_existing_journal_and_rejects_endpoint_override_in_both_modes() {
    let directory = tempfile::tempdir().unwrap();
    let journal = directory.path().join("missing.jsonl");
    for stop in [false, true] {
        for override_endpoint in [false, true] {
            let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_radar"));
            command
                .args(["alpaca-paper-maintain", "--journal"])
                .arg(&journal)
                .args(["--account-id", "paper", "--max-pages", "1"]);
            if stop {
                command.arg("--cancel-working");
            }
            if override_endpoint {
                command.args(["--url", "https://api.alpaca.markets"]);
            }
            let output = command
                .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
                .env("RADAR_ALPACA_PAPER_SECRET_KEY", "private-secret-sentinel")
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(!error.contains("private-secret-sentinel"));
            assert!(
                error.contains(if override_endpoint {
                    "usage: radar alpaca-paper-maintain"
                } else {
                    "paper order journal is missing"
                }),
                "{error}"
            );
            assert!(!journal.exists());
        }
    }
}
