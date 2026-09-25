// SPDX-License-Identifier: Apache-2.0
//! `GET /v1/market/quote` and `POST /v1/customer/swap`: pricing and building a
//! Jupiter-routed Solana swap. Plan 0013 Phase D, [ADR
//! 0024](../../../docs/adr/0024-radar-builds-a-visitors-swap-and-only-their-wallet-signs-it.md).
//!
//! **This module still does not execute anything.** [`crate::AppState::trading`]
//! calls [`radar_exec::route::Router`], which prices and — for the customer
//! route — compiles an unsigned transaction. Nothing here signs or submits: the
//! wallet does both, client-side, with bytes it never had to trust Radar to
//! keep honest, because it can decode and check them itself before signing.
//!
//! # Wire contract (coordinator-supplied, authoritative)
//!
//! Amounts are JSON decimal strings everywhere: a `u64` base-unit amount can
//! exceed 2^53 and a JSON number would silently round it.
//!
//! `GET /v1/market/quote?mint=<base58>&side=buy|sell&amount=<string>&slippage_bps=<optional int>`
//!
//! 200 body:
//! ```text
//! {
//!   "mint": "<base58>", "side": "buy"|"sell",
//!   "in_mint": "<base58>", "out_mint": "<base58>",
//!   "in_amount": "<string>", "out_amount": "<string>", "worst_out": "<string>",
//!   "in_decimals": <int|null>, "out_decimals": <int|null>,
//!   "slippage_bps": <int>, "impact_bps": <int|null>,
//!   "venues": ["<label>", ...],
//!   "quoted_at": <unix seconds int>
//! }
//! ```
//! Decimals are read via [`Trading::decimals_of`] and cached forever — a
//! mint's decimals do not change once the account exists. SOL and wrapped SOL
//! are hardcoded to 9 with no network call. `null` when the mint account could
//! not be read, rather than failing the quote over a fact this route does not
//! need to answer the price. `worst_out` is always present: Jupiter's own floor
//! (`otherAmountThreshold`) when it gave one, else computed here from
//! `out_amount` and `slippage_bps` — never omitted, never `null`.
//!
//! `POST /v1/customer/swap` (behind [`crate::tenant::Tenant`]), body
//! `{"mint","side","amount":"<string>","slippage_bps"?:int}`
//!
//! 200 body:
//! ```text
//! {
//!   "transaction": "<base64 unsigned v0 tx, fee payer = session wallet>",
//!   "last_valid_block_height": <int>,
//!   "quote": { ...exactly the quote object above... }
//! }
//! ```
//!
//! Both responses carry `Cache-Control: no-store` — one is public but time-
//! sensitive, the other is a wallet-specific transaction, and neither belongs
//! in a shared cache or on a browser's disk. Refusals use the existing
//! `{"error","reason"}` shape (see [`crate::tenant::refusal`]). `/health`
//! gains `"trading": true|false`.
//!
//! # `RADAR_TRADE`
//!
//! Off is the shipped state. AGENTS rule 8: an operator who wants Radar to
//! build swaps says so in as many words, in a variable named for what it does,
//! rather than trading turning on because a Jupiter key happened to be set for
//! some other reason. `RADAR_TRADE=on` with no [`radar_exec::route::Credentials`]
//! refuses to **start** rather than answering `trading_off` for a reason that
//! is actually a misconfiguration — see [`from_vars`].
//!
//! # Four limits, for four different things
//!
//! Mirrors [`crate::positions`]'s reasoning exactly, with more tiers because
//! this module has two routes against one upstream, and neither route may
//! starve the other of it. The **quote-route global** cap (30 Jupiter calls a
//! minute) exists because a Jupiter API key has a real rate limit that every
//! caller of the *pricing* route draws from together. The **per-visitor** cap
//! on that same route (6 calls a minute, keyed on `CF-Connecting-IP`) exists
//! because that route has no identity to charge — anyone can ask it, so the
//! thing rationed is the best guess at "anyone" Radar has. The header is
//! Cloudflare's own and is not authenticated: a caller reaching this route
//! directly (bypassing Cloudflare) can set it to whatever it likes. That is a
//! ceiling on how *precisely* one abusive visitor can be isolated from another
//! sharing the same guess, never a ceiling on the **quote-route global** cap
//! itself, which does not trust the header at all. When the header is absent,
//! the connection's own peer address is used instead — worse at telling two
//! visitors behind one NAT apart, but not spoofable by the request itself.
//!
//! The **swap-route global** cap and the **per-wallet** cap on that route (6
//! calls a minute each) exist because a signed-in wallet *is* an identity, and
//! is charged against it rather than against a visitor key — but the *global*
//! half of that pair is its own separate budget from the quote route's,
//! deliberately not drawn from the same pool. Public quote traffic is the
//! larger and more elastic of the two (anyone can ask, at no cost) and a burst
//! of it filling a shared global bucket would leave a signed-in wallet's swap
//! build refused for a reason that has nothing to do with that wallet or with
//! building transactions at all. See [`MAX_JUPITER_BUILD_CALLS_PER_MINUTE`]
//! for why its number is smaller than the quote route's.
//!
//! All four checks run under the same lock-ordering discipline
//! [`crate::positions`] documents: the relevant global counter first, then the
//! per-key map, both checked before either is charged, and a refused caller is
//! never inserted.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, Extension, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use radar_exec::route::{QuoteRequest, RouteError, Router};
use radar_onchain::budget::Budget;
use radar_onchain::rpc::RpcClient;
use radar_types::{Address, Asset};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::AppState;
use crate::tenant::{Tenant, refusal};

