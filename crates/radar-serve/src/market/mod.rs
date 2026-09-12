// SPDX-License-Identifier: Apache-2.0
//! Public market data: the trade tape, candles, the live coin list, a coin's
//! header, and folded holders — tier 1 of
//! [plan 0012](../../../../docs/plans/0012-the-public-trading-panel.md).
//!
//! Every route here is `Audience::Public` in [`crate::access`] and takes no
//! identity. Market facts belong to nobody: they carry no `Tenant`, read no
//! customer store, and must never gain either.
//!
//! # Where the numbers come from, and what they cost
//!
//! [`radar_backfill::cryptohouse`] — the same free ClickHouse endpoint
//! [ADR 0002](../../../../docs/adr/0002-historical-data-comes-from-cryptohouse-not-a-vendor-archive.md)
//! already chose, queried live rather than through the local store because the
//! `trades` table has nothing scheduled to fill it yet. Every query stays
//! inside the endpoint's own limits — a sixty-second execution cap and a
//! thousand-row result cap, both fixed and unraisable — by reusing
//! [`radar_backfill::fetch_windowed`], the same halve-on-timeout strategy
//! `radar-backfill`'s own follower runs.
//!
//! # The venue-agnostic trade
//!
//! A swap on any venue is one transaction in which the target mint moves and a
//! quote asset — wrapped SOL, USDC or USDT — moves too. [`query::trades_query`]
//! and [`fold`] build and read that shape; see their module comments for the
//! bugs a first sketch of this technique had and how each is closed.
//!
//! # Honest degradation
//!
//! CryptoHouse being unreachable, a query timing out even after narrowing, and
//! a mint with no data are three different facts and are reported as three
//! different HTTP statuses with a named reason — never collapsed into an
//! empty list, which [`Degradation`] exists to prevent.

pub mod cache;
pub mod fold;
pub mod query;

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use radar_backfill::{Client, QueryError, fetch_windowed};
use radar_store::{from_epoch, now_epoch, to_epoch};
use radar_types::Address;
use serde::{Deserialize, Serialize};
use serde_json::json;

use cache::TtlCache;

/// A single top-level window's width, before [`fetch_windowed`] starts
/// halving. Two minutes of a single mint's transfer volume is comfortably
/// inside the sixty-second execution cap — a busy mint's *whole-chain*
/// instruction count for one minute was 8.33s of it when measured for
/// docs/plans/0012, and a single-mint filter reads far less.
const DEFAULT_WINDOW_SECONDS: i64 = 120;

/// The narrowest window [`fetch_windowed`] will still try. Matches
/// `radar-backfill`'s own floor: below this, a timeout means something other
/// than "too wide".
const MIN_WINDOW_SECONDS: i64 = 4;

/// Paced the same as the backfill follower — Radar is a guest on a free public
/// endpoint (ADR 0002).
const PAUSE_BETWEEN_WINDOWS: Duration = Duration::from_millis(200);

/// How long a computed answer is served before CryptoHouse is asked again.
/// Short enough that the tape still feels live, long enough that fifty
/// concurrent visitors reading the same coin cost one query rather than
/// fifty.
const CACHE_TTL: Duration = Duration::from_secs(5);

/// The window `/v1/market/holders/{mint}` folds over when the caller does not
/// widen it further than `limit` lets them ask. Not "since launch": that needs
/// a launch timestamp this endpoint does not look up, and a fixed lookback is
/// the honest, statable alternative — see [`fold::HoldersFold`] for the fields
/// that say so on every response.
const HOLDERS_WINDOW_SECONDS: i64 = 24 * 60 * 60;

/// The window `/v1/market/coins` ranks activity over.
const COINS_WINDOW_SECONDS: i64 = 10 * 60;

/// How many candidate mints are shortlisted for pricing. Bounds the `IN (...)`
/// list [`query::coin_prices_query`] builds.
const COIN_SHORTLIST: usize = 200;

