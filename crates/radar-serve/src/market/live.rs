// SPDX-License-Identifier: Apache-2.0
//! The market routes, answered from the live feed.
//!
//! Used instead of the store whenever `RADAR_STREAM_ENDPOINT` is set; see
//! [`super::Market::with_live`]. Every response keeps the shape the store-backed
//! handler returns, so the screen does not know which one answered. What it
//! does learn is more: coin names for launches the feed saw, holders by wallet,
//! and a `complete` flag that is `false` whenever the window reaches back
//! before the feed was watching that coin.
//!
//! # Windows end at the newest block, not the clock
//!
//! The same rule the store-backed routes keep, for a smaller reason: the feed
//! is a second or two behind the chain rather than minutes, but `now()` still
//! names a moment nothing has arrived for. The end is exclusive, so it is the
//! newest block's second **plus one**, or the newest trades would be cut off.

use axum::Json;
use axum::response::{IntoResponse, Response};
use radar_backfill::market::fold as market_fold;
use radar_store::from_epoch;
use radar_stream::Live;
use radar_stream::tape::Minute;
use radar_types::Address;
use serde_json::json;

use super::{
    COINS_WINDOW_SECONDS, DEFAULT_CANDLE_WINDOW_SECONDS, DEFAULT_TRADE_LIMIT,
    DEFAULT_WINDOW_SECONDS, Degradation, MAX_CANDLE_WINDOW_SECONDS, MAX_COINS_LIMIT,
    MAX_TRADE_LIMIT, clamped_start, is_a_backwards_range, reaching_back, sort_coins,
    to_fold_trade,
};

/// Holders returned when the caller names no limit.
const DEFAULT_HOLDERS_LIMIT: usize = 20;
/// The most holders one request returns.
const MAX_HOLDERS_LIMIT: usize = 100;

/// Said while the feed is configured and nothing has arrived yet.
const NOTHING_YET: &str = "the live market feed has not delivered a block yet; it may still be connecting";

/// The exclusive end of a default window: one past the newest block's second.
const fn window_end(newest: i64) -> i64 {
    newest + 1
}

fn nothing_yet() -> Response {
    Degradation::NotCollected(NOTHING_YET).into_response()
}

/// The tape for one coin.
pub fn trades(live: &Live, mint: Address, limit: Option<usize>, before: Option<i64>) -> Response {
    let tape = live.tape();
    let Some(newest) = tape.newest() else {
        return nothing_yet();
    };
    let to = before.unwrap_or_else(|| window_end(newest));
    let from = reaching_back(to, DEFAULT_WINDOW_SECONDS);
    let (rows, complete) = tape.trades(&mint, from, to);
    drop(tape);

    let limit = limit.unwrap_or(DEFAULT_TRADE_LIMIT).clamp(1, MAX_TRADE_LIMIT);
    let trades: Vec<market_fold::Trade> = rows.iter().take(limit).map(to_fold_trade).collect();
    Json(json!({
        "mint": mint.to_string(),
        "window": { "from": from_epoch(from), "to": from_epoch(to), "complete": complete },
        "trades": trades,
    }))
    .into_response()
}

/// Minute candles rolled up to `interval` seconds, oldest first.
///
/// A bucket's open is its first minute's open and its close its last minute's
/// close, which is exactly the candle the trades would have folded into:
/// minutes are themselves folded from trades in time order.
#[must_use]
pub fn roll_up(minutes: &[Minute], interval: i64) -> Vec<market_fold::Candle> {
    let mut out: Vec<market_fold::Candle> = Vec::new();
    if interval <= 0 {
        return out;
    }
    for m in minutes {
        let bucket = m.start.div_euclid(interval) * interval;
        match out.last_mut() {
            Some(c) if c.time == bucket => {
                c.high = c.high.max(m.high);
                c.low = c.low.min(m.low);
                c.close = m.close;
                c.quote_volume += m.quote_volume;
                c.token_volume += m.token_volume;
                c.trade_count += m.trades;
            }
            _ => out.push(market_fold::Candle {
                time: bucket,
                bucket_start: from_epoch(bucket),
                open: m.open,
                high: m.high,
                low: m.low,
                close: m.close,
                quote_volume: m.quote_volume,
                token_volume: m.token_volume,
                trade_count: m.trades,
            }),
        }
    }
    out
}

