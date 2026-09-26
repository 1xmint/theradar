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
//! `GET /v1/customer/tx/{signature}?last_valid_block_height=<int>` (behind
//! [`crate::tenant::Tenant`], same as `swap`), Plan 0014 F11: whether a
//! transaction the signed-in wallet already built, signed and sent has
//! landed. `signature` is base58 of exactly 64 bytes or the route answers
//! `bad_request` without reading the chain; `last_valid_block_height` is the
//! value the matching `swap` response carried.
//!
//! 200 body:
//! ```text
//! {"state": "pending"}
//! {"state": "landed", "slot": <int>}
//! {"state": "failed", "reason": "<short plain string, never raw JSON>"}
//! {"state": "expired"}
//! ```
//!
//! `landed` and `failed` require `confirmed` or `finalized` commitment --
//! an on-chain error observed only at `processed` commitment is `pending`,
//! because a `processed` result can still belong to a fork the cluster
//! drops. `expired` requires both chain reads (block height and signature
//! status) to have succeeded, no status found, **and** the chain's current
//! block height to be strictly past `last_valid_block_height`; any failure
//! reading the chain is `chain_unreadable` (502), never `expired`, which is
//! a claim this route makes only on the chain's own word. See
//! [`render_tx_status`] for the exact state table and
//! [`tx_status_inner`]'s doc comment for the read order (block height
//! before signature status, deliberately, to close a landed-but-reads-as-
//! expired race).
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
//! # Six limits, for six different things
//!
//! Mirrors [`crate::positions`]'s reasoning exactly, with more tiers because
//! this module has three routes against two upstreams (Jupiter, and the
//! chain RPC node), and neither route sharing an upstream may starve the
//! other of it. The **quote-route global** cap (30 Jupiter calls a
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
//! The **tx-status-route global** cap and its own **per-wallet** cap (600 and
//! 60 calls a minute — see [`MAX_TX_STATUS_CALLS_PER_MINUTE`] and
//! [`MAX_TX_STATUS_CALLS_PER_WALLET_PER_MINUTE`]) exist for the same reason as
//! the swap route's pair, and are the same shape, but drawn from their own
//! pool again: this route reads the chain RPC node, not Jupiter, so its
//! budget has nothing to do with either Jupiter cap, and a poll loop
//! checking one trade's landing must not be able to starve another wallet's
//! swap build (or vice versa).
//!
//! All six checks run under the same lock-ordering discipline
//! [`crate::positions`] documents: the relevant global counter first, then the
//! per-key map, both checked before either is charged, and a refused caller is
//! never inserted.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{ConnectInfo, Extension, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use radar_exec::route::{QuoteRequest, RouteError, Router};
use radar_onchain::budget::Budget;
use radar_onchain::rpc::{RpcClient, RpcError, SignatureStatus, decode_base58};
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
/// the module doc comment's "Six limits" section for why the two routes do
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
/// route as a whole ever asking Jupiter for more than this in a minute.
/// Twenty-four is exactly four wallets' worth of simultaneous full-tilt
/// building (4 × [`MAX_CALLS_PER_WALLET_PER_MINUTE`]'s 6), and 30 (the quote
/// route's own cap) plus this 24 is 54 — still under Jupiter's free-tier
/// budget of 60 calls a minute, so the two routes can both run at their own
/// ceiling at once without either starving the other or the pair together
/// tripping Jupiter's own limit.
const MAX_JUPITER_BUILD_CALLS_PER_MINUTE: u32 = 24;

/// The most calls one wallet may draw from the customer swap route in one
/// rolling minute, regardless of room left in that route's own global budget.
const MAX_CALLS_PER_WALLET_PER_MINUTE: u32 = 6;

/// The most calls one wallet may draw from `GET /v1/customer/tx/{signature}`
/// in one rolling minute.
///
/// A different order of magnitude from [`MAX_CALLS_PER_WALLET_PER_MINUTE`] on
/// purpose: that cap paces a wallet *building* a transaction, which it does
/// rarely. This one paces a browser tab *polling* for one transaction's
/// outcome every 1.5s for up to 90s after a real send -- roughly 60 reads in
/// the worst case -- so a cap sized like the swap route's would refuse a
/// single normal poll loop partway through. Two RPC reads per call (a
/// signature-status lookup and a block-height read), so this is a much
/// smaller draw on the node per call than a swap build's single Jupiter call.
const MAX_TX_STATUS_CALLS_PER_WALLET_PER_MINUTE: u32 = 60;

