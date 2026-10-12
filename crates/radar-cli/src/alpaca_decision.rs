// SPDX-License-Identifier: Apache-2.0
//! Local forward research records, never broker permissions or model attestations.

use std::io::{Read as _, Write as _};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::alpaca_score::Assumptions;

const MAX_RECORD: u64 = 2_097_152;

#[derive(Deserialize, Serialize)]
enum Format {
    #[serde(rename = "alpaca-forward-decision-v1")]
    V1,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Action {
    Enter,
    Wait,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Proposal {
    pub(super) strategy_id: String,
    pub(super) action: Action,
    pub(super) quantity_nanoshares: u64,
    pub(super) horizon_ms: u64,
    // Untrusted explanation, stored as data; never instructions to an executor.
    pub(super) reason: String,
}

pub(super) struct Input {
    pub(super) symbol: String,
    pub(super) raw: String,
    pub(super) available: u64,
    pub(super) as_of: u64,
    pub(super) model_observed: Option<u64>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    format: Format,
    symbol: String,
    frozen_at_unix_ms: u64,
    recorded_at_unix_ms: u64,
    outcome_not_before_unix_ms: u64,
    entry_available_at_unix_ms: u64,
    entry_raw_response: String,
    model_observation_as_of_unix_ms: Option<u64>,
    proposal: Proposal,
    assumptions: Assumptions,
}

fn now() -> Result<u64, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "decision host clock before epoch")?;
    u64::try_from(elapsed.as_millis()).map_err(|_| "decision clock overflow".into())
}

impl Decision {
    fn entry(&self) -> Result<Value, String> {
        self.assumptions
            .validate(self.proposal.quantity_nanoshares)?;
        if self.proposal.strategy_id.trim().is_empty()
            || self.proposal.reason.trim().is_empty()
            || self.frozen_at_unix_ms > self.recorded_at_unix_ms
            || self.recorded_at_unix_ms >= self.outcome_not_before_unix_ms
            || self.frozen_at_unix_ms.checked_add(self.proposal.horizon_ms)
                != Some(self.outcome_not_before_unix_ms)
            || self.model_observation_as_of_unix_ms.is_some_and(|at| {
                at > self.frozen_at_unix_ms || self.entry_available_at_unix_ms > at
            })
        {
            return Err("invalid forward decision identity or chronology".into());
        }
        super::alpaca_quote::validate(
            &self.entry_raw_response,
            &self.symbol,
            self.entry_available_at_unix_ms,
            self.frozen_at_unix_ms,
            self.assumptions.max_quote_age_ms,
            self.assumptions.max_spread_bps,
        )
    }
}

fn publish(path: &Path, decision: &Decision) -> Result<(), String> {
    publish_at(path, decision, now)
}

fn publish_at(
    path: &Path,
    decision: &Decision,
    clock: impl FnOnce() -> Result<u64, String>,
) -> Result<(), String> {
    decision.entry()?;
    let bytes = serde_json::to_vec(decision).map_err(|_| "decision encoding failed")?;
    if u64::try_from(bytes.len()).map_err(|_| "decision size overflow")? > MAX_RECORD {
        return Err("decision exceeds two MiB".into());
    }
    let mut pending = path.as_os_str().to_os_string();
    pending.push(format!(".{}.pending", std::process::id()));
    let pending = std::path::PathBuf::from(pending);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(&pending)
        .map_err(|_| "decision pending file unavailable")?;
    let result = (|| {
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| "decision write failed")?;
        drop(file);
        if clock()? >= decision.outcome_not_before_unix_ms {
            return Err("decision horizon passed before publication".into());
        }
        std::fs::hard_link(&pending, path)
            .map_err(|_| "decision publication refused; use a new filename".to_owned())
    })();
    let _ = std::fs::remove_file(&pending);
    result
}