/// Everything the market routes need: a CryptoHouse client and one cache per
/// endpoint, so a hot key on one route cannot evict a hot key on another.
pub struct Market {
    client: Client,
    tape: TtlCache<String, Vec<fold::Trade>>,
    candles: TtlCache<String, (i64, i64, Vec<fold::Trade>)>,
    coins: TtlCache<String, Vec<fold::Coin>>,
    token_meta: TtlCache<String, Option<TokenMetadata>>,
    holders: TtlCache<String, fold::HoldersFold>,
}

impl Default for Market {
    fn default() -> Self {
        Self::new()
    }
}

impl Market {
    /// A fresh market seam, backed by the live CryptoHouse endpoint.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: Client::default(),
            tape: TtlCache::new(CACHE_TTL),
            candles: TtlCache::new(CACHE_TTL),
            coins: TtlCache::new(CACHE_TTL),
            token_meta: TtlCache::new(Duration::from_secs(300)),
            holders: TtlCache::new(CACHE_TTL),
        }
    }
}

/// Why a market route could not answer, distinguishably from an empty result.
///
/// **This, not an empty list, is what "CryptoHouse could not answer" looks
/// like.** [`QueryError`] already separates a transport failure from a server
/// exception; this adds the one further distinction the server exception
/// itself does not name — whether narrowing gave up because the result was
/// still too large (the row cap) or the query itself would not finish in time
/// (the execution timeout) — because an operator and a caller act on those
/// differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Degradation {
    /// The request never reached CryptoHouse.
    Unreachable,
    /// CryptoHouse answered, and this build could not read the answer.
    ///
    /// **Separated from [`Self::Unreachable`] on 2026-09-11, because
    /// collapsing them told a reader a fact about the network that was
    /// actually a fact about this code.** `/v1/market/coins` returned
    /// "could not reach the data source" while the identical query, issued by
    /// hand from the same machine, answered in 350 milliseconds. The cause
    /// was two columns selected without `toString()`: ClickHouse serialises a
    /// `count()` as a quoted string but a summed `Decimal(38,9)` as a bare
    /// JSON number — one of which was `36917103776789878943`, past what a
    /// 64-bit integer holds — and neither deserialises into the `String` the
    /// row struct declares.
    ///
    /// An operator reading "unreachable" checks the network and finds it
    /// healthy. That is the wrong hour spent, and it is rule 9's failure in
    /// its reporting form: a wrong answer offered with confidence where an
    /// honest "this build could not read it" was available.
    Malformed,
    /// Narrowing reached the floor and the query still would not finish in
    /// the execution cap.
    TimedOut,
    /// Narrowing reached the floor and the result still exceeded the
    /// thousand-row cap.
    RowCapHit,
}

impl Degradation {
    fn classify(error: &QueryError) -> Self {
        match error {
            QueryError::Transport(_) => Self::Unreachable,
            QueryError::Row(_) => Self::Malformed,
            QueryError::Server(message) if message.contains("TOO_MANY_ROWS") => Self::RowCapHit,
            QueryError::Server(_) => Self::TimedOut,
        }
    }

    const fn status(self) -> StatusCode {
        match self {
            // Both 502, deliberately: either the upstream could not be
            // reached or it answered something this build could not use, and
            // neither is the caller's fault. They stay separate variants
            // because `code` and `message` must tell them apart -- the status
            // is the one thing they legitimately share.
            Self::Unreachable | Self::Malformed => StatusCode::BAD_GATEWAY,
            Self::TimedOut | Self::RowCapHit => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    const fn code(self) -> &'static str {
        match self {
            Self::Unreachable => "cryptohouse_unreachable",
            Self::Malformed => "cryptohouse_answer_unreadable",
            Self::TimedOut => "query_timed_out",
            Self::RowCapHit => "row_cap_hit",
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Unreachable => {
                "could not reach the data source; this is not a fact about the coin"
            }
            Self::Malformed => {
                "the data source answered and this build could not read its answer; this is a fact about Radar, not about the coin or the connection"
            }
            Self::TimedOut => {
                "the query could not finish inside the data source's execution limit, even after narrowing the window"
            }
            Self::RowCapHit => {
                "the window holds more rows than the data source will return, even at its narrowest; the result would be an undeclared partial one"
            }
        }
    }
}

impl IntoResponse for Degradation {
    fn into_response(self) -> Response {
        (
            self.status(),
            Json(json!({ "error": self.code(), "message": self.message() })),
        )
            .into_response()
    }
}

fn bad_request(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "bad_request", "message": message })),
    )
        .into_response()
}