/// The most calls this instance will make for `GET /v1/customer/tx/{signature}`,
/// across every wallet, in one rolling minute.
///
/// Its own pool, separate from [`Self::build_calls`] and [`Self::quote_calls`]
/// for the same reason those two are separate from each other -- a burst of
/// polling from one wallet must not starve another wallet's own swap build,
/// and vice versa. Ten wallets' worth of simultaneous full-tilt polling
/// (10 x [`MAX_TX_STATUS_CALLS_PER_WALLET_PER_MINUTE`]'s 60).
const MAX_TX_STATUS_CALLS_PER_MINUTE: u32 = 600;

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
    /// `GET /v1/customer/tx/{signature}`'s own global budget. Never shared
    /// with [`Self::quote_calls`] or [`Self::build_calls`] — see the module
    /// doc comment's "Six limits" section, and [`MAX_TX_STATUS_CALLS_PER_MINUTE`].
    tx_status_calls: Mutex<VecDeque<Instant>>,
    wallet_tx_status_calls: Mutex<HashMap<Address, VecDeque<Instant>>>,
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
            tx_status_calls: Mutex::new(VecDeque::new()),
            wallet_tx_status_calls: Mutex::new(HashMap::new()),
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
    /// draws on. See the module doc comment's "Six limits" section for why
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

    /// Reserves `want` calls against `wallet`'s own share and
    /// `GET /v1/customer/tx/{signature}`'s own global budget
    /// ([`Self::tx_status_calls`]) — its own pool, separate from both the
    /// quote and swap routes'. Same lock-ordering discipline as
    /// [`Self::reserve_visitor`] and [`Self::reserve_wallet`].
    fn reserve_tx_status(&self, wallet: Address, want: u32) -> bool {
        let mut global = self
            .tx_status_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut wallets = self
            .wallet_tx_status_calls
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
        if !fits(&global, want, MAX_TX_STATUS_CALLS_PER_MINUTE)
            || !fits(mine, want, MAX_TX_STATUS_CALLS_PER_WALLET_PER_MINUTE)
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
    // Forces the OFAC list's parse now, at startup, rather than on the first
    // swap: `sanctioned_wallets` panics on a malformed line (see
    // `parse_sanctioned`), and a switch that comes up "on" only to panic on
    // its first real request is the exact failure `from_vars` exists to
    // convert into a refusal to start.
    let _ = sanctioned_wallets();
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

/// A fixed, address-free label for a routing failure's *kind* — never its
/// body. `RouteError`'s `Display` can carry a visitor's own wallet address
/// (`Malformed`, from `radar_exec::assemble`'s signer refusal), an upstream
/// URL (`Unavailable`), or a raw response body (`Unauthorized`), and none of
/// that belongs in a server log line. This match is exhaustive on purpose —
/// with no wildcard arm, a new `RouteError` variant fails to compile here
/// until it is given its own fixed label, rather than silently falling back
/// to logging its `Display`.
fn route_error_label(why: &RouteError) -> &'static str {
    match why {
        RouteError::NoRoute { .. } => "no_route",
        RouteError::NotConfigured => "not_configured",
        RouteError::Unauthorized { .. } => "unauthorized",
        RouteError::Unavailable(_) => "unavailable",
        RouteError::Malformed(_) => "malformed",
        RouteError::Unverifiable(_) => "unverifiable",
    }
}

/// Maps a routing failure to the refusal the wire contract names, logging
/// only the error's kind (via [`route_error_label`]) — never its body, which
/// for `Unauthorized` or `Unavailable` can echo request details, or for
/// `Malformed`, a visitor's own wallet address, back.
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
            eprintln!(
                "radar-serve: trade route failed: {}",
                route_error_label(why)
            );
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
///
/// `pub(crate)`: `lib.rs`'s `/v1/market/events` handler keys its own
/// per-visitor SSE connection cap by the same identity, rather than growing a
/// second copy of this logic.
pub(crate) fn visitor_key(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
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

/// OFAC's Solana ("Digital Currency Address - SOL") sanctions list, checked
/// into the crate at build time. See `data/ofac_sol.txt` for the source URLs,
/// the date it was read, and how to refresh it.
const OFAC_SOL_LIST: &str = include_str!("../data/ofac_sol.txt");

/// Parses an [`OFAC_SOL_LIST`]-shaped text into a set of addresses: one
/// base58 pubkey per line, blank lines and `#`-comments ignored.
///
/// Panics on a line that does not parse as a Solana address. This is a small,
/// hand-curated file compiled into the binary, not caller input, so a typo
/// panics rather than silently dropping a sanctioned wallet from the set it
/// exists to hold. [`from_vars`] forces this parse at startup (with
/// `RADAR_TRADE=on`), so the panic surfaces there and not on the first swap.
fn parse_sanctioned(data: &str) -> HashSet<Address> {
    data.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            line.parse().unwrap_or_else(|e| {
                panic!("{line:?} in the OFAC SOL sanctions list is not a valid Solana address: {e}")
            })
        })
        .collect()
}

