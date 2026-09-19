// SPDX-License-Identifier: Apache-2.0
//! Public market data: the trade tape, candles, the live coin list, a coin's
//! header, and folded holders — tier 1 of
//! [plan 0012](../../../../docs/plans/0012-the-public-trading-panel.md).
//!
//! Every route here is `Audience::Public` in [`crate::access`] and takes no
//! identity. Market facts belong to nobody: they carry no `Tenant`, read no
//! customer store, and must never gain either.
//!
//! # Where the numbers come from now, and why that changed
//!
//! Every route here used to query CryptoHouse live, once or more per HTTP
//! request. CryptoHouse permits 120 queries an hour per IP, shared with
//! `radar-follow` and the hourly `--outcomes` cron — measured at 291 queries
//! in one hour from a handful of terminal loads on 2026-09-11, and every
//! request after that failed with `QUOTA_EXCEEDED` until the hour rolled
//! over. No cache TTL fixes this: the coin list alone cost two queries, so a
//! one-minute refresh consumed the whole hourly budget by itself.
//!
//! So the direction is flipped. [`radar_backfill::market_tape`] is a
//! collector that spends the budget on a fixed schedule (60 queries an hour,
//! see its own doc comment for the arithmetic) and writes what it finds to
//! [`radar_store::Table::MarketTrades`]. **Every route in this module reads
//! only [`crate::AppState::store`] and issues zero CryptoHouse queries on any
//! request path.** [`radar_backfill::market::fold`] — `detect_pool`,
//! `side_and_trader`, `fold_candles`, `fold_holders`, `adjust` — is the same
//! pure code the collector runs; only where its input comes from changed.
//!
//! # Honest degradation
//!
//! A caller asking about a window the collector has not reached yet is told
//! so plainly, distinguishably from a window the collector reached and found
//! quiet — see [`Degradation::NotCollected`], gated by
//! [`market_tape_collected`] against [`radar_store::Table::Coverage`]. A
//! store that cannot be read is a separate fact again, about this build or
//! this disk, never about a coin.
//!
//! Holder balances and token metadata are not collected by
//! [`radar_backfill::market_tape`] at all today — it collects the trade tape
//! only — so [`holders`] and the metadata half of [`token`] say that plainly
//! rather than returning an empty answer that looks like a quiet market.

pub mod live;

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use radar_asof::AsOf;
use radar_backfill::market::fold as market_fold;
// `now_epoch` is deliberately absent. **No handler in this module may end a
// window at wall-clock time**: the collector runs behind by design, so `now()`
// names a span it has not reached, and every such request answered with an
// empty list marked complete. Windows end at `newest_collected` instead, and
// the missing import is what stops that regressing quietly.
use radar_store::{Event, MarketSide, MarketTrade, Reader, StoreError, Table, from_epoch};
use radar_types::{Address, Slot};
use serde::Deserialize;
use serde_json::{Value, json};

/// A single top-level window's width, when a caller does not name one — the
/// span of trades folded into the tape by default.
///
/// **Thirty minutes, not two.** Two was the width while these routes queried
/// CryptoHouse live, where a narrow window was a cheap one. Reading the store
/// it costs nothing, and two minutes was too narrow to show anything: the
/// collector works in five-minute passes, so a coin busy enough to rank in the
/// ten-minute coin list can easily have no trade in the last two minutes, and
/// its tape came back empty and `complete` — a true statement about a sliver,
/// read by everyone as a statement about the coin. Observed in production on
/// 2026-09-12, on four of the four busiest coins at once.
///
/// The answer is bounded by the caller's `limit` regardless, so a wider window
/// changes how far back the newest trades are found, never how many come back.
const DEFAULT_WINDOW_SECONDS: i64 = 30 * 60;

/// How far back a chart reaches when the caller names no range.
///
/// Wider than the tape's window because the two answer different questions: a
/// tape shows what just happened, a chart shows a shape, and a shape needs
/// more than two minutes of it. Bounded rather than unbounded because the
/// response states the range it actually covered, and a caller asking for a
/// day of a coin collected for ten minutes should be told that plainly rather
/// than handed a day-shaped axis with ten minutes drawn on it.
const DEFAULT_CANDLE_WINDOW_SECONDS: i64 = 60 * 60;

/// The window `/v1/market/coins` ranks activity over.
const COINS_WINDOW_SECONDS: i64 = 10 * 60;

/// A chart reaches further back than a tape: a shape needs more than two
/// minutes of itself. Held at compile time so the two windows cannot be
/// reordered by an edit to either.
const _: () = assert!(DEFAULT_CANDLE_WINDOW_SECONDS > DEFAULT_WINDOW_SECONDS);

/// A tape narrower than one collector pass shows an empty sliver of a busy
/// coin -- the defect that made four of the four busiest coins look untraded
/// on 2026-09-12. Held here so a future narrowing of the window fails the
/// build rather than the screen.
const _: () = assert!(DEFAULT_WINDOW_SECONDS > radar_backfill::market_tape::PASS_INTERVAL_SECONDS);

/// Whether a requested range does not run forwards.
///
/// A range whose start is at or after its end is a caller mistake, refused
/// with a message rather than answered with an empty list — the two are
/// different facts and only one is about the market.
///
/// **Named for the true case**, so the call site reads `if
/// is_a_backwards_range(..)` with no `!` in front of it. A leading `!` is one
/// character a mutation deletes, and deleting this one refuses every valid
/// range and accepts every invalid one.
const fn is_a_backwards_range(from: i64, to: i64) -> bool {
    from >= to
}

/// The start of a range, clamped so it reaches back no further than `max`.
///
/// The response says when this clamps, so the caller is never told a day was
/// covered when an hour was. Adding `max` instead of subtracting it moves the
/// floor into the future and the clamp swallows the whole range.
const fn clamped_start(requested_from: i64, to: i64, max_span: i64) -> i64 {
    let floor = reaching_back(to, max_span);
    if requested_from > floor {
        requested_from
    } else {
        floor
    }
}

/// Whether any trade in a window carried both legs, and so has a quote amount
/// to sum.
///
/// Stated positively on purpose: the negated form at the call site carries a
/// `!` that a mutation can delete, turning "no priced fill, so no volume" into
/// "no priced fill, so sum an empty list and report zero volume" — a figure
/// nobody measured, in the column a reader sorts by.
fn has_a_priced_fill<T>(priced: &[T]) -> bool {
    !priced.is_empty()
}

/// Said when the store holds no market trades at all.
///
/// Distinct from a collected-and-quiet window: this instance has never had the
/// collector run against it, so there is nothing to be quiet about. A caller
/// reading it knows to check the collector rather than the market.
const NOTHING_COLLECTED: &str =
    "this instance has collected no market trades yet; run radar-backfill --market-tape";

/// The public market-data seam: where the market routes read from.
///
/// Without a live feed, every route reads [`crate::AppState::store`], which the
/// free `radar-backfill --market-tape` collector fills. With one
/// ([`Self::with_live`], set up in `main.rs` when `RADAR_STREAM_ENDPOINT` is
/// set), every route answers from the feed's in-memory tape instead, in
/// [`live`]. Never both for one request: a screen mixing a coin's live tape
/// with the store's five-minute-old chart would show two different markets.
#[derive(Default)]
pub struct Market {
    live: Option<Arc<radar_stream::Live>>,
}

impl Market {
    /// Reads the store. See the type's own doc comment.
    #[must_use]
    pub fn new() -> Self {
        Self { live: None }
    }

    /// Reads the live feed.
    #[must_use]
    pub const fn with_live(live: Arc<radar_stream::Live>) -> Self {
        Self { live: Some(live) }
    }

    /// The live feed, when this instance has one.
    #[must_use]
    pub fn live(&self) -> Option<&radar_stream::Live> {
        self.live.as_deref()
    }
}

/// How many slots behind the watermark the cached market snapshot reaches.
///
/// **~24h.** [`MAX_CANDLE_WINDOW_SECONDS`] is the widest span any route in
/// this module ever serves, so a snapshot narrower than that would make the
/// snapshot itself the constraint a route is supposed to state on its own.
/// Converted the same way [`LAUNCH_LOOKBACK_SLOTS`] was: one slot is roughly
/// 400ms (`radar_types::SlotDelta::approx_duration`), so 24 * 3600 * 1000 /
/// 400 = 216,000, rounded up for headroom against that approximation's own
/// error rather than shaved thin against it.
pub const SNAPSHOT_WINDOW_SLOTS: u64 = 220_000;

/// The newest timestamp among a set of stored trade rows, trimmed to the
/// second.
///
/// Pulled out of what used to be `newest_collected`'s body so
/// [`Snapshot::build`] can compute the same figure over an in-memory window
/// instead of a fresh store read -- one function, so the trimming rule (a
/// row's microseconds truncate down, never round up past it) is not stated
/// twice with room for the two copies to disagree.
fn newest_ts_of(rows: &[MarketTrade]) -> Option<i64> {
    let newest = rows.iter().map(|t| t.ts.clone()).max();
    // Stored timestamps carry CryptoHouse's microseconds
    // (`2026-09-12 16:31:52.000000`) and `to_epoch` takes whole seconds, so
    // the fraction is trimmed rather than parsed. Truncating toward the
    // second is the safe direction: the window ends no later than the newest
    // row, so it can never claim to cover a moment nothing was read at.
    let trimmed = newest.map(|ts| ts.split('.').next().unwrap_or(&ts).to_owned());
    trimmed.and_then(|ts| radar_store::to_epoch(&ts).ok())
}