/// The environment variable that turns swap-building on.
///
/// Off unless this is exactly `"on"`. Not a generic truthy parse
/// (`"1"`/`"true"`/...): one accepted spelling means a startup log and an
/// operator's memory of what they wrote agree, and there is nothing here to
/// gain from a looser one that "off" and `access::Mode` do not already argue
/// against.
pub const VAR: &str = "RADAR_TRADE";

/// Slippage used when a caller does not name one.
///
/// Tight, for the reason [`Router`]'s own default is: a wide setting is what
/// makes a swap worth sandwiching.
pub const DEFAULT_SLIPPAGE_BPS: u32 = 100;

/// The most slippage this instance will ever ask Jupiter to tolerate,
/// regardless of what a caller asks for.
///
/// A cap, never a clamp: a caller asking for more is refused
/// (`slippage_too_wide`) so they can decide whether to proceed at the cap,
/// rather than being silently charged a tolerance they did not agree to.
pub const MAX_SLIPPAGE_BPS: u32 = 500;

/// The most calls this instance will make to Jupiter for the public quote
/// route, across every visitor, in one rolling minute.
///
/// Its own budget, separate from [`MAX_JUPITER_BUILD_CALLS_PER_MINUTE`] — see
/// the module doc comment's "Four limits" section for why the two routes do
/// not share one pool.
const MAX_JUPITER_CALLS_PER_MINUTE: u32 = 30;

/// The most calls one visitor key may draw from the public quote route in one
/// rolling minute, regardless of room left in that route's own global budget.
const MAX_CALLS_PER_VISITOR_PER_MINUTE: u32 = 6;

/// The most calls this instance will make to Jupiter for the customer swap
/// route, across every wallet, in one rolling minute.
///
/// A smaller, separate pool from the quote route's 30: a swap build is a
/// signed-in wallet about to sign and submit a transaction, not a visitor
/// idly re-pricing, so far fewer of them happen at once in practice, and this
/// cap only needs enough headroom for a handful of wallets to each spend their
/// own [`MAX_CALLS_PER_WALLET_PER_MINUTE`] share concurrently without the
/// route as a whole ever asking Jupiter for more than this in a minute. Ten
/// is a little under two wallets' worth of simultaneous full-tilt building —
/// generous for this route's actual traffic shape, while still being a real
/// ceiling separate from the quote route's, so a quiet swap route is never at
/// the mercy of a busy quote one.
const MAX_JUPITER_BUILD_CALLS_PER_MINUTE: u32 = 10;

/// The most calls one wallet may draw from the customer swap route in one
/// rolling minute, regardless of room left in that route's own global budget.
const MAX_CALLS_PER_WALLET_PER_MINUTE: u32 = 6;

/// One quote or one build: exactly one Jupiter call each (see
/// `radar_exec::route::Router`'s own doc comments).
const CALLS_PER_REQUEST: u32 = 1;

