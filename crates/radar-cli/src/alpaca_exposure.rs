// SPDX-License-Identifier: Apache-2.0
//! Pure, conservative bounds for the initial long-only equity lane.
//! These are broker observations, not available capital or execution authority.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::alpaca_read::{BrokerAccount, Position};
use super::alpaca_reconcile::BrokerOrder;

// Alpaca supports nine decimal places for fractional quantities. Keep original
// decimal digits; refuse unsupported precision/range instead of using f64.
pub(super) fn nanos(text: &str) -> Result<u64, String> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 9
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || (text.contains('.') && fraction.is_empty())
    {
        return Err("unsupported nonnegative broker decimal".into());
    }
    let whole: u64 = whole.parse().map_err(|_| "broker decimal overflow")?;
    let mut fraction = fraction.to_owned();
    while fraction.len() < 9 {
        fraction.push('0');
    }
    whole
        .checked_mul(1_000_000_000)
        .and_then(|v| v.checked_add(fraction.parse().ok()?))
        .ok_or_else(|| "broker decimal overflow".into())
}

fn required(value: Option<&str>) -> Result<u64, String> {
    nanos(value.ok_or("required broker decimal is unknown")?)
}

fn ceil_micros(value: u128, divisor: u128) -> Result<u64, String> {
    u64::try_from(value.div_ceil(divisor)).map_err(|_| "exposure overflow".into())
}

fn add(total: &mut u64, value: u64) -> Result<(), String> {
    *total = total.checked_add(value).ok_or("exposure overflow")?;
    Ok(())
}

fn held_positions(positions: &[Position]) -> Result<(BTreeMap<String, u64>, u64), String> {
    let mut held = BTreeMap::new();
    let mut assets = BTreeSet::new();
    let mut position_exposure = 0;
    for position in positions {
        if position.asset_class != "us_equity"
            || position.side != "long"
            || position.symbol.trim().is_empty()
            || position.asset_id.trim().is_empty()
            || !assets.insert(&position.asset_id)
        {
            return Err("unsupported or ambiguous broker position".into());
        }
        let qty = nanos(&position.qty)?;
        if qty == 0 || held.insert(position.symbol.clone(), qty).is_some() {
            return Err("zero or duplicate broker position".into());
        }
        let cost = nanos(&position.cost_basis)?;
        let value = required(position.market_value.as_deref())?;
        add(
            &mut position_exposure,
            ceil_micros(u128::from(cost.max(value)), 1_000)?,
        )?;
    }

    Ok((held, position_exposure))
}

fn pending_orders(orders: &[BrokerOrder]) -> Result<(u64, BTreeMap<String, u64>), String> {
    let mut buy_bound = 0;
    let mut sell_reserved = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for order in orders {
        if order.id.trim().is_empty() || !ids.insert(&order.id) {
            return Err("ambiguous pending order identity".into());
        }
        if order.asset_class.as_deref() != Some("us_equity")
            || order.kind.as_deref() != Some("limit")
            || order.order_class.as_deref() != Some("simple")
            || order.notional.is_some()
            || order.stop_price.is_some()
            || order.replaces.is_some()
            || order.replaced_by.is_some()
            || order.legs.as_ref().is_some_and(|legs| !legs.is_empty())
            || !matches!(
                order.status.as_str(),
                "new" | "accepted" | "pending_new" | "partially_filled" | "pending_cancel"
            )
        {
            return Err("pending order cannot be bounded by the simple equity lane".into());
        }
        let symbol = order.symbol.as_deref().ok_or("pending symbol is unknown")?;
        if symbol.trim().is_empty() {
            return Err("pending symbol is blank".into());
        }
        let qty = required(order.qty.as_deref())?;
        let filled = required(order.filled_qty.as_deref())?;
        let price = required(order.limit_price.as_deref())?;
        if qty == 0 || price == 0 || filled > qty {
            return Err("invalid pending quantity, fill or price".into());
        }
        // Reserve ORIGINAL quantity, including partial fills and pending cancels.
        // Positions/cash may reflect fills at a different instant; reducing this
        // bound by filled_qty could release capital/shares before reconciliation.
        match order.side.as_deref() {
            Some("buy") => add(
                &mut buy_bound,
                ceil_micros(u128::from(qty) * u128::from(price), 1_000_000_000_000)?,
            )?,
            Some("sell") => add(sell_reserved.entry(symbol.to_owned()).or_insert(0), qty)?,
            _ => return Err("pending side is unknown or unsupported".into()),
        }
    }
    Ok((buy_bound, sell_reserved))
}