fn load(path: &str) -> Result<Decision, String> {
    let file = std::fs::File::open(path).map_err(|_| "decision unavailable")?;
    let mut bytes = Vec::new();
    file.take(MAX_RECORD + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "decision read failed")?;
    if u64::try_from(bytes.len()).map_err(|_| "decision size overflow")? > MAX_RECORD {
        return Err("decision exceeds two MiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid typed decision".into())
}

fn outcome(decision: &Decision, exit: Result<Value, String>, at: u64) -> Result<Value, String> {
    let entry = decision.entry()?;
    if at != decision.outcome_not_before_unix_ms {
        return Err("outcome watermark differs from frozen evaluation horizon".into());
    }
    let baseline = exit.and_then(|exit| {
        super::alpaca_score::score(
            &entry,
            &exit,
            decision.proposal.quantity_nanoshares,
            &decision.assumptions,
        )
    });
    let (status, baseline, strategy, delta, error) = match baseline {
        Ok(report) => {
            let net: i128 = report["net_pnl_microusd"]
                .as_str()
                .ok_or("baseline result missing")?
                .parse()
                .map_err(|_| "baseline result invalid")?;
            let strategy = match decision.proposal.action {
                Action::Enter => net,
                Action::Wait => 0,
            };
            (
                "scored",
                report,
                Some(strategy.to_string()),
                Some((strategy - net).to_string()),
                None,
            )
        }
        Err(error) => ("unscoreable", Value::Null, None, None, Some(error)),
    };
    Ok(
        json!({"status":status,"decision":decision,"exit_as_of_unix_ms":at,
        "always_enter_baseline":baseline,"cash_baseline_net_pnl_microusd":"0",
        "strategy_net_pnl_microusd":strategy,"strategy_minus_enter_microusd":delta,
        "unscoreable_reason":error,"environment":"offline_simulation",
        "decision_origin_verified":false,"execution_authority":false}),
    )
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<_> = args.iter().map(String::as_str).collect();
    let report = match values.as_slice() {
        ["alpaca-paper-decision", "--capture", capture, "--symbol", symbol,
            "--proposal", proposal, "--assumptions", assumptions, "--output", output] => {
            let frozen = now()?;
            let proposal: Proposal = super::alpaca_preflight::load(proposal)?;
            let assumptions = super::alpaca_preflight::load(assumptions)?;
            let (raw, available) = super::alpaca_market::quote_input(Path::new(capture), symbol, frozen)?;
            record(Input { symbol:(*symbol).into(), raw, available, as_of:frozen,
                model_observed:None }, proposal, assumptions, Path::new(output))?
        }
        ["alpaca-paper-decision-score", "--decision", path, "--exit", exit, "--exit-as-of-unix-ms", at] => {
            let decision = load(path)?;
            let at = at.parse().map_err(|_| "invalid outcome watermark")?;
            let quote = super::alpaca_market::quote_input(Path::new(exit), &decision.symbol, at)
                .and_then(|(raw, available)| super::alpaca_quote::validate(&raw, &decision.symbol,
                    available, at, decision.assumptions.max_quote_age_ms, decision.assumptions.max_spread_bps));
            outcome(&decision, quote, at)?
        }
        _ => return Err("usage: radar alpaca-paper-decision --capture <capture> --symbol SPY --proposal <json> --assumptions <json> --output <new-file>; radar alpaca-paper-decision-score --decision <record> --exit <capture> --exit-as-of-unix-ms <N>".into()),
    };
    println!("{report}");
    Ok(())
}

pub(super) fn record(
    input: Input,
    proposal: Proposal,
    assumptions: Assumptions,
    output: &Path,
) -> Result<Value, String> {
    let decision = Decision {
        format: Format::V1,
        symbol: input.symbol,
        frozen_at_unix_ms: input.as_of,
        recorded_at_unix_ms: now()?,
        outcome_not_before_unix_ms: input.as_of.checked_add(proposal.horizon_ms)
            .ok_or("decision horizon overflow")?,
        entry_available_at_unix_ms: input.available,
        entry_raw_response: input.raw,
        model_observation_as_of_unix_ms: input.model_observed,
        proposal,
        assumptions,
    };
    publish(output, &decision)?;
    Ok(json!({"decision_written":true,"frozen_at_unix_ms":decision.frozen_at_unix_ms,
        "outcome_not_before_unix_ms":decision.outcome_not_before_unix_ms,
        "model_observation_as_of_unix_ms":decision.model_observation_as_of_unix_ms,
        "decision_origin_verified":false,"execution_authority":false}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision() -> Decision {
        let at = 1_792_540_800_000_u64;
        serde_json::from_value(json!({"format":"alpaca-forward-decision-v1","symbol":"SPY",
            "frozen_at_unix_ms":at,"recorded_at_unix_ms":at,"outcome_not_before_unix_ms":at+1000,
            "entry_available_at_unix_ms":at,
            "entry_raw_response":json!({"SPY":{"latestQuote":{"t":"2026-10-21T00:00:00Z",
                "bp":100,"ap":100.01,"bs":20,"as":30,"bx":"V","ax":"V","c":["R"],"z":"B"}}}).to_string(),
            "proposal":{"strategy_id":"unit-v1","action":"wait","quantity_nanoshares":1_000_000_000_u64,
                "horizon_ms":1000,"reason":"test"},
            "assumptions":{"model_id":"stress-v1","displayed_size_unit":"shares","participation_bps":1000,
                "slippage_bps_per_side":1,"fee_per_side_microusd":7,"max_quote_age_ms":0,"max_spread_bps":10}})).unwrap()
    }

    #[test]
    fn identity_and_all_chronology_boundaries_are_checked() {
        assert!(decision().entry().is_ok());
        for (field, value) in [
            ("strategy_id", ""),
            ("strategy_id", " "),
            ("reason", ""),
            ("reason", " "),
        ] {
            let mut value_json = serde_json::to_value(decision()).unwrap();
            value_json["proposal"][field] = json!(value);
            let value: Decision = serde_json::from_value(value_json).unwrap();
            assert!(value.entry().is_err());
        }
        let mut value = decision();
        value.recorded_at_unix_ms -= 1;
        assert!(value.entry().is_err());
        let mut value = decision();
        value.recorded_at_unix_ms = value.outcome_not_before_unix_ms;
        assert!(value.entry().is_err());
        value.recorded_at_unix_ms += 1;
        assert!(value.entry().is_err());
        let mut value = decision();
        value.recorded_at_unix_ms += 999;
        assert!(value.entry().is_ok());
        let mut value = decision();
        value.proposal.horizon_ms += 1;
        assert!(value.entry().is_err());
        value.proposal.horizon_ms = u64::MAX;
        assert!(value.entry().is_err());
        let mut value = decision();
        value.proposal.horizon_ms = 0;
        value.outcome_not_before_unix_ms = value.frozen_at_unix_ms;
        assert!(value.entry().is_err());
        let mut value = decision();
        value.entry_available_at_unix_ms += 1;
        assert!(value.entry().is_err());
        let mut value = decision();
        value.proposal.quantity_nanoshares = 0;
        assert!(value.entry().is_err());
        let mut value = decision();
        value.model_observation_as_of_unix_ms = Some(value.frozen_at_unix_ms);
        assert!(value.entry().is_ok());
        value.model_observation_as_of_unix_ms = Some(value.frozen_at_unix_ms + 1);
        assert!(value.entry().is_err());
        value.model_observation_as_of_unix_ms = Some(value.entry_available_at_unix_ms - 1);
        assert!(value.entry().is_err());
        let mut value = decision();
        value.model_observation_as_of_unix_ms = Some(value.frozen_at_unix_ms);
        value.frozen_at_unix_ms += 1;
        value.recorded_at_unix_ms += 1;
        value.outcome_not_before_unix_ms += 1;
        assert!(value.entry().is_err(), "quote age is rechecked after model completion");
    }

    #[test]
    fn invalid_or_oversized_records_and_elapsed_publication_refuse() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("decision.json");
        let mut value = decision();
        // Epoch fixture is used for pure chronology tests, not a live record.
        value.frozen_at_unix_ms = 1_000;
        value.recorded_at_unix_ms = 1_000;
        value.outcome_not_before_unix_ms = 2_000;
        value.entry_available_at_unix_ms = 1_000;
        value.entry_raw_response = value
            .entry_raw_response
            .replace("2026-10-21T00:00:00Z", "1970-01-01T00:00:01Z");
        assert_eq!(
            publish(&path, &value).unwrap_err(),
            "decision horizon passed before publication"
        );
        assert!(!path.exists());
        for clock in [2_000, 2_001] {
            assert_eq!(
                publish_at(&path, &value, || Ok(clock)).unwrap_err(),
                "decision horizon passed before publication"
            );
        }
        assert!(publish_at(&path, &value, || Ok(1_999)).is_ok());
        std::fs::remove_file(&path).unwrap();
        let base = serde_json::to_vec(&value).unwrap().len();
        value
            .entry_raw_response
            .push_str(&" ".repeat(usize::try_from(MAX_RECORD).unwrap() - base));
        assert_eq!(
            publish_at(&path, &value, || Ok(2_000)).unwrap_err(),
            "decision horizon passed before publication"
        );
        value.entry_raw_response.push(' ');
        assert_eq!(
            publish_at(&path, &value, || Ok(1_999)).unwrap_err(),
            "decision exceeds two MiB"
        );
        std::fs::write(&path, vec![b' '; usize::try_from(MAX_RECORD).unwrap()]).unwrap();
        assert_eq!(
            load(path.to_str().unwrap()).err().unwrap(),
            "invalid typed decision"
        );
        std::fs::write(&path, vec![b' '; usize::try_from(MAX_RECORD + 1).unwrap()]).unwrap();
        assert_eq!(
            load(path.to_str().unwrap()).err().unwrap(),
            "decision exceeds two MiB"
        );
        let mut encoded = serde_json::to_value(decision()).unwrap();
        encoded["format"] = json!("v2");
        assert!(serde_json::from_value::<Decision>(encoded).is_err());
    }
}