/// The compiled-in OFAC SOL list, parsed once and reused for every request.
fn sanctioned_wallets() -> &'static HashSet<Address> {
    static SET: OnceLock<HashSet<Address>> = OnceLock::new();
    SET.get_or_init(|| parse_sanctioned(OFAC_SOL_LIST))
}

/// `POST /v1/customer/swap`'s refusal for a wallet in `sanctioned`.
///
/// Jupiter's SDK & API License §7.3 makes the integrator (Radar) responsible
/// for sanctions compliance, and every trade Radar builds routes through
/// Jupiter -- so this runs before Jupiter is ever asked for a route. Quotes,
/// viewing and sign-in are unaffected: only a swap actually being built for a
/// listed wallet is refused.
fn sanctioned_response(sanctioned: &HashSet<Address>, wallet: Address) -> Option<Response> {
    sanctioned.contains(&wallet).then(|| {
        refusal(
            StatusCode::FORBIDDEN,
            "sanctioned",
            "this wallet appears on a sanctions list, so Radar will not build trades for it",
        )
    })
}

/// `POST /v1/customer/swap`: behind [`Tenant`], builds an unsigned transaction
/// naming the signed-in wallet as fee payer. Nothing is persisted and nothing
/// here logs the wallet address — see [`route_error_response`] and
/// `radar_exec::route::Router::build`'s own doc comment for why one Jupiter
/// call produces both the quote and the transaction.
///
/// **Sanctions check first, before rate-limiting or Jupiter.** A wallet on
/// [`sanctioned_wallets`] is refused with `sanctioned` and never spends its
/// rate-limit budget or reaches Jupiter -- see [`sanctioned_response`].
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
    if let Some(refusal) = sanctioned_response(sanctioned_wallets(), wallet) {
        return refusal;
    }
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

/// Query parameters for `GET /v1/customer/tx/{signature}`.
#[derive(Debug, Deserialize)]
pub(crate) struct TxStatusQuery {
    last_valid_block_height: u64,
}

/// A short, plain rendering of a landed-but-failed transaction's on-chain
/// error -- never the raw JSON, which is a debugging shape (`{"InstructionError":
/// [3,{"Custom":6001}]}`) nobody asked this route to teach a customer to read.
///
/// Deliberately not exhaustive on every `TransactionError` variant Solana
/// defines: new variants are rare, and falling back to the compact JSON
/// (still short, still plain enough) is a better failure mode here than this
/// route refusing to answer over a shape it does not recognise yet.
fn describe_tx_err(err: &Value) -> String {
    if let Some(arr) = err.get("InstructionError").and_then(Value::as_array)
        && let (Some(index), Some(detail)) = (arr.first().and_then(Value::as_u64), arr.get(1))
    {
        if let Some(custom) = detail.get("Custom").and_then(Value::as_u64) {
            return format!("instruction {index} failed with custom error {custom}");
        }
        if let Some(kind) = detail.as_str() {
            return format!("instruction {index} failed: {kind}");
        }
    }
    if let Some(kind) = err.as_str() {
        return kind.to_owned();
    }
    err.to_string()
}

