// SPDX-License-Identifier: Apache-2.0
//! Validate selected broker clock evidence; never infer a session from local weekdays.

use serde_json::{Value, json};

fn research(
    horizon: u64,
    mut read: impl FnMut() -> Result<String, String>,
    mut clock: impl FnMut() -> Result<u64, String>,
) -> Result<Value, String> {
    if horizon == 0 {
        return Err("research horizon must be positive".into());
    }
    let raw = read()?;
    let broker: Value = serde_json::from_str(&raw).map_err(|_| "invalid research session response")?;
    let now = clock()?;
    let (_, close) = validate(&broker, now, 5000)?;
    // Bound the worker's 20-second capture and 100-second model deadlines,
    // then leave the entire forward outcome horizon inside this session.
    let outcome = now
        .checked_add(120_000)
        .and_then(|at| at.checked_add(horizon))
        .ok_or("research session horizon overflow")?;
    if outcome >= close {
        return Err("research outcome would reach the session close".into());
    }
    Ok(json!({"environment":"paper","execution_authority":false,
        "regular_session_eligible":true,"as_of_unix_ms":now,
        "session_close_unix_ms":close,"horizon_ms":horizon}))
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<_> = args.iter().map(String::as_str).collect();
    let ["alpaca-research-session", "--horizon-ms", horizon] = values.as_slice() else {
        return Err("usage: radar alpaca-research-session --horizon-ms <positive-ms>".into());
    };
    let horizon: u64 = horizon.parse().map_err(|_| "invalid research horizon")?;
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let report = research(
        horizon,
        || {
            super::alpaca_read::get(
                &agent,
                "https://paper-api.alpaca.markets/v2/clock",
                &key,
                &secret,
            )
        },
        super::alpaca_preflight::now_ms,
    )?;
    println!("{report}");
    Ok(())
}

fn timestamp(text: &str) -> Option<u128> {
    if text.ends_with('Z') {
        return super::alpaca_quote::timestamp_ns(text);
    }
    if !text.is_ascii() || text.len() < 25 {
        return None;
    }
    let (local, offset) = text.split_at(text.len() - 6);
    let bytes = offset.as_bytes();
    if !matches!(bytes[0], b'+' | b'-')
        || bytes[3] != b':'
        || ![bytes[1], bytes[2], bytes[4], bytes[5]]
            .iter()
            .all(u8::is_ascii_digit)
    {
        return None;
    }
    let hours: u128 = offset[1..3].parse().ok()?;
    let minutes: u128 = offset[4..6].parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    let local = super::alpaca_quote::timestamp_ns(&format!("{local}Z"))?;
    let shift = (hours * 3600 + minutes * 60) * 1_000_000_000;
    if bytes[0] == b'+' {
        local.checked_sub(shift)
    } else {
        local.checked_add(shift)
    }
}

pub(super) fn validate(clock: &Value, now: u64, max_age: u64) -> Result<(u64, u64), String> {
    let parse = |key: &str| {
        clock[key]
            .as_str()
            .and_then(timestamp)
            .ok_or("invalid paper session timestamp")
    };
    let observed = parse("timestamp")?;
    let close = parse("next_close")?;
    let open = parse("next_open")?;
    let now_ns = u128::from(now) * 1_000_000;
    if clock["is_open"] != true
        || observed > now_ns
        || close <= now_ns
        || open <= close
        || now_ns - observed > u128::from(max_age) * 1_000_000
    {
        return Err("paper session is closed, stale or inconsistent".into());
    }
    // Floor both values: age is conservative and authorization must expire
    // before even a fractional-millisecond session close.
    Ok((
        u64::try_from(observed / 1_000_000).map_err(|_| "session time overflow")?,
        u64::try_from(close / 1_000_000).map_err(|_| "session close overflow")?,
    ))
}