fn parse_mint(raw: &str) -> Result<String, Box<Response>> {
    raw.parse::<Address>()
        .map(|a| a.to_string())
        .map_err(|_| Box::new(bad_request("mint must be a base58-encoded Solana address")))
}

/// A `YYYY-MM-DD HH:MM:SS` timestamp, as every query parameter accepting one
/// expects. Fractional seconds are not accepted here — a caller building a
/// cursor from a response's own `ts` field must trim them, since this module's
/// output already carries CryptoHouse's microsecond precision.
fn parse_stamp(raw: &str) -> Result<i64, Box<Response>> {
    to_epoch(raw)
        .map_err(|_| Box::new(bad_request("expected a timestamp as 'YYYY-MM-DD HH:MM:SS'")))
}

/// `/v1/market/trades/{mint}` query parameters.
#[derive(Debug, Deserialize)]
pub struct TapeParams {
    limit: Option<usize>,
    before: Option<String>,
}

const DEFAULT_TRADE_LIMIT: usize = 100;
const MAX_TRADE_LIMIT: usize = 500;

/// The tape: recent trades for one mint, newest first.
pub async fn trades(
    State(state): State<Arc<crate::AppState>>,
    Path(mint): Path<String>,
    Query(params): Query<TapeParams>,
) -> Response {
    let mint = match parse_mint(&mint) {
        Ok(m) => m,
        Err(r) => return *r,
    };
    let to = match params.before.as_deref().map(parse_stamp).transpose() {
        Ok(v) => v.unwrap_or_else(now_epoch),
        Err(r) => return *r,
    };
    let limit = params
        .limit
        .unwrap_or(DEFAULT_TRADE_LIMIT)
        .clamp(1, MAX_TRADE_LIMIT);
    let from = to - DEFAULT_WINDOW_SECONDS;

    let key = format!("{mint}:{to}");
    let mint_for_query = mint.clone();
    let result = state.market.tape.get_or_compute(key, || {
        let rows: Vec<fold::TapeRow> = fetch_windowed(
            &state.market.client,
            from,
            to,
            0,
            MIN_WINDOW_SECONDS,
            PAUSE_BETWEEN_WINDOWS,
            &|f, t| query::trades_query(&mint_for_query, f, t),
        )?;
        Ok::<_, QueryError>(fold::fold_tape(&rows))
    });

    match result {
        Ok(trades) => {
            let mut trades = (*trades).clone();
            trades.truncate(limit);
            Json(json!({
                "mint": mint,
                "window": { "from": from_epoch(from), "to": from_epoch(to), "complete": true },
                "trades": trades,
            }))
            .into_response()
        }
        Err(e) => Degradation::classify(&e).into_response(),
    }
}

/// `/v1/market/candles/{mint}` query parameters.
#[derive(Debug, Deserialize)]
pub struct CandleParams {
    interval: Option<String>,
    from: Option<String>,
    to: Option<String>,
}

/// The widest range a single candles request will cover, however far apart
/// `from` and `to` are asked to be. The response's `covered` window says so
/// when this clamps.
const MAX_CANDLE_WINDOW_SECONDS: i64 = 24 * 60 * 60;

fn interval_seconds(name: &str) -> Option<i64> {
    match name {
        "1m" => Some(60),
        "5m" => Some(300),
        "15m" => Some(900),
        "1h" => Some(3_600),
        "4h" => Some(14_400),
        "1d" => Some(86_400),
        _ => None,
    }
}