/// The byte offset of `decimals` in an SPL Mint account's raw layout.
///
/// `COption<Pubkey> mintAuthority` (4 + 32) then `u64 supply` (8) then this
/// byte. Token-2022 mints share this same 82-byte base layout before their
/// extension TLV data, so the offset is safe for both programs.
const MINT_DECIMALS_OFFSET: usize = 44;

/// Server-side Jupiter routing: pricing (public) and building (customer), and
/// the state the three rate limits above need to hold.
pub struct Trading {
    router: Router,
    rpc: RpcClient,
    /// Cached decimals for a mint that has been read before. Never
    /// invalidated — a mint's decimals cannot change once the account exists
    /// — and never populated for SOL or wrapped SOL, which are hardcoded and
    /// never reach this cache.
    decimals: Mutex<HashMap<Address, u8>>,
    /// The quote route's own global budget. Never shared with
    /// [`Self::build_calls`] — see the module doc comment.
    quote_calls: Mutex<VecDeque<Instant>>,
    visitor_calls: Mutex<HashMap<String, VecDeque<Instant>>>,
    /// The swap route's own global budget. Never shared with
    /// [`Self::quote_calls`] — see the module doc comment.
    build_calls: Mutex<VecDeque<Instant>>,
    wallet_calls: Mutex<HashMap<Address, VecDeque<Instant>>>,
}

impl Trading {
    /// Routes through `router`, reads decimals through `rpc`.
    #[must_use]
    pub fn new(router: Router, rpc: RpcClient) -> Self {
        Self {
            router,
            rpc,
            decimals: Mutex::new(HashMap::new()),
            quote_calls: Mutex::new(VecDeque::new()),
            visitor_calls: Mutex::new(HashMap::new()),
            build_calls: Mutex::new(VecDeque::new()),
            wallet_calls: Mutex::new(HashMap::new()),
        }
    }

    /// The mint's decimals, hardcoded for SOL/wrapped SOL, cached thereafter
    /// for anything else, `None` if the account cannot be read.
    ///
    /// Never fails the quote: the wire contract asks for `null` rather than a
    /// refusal when a mint's decimals are unreadable, because a caller pricing
    /// a swap has already gotten the number that matters.
    fn decimals_of(&self, asset: Asset) -> Option<u8> {
        if matches!(asset, Asset::Sol | Asset::WrappedSol) {
            return Some(9);
        }
        let mint = asset.mint()?;
        if let Some(known) = self
            .decimals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&mint)
        {
            return Some(*known);
        }
        let mut budget = Budget::new(1, 1, Duration::from_secs(10));
        let account = self.rpc.account(&mut budget, &mint).ok().flatten()?;
        let decimals = *account.data.get(MINT_DECIMALS_OFFSET)?;
        self.decimals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(mint, decimals);
        Some(decimals)
    }

    /// Reserves `want` Jupiter calls against `visitor`'s own share and the
    /// global budget. See the module doc comment for the lock-ordering rule.
    fn reserve_visitor(&self, visitor: &str, want: u32) -> bool {
        let mut global = self
            .quote_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut visitors = self
            .visitor_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Instant::now();
        let window = Duration::from_secs(60);
        prune(&mut global, now, window);
        visitors.retain(|_, calls| {
            prune(calls, now, window);
            !calls.is_empty()
        });
        let none = VecDeque::new();
        let mine = visitors.get(visitor).unwrap_or(&none);
        if !fits(&global, want, MAX_JUPITER_CALLS_PER_MINUTE)
            || !fits(mine, want, MAX_CALLS_PER_VISITOR_PER_MINUTE)
        {
            return false;
        }
        let entry = visitors.entry(visitor.to_owned()).or_default();
        for _ in 0..want {
            global.push_back(now);
            entry.push_back(now);
        }
        true
    }

    /// Reserves `want` Jupiter calls against `wallet`'s own share and the
    /// swap route's own global budget ([`Self::build_calls`]) — deliberately
    /// *not* [`Self::quote_calls`], which [`Self::reserve_visitor`] alone
    /// draws on. See the module doc comment's "Four limits" section for why
    /// the two routes do not share one pool.
    fn reserve_wallet(&self, wallet: Address, want: u32) -> bool {
        let mut global = self
            .build_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut wallets = self
            .wallet_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Instant::now();
        let window = Duration::from_secs(60);
        prune(&mut global, now, window);
        wallets.retain(|_, calls| {
            prune(calls, now, window);
            !calls.is_empty()
        });
        let none = VecDeque::new();
        let mine = wallets.get(&wallet).unwrap_or(&none);
        if !fits(&global, want, MAX_JUPITER_BUILD_CALLS_PER_MINUTE)
            || !fits(mine, want, MAX_CALLS_PER_WALLET_PER_MINUTE)
        {
            return false;
        }
        let entry = wallets.entry(wallet).or_default();
        for _ in 0..want {
            global.push_back(now);
            entry.push_back(now);
        }
        true
    }
}