/// The tape rows for one mint inside `[from_s, to_s)`, newest first.
///
/// Pulled out of what used to be `tape_for`'s body so a route can filter an
/// already-read [`Snapshot`] in memory with the same logic `tape_for` still
/// uses (and is still tested through) for a direct store read.
fn filter_tape(
    rows: &[MarketTrade],
    mint: Address,
    from_s: &str,
    to_s: &str,
) -> Vec<market_fold::Trade> {
    let mut trades: Vec<market_fold::Trade> = rows
        .iter()
        .filter(|t| t.mint == mint && within_window(&t.ts, from_s, to_s))
        .map(to_fold_trade)
        .collect();
    // The same order `fold_tape` produced when the collector wrote these
    // rows -- newest first, ties broken by signature so the order does not
    // depend on how the rows happened to be laid out across files.
    trades.sort_by(|a, b| b.ts.cmp(&a.ts).then_with(|| b.signature.cmp(&a.signature)));
    trades
}

/// Everything a market route needs, read off the store once and reused by
/// every request until the background refresher (or, in a test with none
/// configured, the next request) replaces it.
///
/// **Every route in this module used to read the store per request** --
/// `read_market_trades` (the whole table: 1,750 files, ~1.5M rows measured),
/// `read_coverage` (6,863 files), and `build_launch_index` (7 days of
/// `Launches`), up to twice for one `/v1/market/coins` call. Two such
/// requests in flight pinned both cores of a 2-core box and `/` stopped
/// answering too, because a blocking store read on the async runtime blocks
/// everything else scheduled on it. This struct is what a request reads
/// instead: an in-memory snapshot, built off the request path by
/// [`Self::build`]/[`Self::refresh_trades`] and handed out through
/// [`SnapshotCache`].
pub struct Snapshot {
    watermark: Slot,
    trades: Vec<MarketTrade>,
    collected: bool,
    newest_ts: Option<i64>,
    launches: Arc<LaunchIndex>,
}

impl Snapshot {
    /// Builds a snapshot at `as_of`: the trade window, coverage, and the
    /// launch index, each read once.
    ///
    /// **Synchronous and blocking** -- exactly the store reads it replaces
    /// were. A caller on the async runtime must run this inside
    /// `tokio::task::spawn_blocking`. Used directly by tests, which construct
    /// `AppState` with [`SnapshotCache::new`] (no background refresher), and
    /// by `main.rs`'s background refresher on its own cadence; a route
    /// handler must never call this when a refresher is configured -- see
    /// [`snapshot_for`].
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if the store cannot be read.
    pub fn build(store: &Reader, as_of: AsOf) -> Result<Self, StoreError> {
        let collected = market_tape_collected(store, as_of)?;
        let from = Slot(as_of.slot().get().saturating_sub(SNAPSHOT_WINDOW_SLOTS));
        let trades = store.read_market_trades_range(as_of, Some(from))?;
        let newest_ts = newest_ts_of(&trades);
        let launches = build_launch_index(store, as_of)?;
        Ok(Self {
            watermark: as_of.slot(),
            trades,
            collected,
            newest_ts,
            launches: Arc::new(launches),
        })
    }

    /// Rebuilds the trade half only, reusing the existing launch index.
    ///
    /// The launch scan is the expensive half by row count -- 7 days of
    /// `Launches` versus ~24h of `MarketTrades` -- so the background
    /// refresher calls this on its frequent tick and reserves the full
    /// [`Self::build`] for its own, much less frequent, cadence. Reusing
    /// `launches` is an `Arc` clone, not a copy of the map.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if the store cannot be read.
    pub fn refresh_trades(&self, store: &Reader, as_of: AsOf) -> Result<Self, StoreError> {
        let collected = market_tape_collected(store, as_of)?;
        let from = Slot(as_of.slot().get().saturating_sub(SNAPSHOT_WINDOW_SLOTS));
        let trades = store.read_market_trades_range(as_of, Some(from))?;
        let newest_ts = newest_ts_of(&trades);
        Ok(Self {
            watermark: as_of.slot(),
            trades,
            collected,
            newest_ts,
            launches: Arc::clone(&self.launches),
        })
    }

    /// Whether the market-tape collector had produced anything at all as of
    /// this snapshot's watermark.
    #[must_use]
    pub const fn collected(&self) -> bool {
        self.collected
    }

    /// The newest moment the store held a market trade at, as of this
    /// snapshot's watermark.
    #[must_use]
    pub const fn newest_ts(&self) -> Option<i64> {
        self.newest_ts
    }

    /// The trades this snapshot holds: the last [`SNAPSHOT_WINDOW_SLOTS`]
    /// slots before its watermark.
    #[must_use]
    pub fn trades(&self) -> &[MarketTrade] {
        &self.trades
    }

    /// The launch index this snapshot holds.
    #[must_use]
    pub fn launches(&self) -> &LaunchIndex {
        &self.launches
    }

    /// The watermark this snapshot was built at.
    #[must_use]
    pub const fn watermark(&self) -> Slot {
        self.watermark
    }
}

/// The current [`Snapshot`], if one has been built yet.
///
/// A route reads this, never the store, for anything a [`Snapshot`] carries.
/// [`Self::has_background_refresh`] tells [`snapshot_for`] which of the two
/// honest answers to give when there is nothing here yet, or nothing at the
/// caller's watermark: `main.rs` sets it and a route waits for the refresher
/// rather than blocking a request on a rebuild; the test files that construct
/// `AppState` directly leave it unset ([`Self::new`]), so a route under test
/// builds one synchronously on a cache miss, which is the one request a test
/// makes.
#[derive(Default)]
pub struct SnapshotCache {
    current: std::sync::Mutex<Option<Arc<Snapshot>>>,
    background: bool,
}

impl SnapshotCache {
    /// No background refresher: [`snapshot_for`] may build one synchronously
    /// on a cache miss. What every test file's `AppState` literal uses.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A background refresher owns this cache: [`snapshot_for`] must never
    /// build one itself, only read what is here and let a route answer
    /// [`Degradation::NotCollected`] until the refresher has filled it in.
    #[must_use]
    pub const fn with_background_refresh() -> Self {
        Self {
            current: std::sync::Mutex::new(None),
            background: true,
        }
    }

    /// Whether a background refresher is responsible for this cache.
    #[must_use]
    pub const fn has_background_refresh(&self) -> bool {
        self.background
    }

    /// The current snapshot, if one exists.
    #[must_use]
    pub fn peek(&self) -> Option<Arc<Snapshot>> {
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Replaces the current snapshot.
    pub fn set(&self, snapshot: Arc<Snapshot>) {
        *self
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(snapshot);
    }
}

/// The snapshot a route should read, building one synchronously only when no
/// background refresher is configured and none exists yet or the existing one
/// is not at `as_of`'s watermark.
///
/// **With a refresher configured, a stale or missing snapshot is never built
/// here.** A route with nothing yet answers [`Degradation::NotCollected`]
/// immediately; a route with a stale one serves it anyway -- at most one
/// refresher tick old, which is the honest cost of moving the read off the
/// request path, never a read past the caller's own watermark (rule 3: the
/// snapshot's `watermark` field is what it was built at, not what a later
/// request asks for).
///
/// Without one (every test's `AppState`), a mismatched watermark is rebuilt
/// rather than served stale, because a test's fixture and its assertions are
/// written for its own watermark, not whatever an earlier test in the same
/// process left behind.
///
/// # Errors
///
/// Returns [`StoreError`] if a synchronous build reads a broken store.
fn snapshot_for(
    cache: &SnapshotCache,
    store: &Reader,
    as_of: AsOf,
) -> Result<Option<Arc<Snapshot>>, StoreError> {
    match cache.peek() {
        Some(snap) if snap.watermark() == as_of.slot() => Ok(Some(snap)),
        Some(snap) if cache.has_background_refresh() => Ok(Some(snap)),
        _ if cache.has_background_refresh() => Ok(None),
        _ => {
            let built = Arc::new(Snapshot::build(store, as_of)?);
            cache.set(Arc::clone(&built));
            Ok(Some(built))
        }
    }
}

/// What one tick of the background refresher should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    /// Nothing moved: no store read at all.
    Skip,
    /// The store moved: re-read the trade window, keep the launch index.
    TradesOnly,
    /// Read everything, the 7-day launch scan included.
    Everything,
}

/// Decides what a refresher tick reads.
///
/// Pulled out of `main.rs`'s loop because this is the decision the whole fix
/// rests on: answer [`Refresh::Skip`] too rarely and the box is back to
/// re-reading the store all day, too often and the screen freezes on an old
/// snapshot while looking healthy. With no snapshot to reuse a launch index
/// from, a moved store means [`Refresh::Everything`], never a half-built one.
#[must_use]
pub const fn refresh_plan(launches_due: bool, store_moved: bool, have_snapshot: bool) -> Refresh {
    if launches_due {
        Refresh::Everything
    } else if !store_moved {
        Refresh::Skip
    } else if have_snapshot {
        Refresh::TradesOnly
    } else {
        Refresh::Everything
    }
}

