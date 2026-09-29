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
//! # Three limits, for three different things
//!
//! The per-wallet cache (30s) exists so one wallet refreshing its own screen
//! does not spend its own budget every few seconds. The global cap (60 calls a
//! minute) exists so no number of *distinct* wallets can together exceed what
//! the configured RPC endpoint tolerates -- the public node is a development
//! convenience, not something to serve traffic through. The per-wallet *rate*
//! limit (6 calls a minute, distinct from the cache above) exists so one
//! wallet retrying aggressively cannot alone spend the whole global budget and
//! starve every other wallet reading at the same time. All three checks run
//! under a lock held across the whole reservation, so two requests racing each
//! other cannot both observe room for the last few calls and together spend
//! more than is left.
//!
//! # Prices are never dollars
//!
//! `radar_store::MarketTrade::price` is `quote_amount / token_amount` in
//! whatever asset the trade's other leg was -- wSOL, USDC or USDT on this
//! tape, never itself a dollar figure. This route's response carries `price`
//! and `quote` (the traded asset's name) instead of a `_usd`-suffixed field,
//! so nothing here can mislabel a SOL-denominated number as a dollar one. See
//! [`crate::market::prices_of`] and [`quote_name`].

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use radar_onchain::budget::Budget;
use radar_onchain::rpc::{RpcClient, RpcError, TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};
use radar_types::Address;
use serde_json::{Value, json};

use crate::AppState;
use crate::tenant::{Tenant, refusal};

/// The most calls this route may spend across every wallet in one rolling
/// minute.
///
/// One view costs three; this is twenty views a minute. The public Solana node
/// this defaults to rate-limits well below that already -- the point of this
/// cap is that a real endpoint's bill is bounded regardless of how many
/// distinct wallets ask.
const MAX_CALLS_PER_MINUTE: u32 = 60;

/// The most calls one wallet may spend in one rolling minute, regardless of
/// room left under [`MAX_CALLS_PER_MINUTE`].
///
/// Two fresh reads: one to load the screen, one deliberate refresh. Without
/// this, a single wallet retrying past its own cache could alone spend the
/// entire global budget and leave nothing for any other wallet.
const MAX_CALLS_PER_WALLET_PER_MINUTE: u32 = 6;

/// One view: a balance read and two `getTokenAccountsByOwner` reads.
const CALLS_PER_VIEW: u32 = 3;

/// How long a wallet's own answer is reused before this route reads the chain
/// for it again, in whole seconds -- the same unit [`crate::now_unix`] uses,
/// so the boundary can be tested exactly instead of by racing a real clock.
const CACHE_TTL_SECONDS: u64 = 30;

/// The most distinct wallets held at once. A bound rather than none, for the
/// reason [`crate::tenant::WATCHLIST_LIMIT`] is one: any wallet can sign in,
/// and a cache with no ceiling is memory anyone who can sign in can grow.
const CACHE_CAPACITY: usize = 2048;

/// The wrapped-SOL mint. Native SOL has no mint account of its own, and this is
/// the token whose trades Radar's tape actually carries -- so this is the
/// price checked before SOL's holding is called priced.
const WRAPPED_SOL_MINT: &str = "So11111111111111111111111111111111111111112";

/// Circulating USDC on Solana.
const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

/// Circulating USDT on Solana.
const USDT_MINT: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";

/// The response's name for a quote asset, or `None` for a mint this route has
/// no name for.
///
/// A caller that cannot name the quote asset must never invent a currency
/// symbol for the number anyway -- [`render`]'s two call sites both drop the
/// price/value pair when this returns `None`, which in practice does not
/// happen for a mint [`priced`] put in its map, because
/// [`crate::market::prices_of`] only ever prices a trade quoted in one of
/// these three.
fn quote_name(mint: Address) -> Option<&'static str> {
    match mint.to_string().as_str() {
        WRAPPED_SOL_MINT => Some("SOL"),
        USDC_MINT => Some("USDC"),
        USDT_MINT => Some("USDT"),
        _ => None,
    }
}