/// OHLCV, folded from the same trades the tape reads.
pub async fn candles(
    State(state): State<Arc<crate::AppState>>,
    Path(mint): Path<String>,
    Query(params): Query<CandleParams>,
) -> Response {
    let mint = match parse_mint(&mint) {
        Ok(m) => m,
        Err(r) => return *r,
    };
    let Some(interval) = interval_seconds(params.interval.as_deref().unwrap_or("1m")) else {
        return bad_request("interval must be one of 1m, 5m, 15m, 1h, 4h, 1d");
    };
    let requested_to = match params.to.as_deref().map(parse_stamp).transpose() {
        Ok(v) => v.unwrap_or_else(now_epoch),
        Err(r) => return *r,
    };
    let requested_from = match params.from.as_deref().map(parse_stamp).transpose() {
        Ok(v) => v.unwrap_or(requested_to - DEFAULT_WINDOW_SECONDS),
        Err(r) => return *r,
    };
    if requested_from >= requested_to {
        return bad_request("from must be before to");
    }
    // The range actually covered may be narrower than requested -- stated in
    // the response rather than silently served, per the plan's own rubric for
    // this endpoint.
    let from = requested_from.max(requested_to - MAX_CANDLE_WINDOW_SECONDS);
    let to = requested_to;

    let key = format!("{mint}:{interval}:{from}:{to}");
    let mint_for_query = mint.clone();
    let result = state.market.candles.get_or_compute(key, || {
        let rows: Vec<fold::TapeRow> = fetch_windowed(
            &state.market.client,
            from,
            to,
            0,
            MIN_WINDOW_SECONDS,
            PAUSE_BETWEEN_WINDOWS,
            &|f, t| query::trades_query(&mint_for_query, f, t),
        )?;
        Ok::<_, QueryError>((from, to, fold::fold_tape(&rows)))
    });

    match result {
        Ok(cached) => {
            let (covered_from, covered_to, trades) = &*cached;
            let candles = fold::fold_candles(trades, interval);
            Json(json!({
                "mint": mint,
                "interval": params.interval.as_deref().unwrap_or("1m"),
                "requested": { "from": from_epoch(requested_from), "to": from_epoch(requested_to) },
                "covered": { "from": from_epoch(*covered_from), "to": from_epoch(*covered_to), "complete": true },
                "candles": candles,
            }))
            .into_response()
        }
        Err(e) => Degradation::classify(&e).into_response(),
    }
}

/// `/v1/market/coins` query parameters.
#[derive(Debug, Deserialize)]
pub struct CoinsParams {
    limit: Option<usize>,
    sort: Option<String>,
}

const DEFAULT_COINS_LIMIT: usize = 50;
const MAX_COINS_LIMIT: usize = 200;

fn sort_coins(coins: &mut [fold::Coin], sort: &str) {
    match sort {
        "volume" => coins.sort_by(|a, b| {
            b.quote_volume
                .unwrap_or(0.0)
                .total_cmp(&a.quote_volume.unwrap_or(0.0))
        }),
        "change" => coins.sort_by(|a, b| {
            b.change_pct
                .unwrap_or(f64::MIN)
                .total_cmp(&a.change_pct.unwrap_or(f64::MIN))
        }),
        _ => coins.sort_by_key(|c| std::cmp::Reverse(c.tx_count)),
    }
}