/// Drops reservations older than `window` from the front of `calls`.
fn prune(calls: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while calls
        .front()
        .is_some_and(|t| now.duration_since(*t) >= window)
    {
        calls.pop_front();
    }
}

/// Whether `want` more reservations still fit under `cap`.
fn fits(calls: &VecDeque<Instant>, want: u32, cap: u32) -> bool {
    let used = u32::try_from(calls.len()).unwrap_or(u32::MAX);
    used.saturating_add(want) <= cap
}

/// `RADAR_TRADE`'s parsed state: whatever [`Trading`] needs, or `None`.
///
/// # Errors
///
/// `RADAR_TRADE=on` with no Jupiter credential is refused rather than started
/// half-configured — the same shape as `access::Mode::from_vars` — because a
/// switch that reads "on" and answers `trading_off` for a config reason is a
/// harder fault to find than a process that never started.
pub fn from_vars(
    get: &impl Fn(&str) -> Option<String>,
    rpc: RpcClient,
) -> Result<Option<Trading>, String> {
    if get(VAR).as_deref() != Some("on") {
        return Ok(None);
    }
    let Some(credentials) = radar_exec::route::Credentials::from_vars(get) else {
        return Err(format!(
            "{VAR}=on but no {}: a trading switch with nothing to price against is a \
             feature that looks on and fails every request",
            radar_exec::route::API_KEY_VAR
        ));
    };
    Ok(Some(Trading::new(Router::new(credentials), rpc)))
}

/// One asset each way, for `side="buy"|"sell"`, or the refusal to send instead.
///
/// `Side::Buy` spends SOL to acquire `mint`; `Side::Sell` spends `mint` to
/// receive SOL — the convention `radar_store::event`'s `Side` already uses
/// everywhere else in this codebase, kept rather than invented fresh here.
// `Response` is a full HTTP response, not a lean error code -- these two
// helpers return one directly (the refusal to send) rather than a small enum,
// because the caller's only two moves are "use the value" or "send the
// refusal verbatim"; there is no intermediate error type worth inventing.
#[allow(clippy::result_large_err)]
fn sides_for(mint: Address, side: &str) -> Result<(Asset, Asset), Response> {
    let other = Asset::spl(mint);
    if other == Asset::WrappedSol {
        return Err(refusal(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "mint is the wrapped-SOL mint; SOL cannot be swapped against itself",
        ));
    }
    match side {
        "buy" => Ok((Asset::WrappedSol, other)),
        "sell" => Ok((other, Asset::WrappedSol)),
        _ => Err(refusal(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "side must be \"buy\" or \"sell\"",
        )),
    }
}

/// Parses and bounds-checks the four fields both routes accept, or the
/// refusal to send instead of pricing anything.
#[allow(clippy::result_large_err)]
fn parse_request(
    mint: &str,
    side: &str,
    amount: &str,
    slippage_bps: Option<u32>,
) -> Result<(Address, Asset, Asset, u64, u32), Response> {
    let mint: Address = mint.parse().map_err(|_| {
        refusal(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "mint is not a valid base58 Solana address",
        )
    })?;
    let (input, output) = sides_for(mint, side)?;
    let amount: u64 = amount.parse().map_err(|_| {
        refusal(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "amount is not a base-10 integer string",
        )
    })?;
    if amount == 0 {
        return Err(refusal(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "amount must be greater than zero",
        ));
    }
    let slippage_bps = slippage_bps.unwrap_or(DEFAULT_SLIPPAGE_BPS);
    if slippage_bps > MAX_SLIPPAGE_BPS {
        return Err(refusal(
            StatusCode::BAD_REQUEST,
            "slippage_too_wide",
            &format!(
                "slippage_bps must be at most {MAX_SLIPPAGE_BPS}; Radar never widens a \
                 request's tolerance to fit, it refuses"
            ),
        ));
    }
    Ok((mint, input, output, amount, slippage_bps))
}

