// SPDX-License-Identifier: Apache-2.0
//! Acknowledgement is explicit and cannot bootstrap missing history or load keys first.

#[test]
fn opening_acknowledgement_requires_existing_genesis_and_explicit_operator_flag() {
    for state in ["missing", "empty", "reserved"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("paper.jsonl");
        if state != "missing" { std::fs::write(&path, b"").unwrap(); }
        if state == "reserved" {
            let mut log = radar_journal::BrokerLog::open(&path, "paper").unwrap();
            log.reserve(radar_journal::BrokerIntent {account_id:"paper".into(), client_order_id:"radar_1".into(),
                symbol:"SPY".into(), side:radar_journal::BrokerSide::Buy, quantity_nanoshares:1,
                limit_microusd_per_share:100_000_000, fee_bound_microusd:7}, 1).unwrap();
        }
        let before = std::fs::read(&path).ok();
        for acknowledgement in ["absent", "present", "other"] {
            let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_radar"));
            command.args(["alpaca-opening-cash-acknowledge", "--journal"]).arg(&path)
                .args(["--account-id", "paper", "--activity-id", "selected", "--max-pages", "1"]);
            if acknowledgement == "present" { command.arg("--already-in-opening-balance"); }
            if acknowledgement == "other" { command.arg("--trust-model"); }
            let output = command.env_remove("RADAR_ALPACA_PAPER_KEY_ID")
                .env("RADAR_ALPACA_PAPER_SECRET_KEY", "private-sentinel").output().unwrap();
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(!error.contains("private-sentinel"));
            assert!(!error.contains("configure both Alpaca paper credentials"));
            assert!(error.contains(if acknowledgement != "present" { "usage: radar alpaca-opening-cash-acknowledge" }
                else if state == "missing" { "paper order journal is missing" }
                else if state == "empty" { "requires retained genesis" }
                else { "no retained orders" }), "{error}");
            assert_eq!(std::fs::read(&path).ok(), before);
        }
    }
}