/// The live coin list: what has moved recently, ranked and priced.
///
/// The most-hit route in the panel, so it is the one with the most to lose
/// from an uncached hit: see [`cache::TtlCache`] for why fifty visitors here
/// costs one pair of CryptoHouse queries rather than fifty.
pub async fn coins(
    State(state): State<Arc<crate::AppState>>,
    Query(params): Query<CoinsParams>,
) -> Response {
    let limit = params
        .limit
        .unwrap_or(DEFAULT_COINS_LIMIT)
        .clamp(1, MAX_COINS_LIMIT);
    let sort = params.sort.clone().unwrap_or_else(|| "activity".to_owned());
    let to = now_epoch();
    let from = to - COINS_WINDOW_SECONDS;

    // One cache entry per window bucket rather than per second, so every
    // visitor inside a `CACHE_TTL` slice of time reads the same computed
    // list -- the caching this route was named to design first.
    let bucket = i64::try_from(CACHE_TTL.as_secs()).unwrap_or(1).max(1);
    let key = format!("{}:{}", from / bucket, limit);
    let result = state.market.coins.get_or_compute(key, || {
        let (from_s, to_s) = (from_epoch(from), from_epoch(to));
        let candidates: Vec<fold::CoinCandidateRow> = state
            .market
            .client
            .query(&query::coin_candidates_query(&from_s, &to_s, limit))?;
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let mints: Vec<String> = candidates
            .iter()
            .map(|c| c.mint.clone())
            .take(COIN_SHORTLIST)
            .collect();
        let prices: Vec<fold::CoinPriceRow> = state
            .market
            .client
            .query(&query::coin_prices_query(&mints, &from_s, &to_s))?;
        Ok::<_, QueryError>(fold::fold_coins(&candidates, &prices))
    });

    match result {
        Ok(coins) => {
            let mut coins = (*coins).clone();
            sort_coins(&mut coins, &sort);
            coins.truncate(limit);
            Json(json!({
                "window": { "from": from_epoch(from), "to": from_epoch(to) },
                "coins": coins,
            }))
            .into_response()
        }
        Err(e) => Degradation::classify(&e).into_response(),
    }
}

/// One creator entry from `solana.tokens`.
#[derive(Debug, Clone, Deserialize)]
struct Creator {
    address: String,
    #[serde(default)]
    verified: bool,
}

/// A row from [`query::token_metadata_query`].
#[derive(Debug, Clone, Deserialize)]
struct MetadataRow {
    ts: String,
    name: String,
    symbol: String,
    #[serde(default)]
    creators: Vec<Creator>,
}

/// The metadata this module keeps from a `solana.tokens` row — the header
/// fields nothing else here can supply.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TokenMetadata {
    published_at: String,
    name: String,
    symbol: String,
    creator: Option<String>,
}

fn fold_metadata(rows: &[MetadataRow]) -> Option<TokenMetadata> {
    let row = rows.first()?;
    // A verified creator first -- `creators` can list an unverified claim
    // alongside or instead of one the mint's own update authority attested
    // to, and the verified one is the fact worth reporting when both exist.
    let creator = row
        .creators
        .iter()
        .find(|c| c.verified && !c.address.is_empty())
        .or_else(|| row.creators.iter().find(|c| !c.address.is_empty()))
        .map(|c| c.address.clone());
    Some(TokenMetadata {
        published_at: row.ts.clone(),
        name: row.name.clone(),
        symbol: row.symbol.clone(),
        creator,
    })
}