/// Server-side Solana reads for the signed-in wallet's own holdings, and the
/// state the three limits above need to hold.
pub struct Positions {
    rpc: Arc<RpcClient>,
    cache: Mutex<HashMap<Address, CacheEntry>>,
    global_calls: Mutex<VecDeque<Instant>>,
    wallet_calls: Mutex<HashMap<Address, VecDeque<Instant>>>,
}

/// One wallet's most recently read holdings, kept for [`CACHE_TTL_SECONDS`].
struct CacheEntry {
    computed_at: u64,
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
    /// SOL's price, quoted only in USDC or USDT -- never in wSOL itself. See
    /// [`priced`] for why a wSOL-quoted trade cannot price SOL.
    sol_price: Option<f64>,
    sol_quote_mint: Option<Address>,
    /// Set exactly when `sol_price` is `None`: a sentence a reader can act on
    /// instead of just a blank price (finding 11).
    sol_price_reason: Option<&'static str>,
    tokens: Vec<Holding>,
}

struct Holding {
    mint: String,
    program: &'static str,
    amount: u128,
    decimals: u8,
    price: Option<f64>,
    quote_mint: Option<Address>,
}

impl Positions {
    /// Reads through `rpc`, starting with an empty cache and full budgets.
    #[must_use]
    pub fn new(rpc: RpcClient) -> Self {
        Self {
            rpc: Arc::new(rpc),
            cache: Mutex::new(HashMap::new()),
            global_calls: Mutex::new(VecDeque::new()),
            wallet_calls: Mutex::new(HashMap::new()),
        }
    }

    /// The host this instance's positions route reads from, for the startup
    /// log and for a failed-read log line -- never the full endpoint, which
    /// can carry a paid provider's key.
    #[must_use]
    pub fn host(&self) -> String {
        host_of(self.rpc.endpoint())
    }

    fn cached(&self, wallet: Address) -> Option<Arc<Held>> {
        let now = crate::now_unix();
        let cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache
            .get(&wallet)
            .filter(|entry| now.saturating_sub(entry.computed_at) < CACHE_TTL_SECONDS)
            .map(|entry| Arc::clone(&entry.data))
    }

    fn remember(&self, wallet: Address, data: Arc<Held>) {
        self.remember_at(wallet, data, crate::now_unix());
    }

    /// [`Self::remember`], with the recorded time given explicitly -- split
    /// out only so a test can fill the cache to [`CACHE_CAPACITY`] with
    /// entries at distinct, known ages instead of racing a real clock to tell
    /// them apart.
    fn remember_at(&self, wallet: Address, data: Arc<Held>, computed_at: u64) {
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
        cache.insert(wallet, CacheEntry { computed_at, data });
    }

