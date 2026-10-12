// SPDX-License-Identifier: Apache-2.0
//! Quantity consistency for the actual durable paper cash checkpoint caller.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::alpaca_activities::Activity;
use super::alpaca_exposure::nanos;
use super::alpaca_read::Position;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Holding {
    asset_id: String,
    nanoshares: u64,
}

pub(super) type Holdings = BTreeMap<String, Holding>;

fn validate(holdings: &Holdings) -> Result<(), String> {
    let mut assets = BTreeSet::new();
    for (symbol, holding) in holdings {
        if symbol.trim().is_empty()
            || holding.asset_id.trim().is_empty()
            || holding.nanoshares == 0
            || !assets.insert(&holding.asset_id)
        {
            return Err("unsupported or ambiguous paper holding".into());
        }
    }
    Ok(())
}

pub(super) fn read(raw: &str) -> Result<Holdings, String> {
    let positions: Vec<Position> =
        serde_json::from_str(raw).map_err(|_| "invalid paper position observation")?;
    let mut holdings = BTreeMap::new();
    for position in positions {
        if position.asset_class != "us_equity" || position.side != "long" {
            return Err("paper position is outside the long-only equity lane".into());
        }
        let holding = Holding {
            asset_id: position.asset_id,
            nanoshares: nanos(&position.qty)?,
        };
        if holdings.insert(position.symbol, holding).is_some() {
            return Err("duplicate paper position symbol".into());
        }
    }
    validate(&holdings)?;
    Ok(holdings)
}

pub(super) fn retained(observation: &Value) -> Result<Holdings, String> {
    // Older cash checkpoints cannot establish a share opening balance. Never
    // silently bootstrap over them, even if today's broker positions are flat.
    let holdings: Holdings = serde_json::from_value(observation["position_quantities"].clone())
        .map_err(|_| "durable paper position baseline is missing or invalid")?;
    validate(&holdings)?;
    Ok(holdings)
}

