// SPDX-License-Identifier: Apache-2.0
//! Quote checks for paper execution inputs. No permissions or order submission.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};

#[derive(Deserialize)]
struct Snapshot {
    #[serde(rename = "latestQuote")]
    latest_quote: Option<Quote>,
}

#[derive(Deserialize)]
struct Quote {
    t: String,
    bp: Box<RawValue>,
    ap: Box<RawValue>,
    bs: Box<RawValue>,
    #[serde(rename = "as")]
    ask_size: Box<RawValue>,
}

// Refuse unsupported precision/exponents instead of silently rounding a risk
// input. JSON is decoded as RawValue so decimal digits never pass through f64.
fn fixed6(text: &str) -> Option<u64> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if text.contains('.') && fraction.is_empty() {
        return None;
    }
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 6
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let whole: u64 = whole.parse().ok()?;
    let mut fraction = fraction.to_owned();
    while fraction.len() < 6 {
        fraction.push('0');
    }
    whole
        .checked_mul(1_000_000)?
        .checked_add(fraction.parse().ok()?)
}

fn timestamp_ns(text: &str) -> Option<u128> {
    let utc = text.strip_suffix('Z')?;
    let (whole, fraction) = utc.split_once('.').unwrap_or((utc, ""));
    // Bound the year before calling the shared civil arithmetic.
    if whole.len() != 19 || !whole.is_ascii() {
        return None;
    }
    if fraction.len() > 9 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if utc.contains('.') && fraction.is_empty() {
        return None;
    }
    let whole_utc = format!("{whole}Z");
    let seconds = radar_types::civil::seconds_from_timestamp(&whole_utc)?;
    // The shared parser accepts dates by shape; round-trip rejects impossible
    // dates, leap seconds and noncanonical widths rather than normalising them.
    if radar_types::civil::timestamp_from_seconds(seconds) != whole_utc {
        return None;
    }
    let mut nanos = fraction.to_owned();
    while nanos.len() < 9 {
        nanos.push('0');
    }
    Some(u128::from(seconds) * 1_000_000_000 + nanos.parse::<u128>().ok()?)
}