/// Answers whether a transaction the caller's own wallet already sent has
/// landed, failed, expired, or is still pending -- the read a success screen
/// needs before it may say "success". See the module doc comment's wire
/// contract note above [`TxStatusQuery`] for the full state table.
fn render_tx_status(
    status: Option<SignatureStatus>,
    height: u64,
    last_valid_block_height: u64,
) -> Value {
    match status {
        Some(status) => {
            let landed = matches!(
                status.confirmation_status.as_deref(),
                Some("confirmed" | "finalized")
            );
            // `err` is only final once the transaction has actually landed
            // at `confirmed` or `finalized` commitment. A `processed`-level
            // error can still belong to a fork the cluster drops -- the
            // transaction may yet land clean on the fork that wins, so this
            // is `pending`, never `failed`, until commitment says the result
            // is not going to be re-decided.
            if let Some(err) = status.err
                && landed
            {
                return json!({ "state": "failed", "reason": describe_tx_err(&err) });
            }
            if landed {
                json!({ "state": "landed", "slot": status.slot })
            } else {
                json!({ "state": "pending" })
            }
        }
        // No status at all is only ever "expired" once both reads succeeded
        // (we are here) and the chain's own clock has passed the transaction's
        // last valid block height -- never on the strength of a missing status
        // alone, which is indistinguishable from "not landed yet".
        None if height > last_valid_block_height => json!({ "state": "expired" }),
        None => json!({ "state": "pending" }),
    }
}