pub(super) fn reconcile(
    prior: &Holdings,
    current: &Holdings,
    previous: &[Activity],
    observed: &[Activity],
) -> Result<(), String> {
    let old_ids: BTreeSet<_> = previous.iter().map(|event| &event.id).collect();
    let mut expected: BTreeMap<_, _> = prior
        .iter()
        .map(|(symbol, holding)| (symbol.clone(), i128::from(holding.nanoshares)))
        .collect();
    for event in observed {
        if old_ids.contains(&event.id) || event.category != "FILL" {
            continue;
        }
        // Cash reconciliation already binds these new fills to retained intents
        // and validates their complete execution intervals. Use exact share units.
        let symbol = event
            .symbol
            .as_ref()
            .ok_or("position fill symbol is unknown")?;
        let qty = i128::from(nanos(
            event
                .qty
                .as_deref()
                .ok_or("position fill quantity is unknown")?,
        )?);
        let change = match event.side.as_deref() {
            Some("buy") => qty,
            Some("sell") => -qty,
            _ => return Err("position fill side is unknown".into()),
        };
        let quantity = expected.entry(symbol.clone()).or_default();
        *quantity = quantity
            .checked_add(change)
            .ok_or("position delta overflow")?;
    }
    for (symbol, holding) in current {
        if prior
            .get(symbol)
            .is_some_and(|old| old.asset_id != holding.asset_id)
        {
            return Err("paper position asset identity changed".into());
        }
        if expected.remove(symbol) != Some(i128::from(holding.nanoshares)) {
            return Err("paper position differs from selected fill delta".into());
        }
    }
    if expected.values().any(|quantity| *quantity != 0) {
        return Err("paper position disappeared or has unexplained shares".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn position(symbol: &str, qty: &str) -> Value {
        json!({"asset_id":format!("asset_{symbol}"),"symbol":symbol,"asset_class":"us_equity",
            "side":"long","qty":qty,"cost_basis":"1"})
    }

    fn fill(id: &str, symbol: &str, side: &str, qty: &str) -> Activity {
        serde_json::from_value(
            json!({"id":id,"activity_type":"FILL","symbol":symbol,"side":side,"qty":qty}),
        )
        .unwrap()
    }

    #[test]
    fn exact_fractional_deltas_close_positions_and_are_independent_of_activity_order() {
        let prior =
            read(&json!([position("SPY", "1.000000001"), position("QQQ", "2")]).to_string())
                .unwrap();
        let current =
            read(&json!([position("SPY", "0.500000002"), position("BTC", "1")]).to_string())
                .unwrap();
        let previous = [fill("old", "SPY", "buy", "1.000000001")];
        let mut events = vec![
            fill("old", "SPY", "buy", "1.000000001"),
            fill("buy", "SPY", "buy", "0.000000001"),
            fill("sell", "SPY", "sell", "0.5"),
            fill("close", "QQQ", "sell", "2"),
            fill("open", "BTC", "buy", "1"),
            serde_json::from_value(json!({"id":"cash","activity_type":"CSD"})).unwrap(),
        ];
        assert!(reconcile(&prior, &current, &previous, &events).is_ok());
        events.reverse();
        assert!(reconcile(&prior, &current, &previous, &events).is_ok());
        assert!(reconcile(&prior, &prior, &previous, &previous).is_ok());
        assert!(reconcile(&Holdings::new(), &Holdings::new(), &[], &[]).is_ok());
        for change in 0..5 {
            let mut value = serde_json::to_value(&current).unwrap();
            match change {
                0 => value["SPY"]["nanoshares"] = json!(500_000_001),
                1 => value["SPY"]["asset_id"] = json!("other"),
                2 => {
                    value.as_object_mut().unwrap().remove("SPY");
                }
                3 => value["QQQ"] = json!({"asset_id":"asset_QQQ","nanoshares":1}),
                _ => {
                    value.as_object_mut().unwrap().remove("BTC");
                }
            }
            let wrong = serde_json::from_value(value).unwrap();
            assert!(reconcile(&prior, &wrong, &previous, &events).is_err());
        }
        assert!(
            reconcile(
                &Holdings::new(),
                &Holdings::new(),
                &[],
                &[fill("short", "SPY", "sell", "1")]
            )
            .is_err()
        );
        assert!(reconcile(&prior, &Holdings::new(), &[], &[]).is_err());
        for field in ["symbol", "side", "qty"] {
            let mut event = serde_json::to_value(fill("bad", "SPY", "buy", "1")).unwrap();
            event[field] = Value::Null;
            assert!(
                reconcile(
                    &prior,
                    &current,
                    &[],
                    &[serde_json::from_value(event).unwrap()]
                )
                .is_err()
            );
        }
        assert!(
            reconcile(
                &prior,
                &current,
                &[],
                &[fill("bad", "SPY", "sell_short", "1")]
            )
            .is_err()
        );
    }

    #[test]
    fn unsupported_positions_and_missing_or_invalid_retained_balances_refuse() {
        assert!(read("[]").unwrap().is_empty());
        let good = position("SPY", "1");
        let valid = read(&json!([good]).to_string()).unwrap();
        assert_eq!(
            retained(&json!({"position_quantities":valid})).unwrap(),
            valid
        );
        for value in [
            json!({}),
            json!({"position_quantities":null}),
            json!({"position_quantities":{"SPY":{"asset_id":"asset","nanoshares":0}}}),
            json!({"position_quantities":{"SPY":{"asset_id":" ","nanoshares":1}}}),
            json!({"position_quantities":{" ":{"asset_id":"asset","nanoshares":1}}}),
            json!({"position_quantities":{"SPY":{"asset_id":"same","nanoshares":1},"QQQ":{"asset_id":"same","nanoshares":1}}}),
        ] {
            assert!(retained(&value).is_err());
        }
        for (field, value) in [
            ("asset_class", "crypto"),
            ("side", "short"),
            ("symbol", " "),
            ("asset_id", " "),
            ("qty", "0"),
            ("qty", "-1"),
            ("qty", "0.0000000001"),
        ] {
            let mut bad = good.clone();
            bad[field] = json!(value);
            assert!(read(&json!([bad]).to_string()).is_err());
        }
        assert!(read(&json!([good, good]).to_string()).is_err());
        let mut duplicate_asset = position("QQQ", "1");
        duplicate_asset["asset_id"] = good["asset_id"].clone();
        assert!(read(&json!([good, duplicate_asset]).to_string()).is_err());
        for raw in ["null", "{}", "[{}]", "private malformed"] {
            assert!(read(raw).is_err());
        }
    }
}