/// Whether the launch index is due a rebuild: never built, or built at least
/// `every` ago.
#[must_use]
pub fn launches_due(since_last: Option<std::time::Duration>, every: std::time::Duration) -> bool {
    since_last.is_none_or(|elapsed| elapsed >= every)
}

/// Why a market route could not answer, distinguishably from an empty
/// result.
///
/// **This, not an empty list, is what "the data is not here" looks like.**
/// Rule 9: a missing measurement must never read like a quiet one.
#[derive(Debug)]
enum Degradation {
    /// The store could not be read. A fact about this build or this disk,
    /// never about a coin — the store-backed analogue of what used to be a
    /// CryptoHouse transport or decoding failure.
    StoreUnreadable(String),
    /// The market-tape collector has not covered the range this answer would
    /// need, or does not collect this kind of fact at all. The `&'static
    /// str` says which, because a caller who cannot tell "not yet" from
    /// "never" cannot decide whether to retry.
    NotCollected(&'static str),
}

impl Degradation {
    fn from_store_error(e: &StoreError) -> Self {
        Self::StoreUnreadable(e.to_string())
    }

    const fn status(&self) -> StatusCode {
        match self {
            Self::StoreUnreadable(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotCollected(_) => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    const fn code(&self) -> &'static str {
        match self {
            Self::StoreUnreadable(_) => "store_unreadable",
            Self::NotCollected(_) => "not_collected",
        }
    }

    fn message(&self) -> String {
        match self {
            Self::StoreUnreadable(detail) => format!(
                "the store could not be read; this is a fact about this build or this disk, not about the coin: {detail}"
            ),
            Self::NotCollected(reason) => format!(
                "not collected -- distinguishable from a collected-and-quiet range: {reason}"
            ),
        }
    }
}

impl IntoResponse for Degradation {
    fn into_response(self) -> Response {
        let status = self.status();
        let code = self.code();
        let message = self.message();
        (status, Json(json!({ "error": code, "message": message }))).into_response()
    }
}

fn bad_request(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "bad_request", "message": message })),
    )
        .into_response()
}

fn parse_mint(raw: &str) -> Result<Address, Box<Response>> {
    raw.parse::<Address>()
        .map_err(|_| Box::new(bad_request("mint must be a base58-encoded Solana address")))
}

/// A `YYYY-MM-DD HH:MM:SS` timestamp, as every query parameter accepting one
/// expects. Fractional seconds are not accepted here — a caller building a
/// cursor from a response's own `ts` field must trim them, since this
/// module's output already carries CryptoHouse's microsecond precision.
fn parse_stamp(raw: &str) -> Result<i64, Box<Response>> {
    radar_store::to_epoch(raw)
        .map_err(|_| Box::new(bad_request("expected a timestamp as 'YYYY-MM-DD HH:MM:SS'")))
}

/// Whether the market-tape collector has ever produced a
/// [`Table::MarketTrades`] coverage record as of `as_of`.
///
/// **This is coarser than "was this exact window collected".** The collector
/// runs a live tape, not a historical backfill, so a fresh deployment has no
/// coverage at all and an established one is asked about recent time almost
/// always inside what it has already reached. A caller naming a window far
/// in the past, before collection began, is not separately detected here —
/// that would need converting the caller's time-native request into the
/// slot-native terms [`radar_store::coverage`] deliberately refuses to
/// invent a conversion for. What this *does* catch, correctly, is the case
/// that actually happens: a store nobody has run the collector against yet.
fn market_tape_collected(store: &Reader, as_of: AsOf) -> Result<bool, StoreError> {
    Ok(store
        .read_coverage(as_of)?
        .iter()
        .any(|c| c.table == Table::MarketTrades))
}

/// One coin's launch-recorded identity, as
/// [`radar_backfill`]'s launch collector wrote it to
/// [`radar_store::Table::Launches`] when the pump.fun `create` instruction
/// was decoded.
///
/// Untrusted (rule 4 in `AGENTS.md`): `name`, `symbol` and `uri` are
/// attacker-chosen strings copied verbatim off an on-chain instruction. They
/// are rendered as text and never interpreted -- see [`capped`] for the
/// bound applied before they leave this process, and `web/src`'s scheme
/// allow-list for the bound applied before `uri` is ever fetched, client
/// side, by a browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchInfo {
    name: String,
    symbol: String,
    /// The off-chain metadata JSON's location. **Never fetched by this
    /// server** -- returned as an opaque string for the browser to resolve.
    uri: String,
}

/// Mint -> its launch identity, for every launch this instance recorded
/// within [`LAUNCH_LOOKBACK_SLOTS`] of the watermark it was built at.
///
/// Built once per watermark by [`build_launch_index`] and held in
/// [`crate::AppState::launches`] -- never read per request. See that cache
/// field's doc comment and [`crate::cache::Cache`]'s module doc for why: the
/// same reuse-until-stale pattern already used for the scoreboard and for a
/// token's evidence.
pub type LaunchIndex = std::collections::HashMap<Address, LaunchInfo>;

/// The longest a `name` or `symbol` field may be in a response.
///
/// Untrusted, attacker-controlled strings (rule 4): pump.fun's `create`
/// instruction does not bound their length, so a launch could otherwise carry
/// an arbitrarily large payload into every `/v1/market/coins` response that
/// mint appears in.
const MAX_METADATA_FIELD_LEN: usize = 200;

/// The longest a `uri` field may be in a response. Wider than
/// [`MAX_METADATA_FIELD_LEN`]: a legitimate metadata URI (an `ipfs://` CID or
/// an HTTPS path) can run longer than a name ever should, but it is still
/// bounded -- the same untrusted-string reasoning applies.
const MAX_URI_LEN: usize = 2_000;

/// Truncates `s` to `max` chars, counted rather than bytes so a truncation
/// point never lands inside a multi-byte character.
fn capped(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        s.chars().take(max).collect()
    }
}

/// How far behind the watermark [`build_launch_index`] reads.
///
/// **~7 days.** docs/STATE.md measured 72,193 launches recorded in a
/// two-day window -- an unbounded index would grow without limit as the
/// store's history grows, and every caller of this module names a mint
/// because it is on the coin list or trade tape, i.e. traded inside the last
/// [`COINS_WINDOW_SECONDS`]/[`DEFAULT_WINDOW_SECONDS`], so a launch older
/// than a week is not one any caller here could plausibly be asking about.
/// Bounding the read also bounds the map: [`Reader::read_range`] skips whole
/// partition files outside this range without opening them, so both the
/// per-rebuild read cost and the index's memory stay flat as history grows.
///
/// The slot count comes from this codebase's own documented approximation --
/// [`radar_types::SlotDelta::approx_duration`]'s "one slot is roughly 400ms"
/// -- rather than a separately invented one: 7 * 24 * 3600 * 1000 / 400 =
/// 1,512,000.
const LAUNCH_LOOKBACK_SLOTS: u64 = 1_512_000;

/// Scans [`radar_store::Table::Launches`] once, back to
/// [`LAUNCH_LOOKBACK_SLOTS`] before `as_of`, and folds it into a lookup by
/// mint.
///
/// **Not called per request.** [`crate::AppState::launches`] calls this
/// through [`crate::cache::Cache::get_or_compute`]/`recent`, exactly the
/// pattern already used for the scoreboard and for a token's evidence
/// (`crates/radar-serve/src/cache.rs`) -- computed once per watermark, reused
/// for every caller asking inside the staleness allowance.
fn build_launch_index(store: &Reader, as_of: AsOf) -> Result<LaunchIndex, StoreError> {
    let from = Slot(as_of.slot().get().saturating_sub(LAUNCH_LOOKBACK_SLOTS));
    let events = store.read_range(Table::Launches, as_of, Some(from), None)?;
    let mut index = LaunchIndex::new();
    for event in events {
        if let Event::Launch(launch) = event {
            index.insert(
                launch.mint,
                LaunchInfo {
                    name: capped(&launch.name, MAX_METADATA_FIELD_LEN),
                    symbol: capped(&launch.symbol, MAX_METADATA_FIELD_LEN),
                    uri: capped(&launch.uri, MAX_URI_LEN),
                },
            );
        }
    }
    Ok(index)
}

/// Converts a stored, folded row into the shape
/// [`market_fold::fold_candles`] and the tape response already expect.
///
/// The store holds exactly [`market_fold::Trade`] plus the mint it belongs
/// to (`radar_store::MarketTrade`'s own doc comment), so this is a type
/// conversion, not a re-fold: every value on the right already came from
/// [`market_fold::fold_tape`] when the collector wrote it.
fn to_fold_trade(row: &MarketTrade) -> market_fold::Trade {
    market_fold::Trade {
        ts: row.ts.clone(),
        slot: row.slot.get(),
        signature: row.signature.to_string(),
        side: match row.side {
            MarketSide::Buy => market_fold::Side::Buy,
            MarketSide::Sell => market_fold::Side::Sell,
            MarketSide::Unknown => market_fold::Side::Unknown,
        },
        token_amount: row.token_amount,
        quote_amount: row.quote_amount,
        quote_mint: row.quote_mint.map(|m| m.to_string()),
        price: row.price,
        trader: row.trader.map(|a| a.to_string()),
    }
}

