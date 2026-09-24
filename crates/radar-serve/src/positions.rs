// SPDX-License-Identifier: Apache-2.0
//! `/v1/customer/positions`: the signed-in wallet's own Solana holdings.
//!
//! Item 2 of plan 0013's Phase C, redesigned from a browser-reads-RPC shape to
//! this one on 2026-09-24: Solana's public node answers `getBalance` and
//! `getTokenAccountsByOwner` with `{"error":{"code":403,"message":"Access
//! forbidden"}}` when the request carries a browser `Origin` header, and the
//! same request with no `Origin` succeeds. A page cannot omit its own Origin.
//! So the server reads the chain -- exactly like [`crate::watchlist`] reads a
//! wallet's own storage -- behind the same [`crate::tenant::Tenant`], and the
//! browser reads Radar instead.
//!
//! # Every read is all-or-nothing
//!
//! A view is three RPC calls: the wallet's SOL balance, its Token-program
//! accounts, and its Token-2022 accounts. AGENTS rules 8 and 9: a missing read
//! is not an empty holding, so if any of the three fails the whole answer is a
//! failure. There is no shape here for "two programs read, one did not" that
//! looks like a complete list with nothing held under the program that failed.
//!
//! # Two limits, for two different things
//!
//! The per-wallet cache (30s) exists so one wallet refreshing its own screen
//! does not spend its own budget every few seconds. The global cap (60 calls a
//! minute, reserved 3 at a time) exists so no number of *distinct* wallets can
//! together exceed what the configured RPC endpoint tolerates -- the public
//! node is a development convenience, not something to serve traffic through.
//! Reservation happens in one critical section before the first call, so two
//! requests racing each other cannot both observe room for the last three
//! calls and together spend six.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use radar_onchain::budget::Budget;
use radar_onchain::rpc::{RpcClient, RpcError, TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};
use radar_types::Address;
use serde_json::{Value, json};

use crate::AppState;
use crate::tenant::{Tenant, refusal};

/// The most calls this route may spend across every wallet in one minute.
///
/// One view costs three; this is twenty views a minute. The public Solana node
/// this defaults to rate-limits well below that already -- the point of this
/// cap is that a real endpoint's bill is bounded regardless of how many
/// distinct wallets ask.
const MAX_CALLS_PER_MINUTE: u32 = 60;

/// One view: a balance read and two `getTokenAccountsByOwner` reads.
const CALLS_PER_VIEW: u32 = 3;

/// How long a wallet's own answer is reused before this route reads the chain
/// for it again.
const CACHE_TTL: Duration = Duration::from_secs(30);

/// The most distinct wallets held at once. A bound rather than none, for the
/// reason [`crate::tenant::WATCHLIST_LIMIT`] is one: any wallet can sign in,
/// and a cache with no ceiling is memory anyone who can sign in can grow.
const CACHE_CAPACITY: usize = 2048;

/// The wrapped-SOL mint. Native SOL has no mint account of its own, and this is
/// the token whose trades Radar's tape actually carries -- so this is the
/// price checked before SOL's holding is called priced.
const WRAPPED_SOL_MINT: &str = "So11111111111111111111111111111111111111112";

/// Server-side Solana reads for the signed-in wallet's own holdings, and the
/// state two limits above need to hold.
pub struct Positions {
    rpc: Arc<RpcClient>,
    cache: Mutex<HashMap<Address, CacheEntry>>,
    window_start: Mutex<Instant>,
    used_this_window: AtomicU32,
}

/// One wallet's most recently read holdings, kept for [`CACHE_TTL`].
struct CacheEntry {
    computed_at: Instant,
    data: Arc<Held>,
}

/// What the chain said the last time this wallet was actually read.
///
/// Deliberately missing `age_seconds`: that is derived at serve time from
/// `read_at`, on both the fresh and the cached path, through the same
/// function -- so there is exactly one place that decides how old an answer
/// is, and a cached entry cannot be relabelled fresh by skipping it.
struct Held {
    wallet: Address,
    slot: u64,
    read_at: u64,
    sol_lamports: u64,
    sol_price_usd: Option<f64>,
    tokens: Vec<Holding>,
}

