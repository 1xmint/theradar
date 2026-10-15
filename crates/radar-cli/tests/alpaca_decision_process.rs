// SPDX-License-Identifier: Apache-2.0
//! Forward-record commands bind assumptions and preserve unscoreable outcomes.

use std::path::Path;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

fn capture(path: &Path, at: u64) {
    let timestamp = radar_types::civil::timestamp_from_seconds(at / 1_000);
    let raw = json!({"SPY":{"latestQuote":{"t":timestamp,"bp":100,"ap":100.01,
        "bs":20,"as":30,"bx":"V","ax":"V","c":["R"],"z":"B"}}});
    std::fs::write(
        path,
        json!({"format":"alpaca-iex-snapshot-v1",
        "read_started_unix_ms":at-1,"available_at_unix_ms":at,"symbols":["SPY"],
        "raw_response":raw.to_string()})
        .to_string(),
    )
    .unwrap();
}

fn command(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_radar"))
        .args(args)
        .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
        .env_remove("RADAR_ALPACA_PAPER_SECRET_KEY")
        .output()
        .unwrap()
}

fn report(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn inputs(entry: &Path, assumptions: &Path) {
    let at = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    capture(entry, at - 1_000);
    std::fs::write(
        assumptions,
        json!({"model_id":"explicit-stress-1","displayed_size_unit":"shares",
        "participation_bps":1000,"slippage_bps_per_side":1,"fee_per_side_microusd":7,
        "max_quote_age_ms":30000,"max_spread_bps":10})
        .to_string(),
    )
    .unwrap();
}

#[test]
fn actual_forward_commands_freeze_compare_and_keep_unknowns_without_keys() {
    let directory = tempfile::tempdir().unwrap();
    let path = |name| directory.path().join(name);
    let entry = path("entry.json");
    let exit = path("exit.json");
    let assumptions = path("assumptions.json");
    let proposal = path("proposal.json");
    let record = path("decision.json");
    inputs(&entry, &assumptions);
    for action in ["enter", "wait"] {
        if record.exists() {
            std::fs::remove_file(&record).unwrap();
        }
        std::fs::write(
            &proposal,
            json!({"strategy_id":"fixture-v1","action":action,
            "quantity_nanoshares":1_000_000_000_u64,"horizon_ms":60000,
            "reason":"test data, not a learned strategy"})
            .to_string(),
        )
        .unwrap();
        let create = || {
            command(&[
                "alpaca-paper-decision",
                "--capture",
                entry.to_str().unwrap(),
                "--symbol",
                "SPY",
                "--proposal",
                proposal.to_str().unwrap(),
                "--assumptions",
                assumptions.to_str().unwrap(),
                "--output",
                record.to_str().unwrap(),
            ])
        };
        let written = report(&create());
        assert_eq!(written["decision_written"], true);
        assert_eq!(written["execution_authority"], false);
        let bytes = std::fs::read(&record).unwrap();
        let stored: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(stored["proposal"]["action"], action);
        assert_eq!(stored["assumptions"]["fee_per_side_microusd"], 7);
        let duplicate = create();
        assert!(!duplicate.status.success());
        assert!(String::from_utf8_lossy(&duplicate.stderr).contains("publication refused"));
        assert_eq!(std::fs::read(&record).unwrap(), bytes);
        let target = stored["outcome_not_before_unix_ms"].as_u64().unwrap();
        capture(&exit, target);
        let score = |at: u64| {
            command(&[
                "alpaca-paper-decision-score",
                "--decision",
                record.to_str().unwrap(),
                "--exit",
                exit.to_str().unwrap(),
                "--exit-as-of-unix-ms",
                &at.to_string(),
            ])
        };
        let early = score(target - 1);
        assert!(!early.status.success());
        assert!(
            String::from_utf8_lossy(&early.stderr)
                .contains("differs from frozen evaluation horizon")
        );
        assert!(!score(target + 1).status.success());
        let outcome = report(&score(target));
        assert_eq!(outcome["status"], "scored");
        assert_eq!(
            outcome["always_enter_baseline"]["net_pnl_microusd"],
            "-30015"
        );
        assert_eq!(
            outcome["strategy_net_pnl_microusd"],
            if action == "wait" { "0" } else { "-30015" }
        );
        assert_eq!(
            outcome["strategy_minus_enter_microusd"],
            if action == "wait" { "30015" } else { "0" }
        );
        assert_eq!(outcome["decision_origin_verified"], false);
        std::fs::write(&exit, "{}").unwrap();
        let unknown = report(&score(target));
        assert_eq!(unknown["status"], "unscoreable");
        assert_eq!(unknown["strategy_net_pnl_microusd"], Value::Null);
        assert_eq!(unknown["always_enter_baseline"], Value::Null);
        assert!(unknown["unscoreable_reason"].is_string());
        // Frozen record owns its assumptions, not the now-edited proposal files.
        std::fs::write(&assumptions, "{}").unwrap();
        std::fs::write(&proposal, "{}").unwrap();
        capture(&exit, target);
        assert_eq!(report(&score(target))["status"], "scored");
        std::fs::write(
            &assumptions,
            serde_json::to_vec(&stored["assumptions"]).unwrap(),
        )
        .unwrap();
    }
}