/// Every stored trade for one mint, newest first, from the store's rows in
/// `(as_of, mint, window)`.
///
/// # Errors
///
/// Returns [`StoreError`] if the store cannot be read.
/// The newest moment the store actually holds market trades for.
///
/// **The default window ends here, not at `now()`, and that is not a
/// refinement — it is the difference between a working tape and an empty
/// one.** The collector runs behind wall-clock by design: it lags
/// `market_tape::PASS_INTERVAL` seconds so the window it asks for has landed
/// in CryptoHouse, and a pass takes time on top of that. A handler that
/// defaulted its window to the last two minutes of wall-clock time therefore
/// asked for a span the collector had not reached yet, and answered every
/// visitor with an empty list and `"complete": true` — a confident claim that
/// nothing traded, when the truth was that nothing had been collected yet.
/// Observed 2026-09-12 against a store the collector had just filled.
///
/// `None` when the store holds no market trades at all, which is a different
/// answer again: not "the market is quiet", but "this instance has collected
/// nothing", and the caller says so.
/// Why a coin's header carries no price.
///
/// Two different facts, and a caller acts on them differently: nothing traded
/// in the window at all, or trades happened and none of them paired with a
/// quote leg. Collapsing them would tell somebody the coin is quiet when it is
/// busy and unpriceable.
///
/// Named so the distinction is testable. Inside the `match` it was a guard no
/// test reached, and both of its mutations -- always empty, never empty --
/// swap one message for the other silently.
const fn why_no_price(no_trades_at_all: bool) -> &'static str {
    if no_trades_at_all {
        "no recorded trades in this window"
    } else {
        "recent trades exist but none paired with a quote leg in this window"
    }
}

/// Whether a stored timestamp falls inside a window.
///
/// **Half-open: `>= from`, `< to`**, so a row exactly at `to` belongs to the
/// next window rather than being counted in both. The default window ends at
/// the newest row the store holds, so an inclusive upper bound would make
/// every default window double-count its own edge.
///
/// Compared as strings, which is exact for this stamp format and is why the
/// bound is easy to get wrong in a test: `2020-01-01 00:00:30.000000` sorts
/// after `2020-01-01 00:00:30` under either operator, so a fixture whose
/// stamp carries microseconds never sits on the boundary at all.
///
/// One function, because this filter was written out twice -- once for the
/// tape and once for the coin list -- and the second copy had its own
/// surviving mutants on every comparison in it.
fn within_window(ts: &str, from: &str, to: &str) -> bool {
    ts >= from && ts < to
}

/// Where a window of `span` seconds ending at `to` begins.
///
/// A window reaches **back** from its end. Written inline as `to - span` in
/// three handlers, where an addition reaches forward instead -- asking for a
/// span the collector has not reached and answering every caller with an empty
/// list. One name, one subtraction, and a test that holds it.
const fn reaching_back(to: i64, span_seconds: i64) -> i64 {
    to - span_seconds
}

// Routes no longer call these directly -- they go through `snapshot_for` and
// the snapshot's own cached rows. Kept `#[cfg(test)]`-only because the tests
// below exercise `newest_ts_of`/`filter_tape` through this exact shape, and
// duplicating that coverage against the new call path would test the
// snapshot, not the pure logic these wrap.
#[cfg(test)]
fn newest_collected(store: &Reader, as_of: AsOf) -> Result<Option<i64>, StoreError> {
    Ok(newest_ts_of(&store.read_market_trades(as_of)?))
}

#[cfg(test)]
fn tape_for(
    store: &Reader,
    as_of: AsOf,
    mint: Address,
    from_s: &str,
    to_s: &str,
) -> Result<Vec<market_fold::Trade>, StoreError> {
    Ok(filter_tape(
        &store.read_market_trades(as_of)?,
        mint,
        from_s,
        to_s,
    ))
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
    if let Some(feed) = state.market.live() {
        return match params.before.as_deref().map(parse_stamp).transpose() {
            Ok(before) => live::trades(feed, mint, params.limit, before),
            Err(r) => *r,
        };
    }
    let watermark = match crate::watermark_of(&state) {
        Ok(w) => w,
        Err(e) => return e.into_response(),
    };
    let as_of = AsOf::at(watermark);
    let snapshot = match snapshot_for(&state.market_snapshot, &state.store, as_of) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Degradation::NotCollected(
                "the market snapshot has not been built yet; check back shortly",
            )
            .into_response();
        }
        Err(e) => return Degradation::from_store_error(&e).into_response(),
    };
    if !snapshot.collected() {
        return Degradation::NotCollected(
            "the market-tape collector has not produced anything for this store yet",
        )
        .into_response();
    }

    let to = match params.before.as_deref().map(parse_stamp).transpose() {
        Ok(Some(before)) => before,
        // No cursor: end the window at what the store actually holds, never at
        // wall-clock time. See `newest_ts_of` -- the collector runs behind
        // by design, so `now()` names a span it has not reached and the answer
        // is an empty tape presented as complete.
        Ok(None) => match snapshot.newest_ts() {
            Some(newest) => newest,
            None => return Degradation::NotCollected(NOTHING_COLLECTED).into_response(),
        },
        Err(r) => return *r,
    };
    let limit = params
        .limit
        .unwrap_or(DEFAULT_TRADE_LIMIT)
        .clamp(1, MAX_TRADE_LIMIT);
    let from = reaching_back(to, DEFAULT_WINDOW_SECONDS);
    let (from_s, to_s) = (from_epoch(from), from_epoch(to));

    let mut trades = filter_tape(snapshot.trades(), mint, &from_s, &to_s);
    trades.truncate(limit);
    Json(json!({
        "mint": mint.to_string(),
        "window": { "from": from_s, "to": to_s, "complete": true },
        "trades": trades,
    }))
    .into_response()
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
    if let Some(feed) = state.market.live() {
        let from = match params.from.as_deref().map(parse_stamp).transpose() {
            Ok(v) => v,
            Err(r) => return *r,
        };
        let to = match params.to.as_deref().map(parse_stamp).transpose() {
            Ok(v) => v,
            Err(r) => return *r,
        };
        let name = params.interval.as_deref().unwrap_or("1m");
        return live::candles(feed, mint, name, interval, from, to);
    }
    let watermark = match crate::watermark_of(&state) {
        Ok(w) => w,
        Err(e) => return e.into_response(),
    };
    let as_of = AsOf::at(watermark);
    let snapshot = match snapshot_for(&state.market_snapshot, &state.store, as_of) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Degradation::NotCollected(
                "the market snapshot has not been built yet; check back shortly",
            )
            .into_response();
        }
        Err(e) => return Degradation::from_store_error(&e).into_response(),
    };
    if !snapshot.collected() {
        return Degradation::NotCollected(
            "the market-tape collector has not produced anything for this store yet",
        )
        .into_response();
    }

    let requested_to = match params.to.as_deref().map(parse_stamp).transpose() {
        Ok(Some(to)) => to,
        // The store's own horizon, not wall-clock -- see `newest_ts_of`.
        // A chart defaulting to the last two minutes of wall-clock time drew
        // nothing at all, because the collector is always behind it.
        Ok(None) => match snapshot.newest_ts() {
            Some(newest) => newest,
            None => return Degradation::NotCollected(NOTHING_COLLECTED).into_response(),
        },
        Err(r) => return *r,
    };
    let requested_from = match params.from.as_deref().map(parse_stamp).transpose() {
        Ok(v) => v.unwrap_or(reaching_back(requested_to, DEFAULT_CANDLE_WINDOW_SECONDS)),
        Err(r) => return *r,
    };
    if is_a_backwards_range(requested_from, requested_to) {
        return bad_request("from must be before to");
    }
    // The range actually covered may be narrower than requested -- clamped
    // rather than silently served, per the plan's own rubric for this
    // endpoint.
    let from = clamped_start(requested_from, requested_to, MAX_CANDLE_WINDOW_SECONDS);
    let to = requested_to;
    let (from_s, to_s) = (from_epoch(from), from_epoch(to));

    let trades = filter_tape(snapshot.trades(), mint, &from_s, &to_s);
    let candles = market_fold::fold_candles(&trades, interval);
    Json(json!({
        "mint": mint.to_string(),
        "interval": params.interval.as_deref().unwrap_or("1m"),
        "requested": { "from": from_epoch(requested_from), "to": from_epoch(requested_to) },
        "covered": { "from": from_s, "to": to_s, "complete": true },
        "candles": candles,
    }))
    .into_response()
}

/// `/v1/market/coins` query parameters.
#[derive(Debug, Deserialize)]
pub struct CoinsParams {
    limit: Option<usize>,
    sort: Option<String>,
}

const DEFAULT_COINS_LIMIT: usize = 50;
const MAX_COINS_LIMIT: usize = 200;