    /// Reserves `want` calls against this wallet's own share and the global
    /// budget, both as rolling one-minute windows.
    ///
    /// Both windows are checked before either is charged, with both locks held
    /// (always global first, then wallets, so two reservations cannot deadlock).
    /// A request the wallet's own share refuses therefore spends nothing from
    /// the global window: one wallet firing many requests at once is refused
    /// by its own share without using up the minute for every other wallet.
    fn reserve(&self, wallet: Address, want: u32) -> bool {
        let window = Duration::from_secs(60);
        let now = Instant::now();
        let mut global = self
            .global_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut wallets = self
            .wallet_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune(&mut global, now, window);
        // Forget every wallet whose reservations have all rolled out of the
        // window. Each reservation is also charged to the global window, so
        // at most MAX_CALLS_PER_MINUTE wallets survive this: the work is
        // bounded, and so is the map, with no capacity rule of its own. A
        // refused wallet is never inserted.
        wallets.retain(|_, calls| {
            prune(calls, now, window);
            !calls.is_empty()
        });
        let none = VecDeque::new();
        let mine = wallets.get(&wallet).unwrap_or(&none);
        if !fits(&global, want, MAX_CALLS_PER_MINUTE)
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
pub(crate) async fn get(state: State<Arc<AppState>>, tenant: Tenant, uri: Uri) -> Response {
    let mut response = get_inner(state, tenant, uri).await;
    // Per-wallet data: never a shared cache's business, and never a browser's
    // disk either (item 13). Every response from this route carries this
    // header, refusals included -- a refusal body is no less specific to the
    // caller than a holdings list is, and one wrapping point here is simpler
    // to keep correct than one per return statement below.
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

async fn get_inner(State(state): State<Arc<AppState>>, tenant: Tenant, uri: Uri) -> Response {
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

    if !positions.reserve(wallet, CALLS_PER_VIEW) {
        return refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "busy",
            "Radar is rate-limiting balance reads; try again shortly",
        );
    }

    let rpc = Arc::clone(&positions.rpc);
    let state_for_read = Arc::clone(&state);
    let read =
        tokio::task::spawn_blocking(move || read_wallet(&rpc, &state_for_read, wallet)).await;
    let held = match read {
        Ok(Ok(held)) => held,
        // The read itself failed (transport, node, timeout, malformed shape,
        // or budget exhaustion): a fact about this attempt, never cached, and
        // never rendered as a partial list. Logged by kind and host only --
        // never the error's own message, which for a transport failure can
        // carry the endpoint.
        Ok(Err(why)) => {
            eprintln!(
                "radar-serve: positions read failed ({}) against {}",
                why.kind(),
                positions.host()
            );
            return refusal(
                StatusCode::BAD_GATEWAY,
                "unreadable_chain",
                "Radar could not read the chain for this wallet; this says nothing about what it holds",
            );
        }
        // The blocking task panicked or was cancelled -- the same "could not
        // look" answer, not a 500 that implies Radar's own state is broken.
        Err(_why) => {
            eprintln!("radar-serve: positions read task did not complete (join error)");
            return refusal(
                StatusCode::BAD_GATEWAY,
                "unreadable_chain",
                "Radar could not read the chain for this wallet; this says nothing about what it holds",
            );
        }
    };

    let held = Arc::new(held);
    positions.remember(wallet, Arc::clone(&held));
    Json(render(&held)).into_response()
}

/// The three calls one view costs, run to completion or not at all, summed
/// and priced before this function returns.
///
/// Runs entirely on a blocking thread, pricing included (item 3):
/// [`RpcClient`] is `ureq`, which blocks the thread it runs on for the whole
/// round trip, and [`crate::market::prices_of`]'s own doc comment gives the
/// same reason for its pricing pass -- there is no reason to hop back onto the
/// async executor between "read the chain" and "price what it said" when both
/// are synchronous store/RPC work.
fn read_wallet(rpc: &RpcClient, state: &AppState, wallet: Address) -> Result<Held, RpcError> {
    let mut budget = Budget::new(CALLS_PER_VIEW, CALLS_PER_VIEW, Duration::from_secs(20));
    let balance = rpc.balance(&mut budget, &wallet)?;
    let token = rpc.token_accounts_by_owner(&mut budget, &wallet, TOKEN_PROGRAM_ID)?;
    let token_2022 = rpc.token_accounts_by_owner(&mut budget, &wallet, TOKEN_2022_PROGRAM_ID)?;

    // The slot reported for the view is the balance read's -- the three calls
    // are not atomic against each other, and naming one of them is more
    // honest than averaging or taking whichever answered last.
    let slot = balance.slot.0;

    let mut by_mint: HashMap<Address, SummedHolding> = HashMap::new();
    for account in token.accounts {
        let entry = by_mint.entry(account.mint).or_insert(SummedHolding {
            program: "token",
            amount: 0,
            decimals: account.decimals,
        });
        entry.amount = entry.amount.checked_add(account.amount).ok_or_else(|| {
            RpcError::Malformed("a wallet's own token amounts overflowed a u128 sum".to_owned())
        })?;
    }
    for account in token_2022.accounts {
        let entry = by_mint.entry(account.mint).or_insert(SummedHolding {
            program: "token-2022",
            amount: 0,
            decimals: account.decimals,
        });
        entry.amount = entry.amount.checked_add(account.amount).ok_or_else(|| {
            RpcError::Malformed("a wallet's own token amounts overflowed a u128 sum".to_owned())
        })?;
    }

    Ok(priced(state, wallet, slot, balance.lamports, by_mint))
}

/// Attaches Radar's own price to each held mint and to SOL, from one pass over
/// the current snapshot's tape (item 3, see [`crate::market::prices_of`]).
///
/// SOL is priced only from a wrapped-SOL trade quoted in USDC or USDT, never
/// from one quoted in wSOL itself: a wSOL-quoted "price" for SOL is SOL
/// trading against itself, not a price (findings 1 and 11). When no such
/// trade is in the window, `sol_price` is `None` and `sol_price_reason`
/// carries a sentence explaining why -- never a silent blank.
fn priced(
    state: &AppState,
    wallet: Address,
    slot: u64,
    sol_lamports: u64,
    by_mint: HashMap<Address, SummedHolding>,
) -> Held {
    let wrapped_sol: Address = WRAPPED_SOL_MINT
        .parse()
        .expect("WRAPPED_SOL_MINT is a valid constant address");

    let mut mints_to_price: Vec<Address> = by_mint.keys().copied().collect();
    mints_to_price.push(wrapped_sol);
    let prices = crate::market::prices_of(state, &mints_to_price);

    let mut tokens: Vec<Holding> = Vec::new();
    for (mint, summed) in by_mint {
        if summed.amount == 0 {
            continue;
        }
        // A price whose quote this route cannot name is dropped, never sent
        // beside `quote: null`: a number with no currency is not a price.
        let (price, quote_mint) = prices
            .get(&mint)
            .filter(|p| quote_name(p.quote_mint).is_some())
            .map_or((None, None), |p| (Some(p.price), Some(p.quote_mint)));
        tokens.push(Holding {
            mint: mint.to_string(),
            program: summed.program,
            amount: summed.amount,
            decimals: summed.decimals,
            price,
            quote_mint,
        });
    }
    // Deterministic order for a caller comparing two responses, and for tests.
    tokens.sort_by(|a, b| a.mint.cmp(&b.mint));

    let sol_priced_in_stable = prices.get(&wrapped_sol).filter(|p| {
        let quote = p.quote_mint.to_string();
        quote == USDC_MINT || quote == USDT_MINT
    });
    let (sol_price, sol_quote_mint, sol_price_reason) = match sol_priced_in_stable {
        Some(p) => (Some(p.price), Some(p.quote_mint), None),
        None => (
            None,
            None,
            Some("no wrapped-SOL trade quoted in USDC or USDT was found in Radar's pricing window"),
        ),
    };

    Held {
        wallet,
        slot,
        read_at: crate::now_unix(),
        sol_lamports,
        sol_price,
        sol_quote_mint,
        sol_price_reason,
        tokens,
    }
}

/// One mint's summed token-account balance, before pricing -- named so
/// [`priced`] does not need to reach into [`read_wallet`]'s local `Summed`
/// type across a function boundary.
struct SummedHolding {
    program: &'static str,
    amount: u128,
    decimals: u8,
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
    // A displayed value is already an estimate once it is multiplied by a
    // floating-point price; losing precision below 2^52 lamports (about
    // 4.5M SOL) does not change which amount is shown.
    #[allow(clippy::cast_precision_loss)]
    let sol_value = held
        .sol_price
        .map(|price| price * (held.sol_lamports as f64 / 1_000_000_000.0));
    json!({
        "wallet": held.wallet.to_string(),
        "slot": held.slot,
        "read_at": held.read_at,
        "age_seconds": age_seconds,
        "sol": {
            "lamports": held.sol_lamports,
            "ui_amount": sol_ui,
            "price": held.sol_price,
            "quote": held.sol_quote_mint.and_then(quote_name),
            "value": sol_value,
            "priced": held.sol_price.is_some(),
            "price_reason": held.sol_price_reason,
        },
        "tokens": held.tokens.iter().map(|t| {
            // Same reasoning as `sol_value`: a displayed estimate, not an
            // accounting figure carried onward.
            #[allow(clippy::cast_precision_loss)]
            let amount_f64 = t.amount as f64 / 10f64.powi(i32::from(t.decimals));
            let value = t.price.map(|price| price * amount_f64);
            json!({
                "mint": t.mint,
                "program": t.program,
                "amount": t.amount.to_string(),
                "decimals": t.decimals,
                "ui_amount": ui_amount(t.amount, t.decimals),
                "price": t.price,
                "quote": t.quote_mint.and_then(quote_name),
                "value": value,
                "priced": t.price.is_some(),
            })
        }).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wallet_n(i: u64) -> Address {
        let mut bytes = [0u8; 32];
        bytes[24..].copy_from_slice(&i.to_be_bytes());
        Address::new(bytes)
    }

    fn minimal_held(wallet: Address) -> Held {
        Held {
            wallet,
            slot: 0,
            read_at: crate::now_unix(),
            sol_lamports: 0,
            sol_price: None,
            sol_quote_mint: None,
            sol_price_reason: None,
            tokens: Vec::new(),
        }
    }

    #[test]
    fn the_token_programs_read_are_the_ones_mainnet_runs() {
        // Every other test here answers from a fake transport that never
        // looks at the program id, so a mistyped one passed them all and
        // failed every live read: the node answered INVALID_PARAMS. These are
        // checked against `radar_pumpfun::token`'s byte constants, which the
        // decoder's own tests pin against real mainnet accounts.
        let parse = |id: &str| id.parse::<Address>().expect("a program id parses");
        assert_eq!(
            parse(radar_onchain::rpc::TOKEN_PROGRAM_ID),
            radar_pumpfun::token::SPL_TOKEN_PROGRAM
        );
        assert_eq!(
            parse(radar_onchain::rpc::TOKEN_2022_PROGRAM_ID),
            radar_pumpfun::token::TOKEN_2022_PROGRAM
        );
    }

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
        // `decimals < raw.len()`, so this exercises the `split` branch a
        // `-` -> `/` mutant lives in (finding: positions.rs ui_amount split).
        // `-`: split = 5 - 3 = 2 -> "12.345". `/`: split = 5 / 3 = 1 -> "1.2345".
        assert_eq!(ui_amount(12_345, 3), "12.345");
        assert_eq!(ui_amount(5, 3), "0.005");
    }

    #[test]
    fn a_cache_entry_exactly_at_the_ttl_boundary_is_expired_not_reused() {
        let positions = Positions::new(RpcClient::default());
        let wallet = wallet_n(1);
        let now = crate::now_unix();
        positions.remember_at(
            wallet,
            Arc::new(minimal_held(wallet)),
            now - CACHE_TTL_SECONDS,
        );
        assert!(
            positions.cached(wallet).is_none(),
            "an entry exactly CACHE_TTL_SECONDS old must already be treated as expired"
        );
        positions.remember_at(
            wallet,
            Arc::new(minimal_held(wallet)),
            now - CACHE_TTL_SECONDS + 1,
        );
        assert!(
            positions.cached(wallet).is_some(),
            "an entry one second inside the TTL must still be reused"
        );
    }

    #[test]
    fn the_cache_evicts_only_its_oldest_entry_once_full() {
        let positions = Positions::new(RpcClient::default());
        for i in 0..u64::try_from(CACHE_CAPACITY).unwrap() {
            let wallet = wallet_n(i);
            positions.remember_at(wallet, Arc::new(minimal_held(wallet)), i);
        }
        {
            let cache = positions.cache.lock().unwrap();
            assert_eq!(
                cache.len(),
                CACHE_CAPACITY,
                "the fill loop must not itself evict"
            );
        }

        let oldest = wallet_n(0);
        let survivor = wallet_n(u64::try_from(CACHE_CAPACITY).unwrap() - 1);
        let extra = wallet_n(u64::try_from(CACHE_CAPACITY).unwrap());
        positions.remember_at(
            extra,
            Arc::new(minimal_held(extra)),
            u64::try_from(CACHE_CAPACITY).unwrap(),
        );

        let cache = positions.cache.lock().unwrap();
        assert_eq!(cache.len(), CACHE_CAPACITY, "still at capacity, not grown");
        assert!(
            !cache.contains_key(&oldest),
            "the single oldest entry is evicted"
        );
        assert!(
            cache.contains_key(&survivor),
            "every other existing entry remains"
        );
        assert!(cache.contains_key(&extra), "the new entry is present");
    }

    #[test]
    fn a_second_reservation_past_the_global_cap_is_refused() {
        let positions = Positions::new(RpcClient::default());
        // Distinct wallets, so this exercises the *global* cap rather than
        // the per-wallet one added alongside it.
        for i in 0..(MAX_CALLS_PER_MINUTE / CALLS_PER_VIEW) {
            assert!(positions.reserve(wallet_n(u64::from(i)), CALLS_PER_VIEW));
        }
        assert!(
            !positions.reserve(wallet_n(9_999), CALLS_PER_VIEW),
            "the global window is spent"
        );
    }

    #[test]
    fn one_wallet_exhausting_its_own_share_does_not_starve_another() {
        let positions = Positions::new(RpcClient::default());
        let a = wallet_n(1);
        let b = wallet_n(2);
        for _ in 0..(MAX_CALLS_PER_WALLET_PER_MINUTE / CALLS_PER_VIEW) {
            assert!(positions.reserve(a, CALLS_PER_VIEW));
        }
        // Many refused attempts at once, as a wallet firing requests in
        // parallel would make: each must be refused without charging the
        // global window, or twenty of them would spend it for everyone.
        for _ in 0..(MAX_CALLS_PER_MINUTE / CALLS_PER_VIEW) {
            assert!(
                !positions.reserve(a, CALLS_PER_VIEW),
                "wallet A has spent its own share"
            );
        }
        assert!(
            positions.reserve(b, CALLS_PER_VIEW),
            "wallet B's share is untouched by wallet A's"
        );
    }

    #[test]
    fn render_computes_value_as_amount_times_price_never_by_some_other_arithmetic() {
        let held = Held {
            wallet: wallet_n(1),
            slot: 1,
            read_at: crate::now_unix(),
            sol_lamports: 1_500_000_000, // 1.5 SOL
            sol_price: Some(2.0),
            sol_quote_mint: Some(USDC_MINT.parse().unwrap()),
            sol_price_reason: None,
            tokens: vec![Holding {
                mint: "TokenMint11111111111111111111111111111111".to_owned(),
                program: "token",
                amount: 7_500, // decimals 3 -> 7.5 ui units
                decimals: 3,
                price: Some(4.0),
                quote_mint: Some(USDC_MINT.parse().unwrap()),
            }],
        };
        let rendered = render(&held);
        assert_eq!(
            rendered["sol"]["value"], 3.0,
            "1.5 SOL * price 2.0 = 3.0 exactly"
        );
        assert_eq!(rendered["sol"]["quote"], "USDC");
        assert_eq!(
            rendered["tokens"][0]["value"], 30.0,
            "7.5 units * price 4.0 = 30.0 exactly"
        );
    }

    #[test]
    fn a_sol_price_quoted_in_wsol_itself_is_never_shown_as_a_price() {
        // `prices_of` never actually returns a wSOL-quoted price for wSOL
        // (its own doc comment explains why not), but `priced`'s filter is
        // what would refuse to display one if it ever did -- this is what
        // that filter is for, exercised directly against `Held`/`render`.
        let held = Held {
            wallet: wallet_n(1),
            slot: 1,
            read_at: crate::now_unix(),
            sol_lamports: 1_000_000_000,
            sol_price: None,
            sol_quote_mint: None,
            sol_price_reason: Some(
                "no wrapped-SOL trade quoted in USDC or USDT was found in Radar's pricing window",
            ),
            tokens: Vec::new(),
        };
        let rendered = render(&held);
        assert_eq!(rendered["sol"]["priced"], false);
        assert!(rendered["sol"]["price"].is_null());
        assert!(rendered["sol"]["price_reason"].is_string());
    }

    #[test]
    fn quote_name_never_invents_a_currency_for_an_unrecognised_mint() {
        assert_eq!(quote_name(WRAPPED_SOL_MINT.parse().unwrap()), Some("SOL"));
        assert_eq!(quote_name(USDC_MINT.parse().unwrap()), Some("USDC"));
        assert_eq!(quote_name(USDT_MINT.parse().unwrap()), Some("USDT"));
        assert_eq!(quote_name(wallet_n(1)), None);
    }
}