/// The floor Jupiter would enforce at `slippage_bps`, computed the same way a
/// caller reading only `out_amount` and `slippage_bps` would: never through a
/// float, so it agrees exactly with integer arithmetic done anywhere else.
fn worst_out_floor(out_amount: u64, slippage_bps: u32) -> u64 {
    let shortfall = u128::from(out_amount) * u128::from(slippage_bps) / 10_000;
    u64::try_from(u128::from(out_amount) - shortfall).unwrap_or(0)
}

/// `impact_bps`'s wire form: [`radar_exec::route::Quote::impact_bps`] uses
/// `u32::MAX` for "absent or unreadable" (AGENTS rule 9: never `0`), and the
/// wire contract's answer to the same fact is `null`.
fn impact_bps_json(impact_bps: u32) -> Value {
    if impact_bps == u32::MAX {
        Value::Null
    } else {
        json!(impact_bps)
    }
}

/// Builds the quote object the wire contract specifies, shared by both routes.
///
/// `in_decimals`/`out_decimals` are resolved by the caller -- inside the same
/// `spawn_blocking` task that already reads the route from Jupiter, since
/// [`Trading::decimals_of`] can itself block on an RPC read -- rather than
/// looked up here on the async runtime's own thread.
#[allow(clippy::too_many_arguments)]
fn render_quote(
    mint: Address,
    side: &str,
    input: Asset,
    output: Asset,
    quote: &radar_exec::route::Quote,
    slippage_bps: u32,
    in_decimals: Option<u8>,
    out_decimals: Option<u8>,
) -> Value {
    let worst_out = quote
        .worst_out
        .unwrap_or_else(|| worst_out_floor(quote.out_amount, slippage_bps));
    json!({
        "mint": mint.to_string(),
        "side": side,
        "in_mint": QuoteRequest::wire_mint(input).to_string(),
        "out_mint": QuoteRequest::wire_mint(output).to_string(),
        "in_amount": quote.in_amount.to_string(),
        "out_amount": quote.out_amount.to_string(),
        "worst_out": worst_out.to_string(),
        "in_decimals": in_decimals,
        "out_decimals": out_decimals,
        "slippage_bps": slippage_bps,
        "impact_bps": impact_bps_json(quote.impact_bps),
        "venues": quote.venues,
        "quoted_at": crate::now_unix(),
    })
}

/// Maps a routing failure to the refusal the wire contract names, logging
/// only the error's kind — never its body, which for `Unauthorized` or
/// `Unavailable` can echo request details back.
fn route_error_response(why: &RouteError) -> Response {
    match why {
        RouteError::NoRoute { .. } => refusal(
            StatusCode::NOT_FOUND,
            "no_route",
            "no route exists for this pair at this size",
        ),
        RouteError::NotConfigured
        | RouteError::Unauthorized { .. }
        | RouteError::Unavailable(_)
        | RouteError::Malformed(_)
        | RouteError::Unverifiable(_) => {
            eprintln!("radar-serve: trade route failed: {why}");
            refusal(
                StatusCode::BAD_GATEWAY,
                "unreadable_route",
                "Radar could not read a route from Jupiter for this request",
            )
        }
    }
}

/// The visitor key a caller with no session is rationed by: `CF-Connecting-IP`
/// when Cloudflare (or a caller pretending to be it) set one, else the
/// connection's own peer address. See the module doc comment for why the
/// header is trusted only as far as it is.
fn visitor_key(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    if let Some(header) = headers
        .get("cf-connecting-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return format!("cf:{header}");
    }
    peer.map_or_else(|| "peer:unknown".to_owned(), |a| format!("peer:{}", a.ip()))
}

/// Query parameters for `GET /v1/market/quote`.
#[derive(Debug, Deserialize)]
pub(crate) struct QuoteQuery {
    mint: String,
    side: String,
    amount: String,
    slippage_bps: Option<u32>,
}