fn sort_coins(coins: &mut [market_fold::Coin], sort: &str) {
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

/// Folds a window of stored trades, across every mint the collector saw,
/// into one row per mint.
///
/// The store-backed analogue of [`market_fold::fold_coins`]: that function
/// reads a live `CoinCandidateRow`/`CoinPriceRow` pair this module no longer
/// fetches, so this reads the rows the collector already wrote instead. Both
/// exist because they read differently-shaped inputs, not because one is a
/// draft of the other -- `market_fold::fold_coins` stays exactly as tested,
/// for a caller that still has the live rows to hand.
fn coins_from_trades(trades: &[MarketTrade]) -> Vec<market_fold::Coin> {
    let mut by_mint: std::collections::BTreeMap<Address, Vec<&MarketTrade>> =
        std::collections::BTreeMap::new();
    for t in trades {
        by_mint.entry(t.mint).or_default().push(t);
    }

    by_mint
        .into_iter()
        .map(|(mint, mut group)| {
            // Chronological, so "first" and "last" priced fill below mean
            // what they say rather than whatever order the store happened
            // to return.
            group.sort_by(|a, b| {
                a.ts.cmp(&b.ts)
                    .then_with(|| a.signature.as_bytes().cmp(b.signature.as_bytes()))
            });
            let tx_count = u64::try_from(group.len()).unwrap_or(u64::MAX);
            let token_volume: f64 = group.iter().map(|t| t.token_amount).sum();
            let priced: Vec<&&MarketTrade> = group.iter().filter(|t| t.price.is_some()).collect();
            let last_price = priced.last().and_then(|t| t.price);
            let first_price = priced.first().and_then(|t| t.price);
            let quote_mint = priced
                .last()
                .and_then(|t| t.quote_mint)
                .map(|m| m.to_string());
            let quote_volume = has_a_priced_fill(&priced)
                .then(|| priced.iter().filter_map(|t| t.quote_amount).sum::<f64>());
            // `market_fold::change_from`, not a second copy of it. These four
            // lines existed here as well, with their own surviving mutants,
            // because the tests beside the original could not reach a
            // duplicate.
            let change_pct = market_fold::change_from(first_price, last_price);
            market_fold::Coin {
                mint: mint.to_string(),
                tx_count,
                token_volume: Some(token_volume),
                quote_mint,
                quote_volume,
                price: last_price,
                change_pct,
            }
        })
        .collect()
}

/// The live coin list: what has moved recently, ranked and priced.
pub async fn coins(
    State(state): State<Arc<crate::AppState>>,
    Query(params): Query<CoinsParams>,
) -> Response {
    let limit = params
        .limit
        .unwrap_or(DEFAULT_COINS_LIMIT)
        .clamp(1, MAX_COINS_LIMIT);
    let sort = params.sort.clone().unwrap_or_else(|| "activity".to_owned());
    if let Some(feed) = state.market.live() {
        return live::coins(feed, Some(limit), &sort);
    }

    let watermark = match crate::watermark_of(&state) {
        Ok(w) => w,
        Err(e) => return e.into_response(),
    };
    let as_of = AsOf::at(watermark);
    let snapshot = match snapshot_for(&state.market_snapshot, &state.store, as_of) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Degradation::NotCollected(
                "the market snapshot has not been built yet; check back shortly",
            )
            .into_response();
        }
        Err(e) => return Degradation::from_store_error(&e).into_response(),
    };
    if !snapshot.collected() {
        return Degradation::NotCollected(
            "the market-tape collector has not produced anything for this store yet",
        )
        .into_response();
    }

    // The store's own horizon, not wall-clock -- see `newest_ts_of`.
    let Some(to) = snapshot.newest_ts() else {
        return Degradation::NotCollected(NOTHING_COLLECTED).into_response();
    };
    let from = reaching_back(to, COINS_WINDOW_SECONDS);
    let (from_s, to_s) = (from_epoch(from), from_epoch(to));

    let windowed: Vec<MarketTrade> = snapshot
        .trades()
        .iter()
        .filter(|t| within_window(&t.ts, &from_s, &to_s))
        .cloned()
        .collect();

    let mut coins = coins_from_trades(&windowed);
    sort_coins(&mut coins, &sort);
    coins.truncate(limit);

    let coins: Vec<Value> = coins
        .into_iter()
        .map(|coin| with_launch_fields(coin, snapshot.launches()))
        .collect();

    Json(json!({
        "window": { "from": from_s, "to": to_s },
        "coins": coins,
    }))
    .into_response()
}

/// Injects a coin's launch-recorded `name`/`symbol` (and, on this
/// store-backed path only, `uri`) into its serialised row.
///
/// [`market_fold::Coin`] itself gains no fields for this: it is shared with
/// `market/live.rs`'s live-stream path (unmerged, `origin/feat/live-stream`),
/// which folds its own launch-sourced `name`/`symbol` in the same way at the
/// JSON level rather than on the struct, so this keeps that shape rather than
/// inventing a second one.
fn with_launch_fields(coin: market_fold::Coin, launches: &LaunchIndex) -> Value {
    let mint = coin.mint.parse::<Address>().ok();
    let found = mint.and_then(|m| launches.get(&m));
    let mut value = serde_json::to_value(coin).unwrap_or(Value::Null);
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "name".to_owned(),
            found.map_or(Value::Null, |l| Value::String(l.name.clone())),
        );
        obj.insert(
            "symbol".to_owned(),
            found.map_or(Value::Null, |l| Value::String(l.symbol.clone())),
        );
        obj.insert(
            "uri".to_owned(),
            found.map_or(Value::Null, |l| Value::String(l.uri.clone())),
        );
    }
    value
}

/// `/v1/market/token/{mint}`: a coin's header.
///
/// `name`, `symbol` and `uri` come from Radar's own recorder, not from
/// CryptoHouse: every pump.fun launch it decodes is written to
/// [`radar_store::Table::Launches`], and [`build_launch_index`] joins that in
/// by mint, bounded and cached exactly as [`coins`] is. A mint with no
/// recorded launch -- launched before Radar began recording, or outside the
/// index's lookback window -- keeps all three `null` with `metadata_reason`
/// saying which. `creator` and `published_at` are never collected by
/// [`radar_backfill::market_tape`] at all -- it collects the trade tape only
/// -- so those two stay `null` with the reason saying so, rather than a live
/// `solana.tokens` query this build no longer makes. Price still comes from
/// the collected tape, the same as [`trades`].
///
/// # Why collecting it was not simply scheduled
///
/// **`solana.tokens` is abandoned.** Checked 2026-09-12: its newest row is
/// dated **2026-08-10**, over a month earlier, and not one of the ten busiest
/// mints of that moment appeared in it at all. A batched
/// `WHERE mint IN (...)` metadata query would have cost one query a pass and
/// fitted the budget comfortably -- it was measured for exactly that -- and it
/// would have returned nothing for every coin a trading screen is about.
///
/// So a name is not available on the free lane at any price in queries. It
/// needs the Metaplex metadata account read from chain, one RPC call per mint
/// against an endpoint Solana's own documentation says is not for production
/// use, or a paid provider. Until one of those is a decision somebody has
/// made, the list shows the mint and the header says the name is absent --
/// which is true, and is a smaller lie than a column of "unknown" where a name
/// would go.
///
/// Market cap and liquidity are always `null` for the reason they always
/// were: both need either an unbounded transfer scan or a live account-state
/// read, neither of which this module performs.
pub async fn token(
    State(state): State<Arc<crate::AppState>>,
    Path(mint): Path<String>,
) -> Response {
    let mint = match parse_mint(&mint) {
        Ok(m) => m,
        Err(r) => return *r,
    };
    if let Some(feed) = state.market.live() {
        return live::token(feed, mint);
    }
    let watermark = match crate::watermark_of(&state) {
        Ok(w) => w,
        Err(e) => return e.into_response(),
    };
    let as_of = AsOf::at(watermark);
    let snapshot = match snapshot_for(&state.market_snapshot, &state.store, as_of) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Degradation::NotCollected(
                "the market snapshot has not been built yet; check back shortly",
            )
            .into_response();
        }
        Err(e) => return Degradation::from_store_error(&e).into_response(),
    };
    if !snapshot.collected() {
        return Degradation::NotCollected(
            "the market-tape collector has not produced anything for this store yet",
        )
        .into_response();
    }

    // The store's own horizon, not wall-clock -- see `newest_ts_of`.
    let Some(to) = snapshot.newest_ts() else {
        return Degradation::NotCollected(NOTHING_COLLECTED).into_response();
    };
    let from = reaching_back(to, DEFAULT_WINDOW_SECONDS);
    let (from_s, to_s) = (from_epoch(from), from_epoch(to));
    let recent_trades = filter_tape(snapshot.trades(), mint, &from_s, &to_s);

    let priced = recent_trades.iter().find(|t| t.price.is_some());
    let (price, price_reason) = match priced {
        Some(t) => (t.price, None),
        None => (None, Some(why_no_price(recent_trades.is_empty()))),
    };

    let found = snapshot.launches().get(&mint);
    let (name, symbol, uri, metadata_reason) = match found {
        Some(l) => (
            Some(l.name.clone()),
            Some(l.symbol.clone()),
            Some(l.uri.clone()),
            "name, symbol and metadata uri are read from Radar's own recorded pump.fun launch; creator and first-seen date are not -- only those three fields are",
        ),
        None => (
            None,
            None,
            None,
            "no pump.fun launch for this coin was recorded by Radar -- it may have launched before Radar began recording, or fall outside the launch index's lookback window",
        ),
    };

    Json(json!({
        "mint": mint.to_string(),
        "name": name,
        "symbol": symbol,
        "uri": uri,
        "creator": Option::<String>::None,
        "published_at": Option::<String>::None,
        "metadata_reason": metadata_reason,
        "price": price,
        "price_reason": price_reason,
        "market_cap": Option::<f64>::None,
        "market_cap_reason": "supply is not computable without an unbounded transfer scan or a live account read, neither of which this build performs",
        "liquidity": Option::<f64>::None,
        "liquidity_reason": "pool reserves require a live account read, which this build does not perform",
    }))
    .into_response()
}