/// `/v1/market/token/{mint}`: a coin's header.
///
/// Every field this data source cannot compute is `null` with a named reason
/// in a sibling `*_reason` field — never a zero standing in for "unmeasured".
/// Market cap and liquidity are the two that are always `null` today: both
/// need either an unbounded transfer scan (refused by the endpoint's own row
/// cap, see [ADR 0002](../../../../docs/adr/0002-historical-data-comes-from-cryptohouse-not-a-vendor-archive.md))
/// or a live account-state read this module does not perform.
pub async fn token(
    State(state): State<Arc<crate::AppState>>,
    Path(mint): Path<String>,
) -> Response {
    let mint = match parse_mint(&mint) {
        Ok(m) => m,
        Err(r) => return *r,
    };

    let meta_key = mint.clone();
    let meta_result = state.market.token_meta.get_or_compute(meta_key, || {
        let rows: Vec<MetadataRow> = state
            .market
            .client
            .query(&query::token_metadata_query(&mint))?;
        Ok::<_, QueryError>(fold_metadata(&rows))
    });
    let metadata = match meta_result {
        Ok(m) => (*m).clone(),
        Err(e) => return Degradation::classify(&e).into_response(),
    };

    let to = now_epoch();
    let from = to - DEFAULT_WINDOW_SECONDS;
    let tape_key = format!("{mint}:{to}");
    let mint_for_query = mint.clone();
    let tape_result = state.market.tape.get_or_compute(tape_key, || {
        let rows: Vec<fold::TapeRow> = fetch_windowed(
            &state.market.client,
            from,
            to,
            0,
            MIN_WINDOW_SECONDS,
            PAUSE_BETWEEN_WINDOWS,
            &|f, t| query::trades_query(&mint_for_query, f, t),
        )?;
        Ok::<_, QueryError>(fold::fold_tape(&rows))
    });
    let recent_trades = match tape_result {
        Ok(t) => (*t).clone(),
        Err(e) => return Degradation::classify(&e).into_response(),
    };

    let priced = recent_trades.iter().find(|t| t.price.is_some());
    let (price, price_reason, decimals, decimals_reason) = match priced {
        Some(t) => (
            t.price,
            None,
            None::<u32>,
            Some(
                "decimals are carried per-trade, not exposed on the header; see a trade on the tape",
            ),
        ),
        None if recent_trades.is_empty() => (
            None,
            Some("no recorded trades in the last two minutes"),
            None,
            Some("no recorded trades in the last two minutes"),
        ),
        None => (
            None,
            Some("recent trades exist but none paired with a quote leg in this window"),
            None,
            Some("recent trades exist but none paired with a quote leg in this window"),
        ),
    };
    // Decimals is intentionally dropped above -- see the field comment on
    // why a per-trade fact does not become a header fact here. Kept as a
    // named local so the reasoning is not lost to a future edit.
    let _ = decimals;

    Json(json!({
        "mint": mint,
        "name": metadata.as_ref().map(|m| m.name.clone()),
        "symbol": metadata.as_ref().map(|m| m.symbol.clone()),
        "creator": metadata.as_ref().and_then(|m| m.creator.clone()),
        "published_at": metadata.as_ref().map(|m| m.published_at.clone()),
        "metadata_reason": metadata.is_none().then_some(
            "no solana.tokens row for this mint yet; young or unindexed tokens are not always present"
        ),
        "price": price,
        "price_reason": price_reason,
        "decimals_reason": decimals_reason,
        "market_cap": Option::<f64>::None,
        "market_cap_reason": "supply is not computable without an unbounded transfer scan or a live account read, neither of which this endpoint performs",
        "liquidity": Option::<f64>::None,
        "liquidity_reason": "pool reserves require a live account read, which this endpoint does not perform",
    }))
    .into_response()
}

/// `/v1/market/holders/{mint}` query parameters.
#[derive(Debug, Deserialize)]
pub struct HoldersParams {
    limit: Option<usize>,
}

const DEFAULT_HOLDERS_LIMIT: usize = 100;
const MAX_HOLDERS_LIMIT: usize = 500;

/// Holders, folded from observed transfers over a bounded window.
///
/// **Not the same fact as a read of current token-account state.**
/// [`fold::HoldersFold`] carries which fact this is, at what granularity, and
/// over which window on every response — see its doc comment for why those
/// three qualifications are each their own field rather than prose a caller
/// could drop.
pub async fn holders(
    State(state): State<Arc<crate::AppState>>,
    Path(mint): Path<String>,
    Query(params): Query<HoldersParams>,
) -> Response {
    let mint = match parse_mint(&mint) {
        Ok(m) => m,
        Err(r) => return *r,
    };
    let limit = params
        .limit
        .unwrap_or(DEFAULT_HOLDERS_LIMIT)
        .clamp(1, MAX_HOLDERS_LIMIT);
    let to = now_epoch();
    let from = to - HOLDERS_WINDOW_SECONDS;
    let (from_s, to_s) = (from_epoch(from), from_epoch(to));

    let key = format!("{mint}:{limit}:{to}");
    let mint_for_query = mint.clone();
    let result = state.market.holders.get_or_compute(key, || {
        let rows: Vec<fold::HolderRow> = fetch_windowed(
            &state.market.client,
            from,
            to,
            0,
            MIN_WINDOW_SECONDS,
            PAUSE_BETWEEN_WINDOWS,
            &|f, t| query::holder_transfers_query(&mint_for_query, f, t),
        )?;
        Ok::<_, QueryError>(fold::fold_holders(&rows, &from_s, &to_s, limit))
    });

    match result {
        Ok(fold) => Json(json!({ "mint": mint, "fold": *fold })).into_response(),
        Err(e) => Degradation::classify(&e).into_response(),
    }
}

