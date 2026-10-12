// SPDX-License-Identifier: Apache-2.0
//! Real CLI/provider subprocess; fixtures never contact a model or broker.
#![cfg(unix)]

use std::path::Path;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

fn inputs(
    capture: &Path,
    experiment: &Path,
    assumptions: &Path,
    script: &Path,
    prompt: &Path,
    reply: &Path,
) {
    let at = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
        - 1000;
    let timestamp = radar_types::civil::timestamp_from_seconds(at / 1000);
    let raw = json!({"SPY":{"latestQuote":{"t":timestamp,"bp":100,"ap":100.01,
        "bs":20,"as":30,"bx":"V","ax":"V","c":["R"],"z":"B"}}});
    std::fs::write(
        capture,
        json!({"format":"alpaca-iex-snapshot-v1","read_started_unix_ms":at-1,
        "available_at_unix_ms":at,"symbols":["SPY"],"raw_response":raw.to_string()})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        experiment,
        json!({"strategy_id":"fixture-v1","quantity_nanoshares":1_000_000_000_u64,
        "horizon_ms":60000})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        assumptions,
        json!({"model_id":"stress-v1","displayed_size_unit":"shares",
        "participation_bps":1000,"slippage_bps_per_side":1,"fee_per_side_microusd":7,
        "max_quote_age_ms":30000,"max_spread_bps":10})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        script,
        format!("cat > '{}'\ncat '{}'\n", prompt.display(), reply.display()),
    )
    .unwrap();
}

fn refuse_bad_experiments(experiment: &Path, prompt: &Path, invoke: impl Fn(&str) -> Output) {
    let original = std::fs::read(experiment).unwrap();
    for (strategy, horizon, error) in [
        ("", 60000, "invalid fixed research experiment"),
        ("fixture-v1", 0, "invalid fixed research experiment"),
        ("fixture-v1", u64::MAX, "experiment horizon overflow"),
    ] {
        std::fs::write(
            experiment,
            json!({"strategy_id":strategy,"quantity_nanoshares":1_000_000_000_u64,
                "horizon_ms":horizon})
            .to_string(),
        )
        .unwrap();
        let result = invoke("unlimited");
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains(error));
        assert!(!prompt.exists(), "invalid experiment called the model");
    }
    std::fs::write(experiment, original).unwrap();
}

