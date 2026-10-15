// SPDX-License-Identifier: Apache-2.0
//! Actual operator dispatch rejects invalid scenarios before credentials or effects.

#[test]
fn paper_model_cli_is_explicit_validated_and_cannot_create_missing_history() {
    let directory = tempfile::tempdir().unwrap();
    let journal = directory.path().join("missing.jsonl");
    let input = directory.path().join("intent.json");
    let policy = directory.path().join("policy.json");
    let model = directory.path().join("model.json");
    let intent = radar_journal::BrokerIntent {
        account_id: "paper".into(),
        client_order_id: "radar_1".into(),
        symbol: "SPY".into(),
        side: radar_journal::BrokerSide::Buy,
        quantity_nanoshares: 1_000_000_000,
        limit_microusd_per_share: 100_000_000,
        fee_bound_microusd: 7,
    };
    std::fs::write(&input, serde_json::to_vec(&intent).unwrap()).unwrap();
    std::fs::write(
        &policy,
        serde_json::to_vec(&radar_risk::equity::Policy::default()).unwrap(),
    )
    .unwrap();
    for command in ["alpaca-order-preflight", "alpaca-paper-submit"] {
        for case in ["absent", "valid", "invalid", "unknown_flag"] {
            let mut scenario = serde_json::json!({"version":"alpaca-paper-stress-v1","account_id":"paper",
                "max_quantity_nanoshares":1_000_000_000,"fee_per_side_microusd":7,"slippage_bps_per_side":10});
            if case == "invalid" {
                scenario["version"] = serde_json::json!("private-sentinel");
            }
            std::fs::write(&model, serde_json::to_vec(&scenario).unwrap()).unwrap();
            let mut process = std::process::Command::new(env!("CARGO_BIN_EXE_radar"));
            process
                .arg(command)
                .arg("--journal")
                .arg(&journal)
                .arg("--intent")
                .arg(&input)
                .arg("--policy")
                .arg(&policy)
                .args(["--max-pages", "1"]);
            if case != "absent" {
                process
                    .arg(if case == "unknown_flag" {
                        "--live-model"
                    } else {
                        "--paper-model"
                    })
                    .arg(&model);
            }
            let output = process
                .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
                .env("RADAR_ALPACA_PAPER_SECRET_KEY", "private-sentinel")
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert_eq!(output.stdout, Vec::<u8>::new());
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(!error.contains("private-sentinel"));
            assert!(!error.contains("configure both Alpaca paper credentials"));
            assert!(
                error.contains(match case {
                    "invalid" => "invalid explicit paper stress model",
                    "unknown_flag" => "usage: radar",
                    _ => "paper order journal is missing",
                }),
                "{error}"
            );
            assert!(!journal.exists());
        }
    }
}