/// `/v1/market/holders/{mint}` query parameters.
///
/// Only the live feed reads `limit`: without one, [`holders`] refuses before
/// it would read a parameter.
#[derive(Debug, Deserialize)]
pub struct HoldersParams {
    limit: Option<usize>,
}

/// Holder balances, folded from observed transfers over a bounded window.
///
/// **Not collected by [`radar_backfill::market_tape`] today.** The collector
/// gathers a batched trade tape (`mint IN (...)` against a shortlist of
/// active mints); a holders fold needs every raw transfer for one mint over a
/// wide window, which is a different query this budget does not have room
/// for yet. Refusing plainly here is rule 9's shape: an empty holder list
/// would look exactly like a token with no holders, which is a different and
/// much more interesting fact than the true one.
pub async fn holders(
    State(state): State<Arc<crate::AppState>>,
    Path(mint): Path<String>,
    Query(params): Query<HoldersParams>,
) -> Response {
    let mint = match parse_mint(&mint) {
        Ok(m) => m,
        Err(r) => return *r,
    };
    if let Some(feed) = state.market.live() {
        return live::holders(feed, mint, params.limit);
    }
    Degradation::NotCollected(
        "holder-balance collection is out of scope for the market-tape collector; only trade data is collected",
    )
    .into_response()
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

    /// Builds a store holding exactly the trades given, and a `Reader` over it.
    ///
    /// **Every handler in this module is now a pure function of the store**,
    /// which is what makes these tests possible at all. While the routes
    /// queried CryptoHouse, the only way to reach their window arithmetic was
    /// a live request against a rate-limited public endpoint, so none of it was
    /// tested and the mutation shards reported every branch of it as surviving.
    /// A fixture store costs a temporary directory.
    fn store_of(trades: &[MarketTrade]) -> (tempfile::TempDir, Reader) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        {
            let mut writer =
                radar_store::Writer::open(dir.path().to_str().expect("a utf-8 path"), 20_000)
                    .expect("a writer");
            for trade in trades {
                writer
                    .append_market_trade(trade.clone())
                    .expect("a market trade appends");
            }
            writer.flush().expect("the writer flushes");
        }
        let reader = Reader::open(dir.path().to_str().expect("a utf-8 path"));
        (dir, reader)
    }

    fn market_trade(mint: &str, ts: &str, slot: u64) -> MarketTrade {
        MarketTrade {
            mint: mint.parse().expect("a mint"),
            ts: ts.to_owned(),
            slot: radar_types::Slot(slot),
            signature: radar_types::Signature::new([7u8; 64]),
            side: MarketSide::Unknown,
            token_amount: 1.0,
            quote_amount: Some(2.0),
            quote_mint: Some(WSOL.parse().expect("a mint")),
            price: Some(2.0),
            trader: None,
        }
    }

    const WSOL: &str = "So11111111111111111111111111111111111111112";
    const A_MINT: &str = "5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump";

    /// The default windows are the spans their names claim.
    ///
    /// Both are written as products -- `60 * 60` and `10 * 60` -- and a mutant
    /// turning either into a sum or a quotient leaves a plausible-looking small
    /// number: 120 seconds for the chart, 70 for the coin list. Neither errors,
    /// and both quietly show a reader a couple of minutes of market while the
    /// interface says an hour.
    #[test]
    fn the_default_windows_are_the_spans_their_names_claim() {
        assert_eq!(DEFAULT_CANDLE_WINDOW_SECONDS, 3_600, "an hour of chart");
        assert_eq!(COINS_WINDOW_SECONDS, 600, "ten minutes of activity");
        assert_eq!(DEFAULT_WINDOW_SECONDS, 1_800, "half an hour of tape");
        assert_eq!(MAX_CANDLE_WINDOW_SECONDS, 86_400, "a day is the ceiling");
        // That a chart reaches further back than a tape is held at compile
        // time beside the constants themselves -- clippy rightly refuses an
        // assertion whose value is already known.
    }

    /// A range must run forwards, and a zero-width one is a mistake.
    #[test]
    fn a_range_that_does_not_run_forwards_is_refused() {
        assert!(!is_a_backwards_range(100, 200), "a real range");
        assert!(is_a_backwards_range(200, 200), "zero width is not a range");
        assert!(is_a_backwards_range(300, 200), "nor is a backwards one");
    }

    /// The clamp holds a start no further back than the ceiling allows.
    #[test]
    fn a_start_is_clamped_to_the_ceiling_but_never_pushed_forward() {
        // Inside the ceiling: left alone.
        assert_eq!(clamped_start(9_000, 10_000, 86_400), 9_000);
        // Further back than the ceiling: pulled up to it, not past it.
        assert_eq!(clamped_start(0, 100_000, 86_400), 100_000 - 86_400);
        assert!(
            clamped_start(0, 100_000, 86_400) < 100_000,
            "a clamped start still precedes its end"
        );
    }

    /// No priced fill means no volume figure, not a volume of zero.
    #[test]
    fn a_window_with_no_priced_fill_has_no_volume_rather_than_zero() {
        let none: [u8; 0] = [];
        assert!(
            !has_a_priced_fill(&none),
            "nothing priced is nothing to sum"
        );
        assert!(
            has_a_priced_fill(&[1u8]),
            "and one priced fill is something"
        );
    }

    /// An unpriced coin says which kind of unpriced it is.
    ///
    /// "Nothing traded" and "things traded but nothing was priceable" are
    /// different facts about a coin, and a reader acts on them differently.
    #[test]
    fn an_unpriced_coin_says_which_kind_of_unpriced_it_is() {
        let quiet = why_no_price(true);
        let busy = why_no_price(false);
        assert_ne!(quiet, busy, "the two cases must not read alike");
        assert!(quiet.contains("no recorded trades"), "{quiet}");
        assert!(busy.contains("none paired with a quote leg"), "{busy}");
    }

    /// A window includes its start and excludes its end.
    ///
    /// Kills every mutant on the comparison: `>=` relaxed to `<` admits
    /// nothing, `<` relaxed to `<=` counts the boundary row in two windows,
    /// and `&&` loosened to `||` admits every row in the store.
    #[test]
    fn a_window_includes_its_start_and_excludes_its_end() {
        let (from, to) = ("2020-01-01 00:00:10", "2020-01-01 00:00:20");
        assert!(
            within_window("2020-01-01 00:00:10", from, to),
            "the start is in"
        );
        assert!(
            within_window("2020-01-01 00:00:15", from, to),
            "the middle is in"
        );
        assert!(
            !within_window("2020-01-01 00:00:20", from, to),
            "the end belongs to the next window, not this one and that one"
        );
        assert!(
            !within_window("2020-01-01 00:00:09", from, to),
            "before the start is out"
        );
        assert!(
            !within_window("2020-01-01 00:00:21", from, to),
            "after the end is out"
        );
    }

    /// A window reaches back from its end, never forward.
    ///
    /// Kills the mutants turning the subtraction into an addition or a
    /// division. Forward, the window names a span that has not happened, the
    /// store holds nothing in it, and every caller is told the market was
    /// quiet.
    #[test]
    fn a_window_reaches_back_from_its_end() {
        assert_eq!(reaching_back(1_000, 120), 880);
        assert!(
            reaching_back(1_000, 120) < 1_000,
            "a window that starts after it ends is not a window"
        );
        assert_eq!(
            1_000 - reaching_back(1_000, 120),
            120,
            "and it is exactly the span asked for"
        );
    }

    /// Coverage for another table is not coverage for this one.
    ///
    /// `radar-follow` writes coverage for `Launches` and `Graduations` against
    /// the same store, continuously. A check that matched any coverage row at
    /// all would therefore report the market tape as collected on every
    /// established instance -- including one where the market-tape unit was
    /// never installed, which is exactly the deployment this is meant to
    /// catch. Kills the mutant replacing `==` with `!=`.
    #[test]
    fn another_tables_coverage_is_not_the_market_tapes() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        {
            let mut writer =
                radar_store::Writer::open(dir.path().to_str().expect("a path"), 20_000)
                    .expect("a writer");
            writer
                .append_coverage(radar_store::Coverage {
                    recorded_at: radar_types::Slot(100),
                    table: Table::Launches,
                    filter: None,
                    observed: radar_store::ObservedSlots::Nothing,
                    source: "test".to_owned(),
                    decoder_version: "test".to_owned(),
                    status: radar_store::Completion::Complete,
                })
                .expect("coverage appends");
            writer.flush().expect("flush");
        }
        let store = Reader::open(dir.path().to_str().expect("a path"));
        let as_of = AsOf::at(radar_types::Slot(10_000));
        assert!(
            !market_tape_collected(&store, as_of).expect("the store reads"),
            "a Launches coverage row does not mean the market tape ran"
        );
    }

    /// The store's own newest moment, not the wall clock.
    ///
    /// **This is the defect that made every route answer empty.** The
    /// collector runs behind wall-clock by design, so a window ending at
    /// `now()` named a span it had not reached, and every handler returned an
    /// empty list marked complete — a confident claim that nothing traded.
    /// Observed against a store holding 900 trades.
    #[test]
    fn the_default_window_ends_where_the_store_reaches_not_at_the_clock() {
        let (_dir, store) = store_of(&[
            market_trade(A_MINT, "2020-01-01 00:00:01.000000", 100),
            market_trade(A_MINT, "2020-01-01 00:00:59.000000", 200),
        ]);
        let as_of = AsOf::at(radar_types::Slot(10_000));

        let newest = newest_collected(&store, as_of).expect("the store reads");
        let expected = radar_store::to_epoch("2020-01-01 00:00:59").expect("a stamp");
        assert_eq!(
            newest,
            Some(expected),
            "the horizon is the newest row, whatever year the clock says"
        );
    }

    /// Microseconds are trimmed toward the second, never rounded up.
    ///
    /// The window's upper bound is exclusive, so a horizon rounded *up* past
    /// the newest row would claim to cover a moment nothing was read at.
    #[test]
    fn the_horizon_truncates_toward_the_second_rather_than_past_it() {
        let (_dir, store) = store_of(&[market_trade(A_MINT, "2020-01-01 00:00:59.999999", 200)]);
        let as_of = AsOf::at(radar_types::Slot(10_000));
        let newest = newest_collected(&store, as_of).expect("the store reads");
        assert_eq!(
            newest,
            radar_store::to_epoch("2020-01-01 00:00:59").ok(),
            "59.999999 is not 60"
        );
    }

    /// An empty store is not a quiet market.
    #[test]
    fn a_store_with_no_trades_has_no_horizon_rather_than_a_zero_one() {
        let (_dir, store) = store_of(&[]);
        let as_of = AsOf::at(radar_types::Slot(10_000));
        assert_eq!(
            newest_collected(&store, as_of).expect("the store reads"),
            None,
            "no rows means no horizon, never the epoch"
        );
    }

    /// Nothing past the watermark is visible, including to the horizon.
    ///
    /// AGENTS §4 rule 3. A horizon read past `as_of` would let a replay see a
    /// later moment than the replay is for, and every window derived from it
    /// would inherit that.
    #[test]
    fn the_horizon_never_reaches_past_the_watermark() {
        let (_dir, store) = store_of(&[
            market_trade(A_MINT, "2020-01-01 00:00:01.000000", 100),
            market_trade(A_MINT, "2020-01-01 00:09:00.000000", 900),
        ]);
        let as_of = AsOf::at(radar_types::Slot(500));
        assert_eq!(
            newest_collected(&store, as_of).expect("the store reads"),
            radar_store::to_epoch("2020-01-01 00:00:01").ok(),
            "the row at slot 900 is past the watermark and must not be seen"
        );
    }

    /// The tape is one mint's, and it is newest first.
    #[test]
    fn the_tape_is_one_mints_own_trades_newest_first() {
        let other = "So11111111111111111111111111111111111111112";
        let (_dir, store) = store_of(&[
            market_trade(A_MINT, "2020-01-01 00:00:01.000000", 100),
            market_trade(other, "2020-01-01 00:00:02.000000", 110),
            market_trade(A_MINT, "2020-01-01 00:00:03.000000", 120),
        ]);
        let as_of = AsOf::at(radar_types::Slot(10_000));
        let tape = tape_for(
            &store,
            as_of,
            A_MINT.parse().expect("a mint"),
            "2020-01-01 00:00:00",
            "2020-01-01 00:01:00",
        )
        .expect("the store reads");
        assert_eq!(tape.len(), 2, "the other mint's trade is not this tape's");
        assert_eq!(tape[0].ts, "2020-01-01 00:00:03.000000", "newest first");
    }

    /// A window that excludes every row returns nothing, without error.
    ///
    /// The bound is half-open — `>= from`, `< to` — so a row exactly at `to`
    /// belongs to the next window, not this one. Pinned because the horizon is
    /// derived from the newest row and an inclusive upper bound would make
    /// every default window double-count its own edge.
    #[test]
    fn the_window_bound_is_half_open_at_the_top() {
        // The stamp is compared as a **string**, so it must equal the bound
        // exactly to sit on the boundary at all. An earlier version of this
        // test used `...00:00:30.000000` against a bound of `...00:00:30`:
        // the longer string sorts after the shorter one under `<` and `<=`
        // alike, so it passed against both and the mutant survived a test
        // written to kill it.
        let (_dir, store) = store_of(&[market_trade(A_MINT, "2020-01-01 00:00:30", 100)]);
        let as_of = AsOf::at(radar_types::Slot(10_000));
        let mint: Address = A_MINT.parse().expect("a mint");

        let excluded = tape_for(
            &store,
            as_of,
            mint,
            "2020-01-01 00:00:00",
            "2020-01-01 00:00:30",
        )
        .expect("the store reads");
        assert!(excluded.is_empty(), "a row at `to` is the next window's");

        let included = tape_for(
            &store,
            as_of,
            mint,
            "2020-01-01 00:00:30",
            "2020-01-01 00:00:31",
        )
        .expect("the store reads");
        assert_eq!(included.len(), 1, "a row at `from` is this window's");
    }

    fn coin(
        tx_count: u64,
        quote_volume: Option<f64>,
        change_pct: Option<f64>,
    ) -> market_fold::Coin {
        market_fold::Coin {
            mint: format!("MINT{tx_count}"),
            tx_count,
            token_volume: None,
            quote_mint: None,
            quote_volume,
            price: None,
            change_pct,
        }
    }

    /// Each named sort orders by its own column, and an unknown one falls back
    /// to activity rather than leaving the page as it arrived.
    ///
    /// Kills the mutants that delete the `"volume"` and `"change"` arms: with
    /// either gone the request still answers, the column header still says what
    /// it sorted by, and the rows are simply in a different order -- a screen
    /// lying about its own controls, with nothing failing.
    #[test]
    fn each_sort_orders_by_its_own_column_and_an_unknown_one_falls_back() {
        // The three orderings are deliberately all different. An earlier
        // version of this test had the change ordering coincide with the
        // activity ordering, so deleting the "change" arm -- which falls
        // through to activity -- changed nothing it asserted, and the mutant
        // survived a test written to kill it.
        let sample = || {
            vec![
                coin(5, Some(1.0), Some(50.0)),
                coin(1, Some(9.0), Some(10.0)),
                coin(9, Some(4.0), Some(-5.0)),
            ]
        };

        let mut by_volume = sample();
        sort_coins(&mut by_volume, "volume");
        assert_eq!(
            by_volume.iter().map(|c| c.tx_count).collect::<Vec<_>>(),
            vec![1, 9, 5],
            "volume order is 9.0, 4.0, 1.0"
        );

        let mut by_change = sample();
        sort_coins(&mut by_change, "change");
        assert_eq!(
            by_change.iter().map(|c| c.tx_count).collect::<Vec<_>>(),
            vec![5, 1, 9],
            "change order is 50%, 10%, -5% -- and not the activity order"
        );

        let mut by_activity = sample();
        sort_coins(&mut by_activity, "nonsense");
        assert_eq!(
            by_activity.iter().map(|c| c.tx_count).collect::<Vec<_>>(),
            vec![9, 5, 1],
            "an unrecognised sort is activity, not whatever order arrived"
        );
    }

    /// An unmeasured figure sorts last in a descending column, never first.
    #[test]
    fn a_coin_with_no_volume_sorts_below_every_coin_that_has_one() {
        let mut coins = vec![coin(1, None, None), coin(2, Some(0.5), None)];
        sort_coins(&mut coins, "volume");
        assert_eq!(
            coins[0].tx_count, 2,
            "a priced coin outranks an unpriced one"
        );
    }

    /// Every degradation says something substantive, and no two say the same
    /// thing.
    ///
    /// Kills the mutants that replace `message` with `""` or a constant: an
    /// operator acts differently on "the store could not be read" than on
    /// "this range was never collected", and a blank or identical message
    /// takes that distinction away at exactly the moment it is needed. The
    /// live-query-era variants this pinned (`Unreachable`, `Malformed`,
    /// `TimedOut`, `RowCapHit`) are gone with the queries they classified --
    /// see the module doc comment -- and this is their replacement pinning
    /// the two that took their place.
    #[test]
    fn every_degradation_carries_its_own_non_empty_message_and_code() {
        let all = [
            Degradation::StoreUnreadable("boom".to_owned()),
            Degradation::NotCollected("fixture"),
        ];
        let mut messages: Vec<String> = all.iter().map(Degradation::message).collect();
        let mut codes: Vec<&str> = all.iter().map(Degradation::code).collect();
        assert!(
            messages.iter().all(|m| m.len() > 20),
            "a message has to explain, not label"
        );
        assert!(codes.iter().all(|c| !c.is_empty()));
        messages.sort_unstable();
        messages.dedup();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(messages.len(), all.len(), "no two degradations read alike");
        assert_eq!(codes.len(), all.len(), "no two degradations share a code");
    }

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
    fn degradation_reasons_are_distinguishable_and_never_look_like_success() {
        let store_error = Degradation::StoreUnreadable("boom".to_owned());
        let not_collected = Degradation::NotCollected("never asked");
        assert!(store_error.status().is_server_error());
        assert!(not_collected.status().is_server_error());
        assert_ne!(store_error.status(), StatusCode::OK);
        assert_ne!(not_collected.status(), StatusCode::OK);
        assert_ne!(store_error.code(), not_collected.code());
    }

    #[test]
    fn not_collected_and_collected_and_quiet_are_different_sentences() {
        // The rule 9 property this module exists to hold: an empty result
        // and "we never looked" must never share a code or a status.
        let not_collected = Degradation::NotCollected("fixture");
        assert_eq!(not_collected.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_ne!(not_collected.status(), StatusCode::OK);
        assert!(not_collected.message().contains("not collected"));
    }

    #[test]
    fn a_malformed_mint_is_refused_before_any_store_read() {
        assert!(parse_mint("not-base58!!").is_err());
        assert!(parse_mint("").is_err());
        assert!(parse_mint("5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump").is_ok());
    }

    #[test]
    fn a_malformed_timestamp_is_refused_rather_than_silently_defaulted() {
        assert!(parse_stamp("not a timestamp").is_err());
        assert!(parse_stamp("2026-09-11 17:00:00").is_ok());
    }

    fn mint(n: u8) -> Address {
        Address::new([n; 32])
    }

    fn trade(
        mint: Address,
        ts: &str,
        sig: u8,
        price: Option<f64>,
        quote: Option<f64>,
    ) -> MarketTrade {
        MarketTrade {
            mint,
            ts: ts.to_owned(),
            slot: radar_types::Slot(1),
            signature: radar_types::Signature::new([sig; 64]),
            side: if price.is_some() {
                MarketSide::Buy
            } else {
                MarketSide::Unknown
            },
            token_amount: 1.0,
            quote_amount: quote,
            quote_mint: quote.map(|_| {
                "So11111111111111111111111111111111111111112"
                    .parse()
                    .expect("quote mint")
            }),
            price,
            trader: None,
        }
    }

    #[test]
    fn coins_from_trades_folds_one_row_per_mint() {
        let rows = vec![
            trade(
                mint(1),
                "2026-09-11 17:00:00.000000",
                1,
                Some(1.0),
                Some(1.0),
            ),
            trade(
                mint(1),
                "2026-09-11 17:01:00.000000",
                2,
                Some(1.5),
                Some(1.5),
            ),
            trade(mint(2), "2026-09-11 17:00:00.000000", 3, None, None),
        ];
        let coins = coins_from_trades(&rows);
        assert_eq!(coins.len(), 2, "one row per mint, not one per trade");

        let a = coins
            .iter()
            .find(|c| c.mint == mint(1).to_string())
            .expect("mint 1");
        assert_eq!(a.tx_count, 2);
        assert_eq!(a.price, Some(1.5), "the last priced fill");
        assert!(
            (a.change_pct.unwrap() - 50.0).abs() < 1e-9,
            "50% from the first priced fill to the last"
        );

        let b = coins
            .iter()
            .find(|c| c.mint == mint(2).to_string())
            .expect("mint 2");
        assert_eq!(b.price, None, "never priced in the window");
        assert_eq!(b.change_pct, None);
    }

    #[test]
    fn a_mint_with_no_priced_trade_reports_no_price_not_a_dropped_row() {
        let rows = vec![trade(mint(1), "2026-09-11 17:00:00.000000", 1, None, None)];
        let coins = coins_from_trades(&rows);
        assert_eq!(coins.len(), 1, "the mint is still on the list");
        assert_eq!(coins[0].price, None);
        assert_eq!(coins[0].quote_mint, None);
    }

    /// A refresher tick reads only what it has to.
    ///
    /// Every row of the table, because each wrong answer is quiet: a `Skip`
    /// that should have been a read freezes the screen on an old snapshot, and
    /// a read that should have been a `Skip` is the all-day store scan this
    /// snapshot exists to stop.
    #[test]
    fn a_refresher_tick_reads_only_what_it_has_to() {
        assert_eq!(refresh_plan(true, false, true), Refresh::Everything);
        assert_eq!(refresh_plan(true, true, false), Refresh::Everything);
        assert_eq!(refresh_plan(false, false, true), Refresh::Skip);
        assert_eq!(refresh_plan(false, false, false), Refresh::Skip);
        assert_eq!(refresh_plan(false, true, true), Refresh::TradesOnly);
        assert_eq!(
            refresh_plan(false, true, false),
            Refresh::Everything,
            "there is no launch index to reuse, so a trades-only read has nothing to stand on"
        );
    }

    #[test]
    fn the_launch_index_is_due_when_never_built_or_old_enough() {
        let every = std::time::Duration::from_secs(300);
        assert!(launches_due(None, every), "never built");
        assert!(!launches_due(
            Some(std::time::Duration::from_secs(299)),
            every
        ));
        assert!(
            launches_due(Some(every), every),
            "due on the interval itself"
        );
        assert!(launches_due(
            Some(std::time::Duration::from_secs(301)),
            every
        ));
    }

    /// With a background refresher, a request never builds a snapshot.
    ///
    /// A build would leave its snapshot in the cache, so an empty cache after
    /// the request is the proof that the request path did not read the store.
    #[test]
    fn a_request_never_builds_a_snapshot_a_refresher_owns() {
        let (_dir, store) = store_of(&[market_trade(A_MINT, "2020-01-01 00:00:01.000000", 100)]);
        let cache = SnapshotCache::with_background_refresh();
        let as_of = AsOf::at(radar_types::Slot(10_000));
        assert!(
            snapshot_for(&cache, &store, as_of)
                .expect("no read happens")
                .is_none(),
            "nothing built yet is an honest nothing, not a blocking build"
        );
        assert!(cache.peek().is_none(), "and the request did not fill it in");

        // Once the refresher has set one, a later watermark is still served
        // the same snapshot rather than a rebuild.
        let built = Arc::new(Snapshot::build(&store, as_of).expect("the store reads"));
        cache.set(Arc::clone(&built));
        let later = AsOf::at(radar_types::Slot(10_500));
        let served = snapshot_for(&cache, &store, later)
            .expect("no read happens")
            .expect("a snapshot exists");
        assert!(Arc::ptr_eq(&served, &built), "served, never rebuilt");
    }

    /// Without a refresher the snapshot is reused at its own watermark and
    /// rebuilt at any other.
    #[test]
    fn without_a_refresher_a_snapshot_is_reused_only_at_its_own_watermark() {
        let (_dir, store) = store_of(&[market_trade(A_MINT, "2020-01-01 00:00:01.000000", 100)]);
        let cache = SnapshotCache::new();
        assert!(!cache.has_background_refresh());
        let as_of = AsOf::at(radar_types::Slot(10_000));
        let first = snapshot_for(&cache, &store, as_of)
            .expect("the store reads")
            .expect("built on a miss");
        assert_eq!(first.watermark(), radar_types::Slot(10_000));
        let again = snapshot_for(&cache, &store, as_of)
            .expect("the store reads")
            .expect("a snapshot");
        assert!(Arc::ptr_eq(&first, &again), "same watermark, same snapshot");

        let later = AsOf::at(radar_types::Slot(10_500));
        let rebuilt = snapshot_for(&cache, &store, later)
            .expect("the store reads")
            .expect("a snapshot");
        assert!(!Arc::ptr_eq(&first, &rebuilt));
        assert_eq!(rebuilt.watermark(), radar_types::Slot(10_500));
    }

    /// The snapshot holds the last day of trades and nothing older.
    #[test]
    fn a_snapshot_holds_the_window_behind_its_watermark_and_nothing_older() {
        assert_eq!(SNAPSHOT_WINDOW_SLOTS, 220_000);
        let (_dir, store) = store_of(&[
            market_trade(A_MINT, "2020-01-01 00:00:01.000000", 100),
            market_trade(A_MINT, "2020-01-03 00:00:01.000000", 400_000),
        ]);
        let as_of = AsOf::at(radar_types::Slot(400_100));
        let snap = Snapshot::build(&store, as_of).expect("the store reads");
        let slots: Vec<u64> = snap.trades().iter().map(|t| t.slot.get()).collect();
        assert_eq!(
            slots,
            vec![400_000],
            "the two-day-old row is outside the window"
        );
        assert_eq!(
            snap.newest_ts(),
            radar_store::to_epoch("2020-01-03 00:00:01").ok()
        );
        assert!(!snap.collected(), "no coverage row was written");

        let refreshed = snap.refresh_trades(&store, as_of).expect("the store reads");
        assert_eq!(refreshed.watermark(), radar_types::Slot(400_100));
        assert_eq!(refreshed.trades().len(), 1);
        assert_eq!(refreshed.newest_ts(), snap.newest_ts());
        assert!(!refreshed.collected());
    }
}
