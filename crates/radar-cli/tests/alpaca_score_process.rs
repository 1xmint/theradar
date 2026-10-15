// SPDX-License-Identifier: Apache-2.0
//! Actual offline command over retained captures; no keys or network required.

use std::process::Command;

use serde_json::{Value, json};

#[test]
fn score_cli_gates_capture_availability_and_charges_explicit_costs_without_keys() {
    let directory = tempfile::tempdir().unwrap();
    let entry = directory.path().join("entry.json");
    let exit = directory.path().join("exit.json");
    let assumptions = directory.path().join("assumptions.json");
    let at = 1_792_540_800_000_u64;
    for (path, time, quote) in [
        (&entry, at, "2026-10-21T00:00:00Z"),
        (&exit, at + 1_000, "2026-10-21T00:00:01Z"),
    ] {
        let raw = json!({"SPY":{"latestQuote":{"t":quote,"bp":100,"ap":100.01,
            "bs":20,"as":30,"bx":"V","ax":"V","c":["R"],"z":"B"}}});
        let capture = json!({"format":"alpaca-iex-snapshot-v1","read_started_unix_ms":time-1,
            "available_at_unix_ms":time,"symbols":["SPY"],"raw_response":raw.to_string()});
        std::fs::write(path, capture.to_string()).unwrap();
    }
    std::fs::write(
        &assumptions,
        json!({"model_id":"explicit-test","displayed_size_unit":"shares",
        "participation_bps":1_000,"slippage_bps_per_side":1,"fee_per_side_microusd":7,
        "max_quote_age_ms":1_000,"max_spread_bps":10})
        .to_string(),
    )
    .unwrap();
    for (entry_at, quantity, symbol, expected) in [
        (at, "1000000000", "SPY", None),
        (
            at - 1,
            "1000000000",
            "SPY",
            Some("unavailable at the replay watermark"),
        ),
        (
            at,
            "2000000001",
            "SPY",
            Some("exceeds assumed displayed participation"),
        ),
        (at, "1", "QQQ", Some("quote symbol was not requested")),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args(["alpaca-paper-score", "--entry"])
            .arg(&entry)
            .args(["--entry-as-of-unix-ms", &entry_at.to_string(), "--exit"])
            .arg(&exit)
            .args([
                "--exit-as-of-unix-ms",
                &(at + 1_000).to_string(),
                "--symbol",
                symbol,
                "--quantity-nanoshares",
                quantity,
                "--assumptions",
            ])
            .arg(&assumptions)
            .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
            .env_remove("RADAR_ALPACA_PAPER_SECRET_KEY")
            .output()
            .unwrap();
        if let Some(error) = expected {
            assert!(!output.status.success());
            assert_eq!(output.stdout, Vec::<u8>::new());
            assert!(String::from_utf8(output.stderr).unwrap().contains(error));
        } else {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["gross_pnl_microusd"], "-10000");
            assert_eq!(report["extra_cost_microusd"], "20015");
            assert_eq!(report["net_pnl_microusd"], "-30015");
            assert_eq!(report["execution_authority"], false);
            assert_eq!(report["capacity_verified"], false);
        }
    }
}