fn validate(
    raw: &str,
    symbol: &str,
    available: u64,
    as_of: u64,
    max_age_ms: u64,
    max_spread_bps: u64,
) -> Result<Value, String> {
    if available > as_of {
        return Err("capture unavailable at quote decision".into());
    }
    let snapshots: BTreeMap<String, Option<Snapshot>> =
        serde_json::from_str(raw).map_err(|_| "invalid quote response")?;
    let quote = snapshots
        .get(symbol)
        .and_then(Option::as_ref)
        .and_then(|s| s.latest_quote.as_ref())
        .ok_or("requested quote is unknown")?;
    let bid = fixed6(quote.bp.get())
        .filter(|v| *v > 0)
        .ok_or("invalid bid price")?;
    let ask = fixed6(quote.ap.get())
        .filter(|v| *v > 0)
        .ok_or("invalid ask price")?;
    let bid_size = fixed6(quote.bs.get())
        .filter(|v| *v > 0)
        .ok_or("invalid bid size")?;
    let ask_size = fixed6(quote.ask_size.get())
        .filter(|v| *v > 0)
        .ok_or("invalid ask size")?;
    let spread = ask.checked_sub(bid).ok_or("crossed quote")?;
    if u128::from(spread) * 10_000 > u128::from(bid) * u128::from(max_spread_bps) {
        return Err("quote spread exceeds configured bound".into());
    }
    let timestamp = timestamp_ns(&quote.t).ok_or("invalid quote UTC timestamp")?;
    if timestamp > u128::from(available) * 1_000_000 {
        return Err("quote timestamp is after capture availability".into());
    }
    let age_ns = u128::from(as_of) * 1_000_000 - timestamp;
    if age_ns > u128::from(max_age_ms) * 1_000_000 {
        return Err("quote exceeds configured age".into());
    }
    Ok(json!({"venue":"alpaca", "feed":"iex", "symbol":symbol,
        "bid_microusd_per_share":bid.to_string(),"ask_microusd_per_share":ask.to_string(),
        "bid_displayed_size_millionths":bid_size.to_string(),"ask_displayed_size_millionths":ask_size.to_string(),
        "displayed_size_unit":null,
        "exchange_timestamp":quote.t,"age_ns":age_ns.to_string(),
        "available_at_unix_ms":available,"as_of_unix_ms":as_of,
        "max_age_ms":max_age_ms,"max_spread_bps":max_spread_bps,
        "consolidated_market":false,"execution_authority":false}))
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let [
        "alpaca-quote",
        "--input",
        path,
        "--symbol",
        symbol,
        "--as-of-unix-ms",
        as_of,
        "--max-age-ms",
        age,
        "--max-spread-bps",
        spread,
    ] = values.as_slice()
    else {
        return Err("usage: radar alpaca-quote --input <capture> --symbol SPY --as-of-unix-ms <N> --max-age-ms <N> --max-spread-bps <N>".into());
    };
    let as_of = as_of.parse().map_err(|_| "invalid quote watermark")?;
    let age = age.parse().map_err(|_| "invalid quote age bound")?;
    let spread = spread.parse().map_err(|_| "invalid quote spread bound")?;
    let (raw, available) = super::alpaca_market::quote_input(Path::new(path), symbol, as_of)?;
    let report = validate(&raw, symbol, available, as_of, age, spread)?;
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: u64 = 1_792_540_800_000; // 2026-10-21T00:00:00Z

    fn raw() -> &'static str {
        r#"{"SPY":{"latestQuote":{"t":"2026-10-21T00:00:00Z","bp":100.000001,"ap":100.01,"bs":20.5,"as":30}}}"#
    }

    #[test]
    fn exact_decimal_inputs_never_round_through_float() {
        for (input, expected) in [
            ("0", 0),
            ("1", 1_000_000),
            ("100.000001", 100_000_001),
            ("0.123456", 123_456),
            ("18446744073709.551615", u64::MAX),
        ] {
            assert_eq!(fixed6(input), Some(expected), "{input}");
        }
        for input in [
            "",
            "-1",
            "+1",
            "1e2",
            "1.0000001",
            "\"1\"",
            "NaN",
            "18446744073709.551616",
            "18446744073710",
            "a.1",
            "1.a",
            "1.",
            ".1",
        ] {
            assert_eq!(fixed6(input), None, "{input}");
        }
    }

    #[test]
    fn timestamps_keep_nanoseconds_and_reject_normalised_or_zoned_dates() {
        assert_eq!(timestamp_ns("1970-01-01T00:00:01Z"), Some(1_000_000_000));
        assert_eq!(
            timestamp_ns("1970-01-01T00:00:01.123456789Z"),
            Some(1_123_456_789)
        );
        assert_eq!(timestamp_ns("1970-01-01T00:00:01.1Z"), Some(1_100_000_000));
        assert_eq!(
            timestamp_ns("2026-10-21T00:00:00Z"),
            Some(u128::from(AT) * 1_000_000)
        );
        for bad in [
            "2026-02-30T00:00:00Z",
            "2026-10-21T00:00:60Z",
            "2026-1-1T00:00:00Z",
            "2026-10-21T00:00:00+00:00",
            "1970-01-01T00:00:01.Z",
            "1970-01-01T00:00:01.aZ",
            "1970-01-01T00:00:01.1234567890Z",
            "garbage",
            "9223372036854775807-01-01T00:00:00Z",
            "2026-10-21T00:00:0éZ",
        ] {
            assert_eq!(timestamp_ns(bad), None, "{bad}");
        }
    }

    #[test]
    fn validated_quote_reports_exact_units_and_all_bounds() {
        assert_eq!(
            validate(raw(), "SPY", AT, AT + 5, 5, 1).unwrap(),
            json!({"venue":"alpaca","feed":"iex","symbol":"SPY",
            "bid_microusd_per_share":"100000001","ask_microusd_per_share":"100010000",
            "bid_displayed_size_millionths":"20500000","ask_displayed_size_millionths":"30000000",
            "displayed_size_unit":null,
            "exchange_timestamp":"2026-10-21T00:00:00Z","age_ns":"5000000",
            "available_at_unix_ms":AT,"as_of_unix_ms":AT+5,"max_age_ms":5,"max_spread_bps":1,
            "consolidated_market":false,"execution_authority":false})
        );
        assert!(validate(raw(), "SPY", AT, AT, 0, 1).is_ok());
        let exact_spread = raw().replace("100.000001", "100");
        assert!(validate(&exact_spread, "SPY", AT, AT, 0, 1).is_ok());
        assert_eq!(
            validate(
                &exact_spread.replace("100.01", "100.010001"),
                "SPY",
                AT,
                AT,
                0,
                1
            )
            .unwrap_err(),
            "quote spread exceeds configured bound"
        );
        assert_eq!(
            validate(raw(), "SPY", AT, AT + 6, 5, 1).unwrap_err(),
            "quote exceeds configured age"
        );
        assert_eq!(
            validate(raw(), "SPY", AT, AT, 0, 0).unwrap_err(),
            "quote spread exceeds configured bound"
        );
        assert_eq!(
            validate(raw(), "SPY", AT + 1, AT, 0, 1).unwrap_err(),
            "capture unavailable at quote decision"
        );
        let future = raw().replace("00:00:00Z", "00:00:00.000000001Z");
        assert_eq!(
            validate(&future, "SPY", AT, AT, 0, 1).unwrap_err(),
            "quote timestamp is after capture availability"
        );
    }

    #[test]
    fn missing_crossed_zero_and_malformed_quotes_are_not_executable() {
        for bad in [
            "{}",
            r#"{"SPY":null}"#,
            r#"{"SPY":{}}"#,
            "null",
            "[]",
            "invalid",
        ] {
            assert!(validate(bad, "SPY", AT, AT, 0, 1).is_err(), "{bad}");
        }
        assert!(validate(raw(), "QQQ", AT, AT, 0, 1).is_err());
        for (from, to, error) in [
            ("100.000001", "0", "invalid bid price"),
            ("100.01", "0", "invalid ask price"),
            ("100.01", "99", "crossed quote"),
            ("20.5", "0", "invalid bid size"),
            ("30", "0", "invalid ask size"),
            ("100.01", "100.0000001", "invalid ask price"),
        ] {
            assert_eq!(
                validate(&raw().replace(from, to), "SPY", AT, AT, 0, 1).unwrap_err(),
                error
            );
        }
        let locked = raw().replace("100.01", "100.000001");
        assert!(validate(&locked, "SPY", AT, AT, 0, 0).is_ok());
    }
}