/// A coin's chart.
pub fn candles(
    live: &Live,
    mint: Address,
    interval_name: &str,
    interval: i64,
    from: Option<i64>,
    to: Option<i64>,
) -> Response {
    let tape = live.tape();
    let Some(newest) = tape.newest() else {
        return nothing_yet();
    };
    let requested_to = to.unwrap_or_else(|| window_end(newest));
    let requested_from =
        from.unwrap_or_else(|| reaching_back(requested_to, DEFAULT_CANDLE_WINDOW_SECONDS));
    if is_a_backwards_range(requested_from, requested_to) {
        return super::bad_request("from must be before to");
    }
    let covered_from = clamped_start(requested_from, requested_to, MAX_CANDLE_WINDOW_SECONDS);
    let (minutes, complete) = tape.minutes(&mint, covered_from, requested_to);
    drop(tape);

    Json(json!({
        "mint": mint.to_string(),
        "interval": interval_name,
        "requested": { "from": from_epoch(requested_from), "to": from_epoch(requested_to) },
        "covered": { "from": from_epoch(covered_from), "to": from_epoch(requested_to), "complete": complete },
        "candles": roll_up(&minutes, interval),
    }))
    .into_response()
}

/// The coin list, with names where the feed saw the launch.
pub fn coins(live: &Live, limit: Option<usize>, sort: &str) -> Response {
    let tape = live.tape();
    let Some(newest) = tape.newest() else {
        return nothing_yet();
    };
    let to = window_end(newest);
    let from = reaching_back(to, COINS_WINDOW_SECONDS);
    let activity = tape.active(from, to);
    drop(tape);

    let mut names: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    let mut coins: Vec<market_fold::Coin> = activity
        .into_iter()
        .map(|a| {
            let mint = a.mint.to_string();
            if let Some(launch) = a.launch {
                names.insert(mint.clone(), (launch.name, launch.symbol));
            }
            market_fold::Coin {
                mint,
                tx_count: a.trades,
                token_volume: Some(a.token_volume),
                quote_mint: a.quote.map(|q| q.to_string()),
                quote_volume: Some(a.quote_volume),
                price: a.close,
                change_pct: market_fold::change_from(a.open, a.close),
            }
        })
        .collect();

    // Sorted by the same function the store-backed list uses; names ride
    // alongside as two extra fields, null for a coin whose launch was not seen.
    sort_coins(&mut coins, sort);
    let limit = limit
        .unwrap_or(super::DEFAULT_COINS_LIMIT)
        .clamp(1, MAX_COINS_LIMIT);
    let rows: Vec<serde_json::Value> = coins
        .into_iter()
        .take(limit)
        .map(|coin| {
            let launch = names.get(&coin.mint);
            let mut row = serde_json::to_value(&coin).unwrap_or_else(|_| json!({}));
            if let Some(object) = row.as_object_mut() {
                object.insert("name".into(), json!(launch.map(|n| n.0.trim())));
                object.insert("symbol".into(), json!(launch.map(|n| n.1.trim())));
            }
            row
        })
        .collect();

    Json(json!({
        "window": { "from": from_epoch(from), "to": from_epoch(to) },
        "coins": rows,
    }))
    .into_response()
}

