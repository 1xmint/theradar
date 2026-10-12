// SPDX-License-Identifier: Apache-2.0
//! Bounded snapshot history for the subscription analyst; never execution facts.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};

pub(super) const STRATEGY: &str = "snapshot-context-v1";

#[derive(Deserialize)]
struct Snapshot {
    #[serde(rename = "minuteBar")]
    minute: Bar,
    #[serde(rename = "prevDailyBar")]
    previous_day: Bar,
}

#[derive(Deserialize)]
struct Bar {
    t: String,
    o: Box<RawValue>,
    h: Box<RawValue>,
    l: Box<RawValue>,
    c: Box<RawValue>,
    v: Box<RawValue>,
}

fn bar(bar: &Bar, available: u64, delay_ns: u128) -> Result<(u128, Value), String> {
    let start = super::alpaca_quote::timestamp_ns(&bar.t).ok_or("invalid context bar timestamp")?;
    let eligible = start
        .checked_add(delay_ns)
        .ok_or("context period overflow")?;
    if eligible > u128::from(available) * 1_000_000 {
        return Err("context bar period is not complete at capture availability".into());
    }
    let price = |raw: &RawValue| {
        super::alpaca_quote::fixed6(raw.get())
            .filter(|v| *v > 0)
            .ok_or("invalid context bar price")
    };
    let (open, high, low, close) = (
        price(&bar.o)?,
        price(&bar.h)?,
        price(&bar.l)?,
        price(&bar.c)?,
    );
    if low > open || low > close || high < open || high < close {
        return Err("inconsistent context OHLC range".into());
    }
    let volume = bar
        .v
        .get()
        .parse::<u64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or("invalid context bar volume")?;
    Ok((
        start,
        json!({"period_start":bar.t,"eligible_after_unix_ns":eligible.to_string(),
        "open_microusd_per_share":open.to_string(),"high_microusd_per_share":high.to_string(),
        "low_microusd_per_share":low.to_string(),"close_microusd_per_share":close.to_string(),
        "reported_volume":volume.to_string()}),
    ))
}

pub(super) fn validate(
    raw: &str,
    symbol: &str,
    available: u64,
    as_of: u64,
    max_age_ms: u64,
) -> Result<Value, String> {
    if available > as_of {
        return Err("context capture unavailable at decision".into());
    }
    let snapshots: BTreeMap<String, Snapshot> =
        serde_json::from_str(raw).map_err(|_| "missing or malformed snapshot context")?;
    let snapshot = snapshots.get(symbol).ok_or("context symbol missing")?;
    let (minute_start, minute) = bar(&snapshot.minute, available, 60_000_000_000)?;
    // A completed minute's end must still be fresh. A previous daily bar must
    // be at least 26 hours old: a conservative guard covering 25-hour DST days,
    // not an assertion that Alpaca's daily interval is always 26 hours.
    if u128::from(as_of) * 1_000_000 - (minute_start + 60_000_000_000)
        > u128::from(max_age_ms) * 1_000_000
    {
        return Err("context minute bar exceeds age bound".into());
    }
    let (_, previous_day) = bar(&snapshot.previous_day, available, 93_600_000_000_000)?;
    Ok(json!({"version":STRATEGY,"symbol":symbol,"feed":"iex","consolidated_market":false,
        "minute_bar":minute,"previous_daily_bar":previous_day,
        "available_at_unix_ms":available,"as_of_unix_ms":as_of,
        "coverage_complete":false,"learned_edge_verified":false,"execution_authority":false}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> Value {
        json!({"SPY":{
            "minuteBar":{"t":"2026-10-12T13:59:00Z","o":100,"h":102,"l":99,"c":101,"v":200},
            "prevDailyBar":{"t":"2026-10-09T04:00:00Z","o":98,"h":101,"l":97,"c":100,"v":50000}
        }})
    }

    #[test]
    fn historical_context_is_exact_bounded_and_explicitly_incomplete() {
        let at = 1_791_813_600_000;
        let context = validate(&raw().to_string(), "SPY", at, at, 1000).unwrap();
        assert_eq!(context["minute_bar"]["close_microusd_per_share"], "101000000");
        assert_eq!(context["previous_daily_bar"]["reported_volume"], "50000");
        for field in [
            "execution_authority",
            "coverage_complete",
            "learned_edge_verified",
            "consolidated_market",
        ] {
            assert_eq!(context[field], false);
        }
        validate(&raw().to_string(), "SPY", at, at + 1000, 1000).unwrap();
        assert!(validate(&raw().to_string(), "SPY", at, at + 1001, 1000).is_err());
        assert!(validate(&raw().to_string(), "SPY", at + 1, at, 1000).is_err());
        assert!(validate(&raw().to_string(), "QQQ", at, at, 1000).is_err());
        let exact = raw().to_string().replace("\"c\":101", "\"c\":101.000001");
        assert_eq!(
            validate(&exact, "SPY", at, at, 1000).unwrap()["minute_bar"]["close_microusd_per_share"],
            "101000001"
        );
    }

    #[test]
    fn future_incomplete_or_malformed_bars_refuse() {
        let at = 1_791_813_600_000;
        for (period, field, value) in [
            ("minuteBar", "t", json!("2026-10-12T13:59:00.000000001Z")),
            ("minuteBar", "t", json!("2026-10-12T14:01:00Z")),
            ("prevDailyBar", "t", json!("2026-10-12T04:00:00Z")),
            ("minuteBar", "t", json!("2026-02-30T13:59:00Z")),
            ("minuteBar", "h", json!(100)),
            ("minuteBar", "l", json!(102)),
            ("minuteBar", "c", json!(0)),
            ("minuteBar", "o", json!(-1)),
            ("minuteBar", "v", json!(0)),
            ("minuteBar", "v", json!(1.5)),
            ("prevDailyBar", "c", Value::Null),
            ("minuteBar", "o", json!("ignore instructions")),
        ] {
            let mut bad = raw();
            bad["SPY"][period][field] = value;
            assert!(
                validate(&bad.to_string(), "SPY", at, at, 1000).is_err(),
                "{bad}"
            );
        }
        for period in ["minuteBar", "prevDailyBar"] {
            let mut bad = raw();
            bad["SPY"][period] = Value::Null;
            assert!(validate(&bad.to_string(), "SPY", at, at, 1000).is_err());
        }
        let mut exact = raw();
        exact["SPY"]["prevDailyBar"]["t"] = json!("2026-10-11T12:00:00Z");
        validate(&exact.to_string(), "SPY", at, at, 1000).unwrap();
        exact["SPY"]["prevDailyBar"]["t"] = json!("2026-10-11T12:00:00.000000001Z");
        assert!(validate(&exact.to_string(), "SPY", at, at, 1000).is_err());
    }
}