pub(super) fn summarize(
    account: &BrokerAccount,
    orders: &[BrokerOrder],
    positions: &[Position],
) -> Result<Value, String> {
    if account.status != "ACTIVE"
        || account.currency.as_deref() != Some("USD")
        || account.trading_blocked != Some(false)
        || account.account_blocked != Some(false)
        || account.trade_suspended_by_user != Some(false)
    {
        return Err("account is not known active, USD and unrestricted".into());
    }
    // Round funds down and liabilities up. Never manufacture a spendable microdollar.
    let cash = required(account.cash.as_deref())? / 1_000;
    let buying_power = required(account.buying_power.as_deref())? / 1_000;
    let equity = required(account.equity.as_deref())? / 1_000;
    let (held, position_exposure) = held_positions(positions)?;
    let (buy_bound, sell_reserved) = pending_orders(orders)?;
    let mut unreserved_held = held.clone();
    for (symbol, reserved) in &sell_reserved {
        let available = unreserved_held
            .get_mut(symbol)
            .ok_or("pending sell has no observed long position")?;
        *available = available
            .checked_sub(*reserved)
            .ok_or("pending sells exceed observed long shares")?;
    }
    let cash_after_buys = cash
        .checked_sub(buy_bound)
        .ok_or("pending buy bounds exceed observed cash")?;
    // Broker buying_power already reflects open orders. Do NOT subtract them
    // again from buying_power. Apply the cash bound independently, then min.
    let cash_bound = cash_after_buys.min(buying_power).min(equity);
    let mut committed = position_exposure;
    add(&mut committed, buy_bound)?;
    let strings = |values: &BTreeMap<String, u64>| {
        values
            .iter()
            .map(|(key, value)| (key.clone(), value.to_string()))
            .collect::<BTreeMap<_, _>>()
    };
    Ok(json!({"pending_buy_bound_microusd":buy_bound.to_string(),
        "position_exposure_bound_microusd":position_exposure.to_string(),
        "committed_exposure_bound_microusd":committed.to_string(),
        "cash_bound_microusd":cash_bound.to_string(),
        "held_nanoshares":strings(&held),"reserved_sell_nanoshares":strings(&sell_reserved),
        "unreserved_held_nanoshares":strings(&unreserved_held),
        "fees_included":false,"local_claims_reconciled":false,"execution_authority":false}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account_value() -> Value {
        json!({"id":"paper-id","status":"ACTIVE","currency":"USD","cash":"50",
            "equity":"100","buying_power":"45","trading_blocked":false,
            "account_blocked":false,"trade_suspended_by_user":false})
    }

    fn position_value() -> Value {
        json!({"asset_id":"asset-id","symbol":"SPY","asset_class":"us_equity",
            "qty":"0.1","side":"long","market_value":"12","cost_basis":"10"})
    }

    fn order_value() -> Value {
        json!({"id":"order-id","status":"partially_filled","symbol":"SPY",
            "asset_class":"us_equity","side":"buy","type":"limit","order_class":"simple",
            "qty":"0.02","filled_qty":"0.01","limit_price":"100"})
    }

    fn evaluate(
        account: Value,
        orders: Vec<Value>,
        positions: Vec<Value>,
    ) -> Result<Value, String> {
        let orders: Vec<BrokerOrder> =
            serde_json::from_value(serde_json::to_value(orders).unwrap()).unwrap();
        let positions: Vec<Position> =
            serde_json::from_value(serde_json::to_value(positions).unwrap()).unwrap();
        summarize(
            &serde_json::from_value(account).unwrap(),
            &orders,
            &positions,
        )
    }

    #[test]
    fn partial_fills_stay_reserved_without_subtracting_orders_twice_from_buying_power() {
        let mut sell = order_value();
        sell["id"] = json!("sell-id");
        sell["side"] = json!("sell");
        sell["qty"] = json!("0.03");
        sell["status"] = json!("pending_cancel");
        let run = |account| {
            evaluate(
                account,
                vec![order_value(), sell.clone()],
                vec![position_value()],
            )
            .unwrap()
        };
        let bounds = run(account_value());
        assert_eq!(bounds["pending_buy_bound_microusd"], "2000000");
        assert_eq!(bounds["position_exposure_bound_microusd"], "12000000");
        assert_eq!(bounds["committed_exposure_bound_microusd"], "14000000");
        assert_eq!(bounds["cash_bound_microusd"], "45000000");
        assert_eq!(bounds["held_nanoshares"], json!({"SPY":"100000000"}));
        assert_eq!(
            bounds["reserved_sell_nanoshares"],
            json!({"SPY":"30000000"})
        );
        assert_eq!(
            bounds["unreserved_held_nanoshares"],
            json!({"SPY":"70000000"})
        );
        for flag in [
            "fees_included",
            "local_claims_reconciled",
            "execution_authority",
        ] {
            assert_eq!(bounds[flag], false);
        }
        let mut account = account_value();
        account["buying_power"] = json!("60");
        assert_eq!(run(account.clone())["cash_bound_microusd"], "48000000");
        account["equity"] = json!("40");
        assert_eq!(run(account)["cash_bound_microusd"], "40000000");
        let mut position = position_value();
        position["cost_basis"] = json!("15");
        assert_eq!(
            evaluate(account_value(), vec![], vec![position]).unwrap()["position_exposure_bound_microusd"],
            "15000000"
        );
    }

    #[test]
    fn active_statuses_and_multiple_reservations_do_not_depend_on_input_order() {
        for status in [
            "new",
            "accepted",
            "pending_new",
            "partially_filled",
            "pending_cancel",
        ] {
            let mut first = order_value();
            first["status"] = json!(status);
            first["legs"] = json!([]);
            let mut second = first.clone();
            second["id"] = json!("second-id");
            second["qty"] = json!("0.03");
            let forward =
                evaluate(account_value(), vec![first.clone(), second.clone()], vec![]).unwrap();
            let reverse =
                evaluate(account_value(), vec![second.clone(), first.clone()], vec![]).unwrap();
            assert_eq!(forward, reverse);
            assert_eq!(forward["pending_buy_bound_microusd"], "5000000");
            first["side"] = json!("sell");
            second["side"] = json!("sell");
            let sell =
                evaluate(account_value(), vec![first, second], vec![position_value()]).unwrap();
            assert_eq!(sell["reserved_sell_nanoshares"]["SPY"], "50000000");
        }
        let mut second = position_value();
        second["asset_id"] = json!("second-asset");
        second["symbol"] = json!("QQQ");
        let forward = evaluate(
            account_value(),
            vec![],
            vec![position_value(), second.clone()],
        )
        .unwrap();
        let reverse = evaluate(account_value(), vec![], vec![second, position_value()]).unwrap();
        assert_eq!(forward, reverse);
        assert_eq!(forward["position_exposure_bound_microusd"], "24000000");
    }

    #[test]
    fn account_unknowns_restrictions_and_negative_funds_refuse_bounds() {
        for (field, value) in [
            ("status", json!("INACTIVE")),
            ("currency", json!("EUR")),
            ("currency", Value::Null),
            ("trading_blocked", json!(true)),
            ("trading_blocked", Value::Null),
            ("account_blocked", json!(true)),
            ("account_blocked", Value::Null),
            ("trade_suspended_by_user", json!(true)),
            ("trade_suspended_by_user", Value::Null),
            ("cash", json!("-1")),
            ("cash", Value::Null),
            ("buying_power", json!("-1")),
            ("buying_power", Value::Null),
            ("equity", json!("-1")),
            ("equity", Value::Null),
        ] {
            let mut account = account_value();
            account[field] = value;
            assert!(evaluate(account, vec![], vec![]).is_err(), "{field}");
        }
        let bounds = evaluate(account_value(), vec![], vec![]).unwrap();
        assert_eq!(bounds["pending_buy_bound_microusd"], "0");
        assert_eq!(bounds["position_exposure_bound_microusd"], "0");
        assert_eq!(bounds["unreserved_held_nanoshares"], json!({}));
    }

    #[test]
    fn unsupported_or_ambiguous_pending_orders_never_become_zero_commitments() {
        for (field, value) in [
            ("id", json!("")),
            ("id", json!(" ")),
            ("asset_class", json!("crypto")),
            ("asset_class", Value::Null),
            ("type", json!("market")),
            ("type", Value::Null),
            ("order_class", json!("bracket")),
            ("order_class", Value::Null),
            ("notional", json!("2")),
            ("stop_price", json!("99")),
            ("replaces", json!("old-id")),
            ("replaced_by", json!("new-id")),
            ("legs", json!([{"id":"leg","status":"held"}])),
            ("status", json!("filled")),
            ("status", json!("pending_replace")),
            ("symbol", json!("")),
            ("symbol", json!(" ")),
            ("symbol", Value::Null),
            ("qty", json!("0")),
            ("qty", Value::Null),
            ("filled_qty", json!("0.03")),
            ("filled_qty", Value::Null),
            ("limit_price", json!("0")),
            ("limit_price", Value::Null),
            ("side", json!("unknown")),
            ("side", Value::Null),
        ] {
            let mut order = order_value();
            order[field] = value;
            assert!(
                evaluate(account_value(), vec![order], vec![position_value()]).is_err(),
                "{field}"
            );
        }
        assert!(evaluate(account_value(), vec![order_value(), order_value()], vec![]).is_err());
        let mut zero = order_value();
        zero["qty"] = json!("0");
        zero["filled_qty"] = json!("0");
        assert!(evaluate(account_value(), vec![zero], vec![]).is_err());
        let mut order = order_value();
        order["filled_qty"] = order["qty"].clone();
        assert_eq!(
            evaluate(account_value(), vec![order], vec![]).unwrap()["pending_buy_bound_microusd"],
            "2000000"
        );
    }

    #[test]
    fn bad_positions_and_oversold_shares_refuse_instead_of_creating_capacity() {
        for (field, value) in [
            ("asset_class", json!("crypto")),
            ("side", json!("short")),
            ("symbol", json!("")),
            ("symbol", json!(" ")),
            ("asset_id", json!("")),
            ("asset_id", json!(" ")),
            ("qty", json!("0")),
            ("qty", json!("-1")),
            ("cost_basis", json!("-1")),
            ("market_value", Value::Null),
            ("market_value", json!("-1")),
        ] {
            let mut position = position_value();
            position[field] = value;
            assert!(
                evaluate(account_value(), vec![], vec![position]).is_err(),
                "{field}"
            );
        }
        for field in ["asset_id", "symbol"] {
            let mut second = position_value();
            second[field] = json!("different");
            assert!(
                evaluate(account_value(), vec![], vec![position_value(), second]).is_err(),
                "duplicate other identity"
            );
        }
        let mut sell = order_value();
        sell["side"] = json!("sell");
        assert!(evaluate(account_value(), vec![sell.clone()], vec![]).is_err());
        sell["qty"] = json!("0.100000001");
        assert!(evaluate(account_value(), vec![sell.clone()], vec![position_value()]).is_err());
        sell["qty"] = json!("0.1");
        assert_eq!(
            evaluate(account_value(), vec![sell], vec![position_value()]).unwrap()["unreserved_held_nanoshares"]
                ["SPY"],
            "0"
        );
        let mut buy = order_value();
        buy["qty"] = json!("0.500000001");
        assert!(evaluate(account_value(), vec![buy.clone()], vec![]).is_err());
        buy["qty"] = json!("0.5");
        assert_eq!(
            evaluate(account_value(), vec![buy], vec![]).unwrap()["cash_bound_microusd"],
            "0"
        );
    }

    #[test]
    fn exact_nine_place_decimals_round_funds_down_and_exposure_up_without_overflow() {
        for (text, expected) in [
            ("0", 0),
            ("1", 1_000_000_000),
            ("0.000000001", 1),
            ("1.123456789", 1_123_456_789),
            ("18446744073.709551615", u64::MAX),
        ] {
            assert_eq!(nanos(text).unwrap(), expected, "{text}");
        }
        for text in [
            "",
            ".1",
            "1.",
            "-1",
            "+1",
            "1e2",
            "NaN",
            "1.2.3",
            "0.0000000001",
            " 1",
            "1 ",
            "18446744073.709551616",
            "18446744074",
            "18446744073709551616",
        ] {
            assert!(nanos(text).is_err(), "{text}");
        }
        let mut account = account_value();
        account["cash"] = json!("0.000002999");
        account["buying_power"] = json!("0.000002999");
        account["equity"] = json!("0.000002999");
        let mut order = order_value();
        order["qty"] = json!("0.000000001");
        order["filled_qty"] = json!("0");
        order["limit_price"] = json!("0.000000001");
        let mut position = position_value();
        position["qty"] = json!("0.000000001");
        position["cost_basis"] = json!("0.000000001");
        position["market_value"] = json!("0.000000002");
        let bounds = evaluate(account, vec![order], vec![position]).unwrap();
        assert_eq!(bounds["pending_buy_bound_microusd"], "1");
        assert_eq!(bounds["position_exposure_bound_microusd"], "1");
        assert_eq!(bounds["committed_exposure_bound_microusd"], "2");
        assert_eq!(bounds["cash_bound_microusd"], "1");
        assert_eq!(ceil_micros(1_000, 1_000).unwrap(), 1);
        assert!(ceil_micros(u128::MAX, 1_000).is_err());
        let mut total = u64::MAX;
        assert!(add(&mut total, 1).is_err());
        let mut order = order_value();
        order["qty"] = json!("18446744073.709551615");
        order["limit_price"] = json!("18446744073.709551615");
        assert!(evaluate(account_value(), vec![order], vec![]).is_err());
    }
}