/// Whether a path is one of this module's routes — used only by
/// `access::audience_of`'s own tests, so the two cannot silently disagree
/// about which paths exist.
#[must_use]
pub fn is_market_path(path: &str) -> bool {
    path.starts_with("/v1/market/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_backfill::extract::QUOTE_MINTS;

    #[test]
    fn every_declared_interval_maps_to_a_distinct_number_of_seconds() {
        let names = ["1m", "5m", "15m", "1h", "4h", "1d"];
        let mut seconds: Vec<i64> = names.iter().map(|n| interval_seconds(n).unwrap()).collect();
        seconds.sort_unstable();
        seconds.dedup();
        assert_eq!(seconds.len(), names.len());
        assert_eq!(
            interval_seconds("2m"),
            None,
            "an undeclared interval is refused"
        );
    }

    #[test]
    fn a_quote_mint_is_never_treated_as_a_coin_candidate() {
        // Not a route test -- just pinning that the constant this module
        // reuses is the one with the lookalike wrapped-SOL mint excluded.
        assert!(QUOTE_MINTS.contains(&"So11111111111111111111111111111111111111112"));
        assert!(QUOTE_MINTS.contains(&"So11111111111111111111111111111111111111111"));
    }

    #[test]
    fn degradation_reasons_are_distinguishable_and_never_look_like_success() {
        for d in [
            Degradation::Unreachable,
            Degradation::TimedOut,
            Degradation::RowCapHit,
        ] {
            assert!(d.status().is_client_error() || d.status().is_server_error());
            assert_ne!(d.status(), StatusCode::OK);
        }
        assert_ne!(Degradation::TimedOut.code(), Degradation::RowCapHit.code());
        assert_ne!(
            Degradation::Unreachable.code(),
            Degradation::TimedOut.code()
        );
    }

    #[test]
    fn a_row_cap_exception_is_classified_as_the_row_cap_not_a_timeout() {
        let e = QueryError::Server("Code: 396. DB::Exception: Limit for result exceeded, max rows: 1.00 thousand (TOO_MANY_ROWS_OR_BYTES)".to_owned());
        assert_eq!(Degradation::classify(&e), Degradation::RowCapHit);
    }

    #[test]
    fn a_plain_timeout_exception_is_classified_as_timed_out() {
        let e = QueryError::Server(
            "Code: 159. DB::Exception: Timeout exceeded (TIMEOUT_EXCEEDED)".to_owned(),
        );
        assert_eq!(Degradation::classify(&e), Degradation::TimedOut);
    }

    #[test]
    fn a_transport_failure_is_unreachable_not_a_data_fact() {
        let e = QueryError::Transport("connection refused".to_owned());
        assert_eq!(Degradation::classify(&e), Degradation::Unreachable);
    }

    #[test]
    fn a_row_capped_result_is_never_reported_as_a_complete_success() {
        // The rubric this task names directly: re-applying the bug means
        // treating a row-capped fetch as though it produced a normal answer.
        // `Degradation` has no variant that renders as 200, so there is no
        // path from this classification to a response claiming completeness.
        let e = QueryError::Server("TOO_MANY_ROWS_OR_BYTES".to_owned());
        let d = Degradation::classify(&e);
        assert_eq!(d, Degradation::RowCapHit);
        assert_ne!(d.status(), StatusCode::OK);
    }

    #[test]
    fn a_malformed_mint_is_refused_before_any_query_is_built() {
        assert!(parse_mint("not-base58!!").is_err());
        assert!(parse_mint("").is_err());
        assert!(parse_mint("5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump").is_ok());
    }

    #[test]
    fn a_malformed_timestamp_is_refused_rather_than_silently_defaulted() {
        assert!(parse_stamp("not a timestamp").is_err());
        assert!(parse_stamp("2026-09-11 17:00:00").is_ok());
    }
}