/// `GET /v1/market/quote`: public, unscoped, no wallet required.
pub(crate) async fn quote(
    state: State<Arc<AppState>>,
    headers: HeaderMap,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    query: Query<QuoteQuery>,
) -> Response {
    let mut response = quote_inner(state, headers, connect_info, query).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn quote_inner(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Query(q): Query<QuoteQuery>,
) -> Response {
    let (mint, input, output, amount, slippage_bps) =
        match parse_request(&q.mint, &q.side, &q.amount, q.slippage_bps) {
            Ok(parsed) => parsed,
            Err(refusal) => return refusal,
        };

    if state.trading.is_none() {
        return refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "trading_off",
            "this instance does not build or price swaps; set RADAR_TRADE=on to enable it",
        );
    }

    let visitor = visitor_key(&headers, connect_info.map(|Extension(c)| c.0));
    {
        let trading = state.trading.as_ref().expect("checked above");
        if !trading.reserve_visitor(&visitor, CALLS_PER_REQUEST) {
            return refusal(
                StatusCode::SERVICE_UNAVAILABLE,
                "busy",
                "Radar is rate-limiting quotes; try again shortly",
            );
        }
    }

    let request = QuoteRequest::new(input, output, amount, Address::SYSTEM_PROGRAM);
    let state_for_call = Arc::clone(&state);
    let result = tokio::task::spawn_blocking(move || {
        let trading = state_for_call
            .trading
            .as_ref()
            .expect("trading is checked present before this task is spawned, and never removed");
        let quote_result = trading.router.quote_at(&request, slippage_bps)?;
        // Resolved here, off the async runtime's own thread: an uncached mint
        // reads its decimals over the same blocking RPC client `positions.rs`
        // isolates the same way.
        let in_decimals = trading.decimals_of(input);
        let out_decimals = trading.decimals_of(output);
        Ok((quote_result, in_decimals, out_decimals))
    })
    .await;

    match result {
        Ok(Ok((quote_result, in_decimals, out_decimals))) => {
            let side = if input == Asset::WrappedSol {
                "buy"
            } else {
                "sell"
            };
            Json(render_quote(
                mint,
                side,
                input,
                output,
                &quote_result,
                slippage_bps,
                in_decimals,
                out_decimals,
            ))
            .into_response()
        }
        Ok(Err(why)) => route_error_response(&why),
        Err(_join_error) => {
            eprintln!("radar-serve: quote task did not complete (join error)");
            refusal(
                StatusCode::BAD_GATEWAY,
                "unreadable_route",
                "Radar could not read a route from Jupiter for this request",
            )
        }
    }
}

/// The body of `POST /v1/customer/swap`.
#[derive(Debug, Deserialize)]
pub(crate) struct SwapBody {
    mint: String,
    side: String,
    amount: String,
    slippage_bps: Option<u32>,
}