/// A coin's header.
pub fn token(live: &Live, mint: Address) -> Response {
    let tape = live.tape();
    let launch = tape.launch_of(&mint).cloned();
    let price = tape.last_price(&mint);
    let seen = tape.newest().is_some();
    drop(tape);

    let (price_value, price_reason) = match price {
        Some((p, _)) => (Some(p), None),
        None if seen => (None, Some("no priced trade for this coin since the live feed began watching it")),
        None => (None, Some(NOTHING_YET)),
    };
    let metadata_reason = launch.is_none().then_some(
        "the live feed did not see this coin launch, so its name is not known; names are read from pump.fun launches as they happen",
    );

    Json(json!({
        "mint": mint.to_string(),
        "name": launch.as_ref().map(|l| l.name.trim().to_owned()),
        "symbol": launch.as_ref().map(|l| l.symbol.trim().to_owned()),
        "creator": launch.as_ref().map(|l| l.creator.to_string()),
        "published_at": launch.as_ref().map(|l| from_epoch(l.at)),
        "metadata_reason": metadata_reason,
        "price": price_value,
        "price_reason": price_reason,
        "quote_mint": price.map(|(_, q)| q.to_string()),
        "market_cap": Option::<f64>::None,
        "market_cap_reason": "supply is not read by the live feed yet",
        "liquidity": Option::<f64>::None,
        "liquidity_reason": "pool reserves are not read by the live feed yet",
    }))
    .into_response()
}

/// A coin's holders, by wallet.
pub fn holders(live: &Live, mint: Address, limit: Option<usize>) -> Response {
    let tape = live.tape();
    let Some(newest) = tape.newest() else {
        return nothing_yet();
    };
    let limit = limit.unwrap_or(DEFAULT_HOLDERS_LIMIT).clamp(1, MAX_HOLDERS_LIMIT);
    let Some(held) = tape.holders(&mint, limit) else {
        return Degradation::NotCollected(
            "the live feed has seen no balance of this coin since it began watching",
        )
        .into_response();
    };
    drop(tape);

    let rows: Vec<serde_json::Value> = held
        .rows
        .iter()
        .map(|r| json!({ "account": r.owner.to_string(), "balance": r.balance, "pool": r.pool }))
        .collect();
    Json(json!({
        "mint": mint.to_string(),
        "fold": {
            "fact": if held.since_launch { "balances_since_launch" } else { "balances_seen_while_watching" },
            "granularity": "wallet",
            "from": from_epoch(held.since),
            "to": from_epoch(newest),
            "complete": held.since_launch && !held.truncated,
            "holders": rows,
        },
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minute(start: i64, open: f64, high: f64, low: f64, close: f64) -> Minute {
        Minute {
            start,
            open,
            high,
            low,
            close,
            quote_volume: 1.0,
            token_volume: 10.0,
            trades: 2,
        }
    }

    #[test]
    fn minutes_roll_up_into_the_candle_their_trades_would_have_made() {
        let t0 = 1_800_000_000;
        let minutes = [
            minute(t0, 1.0, 3.0, 0.5, 2.0),
            minute(t0 + 60, 2.0, 5.0, 1.5, 4.0),
            minute(t0 + 300, 4.0, 4.0, 4.0, 4.0),
        ];
        let candles = roll_up(&minutes, 300);
        assert_eq!(candles.len(), 2);
        let c = &candles[0];
        assert_eq!(c.time, t0);
        assert_eq!(c.bucket_start, from_epoch(t0));
        assert_eq!((c.open, c.high, c.low, c.close), (1.0, 5.0, 0.5, 4.0));
        assert_eq!(c.trade_count, 4);
        assert!((c.quote_volume - 2.0).abs() < f64::EPSILON);
        assert_eq!(candles[1].time, t0 + 300);
    }

    #[test]
    fn a_one_minute_roll_up_is_the_minutes_themselves() {
        let t0 = 1_800_000_000;
        let minutes = [minute(t0, 1.0, 2.0, 1.0, 2.0), minute(t0 + 60, 2.0, 2.0, 1.0, 1.0)];
        assert_eq!(roll_up(&minutes, 60).len(), 2);
        assert!(roll_up(&minutes, 0).is_empty());
    }

    #[test]
    fn a_default_window_includes_the_newest_blocks_second() {
        assert_eq!(window_end(100), 101);
    }
}