/// `GET /v1/customer/tx/{signature}?last_valid_block_height=N`: whether a
/// transaction the signed-in wallet already sent has landed. See the module
/// doc comment for the wire contract.
pub(crate) async fn tx_status(
    state: State<Arc<AppState>>,
    tenant: Tenant,
    path: Path<String>,
    query: Query<TxStatusQuery>,
) -> Response {
    let mut response = tx_status_inner(state, tenant, path, query).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn tx_status_inner(
    State(state): State<Arc<AppState>>,
    tenant: Tenant,
    Path(signature): Path<String>,
    Query(query): Query<TxStatusQuery>,
) -> Response {
    let decoded = decode_base58(&signature);
    if decoded.is_none_or(|bytes| bytes.len() != 64) {
        return refusal(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "signature must be base58 of 64 bytes",
        );
    }

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
        if !trading.reserve_tx_status(wallet, CALLS_PER_REQUEST) {
            return refusal(
                StatusCode::SERVICE_UNAVAILABLE,
                "busy",
                "Radar is rate-limiting transaction-status reads; try again shortly",
            );
        }
    }

    let last_valid_block_height = query.last_valid_block_height;
    let state_for_call = Arc::clone(&state);
    let result = tokio::task::spawn_blocking(move || {
        let trading = state_for_call
            .trading
            .as_ref()
            .expect("trading is checked present before this task is spawned, and never removed");
        let mut budget = Budget::new(2, 1, Duration::from_secs(10));
        // Block height first, then the signature status -- never the other
        // order. A landed transaction moves both facts forward together; if
        // the status read ran first and came back empty, then the
        // transaction landed, then height read past the limit, an
        // after-the-status height would show "expired" for a trade that
        // landed while this read was in flight. Reading height first means
        // any height this call sees was already true at (or before) the
        // moment the status read follows it, so "no status yet, and already
        // past the limit" cannot be true of a transaction that lands between
        // the two reads.
        let height = trading.rpc.block_height(&mut budget)?;
        let status = trading.rpc.signature_status(&mut budget, &signature)?;
        Ok::<(Option<SignatureStatus>, u64), RpcError>((status, height))
    })
    .await;

    match result {
        Ok(Ok((status, height))) => {
            Json(render_tx_status(status, height, last_valid_block_height)).into_response()
        }
        Ok(Err(why)) => {
            eprintln!("radar-serve: tx status read failed: {}", why.kind());
            refusal(
                StatusCode::BAD_GATEWAY,
                "chain_unreadable",
                "Radar could not read the chain to check this transaction",
            )
        }
        Err(_join_error) => {
            eprintln!("radar-serve: tx status task did not complete (join error)");
            refusal(
                StatusCode::BAD_GATEWAY,
                "chain_unreadable",
                "Radar could not read the chain to check this transaction",
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
    const OTHER_MINT: &str = "So11111111111111111111111111111111111111112";

    /// Every line the crate actually ships must parse -- `parse_sanctioned`
    /// panics on a bad line, so this test is what catches a typo here in CI
    /// rather than at startup on whoever next runs `RADAR_TRADE=on`.
    #[test]
    fn every_line_of_the_shipped_ofac_list_parses_as_an_address() {
        let set = parse_sanctioned(OFAC_SOL_LIST);
        // Not asserting a nonzero count: an empty list is a valid, honest
        // state (rule 9 -- absent is not zero, but zero is also not a bug),
        // and this test's job is only that every line present is a real
        // address, never that the list has any particular size.
        for line in OFAC_SOL_LIST
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            assert!(
                line.parse::<Address>().is_ok(),
                "{line:?} does not parse as a Solana address"
            );
        }
        // The parsed set and the source file agree on how many entries there
        // are -- guards against two different lines decoding to the same
        // 32 bytes and silently collapsing in the `HashSet`.
        let line_count = OFAC_SOL_LIST
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .count();
        assert_eq!(set.len(), line_count);
    }

    /// The check is parameterised on the set precisely so this test does not
    /// depend on how many real addresses OFAC currently lists for SOL: a
    /// listed address is refused with `sanctioned`, and an unlisted one
    /// passes through untouched, regardless of what `data/ofac_sol.txt`
    /// happens to contain today.
    #[test]
    fn sanctioned_response_refuses_only_a_listed_wallet() {
        let listed = usdc(); // stands in for a listed address; only its
        // membership in the injected set matters here, not its real-world
        // meaning as USDC's mint.
        let unlisted: Address = OTHER_MINT.parse().expect("valid address");
        let mut set = HashSet::new();
        set.insert(listed);

        let refused = sanctioned_response(&set, listed).expect("a listed wallet must be refused");
        assert_eq!(refused.status(), StatusCode::FORBIDDEN);

        assert!(
            sanctioned_response(&set, unlisted).is_none(),
            "an unlisted wallet must pass the check"
        );
    }

    fn usdc() -> Address {
        USDC.parse().expect("valid address")
    }

    /// No I/O happens until a request is actually sent through it, and none
    /// of these tests send one -- `from_vars` only stores the client.
    fn rpc() -> RpcClient {
        RpcClient::new("http://test.invalid")
    }

    /// A [`Trading`] built the same way [`from_vars`] does, with no I/O:
    /// `Router::new` only stores the credential.
    fn trading_for_test() -> Trading {
        let get = |k: &str| match k {
            VAR => Some("on".to_owned()),
            radar_exec::route::API_KEY_VAR => Some("a-key".to_owned()),
            _ => None,
        };
        from_vars(&get, rpc())
            .expect("a supplied key builds Trading")
            .expect("RADAR_TRADE=on with a credential returns Some")
    }

    fn wallet(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    /// The per-wallet tx-status cap is its own budget, separate from every
    /// other wallet's: one wallet spending its whole 60-call minute must
    /// leave a second wallet fully served, never `busy` for a reason that
    /// has nothing to do with it.
    #[test]
    fn the_per_wallet_tx_status_cap_busies_only_the_wallet_that_hit_it() {
        let trading = trading_for_test();
        let exhausted = wallet(1);
        let other = wallet(2);
        for i in 0..MAX_TX_STATUS_CALLS_PER_WALLET_PER_MINUTE {
            assert!(
                trading.reserve_tx_status(exhausted, CALLS_PER_REQUEST),
                "call {i} is still within the per-wallet cap"
            );
        }
        assert!(
            !trading.reserve_tx_status(exhausted, CALLS_PER_REQUEST),
            "the call past the per-wallet cap must be refused"
        );
        assert!(
            trading.reserve_tx_status(other, CALLS_PER_REQUEST),
            "a different wallet must still be served -- its own budget is untouched"
        );
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

    /// Every `RouteError` variant must log as a fixed, hand-written label —
    /// never its own `Display`, which for several variants carries request
    /// details (an upstream URL or response body) or, for `Malformed`, can
    /// carry a visitor's own wallet address (`radar_exec::assemble`'s signer
    /// refusal). Each variant is built here with data that would fail this
    /// test immediately if `route_error_label` ever fell back to `{why}`.
    #[test]
    fn route_error_label_is_a_fixed_string_for_every_variant() {
        let cases: &[(RouteError, &str)] = &[
            (
                RouteError::NoRoute {
                    mint: usdc().to_string(),
                    size_lamports: 1,
                },
                "no_route",
            ),
            (RouteError::NotConfigured, "not_configured"),
            (
                RouteError::Unauthorized {
                    status: 401,
                    body: "secret-detail".to_owned(),
                },
                "unauthorized",
            ),
            (
                RouteError::Unavailable("https://api.jup.ag/secret-detail".to_owned()),
                "unavailable",
            ),
            (RouteError::Malformed(usdc().to_string()), "malformed"),
            (
                RouteError::Unverifiable("secret-detail".to_owned()),
                "unverifiable",
            ),
        ];
        for (err, expected_label) in cases {
            assert_eq!(
                route_error_label(err),
                *expected_label,
                "wrong label for {err:?}"
            );
        }
    }
}
