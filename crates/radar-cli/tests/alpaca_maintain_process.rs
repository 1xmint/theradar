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
            assert_eq!(output.stdout, Vec::<u8>::new());
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

#[test]
fn maintenance_refuses_existing_empty_or_reservation_only_history_before_credentials() {
    for reserved in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let journal = directory.path().join("paper.jsonl");
        std::fs::write(&journal, b"").unwrap();
        {
            let mut log = radar_journal::BrokerLog::open(&journal, "paper").unwrap();
            if reserved {
                log.reserve(
                    radar_journal::BrokerIntent {
                        account_id: "paper".into(),
                        client_order_id: "radar_1".into(),
                        symbol: "SPY".into(),
                        side: radar_journal::BrokerSide::Buy,
                        quantity_nanoshares: 1_000_000_000,
                        limit_microusd_per_share: 100_000_000,
                        fee_bound_microusd: 7,
                    },
                    1,
                )
                .unwrap();
            }
        }
        let before = std::fs::read(&journal).unwrap();
        for stop in [false, true] {
            let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_radar"));
            command
                .args(["alpaca-paper-maintain", "--journal"])
                .arg(&journal)
                .args(["--account-id", "paper", "--max-pages", "1"]);
            if stop {
                command.arg("--cancel-working");
            }
            let output = command
                .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
                .env("RADAR_ALPACA_PAPER_SECRET_KEY", "private-secret-sentinel")
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert_eq!(output.stdout, Vec::<u8>::new());
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(error.contains("explicit initialization required"), "{error}");
            assert!(!error.contains("private-secret-sentinel"));
            assert_eq!(std::fs::read(&journal).unwrap(), before);
        }
    }
}