struct Holding {
    mint: String,
    program: &'static str,
    amount: u128,
    decimals: u8,
    price_usd: Option<f64>,
}

impl Positions {
    /// Reads through `rpc`, starting with an empty cache and a full budget.
    #[must_use]
    pub fn new(rpc: RpcClient) -> Self {
        Self {
            rpc: Arc::new(rpc),
            cache: Mutex::new(HashMap::new()),
            window_start: Mutex::new(Instant::now()),
            used_this_window: AtomicU32::new(0),
        }
    }

    /// The host this instance's positions route reads from, for the startup
    /// log -- never the full endpoint, which can carry a paid provider's key.
    #[must_use]
    pub fn host(&self) -> String {
        host_of(self.rpc.endpoint())
    }

    fn cached(&self, wallet: Address) -> Option<Arc<Held>> {
        let cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache
            .get(&wallet)
            .filter(|entry| entry.computed_at.elapsed() < CACHE_TTL)
            .map(|entry| Arc::clone(&entry.data))
    }

    fn remember(&self, wallet: Address, data: Arc<Held>) {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let oldest = if cache.len() >= CACHE_CAPACITY && !cache.contains_key(&wallet) {
            cache
                .iter()
                .min_by_key(|(_, entry)| entry.computed_at)
                .map(|(addr, _)| *addr)
        } else {
            None
        };
        if let Some(oldest) = oldest {
            cache.remove(&oldest);
        }
        cache.insert(
            wallet,
            CacheEntry {
                computed_at: Instant::now(),
                data,
            },
        );
    }

    /// Reserves `want` calls against this minute's budget, atomically: the
    /// window is checked and spent inside one lock, so two requests racing
    /// each other cannot both see room for the last few calls.
    fn reserve(&self, want: u32) -> bool {
        let mut start = self
            .window_start
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if start.elapsed() >= Duration::from_secs(60) {
            *start = Instant::now();
            self.used_this_window.store(0, Ordering::SeqCst);
        }
        let used = self.used_this_window.load(Ordering::SeqCst);
        if used.saturating_add(want) > MAX_CALLS_PER_MINUTE {
            return false;
        }
        self.used_this_window.store(used + want, Ordering::SeqCst);
        true
    }
}

/// `host:port` (or just the host) of an endpoint, with no path, query or
/// credential. Falls back to the whole string if it does not parse as a URL --
/// still better than refusing to log anything at startup.
fn host_of(endpoint: &str) -> String {
    let after_scheme = endpoint
        .split_once("://")
        .map_or(endpoint, |(_, rest)| rest);
    let host = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    // Strip a userinfo prefix (`user:pass@host`), which is also not this
    // route's business to print.
    host.rsplit('@').next().unwrap_or(host).to_owned()
}

/// `GET /v1/customer/positions`: the calling wallet's SOL and token holdings.
pub(crate) async fn get(State(state): State<Arc<AppState>>, tenant: Tenant, uri: Uri) -> Response {
    if uri.query().is_some() {
        return refusal(
            StatusCode::BAD_REQUEST,
            "unscoped",
            "this route reads only the signed-in wallet's own holdings, and takes no parameters",
        );
    }
    let Some(positions) = state.positions.as_ref() else {
        return refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_configured",
            "this instance cannot read on-chain balances",
        );
    };
    let wallet = *tenant.address();

    if let Some(held) = positions.cached(wallet) {
        return Json(render(&held)).into_response();
    }

    if !positions.reserve(CALLS_PER_VIEW) {
        return refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "busy",
            "Radar is rate-limiting balance reads; try again shortly",
        );
    }

    let rpc = Arc::clone(&positions.rpc);
    let read = tokio::task::spawn_blocking(move || read_wallet(&rpc, wallet)).await;
    let raw = match read {
        Ok(Ok(raw)) => raw,
        // The read itself failed (transport, node, timeout, malformed shape,
        // or budget exhaustion): a fact about this attempt, never cached, and
        // never rendered as a partial list.
        Ok(Err(_why)) => {
            return refusal(
                StatusCode::BAD_GATEWAY,
                "unreadable_chain",
                "Radar could not read the chain for this wallet; this says nothing about what it holds",
            );
        }
        // The blocking task panicked or was cancelled -- the same "could not
        // look" answer, not a 500 that implies Radar's own state is broken.
        Err(_why) => {
            return refusal(
                StatusCode::BAD_GATEWAY,
                "unreadable_chain",
                "Radar could not read the chain for this wallet; this says nothing about what it holds",
            );
        }
    };

    let held = match priced(&state, wallet, raw) {
        Ok(held) => Arc::new(held),
        Err(response) => return *response,
    };
    positions.remember(wallet, Arc::clone(&held));
    Json(render(&held)).into_response()
}