#[test]
fn subscription_command_receives_instructions_and_only_records_valid_research() {
    let dir = tempfile::tempdir().unwrap();
    let path = |name| dir.path().join(name);
    let capture = path("capture.json");
    let experiment = path("experiment.json");
    let assumptions = path("assumptions.json");
    let output = path("output.json");
    let script = path("stub.sh");
    let prompt = path("prompt.txt");
    let reply = path("reply.json");
    inputs(
        &capture,
        &experiment,
        &assumptions,
        &script,
        &prompt,
        &reply,
    );
    let invoke = |daily: &str| -> Output {
        Command::new(env!("CARGO_BIN_EXE_radar"))
            .args(["alpaca-paper-propose", "--capture"])
            .arg(&capture)
            .args(["--symbol", "SPY", "--experiment"])
            .arg(&experiment)
            .arg("--assumptions")
            .arg(&assumptions)
            .arg("--output")
            .arg(&output)
            .env("RADAR_MODEL_CODEX", format!("sh {}", script.display()))
            .env("RADAR_MODEL_DAILY_USD", daily)
            .env_remove("RADAR_MODEL_PER_CALL_USD")
            .env_remove("RADAR_MODEL_API_KEY")
            .env_remove("RADAR_MODEL_OPENAI_KEY")
            .env_remove("RADAR_ALPACA_PAPER_KEY_ID")
            .env_remove("RADAR_ALPACA_PAPER_SECRET_KEY")
            .output()
            .unwrap()
    };
    refuse_bad_experiments(&experiment, &prompt, invoke);
    assert!(!invoke("1.00").status.success());
    assert!(!prompt.exists());
    std::fs::write(
        &reply,
        r#"{"action":"enter","reason":"test only","quantity_nanoshares":99}"#,
    )
    .unwrap();
    assert!(!invoke("unlimited").status.success());
    assert!(!output.exists());
    std::fs::write(
        &reply,
        r#"{"action":"wait","reason":"one quote is insufficient"}"#,
    )
    .unwrap();
    let result = invoke("unlimited");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["model_calls_attempted"], 1);
    assert_eq!(report["reported_cost_microusd"], Value::Null);
    assert_eq!(report["nominal_usage_microusd"], "10000");
    let sent = std::fs::read_to_string(&prompt).unwrap();
    assert!(sent.starts_with("You are Radar's equity paper research analyst."));
    assert!(sent.contains("No markdown, extra fields or guarantees of profit."));
    assert_eq!(sent.matches("<<<RADAR-UNTRUSTED>>>").count(), 6);
    let bytes = std::fs::read(&output).unwrap();
    let record: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(record["proposal"]["action"], "wait");
    assert_eq!(record["proposal"]["quantity_nanoshares"], 1_000_000_000_u64);
    assert!(
        record["model_observation_as_of_unix_ms"].as_u64().unwrap()
            <= record["frozen_at_unix_ms"].as_u64().unwrap()
    );
    std::fs::write(&reply, r#"{"action":"enter","reason":"replacing"}"#).unwrap();
    assert!(!invoke("unlimited").status.success());
    assert_eq!(std::fs::read(&output).unwrap(), bytes);
    assert_eq!(std::fs::read_to_string(&prompt).unwrap(), sent);

    // A new, explicitly versioned experiment requires context before the model
    // is called. The quote-only path above remains unchanged.
    std::fs::remove_file(&output).unwrap();
    std::fs::remove_file(&prompt).unwrap();
    let mut plan: Value = serde_json::from_slice(&std::fs::read(&experiment).unwrap()).unwrap();
    plan["strategy_id"] = json!("snapshot-context-v1");
    std::fs::write(&experiment, plan.to_string()).unwrap();
    assert!(!invoke("unlimited").status.success());
    assert!(!prompt.exists(), "missing context must not call the model");
    assert!(!output.exists());
    let mut captured: Value = serde_json::from_slice(&std::fs::read(&capture).unwrap()).unwrap();
    let at = captured["available_at_unix_ms"].as_u64().unwrap();
    let mut raw: Value = serde_json::from_str(captured["raw_response"].as_str().unwrap()).unwrap();
    raw["SPY"]["minuteBar"] = json!({"t":radar_types::civil::timestamp_from_seconds(at / 60000 * 60 - 60),
        "o":99,"h":101,"l":98,"c":100,"v":100});
    raw["SPY"]["prevDailyBar"] = json!({"t":radar_types::civil::timestamp_from_seconds(at / 1000 - 172800),
        "o":98,"h":101,"l":97,"c":99,"v":5000});
    captured["raw_response"] = json!(raw.to_string());
    std::fs::write(&capture, captured.to_string()).unwrap();
    let mut costs: Value = serde_json::from_slice(&std::fs::read(&assumptions).unwrap()).unwrap();
    costs["max_quote_age_ms"] = json!(120000);
    std::fs::write(&assumptions, costs.to_string()).unwrap();
    let result = invoke("unlimited");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let contextual_prompt = std::fs::read_to_string(&prompt).unwrap();
    assert_eq!(contextual_prompt.matches("<<<RADAR-UNTRUSTED>>>").count(), 8);
    assert!(contextual_prompt.contains("incomplete IEX coverage"));
    assert!(contextual_prompt.contains("close_microusd_per_share"));
    let record: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(record["proposal"]["strategy_id"], "snapshot-context-v1");
    assert_eq!(record["entry_raw_response"], raw.to_string());
}