pub(super) fn limit(price: u64) -> Result<(), String> {
    let increment = if price >= 1_000_000 { 10_000 } else { 100 };
    if price == 0 || !price.is_multiple_of(increment) {
        return Err("paper limit price exceeds supported broker increment".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn clock() -> Value {
        json!({"is_open":true,"timestamp":"2026-10-12T10:00:00.123456789-04:00",
            "next_close":"2026-10-12T16:00:00-04:00","next_open":"2026-10-13T09:30:00-04:00"})
    }

    #[test]
    fn research_requires_fresh_open_session_and_outcome_strictly_before_close() {
        let at = 1_791_813_600_124;
        let raw = clock().to_string();
        let report = research(600_000, || Ok(raw.clone()), || Ok(at)).unwrap();
        assert_eq!(report["regular_session_eligible"], true);
        assert_eq!(report["execution_authority"], false);
        let close = report["session_close_unix_ms"].as_u64().unwrap();
        let remaining = close - at - 120_000;
        research(remaining - 1, || Ok(raw.clone()), || Ok(at)).unwrap();
        assert!(research(remaining, || Ok(raw.clone()), || Ok(at)).is_err());
        assert!(research(u64::MAX, || Ok(raw.clone()), || Ok(at)).is_err());
        assert!(research(0, || panic!("must not read"), || Ok(at)).is_err());
        for changed in [
            json!({"is_open":false}),
            Value::Null,
            json!({"timestamp":"2026-10-12T09:59:00-04:00"}),
        ] {
            let mut bad = clock();
            if let Some(fields) = changed.as_object() {
                for (key, value) in fields {
                    bad[key] = value.clone();
                }
            } else {
                bad = changed;
            }
            assert!(research(1, || Ok(bad.to_string()), || Ok(at)).is_err());
        }
        assert!(research(1, || Err("unavailable".into()), || Ok(at)).is_err());
        assert!(research(1, || Ok("{".into()), || Ok(at)).is_err());
        assert!(research(1, || Ok(raw.clone()), || Err("clock".into())).is_err());
    }

    #[test]
    fn zoned_clock_preserves_nanoseconds_and_rejects_malformed_times() {
        let utc = timestamp("2026-10-12T14:00:00.123456789Z").unwrap();
        assert_eq!(timestamp("2026-10-12T10:00:00.123456789-04:00"), Some(utc));
        assert_eq!(timestamp("2026-10-12T15:30:00.123456789+01:30"), Some(utc));
        assert_eq!(
            timestamp("2026-10-12T14:00:00.123456789+23:59"),
            Some(utc - 86_340_000_000_000)
        );
        for bad in [
            "2026-02-30T14:00:00Z",
            "2026-10-12T14:00:60Z",
            "2026-10-12T14:00:00",
            "2026-10-12T14:00:00+24:00",
            "2026-10-12T14:00:00-04:60",
            "2026-10-12T14:00:00+0+:00",
            "2026-10-12T14:00:00+04x00",
            "x",
            "é",
            "1970-01-01T00:00:00+01:00",
        ] {
            assert_eq!(timestamp(bad), None, "{bad}");
        }
    }

    #[test]
    fn session_requires_fresh_open_clock_and_future_ordered_close() {
        let now = 1_791_813_600_124;
        let valid = clock();
        let (observed, close) = validate(&valid, now, 1).unwrap();
        assert_eq!(observed, now - 1);
        assert_eq!(close, 1_791_835_200_000);
        assert!(validate(&valid, now, 0).is_err());
        assert!(validate(&valid, now - 1, 1000).is_err());
        assert!(validate(&valid, close, u64::MAX).is_err());
        for (key, value) in [
            ("is_open", json!(false)),
            ("is_open", json!("true")),
            ("timestamp", json!("2026-10-12T14:00:01Z")),
            ("next_close", json!("2026-10-12T14:00:00Z")),
            ("next_open", json!("2026-10-12T20:00:00Z")),
            ("next_open", Value::Null),
        ] {
            let mut bad = valid.clone();
            bad[key] = value;
            assert!(validate(&bad, now, 1000).is_err(), "{bad}");
        }
        let mut exact = valid;
        exact["timestamp"] = json!("2026-10-12T14:00:00.124Z");
        validate(&exact, now, 0).unwrap();
        validate(&exact, now + 1000, 1000).unwrap();
        assert!(validate(&exact, now + 1001, 1000).is_err());
    }

    #[test]
    fn limit_increments_change_at_one_dollar_without_rounding() {
        for price in [100, 999_900, 1_000_000, 1_010_000, 100_000_000] {
            limit(price).unwrap();
        }
        for price in [0, 1, 101, 999_999, 1_000_001, 1_000_100, u64::MAX] {
            assert!(limit(price).is_err());
        }
    }
}
