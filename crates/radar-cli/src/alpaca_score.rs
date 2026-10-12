// SPDX-License-Identifier: Apache-2.0
//! Offline long-only outcome scoring. Assumptions never become execution facts.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Assumptions {
    model_id: String,
    displayed_size_unit: String,
    participation_bps: u16,
    slippage_bps_per_side: u16,
    fee_per_side_microusd: u64,
    max_quote_age_ms: u64,
    max_spread_bps: u64,
}

fn amount(quote: &Value, field: &str) -> Result<u64, String> {
    quote[field]
        .as_str()
        .ok_or("scoring quote amount missing")?
        .parse()
        .map_err(|_| "scoring quote amount invalid".into())
}

fn bounded(value: u128) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| "paper scoring amount overflow".into())
}

fn score(
    entry: &Value,
    exit: &Value,
    quantity: u64,
    assumptions: &Assumptions,
) -> Result<Value, String> {
    if quantity == 0
        || assumptions.model_id.trim().is_empty()
        || assumptions.displayed_size_unit != "shares"
        || assumptions.participation_bps == 0
        || assumptions.participation_bps > 10_000
        || assumptions.slippage_bps_per_side > 10_000
    {
        return Err("invalid explicit paper scoring assumptions".into());
    }
    let entry_at = entry["as_of_unix_ms"]
        .as_u64()
        .ok_or("entry watermark missing")?;
    let exit_at = exit["as_of_unix_ms"]
        .as_u64()
        .ok_or("exit watermark missing")?;
    if exit_at <= entry_at || entry["symbol"] != exit["symbol"] {
        return Err("paper score requires a later exit for the same symbol".into());
    }
    // Sizes remain an explicit shares assumption. Never inject this capacity
    // into State or treat a paper broker fill as proof of routed liquidity.
    let displayed = amount(entry, "ask_displayed_size_millionths")?
        .min(amount(exit, "bid_displayed_size_millionths")?);
    let capacity = bounded(
        u128::from(displayed) * 1_000 * u128::from(assumptions.participation_bps) / 10_000,
    )?;
    if quantity > capacity {
        return Err("paper score quantity exceeds assumed displayed participation".into());
    }
    // Buy at ask, sell at bid. Spread is already paid here; do not deduct it twice.
    let entry_value = bounded(
        (u128::from(quantity) * u128::from(amount(entry, "ask_microusd_per_share")?))
            .div_ceil(1_000_000_000),
    )?;
    let exit_value = bounded(
        u128::from(quantity) * u128::from(amount(exit, "bid_microusd_per_share")?)
            / 1_000_000_000,
    )?;
    let slippage = |value: u64| {
        bounded((u128::from(value) * u128::from(assumptions.slippage_bps_per_side)).div_ceil(10_000))
    };
    let entry_slippage = slippage(entry_value)?;
    let exit_slippage = slippage(exit_value)?;
    let fees = assumptions
        .fee_per_side_microusd
        .checked_mul(2)
        .ok_or("paper fee overflow")?;
    let costs = entry_slippage
        .checked_add(exit_slippage)
        .and_then(|value| value.checked_add(fees))
        .ok_or("paper cost overflow")?;
    let gross = i128::from(exit_value) - i128::from(entry_value);
    let net = gross - i128::from(costs);
    Ok(json!({"version":1,"venue":"alpaca","environment":"offline_simulation",
        "symbol":entry["symbol"],"quantity_nanoshares":quantity.to_string(),
        "entry_as_of_unix_ms":entry_at,"exit_as_of_unix_ms":exit_at,
        "entry_quote":entry,"exit_quote":exit,"assumptions":assumptions,
        "assumed_capacity_nanoshares":capacity.to_string(),"capacity_verified":false,
        "entry_notional_microusd":entry_value.to_string(),"exit_notional_microusd":exit_value.to_string(),
        "entry_slippage_microusd":entry_slippage.to_string(),"exit_slippage_microusd":exit_slippage.to_string(),
        "fees_microusd":fees.to_string(),"extra_cost_microusd":costs.to_string(),
        "gross_pnl_microusd":gross.to_string(),"net_pnl_microusd":net.to_string(),
        "realized_broker_pnl":false,"execution_authority":false}))
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<_> = args.iter().map(String::as_str).collect();
    let [
        "alpaca-paper-score",
        "--entry",
        entry,
        "--entry-as-of-unix-ms",
        entry_at,
        "--exit",
        exit,
        "--exit-as-of-unix-ms",
        exit_at,
        "--symbol",
        symbol,
        "--quantity-nanoshares",
        quantity,
        "--assumptions",
        path,
    ] = values.as_slice()
    else {
        return Err("usage: radar alpaca-paper-score --entry <capture> --entry-as-of-unix-ms <N> --exit <capture> --exit-as-of-unix-ms <N> --symbol SPY --quantity-nanoshares <N> --assumptions <json>".into());
    };
    let assumptions: Assumptions = super::alpaca_preflight::load(path)?;
    let read = |path: &str, at: &str| {
        let at = at.parse().map_err(|_| "invalid score watermark")?;
        let (raw, available) = super::alpaca_market::quote_input(Path::new(path), symbol, at)?;
        super::alpaca_quote::validate(
            &raw,
            symbol,
            available,
            at,
            assumptions.max_quote_age_ms,
            assumptions.max_spread_bps,
        )
    };
    let entry = read(entry, entry_at)?;
    let exit = read(exit, exit_at)?;
    let quantity = quantity.parse().map_err(|_| "invalid scoring quantity")?;
    println!("{}", score(&entry, &exit, quantity, &assumptions)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assumptions() -> Assumptions {
        Assumptions {
            model_id: "stress-1".into(),
            displayed_size_unit: "shares".into(),
            participation_bps: 1_000,
            slippage_bps_per_side: 1,
            fee_per_side_microusd: 7,
            max_quote_age_ms: 1_000,
            max_spread_bps: 100,
        }
    }

    fn quotes() -> (Value, Value) {
        let raw = r#"{"SPY":{"latestQuote":{"t":"2026-10-21T00:00:00Z","bp":100,"ap":100.01,"bs":20.5,"as":30,"bx":"V","ax":"V","c":["R"],"z":"B"}}}"#;
        let at = 1_792_540_800_000;
        let entry = super::super::alpaca_quote::validate(raw, "SPY", at, at, 0, 100).unwrap();
        let later = raw
            .replace("00:00:00Z", "00:00:01Z")
            .replace("100.01", "102")
            .replace("\"bp\":100,", "\"bp\":101.99,");
        let exit =
            super::super::alpaca_quote::validate(&later, "SPY", at + 1_000, at + 1_000, 0, 100).unwrap();
        (entry, exit)
    }

    #[test]
    fn ask_to_bid_charges_each_friction_once_and_preserves_assumptions() {
        let (entry, exit) = quotes();
        let report = score(&entry, &exit, 1_000_000_000, &assumptions()).unwrap();
        assert_eq!(report["entry_notional_microusd"], "100010000");
        assert_eq!(report["exit_notional_microusd"], "101990000");
        assert_eq!(report["gross_pnl_microusd"], "1980000");
        assert_eq!(report["extra_cost_microusd"], "20214");
        assert_eq!(report["net_pnl_microusd"], "1959786");
        assert_eq!(report["assumed_capacity_nanoshares"], "2050000000");
        assert_eq!(report["assumptions"]["model_id"], "stress-1");
        assert_eq!(report["capacity_verified"], false);
        assert_eq!(report["execution_authority"], false);
        assert_eq!(report["realized_broker_pnl"], false);
        assert!(score(&entry, &exit, 2_050_000_000, &assumptions()).is_ok());
        assert!(score(&entry, &exit, 2_050_000_001, &assumptions()).is_err());
    }

    #[test]
    fn rounding_losses_missing_inputs_and_overflow_never_improve_a_score() {
        let (entry, mut exit) = quotes();
        exit["bid_microusd_per_share"] = json!("100000000");
        let report = score(&entry, &exit, 1, &assumptions()).unwrap();
        assert_eq!(report["entry_notional_microusd"], "1");
        assert_eq!(report["exit_notional_microusd"], "0");
        assert_eq!(report["net_pnl_microusd"], "-16");
        assert!(score(&entry, &exit, 0, &assumptions()).is_err());
        for field in [
            "bid_microusd_per_share",
            "bid_displayed_size_millionths",
            "as_of_unix_ms",
        ] {
            let mut bad = exit.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(score(&entry, &bad, 1, &assumptions()).is_err());
        }
        exit["as_of_unix_ms"] = entry["as_of_unix_ms"].clone();
        assert!(score(&entry, &exit, 1, &assumptions()).is_err());
        exit["as_of_unix_ms"] = json!(entry["as_of_unix_ms"].as_u64().unwrap() - 1);
        assert!(score(&entry, &exit, 1, &assumptions()).is_err());
        let (entry, mut exit) = quotes();
        exit["symbol"] = json!("QQQ");
        assert!(score(&entry, &exit, 1, &assumptions()).is_err());
        let (entry, exit) = quotes();
        let mut model = assumptions();
        model.fee_per_side_microusd = u64::MAX;
        assert!(score(&entry, &exit, 1, &model).is_err());
        model.fee_per_side_microusd = u64::MAX / 2;
        assert!(score(&entry, &exit, 1_000_000_000, &model).is_err());
        assert_eq!(bounded(u128::from(u64::MAX)).unwrap(), u64::MAX);
        assert!(bounded(u128::from(u64::MAX) + 1).is_err());
        for bad in [Value::Null, json!(1), json!("-1"), json!("1e3")] {
            let mut malformed = entry.clone();
            malformed["ask_microusd_per_share"] = bad;
            assert!(score(&malformed, &exit, 1, &assumptions()).is_err());
        }
        for (participation, slippage, unit, id) in [
            (0, 1, "shares", "x"),
            (10_001, 1, "shares", "x"),
            (1_000, 10_001, "shares", "x"),
            (1_000, 1, "round_lots", "x"),
            (1_000, 1, "shares", " "),
        ] {
            let mut model = assumptions();
            model.participation_bps = participation;
            model.slippage_bps_per_side = slippage;
            model.displayed_size_unit = unit.into();
            model.model_id = id.into();
            assert!(score(&entry, &exit, 1, &model).is_err());
        }
    }

    #[test]
    fn explicit_zero_friction_and_maximum_stress_are_distinct_models() {
        let (entry, exit) = quotes();
        let mut model = assumptions();
        model.participation_bps = 10_000;
        model.slippage_bps_per_side = 0;
        model.fee_per_side_microusd = 0;
        let zero = score(&entry, &exit, 1_000_000_000, &model).unwrap();
        assert_eq!(zero["extra_cost_microusd"], "0");
        assert_eq!(zero["net_pnl_microusd"], "1980000");
        model.slippage_bps_per_side = 10_000;
        let stress = score(&entry, &exit, 1_000_000_000, &model).unwrap();
        assert_eq!(stress["extra_cost_microusd"], "202000000");
        assert_eq!(stress["net_pnl_microusd"], "-200020000");
    }
}
