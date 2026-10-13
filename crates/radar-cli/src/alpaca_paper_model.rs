// SPDX-License-Identifier: Apache-2.0
//! Explicit operator assumptions for fixed-host paper execution, never live evidence.

use radar_journal::BrokerIntent;
use radar_risk::equity::State;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const VERSION: &str = "alpaca-paper-stress-v1";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Model {
    version: String,
    account_id: String,
    max_quantity_nanoshares: u64,
    fee_per_side_microusd: u64,
    slippage_bps_per_side: u64,
}

impl Model {
    pub(super) fn validate(&self, account: &str) -> Result<(), String> {
        if self.version != VERSION
            || account.trim().is_empty()
            || self.account_id != account
            || self.max_quantity_nanoshares == 0
            || self.fee_per_side_microusd == 0
            || !(1..=10_000).contains(&self.slippage_bps_per_side)
        {
            return Err("invalid explicit paper stress model".into());
        }
        Ok(())
    }

    fn costs(&self, quantity: u64, limit: u64, bid: u64, ask: u64) -> Result<u64, String> {
        if quantity == 0 || limit == 0 || bid == 0 || ask < bid {
            return Err("paper stress model requires positive noncrossed quote inputs".into());
        }
        let shares = u128::from(quantity);
        let rounded = |value: u128, scale| {
            u64::try_from(value.div_ceil(scale)).map_err(|_| "paper stress cost overflow".to_owned())
        };
        let spread = rounded(shares * u128::from(ask - bid), 1_000_000_000)?;
        // Charge each side at the larger quote/limit value. A reduction's limit
        // may be below bid; using its lower limit would understate scenario costs.
        let notional = rounded(shares * u128::from(limit.max(ask)), 1_000_000_000)?;
        let slippage = rounded(
            u128::from(notional) * u128::from(self.slippage_bps_per_side),
            10_000,
        )?;
        self.fee_per_side_microusd
            .checked_add(slippage)
            .and_then(|side| side.checked_mul(2))
            .and_then(|round_trip| round_trip.checked_add(spread))
            .ok_or_else(|| "paper stress cost overflow".into())
    }

    pub(super) fn apply(&self, intent: &BrokerIntent, state: &mut State) -> Result<Value, String> {
        self.validate(&intent.account_id)?;
        if state.account_id != intent.account_id
            || state.capacity_nanoshares.is_some()
            || state.required_fee_microusd.is_some()
            || state.round_trip_cost_microusd.is_some()
        {
            return Err("paper model cannot replace existing execution evidence or another account".into());
        }
        let cost = self.costs(
            intent.quantity_nanoshares,
            intent.limit_microusd_per_share,
            state.bid_microusd_per_share,
            state.ask_microusd_per_share,
        )?;
        // Alpaca paper ignores displayed quantity and omits live costs. This
        // finite scenario ceiling is NOT observed liquidity. Risk still checks
        // cash, owned shares, policy ceilings, quote/session age and revocation.
        state.capacity_nanoshares = Some(self.max_quantity_nanoshares);
        state.required_fee_microusd = Some(self.fee_per_side_microusd);
        state.round_trip_cost_microusd = Some(cost);
        Ok(
            json!({"assumptions":self,"environment":"paper","basis":"operator_stress_scenario",
            "round_trip_cost_microusd":cost.to_string(),"live_capacity_verified":false,
            "live_costs_verified":false,"profit_measured":false}),
        )
    }
}

/// Re-read the operator file both before risk evaluation and after durable
/// submission uncertainty. Changed/removed/invalid assumptions must recollect.
pub(super) fn unchanged(expected: &Option<Model>, path: Option<&str>) -> Result<(), String> {
    let current: Option<Model> = path.map(super::alpaca_preflight::load).transpose()?;
    if &current != expected {
        return Err("paper stress model changed; recollect before any submission".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        serde_json::from_value(json!({"version":VERSION,"account_id":"paper",
            "max_quantity_nanoshares":1_000_000_000,"fee_per_side_microusd":7,
            "slippage_bps_per_side":10}))
        .unwrap()
    }

    #[test]
    fn exact_fractional_costs_round_up_and_use_the_larger_quote_or_limit() {
        let model = model();
        assert_eq!(
            model.costs(1_000_000_000, 100_000_000, 99_000_000, 100_000_000).unwrap(),
            1_200_014
        );
        assert_eq!(
            model.costs(1, 100_000_001, 100_000_000, 100_000_001).unwrap(),
            17
        );
        assert_eq!(
            model.costs(1_000_000_000, 99_000_000, 100_000_000, 101_000_000).unwrap(),
            1_202_014
        );
        assert_eq!(
            model.costs(1_000_000_000, 102_000_000, 100_000_000, 101_000_000).unwrap(),
            1_204_014
        );
    }

    #[test]
    fn schema_account_and_positive_finite_assumptions_are_explicit() {
        let value = serde_json::to_value(model()).unwrap();
        model().validate("paper").unwrap();
        for account in ["", " ", "other"] {
            assert!(model().validate(account).is_err());
        }
        for (field, wrong) in [
            ("version", json!("live")),
            ("account_id", json!("other")),
            ("max_quantity_nanoshares", json!(0)),
            ("fee_per_side_microusd", json!(0)),
            ("slippage_bps_per_side", json!(0)),
            ("slippage_bps_per_side", json!(10_001)),
        ] {
            let mut changed = value.clone();
            changed[field] = wrong;
            let model: Model = serde_json::from_value(changed).unwrap();
            assert!(model.validate("paper").is_err());
        }
        for field in [
            "version",
            "account_id",
            "max_quantity_nanoshares",
            "fee_per_side_microusd",
            "slippage_bps_per_side",
        ] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<Model>(missing).is_err());
        }
        for wrong in [Value::Null, json!(-1), json!(true), json!(1.5)] {
            let mut changed = value.clone();
            changed["max_quantity_nanoshares"] = wrong;
            assert!(serde_json::from_value::<Model>(changed).is_err());
        }
        let mut unknown = value;
        unknown["live_capacity_verified"] = json!(true);
        assert!(serde_json::from_value::<Model>(unknown).is_err());
    }

    #[test]
    fn invalid_cost_inputs_and_overflow_refuse() {
        let mut model = model();
        for (quantity, limit, bid, ask) in [
            (0, 1, 1, 1),
            (1, 0, 1, 1),
            (1, 1, 0, 1),
            (1, 1, 2, 1),
            (u64::MAX, u64::MAX, 1, u64::MAX),
        ] {
            assert!(model.costs(quantity, limit, bid, ask).is_err());
        }
        model.fee_per_side_microusd = u64::MAX;
        assert!(model.costs(1, 1, 1, 1).is_err());
    }

    #[test]
    fn changed_missing_invalid_or_injected_model_cannot_pass_the_reload_guard() {
        unchanged(&None, None).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("model.json");
        let name = path.to_str().unwrap();
        let expected = Some(model());
        std::fs::write(&path, serde_json::to_vec(&expected).unwrap()).unwrap();
        unchanged(&expected, Some(name)).unwrap();
        assert!(unchanged(&expected, None).is_err());
        assert!(unchanged(&None, Some(name)).is_err());
        let mut changed = model();
        changed.fee_per_side_microusd += 1;
        std::fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(unchanged(&expected, Some(name)).is_err());
        std::fs::write(&path, "{private-invalid-value}").unwrap();
        assert!(unchanged(&expected, Some(name)).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(unchanged(&expected, Some(name)).is_err());
    }
}