/// The raw shape read from the chain, before pricing and before the response
/// is built -- nothing here has failed, or [`read_wallet`] would have returned
/// an error instead.
struct RawRead {
    slot: u64,
    sol_lamports: u64,
    token_accounts: Vec<(radar_onchain::rpc::TokenAccount, &'static str)>,
}

/// The three calls one view costs, run to completion or not at all -- this
/// function returns as soon as any one of them fails.
///
/// Runs on a blocking thread: [`RpcClient`] is `ureq`, which blocks the thread
/// it runs on for the whole round trip.
fn read_wallet(rpc: &RpcClient, wallet: Address) -> Result<RawRead, RpcError> {
    let mut budget = Budget::new(CALLS_PER_VIEW, CALLS_PER_VIEW, Duration::from_secs(20));
    let balance = rpc.balance(&mut budget, &wallet)?;
    let token = rpc.token_accounts_by_owner(&mut budget, &wallet, TOKEN_PROGRAM_ID)?;
    let token_2022 = rpc.token_accounts_by_owner(&mut budget, &wallet, TOKEN_2022_PROGRAM_ID)?;

    // The slot reported for the view is the balance read's -- the three calls
    // are not atomic against each other, and naming one of them is more honest
    // than averaging or taking whichever answered last.
    let slot = balance.slot.map_or(0, |s| s.0);

    let mut token_accounts = Vec::with_capacity(token.accounts.len() + token_2022.accounts.len());
    for account in token.accounts {
        token_accounts.push((account, "token"));
    }
    for account in token_2022.accounts {
        token_accounts.push((account, "token-2022"));
    }

    Ok(RawRead {
        slot,
        sol_lamports: balance.lamports,
        token_accounts,
    })
}

/// Sums same-mint accounts, drops zero balances, and attaches Radar's own
/// price to each holding -- never a partial answer, even here: a checked-add
/// overflow across a wallet's own accounts is reported rather than wrapped.
fn priced(state: &AppState, wallet: Address, raw: RawRead) -> Result<Held, Box<Response>> {
    struct Summed {
        program: &'static str,
        amount: u128,
        decimals: u8,
    }
    let mut by_mint: HashMap<String, Summed> = HashMap::new();
    for (account, program) in raw.token_accounts {
        let entry = by_mint.entry(account.mint).or_insert(Summed {
            program,
            amount: 0,
            decimals: account.decimals,
        });
        entry.amount = entry.amount.checked_add(account.amount).ok_or_else(|| {
            Box::new(refusal(
                StatusCode::BAD_GATEWAY,
                "unreadable_chain",
                "Radar could not read the chain for this wallet; this says nothing about what it holds",
            ))
        })?;
    }

    let mut tokens: Vec<Holding> = Vec::new();
    for (mint, summed) in by_mint {
        if summed.amount == 0 {
            continue;
        }
        let price_usd = mint
            .parse::<Address>()
            .ok()
            .and_then(|address| crate::market::price_of(state, address));
        tokens.push(Holding {
            mint,
            program: summed.program,
            amount: summed.amount,
            decimals: summed.decimals,
            price_usd,
        });
    }
    // Deterministic order for a caller comparing two responses, and for tests.
    tokens.sort_by(|a, b| a.mint.cmp(&b.mint));

    let sol_price_usd = WRAPPED_SOL_MINT
        .parse::<Address>()
        .ok()
        .and_then(|address| crate::market::price_of(state, address));

    Ok(Held {
        wallet,
        slot: raw.slot,
        read_at: crate::now_unix(),
        sol_lamports: raw.sol_lamports,
        sol_price_usd,
        tokens,
    })
}

/// The raw integer amount as a decimal string, using `decimals` places -- never
/// through a float, so a 64-bit amount never loses precision on the way to the
/// wire.
fn ui_amount(amount: u128, decimals: u8) -> String {
    let raw = amount.to_string();
    let decimals = usize::from(decimals);
    if decimals == 0 {
        return raw;
    }
    if raw.len() <= decimals {
        let mut fraction = "0".repeat(decimals - raw.len());
        fraction.push_str(&raw);
        format!("0.{fraction}")
    } else {
        let split = raw.len() - decimals;
        format!("{}.{}", &raw[..split], &raw[split..])
    }
}

fn render(held: &Held) -> Value {
    let now = crate::now_unix();
    let age_seconds = now.saturating_sub(held.read_at);
    let sol_ui = ui_amount(u128::from(held.sol_lamports), 9);
    // A dollar value is already an estimate once it is multiplied by a
    // floating-point price; losing precision below 2^52 lamports (about
    // 4.5M SOL) does not change which dollar amount is shown.
    #[allow(clippy::cast_precision_loss)]
    let sol_value_usd = held
        .sol_price_usd
        .map(|price| price * (held.sol_lamports as f64 / 1_000_000_000.0));
    json!({
        "wallet": held.wallet.to_string(),
        "slot": held.slot,
        "read_at": held.read_at,
        "age_seconds": age_seconds,
        "sol": {
            "lamports": held.sol_lamports,
            "ui_amount": sol_ui,
            "price_usd": held.sol_price_usd,
            "value_usd": sol_value_usd,
            "priced": held.sol_price_usd.is_some(),
        },
        "tokens": held.tokens.iter().map(|t| {
            // Same reasoning as `sol_value_usd`: a displayed USD estimate,
            // not an accounting figure carried onward.
            #[allow(clippy::cast_precision_loss)]
            let amount_f64 = t.amount as f64 / 10f64.powi(i32::from(t.decimals));
            let value_usd = t.price_usd.map(|price| price * amount_f64);
            json!({
                "mint": t.mint,
                "program": t.program,
                "amount": t.amount.to_string(),
                "decimals": t.decimals,
                "ui_amount": ui_amount(t.amount, t.decimals),
                "price_usd": t.price_usd,
                "value_usd": value_usd,
                "priced": t.price_usd.is_some(),
            })
        }).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_of_an_endpoint_never_carries_a_key_or_a_path() {
        assert_eq!(
            host_of("https://mainnet.helius-rpc.com/?api-key=secret"),
            "mainnet.helius-rpc.com"
        );
        assert_eq!(
            host_of("https://api.mainnet-beta.solana.com"),
            "api.mainnet-beta.solana.com"
        );
        assert_eq!(host_of("https://user:pass@node.example/v1"), "node.example");
    }

    #[test]
    fn ui_amount_never_goes_through_a_float() {
        assert_eq!(ui_amount(1_500_000_000, 9), "1.500000000");
        assert_eq!(ui_amount(5, 9), "0.000000005");
        assert_eq!(ui_amount(0, 9), "0.000000000");
        assert_eq!(ui_amount(42, 0), "42");
    }

    #[test]
    fn a_second_reservation_past_the_cap_is_refused() {
        let positions = Positions::new(RpcClient::default());
        for _ in 0..(MAX_CALLS_PER_MINUTE / CALLS_PER_VIEW) {
            assert!(positions.reserve(CALLS_PER_VIEW));
        }
        assert!(!positions.reserve(CALLS_PER_VIEW), "the window is spent");
    }
}