/// `POST /v1/customer/swap`: behind [`Tenant`], builds an unsigned transaction
/// naming the signed-in wallet as fee payer. Nothing is persisted and nothing
/// here logs the wallet address — see [`route_error_response`] and
/// `radar_exec::route::Router::build`'s own doc comment for why one Jupiter
/// call produces both the quote and the transaction.
pub(crate) async fn swap(
    state: State<Arc<AppState>>,
    tenant: Tenant,
    body: Json<SwapBody>,
) -> Response {
    let mut response = swap_inner(state, tenant, body).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn swap_inner(
    State(state): State<Arc<AppState>>,
    tenant: Tenant,
    Json(body): Json<SwapBody>,
) -> Response {
    let (mint, input, output, amount, slippage_bps) =
        match parse_request(&body.mint, &body.side, &body.amount, body.slippage_bps) {
            Ok(parsed) => parsed,
            Err(refusal) => return refusal,
        };

    if state.trading.is_none() {
        return refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "trading_off",
            "this instance does not build or price swaps; set RADAR_TRADE=on to enable it",
        );
    }

    let wallet = *tenant.address();
    {
        let trading = state.trading.as_ref().expect("checked above");
        if !trading.reserve_wallet(wallet, CALLS_PER_REQUEST) {
            return refusal(
                StatusCode::SERVICE_UNAVAILABLE,
                "busy",
                "Radar is rate-limiting swap builds; try again shortly",
            );
        }
    }

    let request = QuoteRequest::new(input, output, amount, wallet);
    let state_for_call = Arc::clone(&state);
    let result = tokio::task::spawn_blocking(move || {
        let trading = state_for_call
            .trading
            .as_ref()
            .expect("trading is checked present before this task is spawned, and never removed");
        let built = trading.router.build(&request, slippage_bps)?;
        // Resolved here, off the async runtime's own thread: an uncached mint
        // reads its decimals over the same blocking RPC client `positions.rs`
        // isolates the same way.
        let in_decimals = trading.decimals_of(input);
        let out_decimals = trading.decimals_of(output);
        Ok((built, in_decimals, out_decimals))
    })
    .await;

    match result {
        Ok(Ok((built, in_decimals, out_decimals))) => {
            let side = if input == Asset::WrappedSol {
                "buy"
            } else {
                "sell"
            };
            let quote_json = render_quote(
                mint,
                side,
                input,
                output,
                &built.quote,
                slippage_bps,
                in_decimals,
                out_decimals,
            );
            Json(json!({
                "transaction": built.transaction.to_base64(),
                "last_valid_block_height": built.transaction.last_valid_block_height,
                "quote": quote_json,
            }))
            .into_response()
        }
        Ok(Err(why)) => route_error_response(&why),
        Err(_join_error) => {
            eprintln!("radar-serve: swap build task did not complete (join error)");
            refusal(
                StatusCode::BAD_GATEWAY,
                "unreadable_route",
                "Radar could not build a transaction for this request",
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

    fn usdc() -> Address {
        USDC.parse().expect("valid address")
    }

    /// No I/O happens until a request is actually sent through it, and none
    /// of these tests send one -- `from_vars` only stores the client.
    fn rpc() -> RpcClient {
        RpcClient::new("http://test.invalid")
    }

    #[test]
    fn from_vars_off_is_off_without_needing_a_credential() {
        let result = from_vars(&|_| None, rpc());
        assert!(
            matches!(result, Ok(None)),
            "RADAR_TRADE unset must mean no Trading at all, not a refusal"
        );
    }

    #[test]
    fn from_vars_on_with_a_credential_builds_trading() {
        let get = |k: &str| match k {
            VAR => Some("on".to_owned()),
            radar_exec::route::API_KEY_VAR => Some("a-key".to_owned()),
            _ => None,
        };
        let result = from_vars(&get, rpc());
        assert!(
            matches!(result, Ok(Some(_))),
            "RADAR_TRADE=on with a credential must build Trading"
        );
    }

    #[test]
    fn from_vars_on_without_a_credential_refuses_to_start() {
        let get = |k: &str| {
            if k == VAR {
                Some("on".to_owned())
            } else {
                None
            }
        };
        let result = from_vars(&get, rpc());
        assert!(
            result.is_err(),
            "a switch with nothing to price against must refuse startup rather than start half-configured"
        );
    }

    #[test]
    fn sides_for_sell_spends_the_mint_for_sol() {
        let (input, output) = sides_for(usdc(), "sell").expect("sell is a valid side");
        assert_eq!(input, Asset::spl(usdc()));
        assert_eq!(output, Asset::WrappedSol);
    }

    #[test]
    fn sides_for_buy_spends_sol_for_the_mint() {
        let (input, output) = sides_for(usdc(), "buy").expect("buy is a valid side");
        assert_eq!(input, Asset::WrappedSol);
        assert_eq!(output, Asset::spl(usdc()));
    }

    #[test]
    fn sides_for_refuses_any_other_side() {
        let response = sides_for(usdc(), "hold").expect_err("\"hold\" is not a side");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn parse_request_accepts_slippage_exactly_at_the_cap() {
        let (_, _, _, _, slippage_bps) =
            parse_request(USDC, "buy", "100000000", Some(MAX_SLIPPAGE_BPS))
                .expect("the cap itself is not \"too wide\"");
        assert_eq!(slippage_bps, MAX_SLIPPAGE_BPS);
    }

    #[test]
    fn parse_request_refuses_slippage_one_above_the_cap() {
        let response = parse_request(USDC, "buy", "100000000", Some(MAX_SLIPPAGE_BPS + 1))
            .expect_err("one more than the cap must be refused");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn worst_out_floor_computes_the_exact_shortfall() {
        // 1% of 1,000,000 is 10,000; the floor is what remains after it.
        assert_eq!(worst_out_floor(1_000_000, 100), 990_000);
    }

    #[test]
    fn impact_bps_json_is_null_for_the_unreadable_sentinel() {
        assert_eq!(impact_bps_json(u32::MAX), Value::Null);
    }

    #[test]
    fn impact_bps_json_is_the_number_otherwise() {
        assert_eq!(impact_bps_json(123), json!(123));
    }
}
