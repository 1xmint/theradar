// SPDX-License-Identifier: Apache-2.0
//! Asking Jupiter what a swap would get, for any admitted pair in either
//! direction.
//!
//! **This module is read-only.** It requests a quote and describes the route.
//! It does not sign, does not submit, and — as of the move to the Router API —
//! no longer produces a transaction at all. See [`Router::quote`].
//!
//! # Why the endpoint changed
//!
//! Until 2026-09-09 this module called `lite-api.jup.ag/swap/v1/{quote,swap}`
//! unauthenticated, and asked for `asLegacyTransaction=true` so
//! [`radar_signer`] could read every account inline (ADR 0003). Jupiter has
//! deprecated `lite-api` and the v1 Swap API. The replacement is the **Router**
//! at `https://api.jup.ag/swap/v2/build`, and it differs in a way that decides
//! this module's shape:
//!
//! - It has **no `asLegacyTransaction` parameter**, and it returns **no
//!   transaction**. It returns raw instructions — `swapInstruction`,
//!   `setupInstructions`, `computeBudgetInstructions`, `cleanupInstruction` —
//!   plus `addressesByLookupTableAddress`, and the caller assembles the
//!   transaction itself.
//! - Every route captured on 2026-09-09 came back with lookup tables in that
//!   map: five for SOL→USDC, and still one for USDC→SOL asked with
//!   `maxAccounts=20`.
//!
//! So the old buy-a-transaction path cannot be served here, and is not faked.
//! [`Router`]'s [`crate::pipeline::Routing`] implementation refuses, in writing,
//! rather than returning bytes the signer would reject at the end of the lane.
//! Assembling a signer-readable transaction from these instructions is a
//! separate piece of work; nothing in this module pretends it is done.
//!
//! That confirms rather than contradicts
//! [research 0021](https://github.com/hey-vera/radar/blob/main/docs/research/0021-the-signer-cannot-read-the-only-venue-that-lists-them.md):
//! Jupiter would not hand Radar a legacy transaction then either. Radar's own
//! trading venue is built directly in `radar-pumpfun` (ADR 0009), and this
//! module is a *pricing* instrument, not the route to a fill.
//!
//! # Quoting is not decoding, and it is not support
//!
//! A route here may pass through Whirlpool, Raydium, Manifest or anything else
//! Jupiter lists. Radar has a decoder for **pump.fun only** — grep the tree.
//! Reaching a venue through an aggregator's quote is not the same as
//! understanding it, and nothing downstream should read a venue label in a
//! [`Quote`] as a claim that Radar can price, verify or trade there.
//!
//! # The key
//!
//! `RADAR_JUPITER_API_KEY`, and the module refuses without it — see
//! [`Credentials`]. That refusal is entirely Radar's own: Jupiter still answers
//! **keyless** at a lower rate limit, verified on 2026-09-09, so an
//! unauthenticated fallback would silently succeed instead of failing loudly.
//! AGENTS rule 8: deny by default when config is missing.

use std::fmt;

use radar_types::{Address, Asset, MicroUsd};
use serde::Deserialize;

/// Jupiter's Router endpoint: a quote and raw swap instructions, in one GET.
pub const BUILD_API: &str = "https://api.jup.ag/swap/v2/build";

/// The environment variable holding the Jupiter API key.
pub const API_KEY_VAR: &str = "RADAR_JUPITER_API_KEY";

/// The header Jupiter authenticates with.
const API_KEY_HEADER: &str = "x-api-key";

/// How much of a refusal body to keep in an error.
///
/// Enough for Jupiter's error object — `{"error":"No routes found"}` is 27
/// bytes — and short enough that an HTML error page from something in front of
/// the API does not land whole in a log line.
const REFUSAL_BODY_LIMIT: usize = 400;

/// What Jupiter needs before this module will ask it anything.
///
/// Holds the API key and nothing else. Constructed only from a lookup that
/// actually supplies one, so a [`Router`] without a key is unrepresentable
/// rather than merely discouraged — there is no `Default`, and no constructor
/// that invents a value.
///
/// The key is a credential in a public repository's blast radius, so it never
/// reaches a formatter: see the [`fmt::Debug`] implementation below, which
/// prints a placeholder. A struct that derived `Debug` would put the key into
/// the first panic message or `dbg!` that touched a [`Router`].
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    key: String,
}

impl fmt::Debug for Credentials {
    /// Redacted, deliberately. See [`Credentials`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("key", &"<redacted>")
            .finish()
    }
}

impl Credentials {
    /// Reads the key from the process environment.
    ///
    /// `None` when it is absent or blank, which leaves the caller unable to
    /// build a [`Router`] at all.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        Self::from_vars(|k| std::env::var(k).ok())
    }

    /// Reads the key from an arbitrary lookup.
    ///
    /// `None` disables quoting entirely rather than falling back to Jupiter's
    /// keyless tier. The keyless tier answers — that is the point. A fallback
    /// would turn a missing credential into a quiet rate-limited success, and
    /// the first sign of it would be a throttled quote inside a decision.
    ///
    /// Takes a lookup rather than reading the environment directly so the rules
    /// can be tested without mutating process state, which in this edition is
    /// `unsafe` and which the workspace forbids outright. The shape is
    /// `radar_serve::x402::Config::from_vars`'s.
    #[must_use]
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let key = get(API_KEY_VAR)?;
        if key.trim().is_empty() {
            return None;
        }
        Some(Self { key })
    }

    /// The header value. Crate-private: the key does not leave this module.
    fn header_value(&self) -> &str {
        &self.key
    }
}

/// Why a route could not be built or priced.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    /// Nothing will trade this pair at this size.
    #[error("no route for {mint} at {size_lamports} lamports")]
    NoRoute {
        /// The token.
        mint: String,
        /// The size attempted.
        size_lamports: u64,
    },
    /// No API key, so nothing was asked.
    ///
    /// Distinct from [`Self::Unauthorized`]: this one never reached the
    /// network. Nothing is wrong with the key, because there is no key.
    #[error("no Jupiter API key: set RADAR_JUPITER_API_KEY")]
    NotConfigured,
    /// Jupiter rejected the key.
    ///
    /// An operator problem rather than a market condition, and worth its own
    /// variant for that reason: retrying it will never help.
    #[error("Jupiter rejected the API key ({status}): {body}")]
    Unauthorized {
        /// The status returned.
        status: u16,
        /// Jupiter's own words, truncated.
        body: String,
    },
    /// The router could not be reached, or answered with an error.
    #[error("router unavailable: {0}")]
    Unavailable(String),
    /// The router's answer did not have the shape expected.
    #[error("unreadable router response: {0}")]
    Malformed(String),
    /// The router returned a transaction the signer cannot verify.
    ///
    /// Not a transport failure — a refusal. Submitting it would mean signing
    /// bytes nothing checked.
    #[error("router returned a transaction the signer cannot read: {0}")]
    Unverifiable(String),
}

impl RouteError {
    /// Classifies a non-success HTTP answer using Jupiter's own words.
    ///
    /// Split out from the request so it can be tested against the captured
    /// refusal bodies in `crates/radar-exec/fixtures/` rather than against a
    /// guess about them. Both were captured on 2026-09-09:
    /// `400 {"error":"No routes found"}` and
    /// `401 {"code":401,"message":"Unauthorized"}` — two different key names
    /// for the message, which is why both are read.
    ///
    /// `mint` and `amount` describe what was asked, so a [`Self::NoRoute`]
    /// names it.
    #[must_use]
    pub fn from_status(status: u16, body: &str, mint: &str, amount: u64) -> Self {
        let said = ApiError::message(body).unwrap_or_else(|| truncated(body));
        match status {
            401 | 403 => Self::Unauthorized { status, body: said },
            429 => Self::Unavailable(format!(
                "rate limited by Jupiter (429): {said}. The free tier is one \
                 request per second."
            )),
            _ if said.to_lowercase().contains("no route") => Self::NoRoute {
                mint: mint.to_owned(),
                size_lamports: amount,
            },
            _ => Self::Unavailable(format!("Jupiter answered {status}: {said}")),
        }
    }
}

/// Jupiter's refusal body, under either of the two key names it uses.
#[derive(Debug, Deserialize)]
struct ApiError {
    error: Option<String>,
    message: Option<String>,
}

impl ApiError {
    /// The refusal text, or `None` if the body is not one of these.
    fn message(body: &str) -> Option<String> {
        let parsed: Self = serde_json::from_str(body).ok()?;
        parsed.error.or(parsed.message).map(|m| truncated(&m))
    }
}

/// Clips a body to [`REFUSAL_BODY_LIMIT`] on a character boundary.
fn truncated(body: &str) -> String {
    let body = body.trim();
    if body.len() <= REFUSAL_BODY_LIMIT {
        return body.to_owned();
    }
    let end = (0..=REFUSAL_BODY_LIMIT)
        .rev()
        .find(|&i| body.is_char_boundary(i))
        .unwrap_or(0);
    format!("{}…", &body[..end])
}

/// A transaction ready to be sent to the signer.
///
/// Nothing in this module produces one any more — see the module documentation.
/// It remains the currency of [`crate::pipeline`], whose ordering is written
/// against it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// The unsigned transaction, base64.
    pub transaction: String,
    /// What the router expects out, in the output mint's base units.
    pub expected_out: u64,
    /// The price impact the router reported, in basis points.
    pub impact_bps: u32,
    /// The venues the route passes through, for the audit record.
    pub venues: Vec<String>,
}

/// What to ask Jupiter about.
///
/// Both sides are an [`Asset`], so every admitted pair is expressible in either
/// direction: SOL in and USDC out, USDC in and an SPL mint out, one SPL mint in
/// and another out. The predecessor of this type was
/// `build_buy(mint, wallet, size_lamports)`, which could say only one thing —
/// SOL in, one named mint out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteRequest {
    /// What is being spent.
    pub input: Asset,
    /// What is being received.
    pub output: Asset,
    /// How much of `input`, in its own base units.
    pub amount: u64,
    /// The account the swap would be built for.
    ///
    /// Required by the Router — it was `userPublicKey` on the v1 API and is
    /// `taker` here — because the response carries account-specific setup
    /// instructions. **Nothing is signed for it and nothing is sent**; it
    /// selects a route, and no key for it is needed or held.
    pub taker: Address,
}

impl QuoteRequest {
    /// A request for `amount` base units of `input` into `output`.
    #[must_use]
    pub const fn new(input: Asset, output: Asset, amount: u64, taker: Address) -> Self {
        Self {
            input,
            output,
            amount,
            taker,
        }
    }

    /// The mint Jupiter is asked about for an asset.
    ///
    /// Native SOL has no mint account, and Jupiter names it by the wrapped-SOL
    /// mint, wrapping and unwrapping around the swap. So both [`Asset::Sol`]
    /// and [`Asset::WrappedSol`] go on the wire as `So111…112` — while
    /// remaining distinct balances, which is the distinction [`Asset`] exists
    /// to keep. A [`Quote`] carries the [`Asset`] that was asked for, not the
    /// mint it was asked under, so nothing downstream loses it.
    #[must_use]
    pub fn wire_mint(asset: Asset) -> Address {
        asset.mint().unwrap_or(Asset::WRAPPED_SOL_MINT)
    }
}

/// What Jupiter says a swap would get, and what it would pass through.
///
/// A price and a description of a route. Not a transaction, not an
/// authorisation, and not a claim that Radar can execute it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    /// What would be spent.
    pub input: Asset,
    /// What would be received.
    pub output: Asset,
    /// How much of `input` this quote is for, in its base units.
    pub in_amount: u64,
    /// What Jupiter expects out, in `output`'s base units.
    pub out_amount: u64,
    /// The floor Jupiter would enforce at the configured slippage, in
    /// `output`'s base units — its `otherAmountThreshold`.
    ///
    /// `None` when Jupiter did not say. Not `0`: a floor of zero is a
    /// statement, and "it did not say" is not that statement. AGENTS rule 9.
    pub worst_out: Option<u64>,
    /// The price impact Jupiter reported, in basis points.
    ///
    /// [`u32::MAX`] when it was absent or unreadable, never `0`.
    pub impact_bps: u32,
    /// The venue labels the route passes through, in order.
    ///
    /// Labels, not support. See the module documentation.
    pub venues: Vec<String>,
    /// How many address lookup tables the route's instructions reference.
    ///
    /// Route inspection rather than decoration: `radar_signer` refuses a
    /// transaction that names accounts through a lookup table, so a non-zero
    /// count is the measurement that says this route could not be signed even
    /// if the transaction were assembled. See [`Self::signer_could_read`].
    pub lookup_tables: usize,
    /// Jupiter's `swapMode`, `ExactIn` on every capture so far.
    ///
    /// `None` when absent, rather than a guessed default: the mode decides
    /// which side of the quote is the fixed one.
    pub swap_mode: Option<String>,
}

impl Quote {
    /// Whether a transaction assembled from this route could be signed.
    ///
    /// False when the route uses address lookup tables, which every route
    /// captured on 2026-09-09 did. ADR 0003 and `radar_signer::tx::decode`.
    #[must_use]
    pub const fn signer_could_read(&self) -> bool {
        self.lookup_tables == 0
    }

    /// Reads Jupiter's `/build` body into a quote.
    ///
    /// Separate from the request so the parser can be exercised against the
    /// captured responses in `crates/radar-exec/fixtures/` — the wire, not a
    /// description of it — instead of against a network in a test.
    ///
    /// # Errors
    ///
    /// [`RouteError::Malformed`] if the body is not a `/build` response, if the
    /// amounts do not parse, or if Jupiter answered about a **different pair**
    /// from the one asked about. That last check is not a formality: the
    /// response echoes `inputMint` and `outputMint`, and a quote filed against
    /// the wrong market is a price nothing can act on and nothing would notice.
    ///
    /// [`RouteError::NoRoute`] if the route returns nothing.
    pub fn from_response(body: &str, request: &QuoteRequest) -> Result<Self, RouteError> {
        let parsed: BuildResponse =
            serde_json::from_str(body).map_err(|e| RouteError::Malformed(e.to_string()))?;

        let asked_in = QuoteRequest::wire_mint(request.input).to_string();
        let asked_out = QuoteRequest::wire_mint(request.output).to_string();
        if parsed.input_mint != asked_in || parsed.output_mint != asked_out {
            return Err(RouteError::Malformed(format!(
                "asked about {asked_in} -> {asked_out}, answered about {} -> {}",
                parsed.input_mint, parsed.output_mint
            )));
        }

        let out_amount = parse_units(&parsed.out_amount, "outAmount")?;
        if out_amount == 0 {
            return Err(RouteError::NoRoute {
                mint: asked_out,
                size_lamports: request.amount,
            });
        }

        Ok(Self {
            input: request.input,
            output: request.output,
            in_amount: parse_units(&parsed.in_amount, "inAmount")?,
            out_amount,
            worst_out: parsed
                .other_amount_threshold
                .as_deref()
                .and_then(|v| v.parse().ok()),
            impact_bps: impact_to_bps(parsed.price_impact_pct.as_deref()),
            venues: parsed
                .route_plan
                .iter()
                .filter_map(|s| s.swap_info.label.clone())
                .collect(),
            lookup_tables: parsed
                .addresses_by_lookup_table_address
                .map_or(0, |m| m.len()),
            swap_mode: parsed.swap_mode,
        })
    }
}

/// Parses a base-unit string, naming the field that failed.
fn parse_units(raw: &str, field: &str) -> Result<u64, RouteError> {
    raw.parse()
        .map_err(|_| RouteError::Malformed(format!("bad {field}: {raw}")))
}

/// Jupiter's `/build` response, in the parts a quote is made of.
///
/// The instruction fields — `swapInstruction`, `setupInstructions`,
/// `computeBudgetInstructions`, `cleanupInstruction`, `otherInstructions`,
/// `tipInstruction`, `blockhashWithMetadata` — are deliberately unmodelled.
/// This module does not assemble a transaction, and a type that read those
/// bytes would suggest it could.
#[derive(Debug, Deserialize)]
struct BuildResponse {
    #[serde(rename = "inputMint")]
    input_mint: String,
    #[serde(rename = "outputMint")]
    output_mint: String,
    #[serde(rename = "inAmount")]
    in_amount: String,
    #[serde(rename = "outAmount")]
    out_amount: String,
    #[serde(rename = "otherAmountThreshold")]
    other_amount_threshold: Option<String>,
    #[serde(rename = "swapMode")]
    swap_mode: Option<String>,
    #[serde(rename = "priceImpactPct")]
    price_impact_pct: Option<String>,
    #[serde(rename = "routePlan", default)]
    route_plan: Vec<RouteStep>,
    /// A map of lookup table address to the accounts it names, or `null`.
    #[serde(rename = "addressesByLookupTableAddress")]
    addresses_by_lookup_table_address: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct RouteStep {
    #[serde(rename = "swapInfo")]
    swap_info: SwapInfo,
}

#[derive(Debug, Deserialize)]
struct SwapInfo {
    label: Option<String>,
}

/// Asks Jupiter's Router what a swap would get.
///
/// Unconstructible without [`Credentials`], which are unconstructible without
/// the key. There is no `Default` and no keyless constructor, so "quote without
/// a key" is a program that does not compile rather than a call that quietly
/// works at Jupiter's keyless rate limit.
#[derive(Debug)]
pub struct Router {
    endpoint: String,
    agent: ureq::Agent,
    credentials: Credentials,
    slippage_bps: u32,
}

impl Router {
    /// A router against the live Router endpoint.
    #[must_use]
    pub fn new(credentials: Credentials) -> Self {
        Self::with_endpoint(BUILD_API, credentials)
    }

    /// A router against a given endpoint, for diagnostics and tests.
    #[must_use]
    pub fn with_endpoint(endpoint: impl Into<String>, credentials: Credentials) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(15)))
            // So a 4xx arrives as a response with its body intact. Jupiter puts
            // the only useful distinction there -- `{"error":"No routes found"}`
            // is a market condition, `{"message":"Unauthorized"}` is an
            // operator one -- and `Error::StatusCode` carries the number alone.
            .http_status_as_error(false)
            .build();
        Self {
            endpoint: endpoint.into(),
            agent: config.into(),
            credentials,
            // Tight. A wide setting is what makes a swap worth sandwiching, and
            // memecoin swaps are still being sandwiched several times a minute.
            slippage_bps: 100,
        }
    }

    /// A router from the environment, or `None` if no key is set.
    ///
    /// The whole deny-by-default rule in one function: no key, no router, and
    /// therefore no quote.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        Self::from_vars(|k| std::env::var(k).ok())
    }

    /// The same, from a supplied reader rather than the process environment.
    ///
    /// Split for the reason `Credentials::from_vars` is split, and it is the
    /// same reason: `std::env::set_var` is `unsafe` in edition 2024 against a
    /// workspace that forbids unsafe, and these tests run in parallel threads
    /// where process-wide state races unrelated ones. So the decision lives
    /// here, where a test can make it, and `from_env` is left holding only the
    /// reading.
    #[must_use]
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        Credentials::from_vars(get).map(Self::new)
    }

    /// Slippage tolerance, in basis points.
    #[must_use]
    pub const fn with_slippage_bps(mut self, bps: u32) -> Self {
        self.slippage_bps = bps;
        self
    }

    /// Asks what `request` would get.
    ///
    /// One GET. Nothing is signed, nothing is submitted, and the instructions
    /// in the answer are read for their lookup-table count and then dropped.
    ///
    /// # Errors
    ///
    /// [`RouteError::NoRoute`] when Jupiter will not route the pair,
    /// [`RouteError::Unauthorized`] when it rejects the key,
    /// [`RouteError::Unavailable`] for transport and rate limits, and
    /// [`RouteError::Malformed`] when the answer is not one this module reads.
    pub fn quote(&self, request: &QuoteRequest) -> Result<Quote, RouteError> {
        let input = QuoteRequest::wire_mint(request.input).to_string();
        let output = QuoteRequest::wire_mint(request.output).to_string();

        let response = self
            .agent
            .get(&self.endpoint)
            .header(API_KEY_HEADER, self.credentials.header_value())
            .query("inputMint", &input)
            .query("outputMint", &output)
            .query("amount", request.amount.to_string())
            .query("taker", request.taker.to_string())
            .query("slippageBps", self.slippage_bps.to_string())
            .call();

        let mut response = match response {
            Ok(r) => r,
            // Still matched: `http_status_as_error(false)` is configuration, and
            // an agent built without it would otherwise report a refusal as
            // transport.
            Err(ureq::Error::StatusCode(status)) => {
                return Err(RouteError::from_status(status, "", &output, request.amount));
            }
            Err(e) => return Err(RouteError::Unavailable(e.to_string())),
        };

        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| RouteError::Unavailable(e.to_string()))?;

        if status >= 400 {
            return Err(RouteError::from_status(
                status,
                &body,
                &output,
                request.amount,
            ));
        }
        Quote::from_response(&body, request)
    }
}

/// Converts a percentage string to basis points, treating unreadable as maximal.
#[must_use]
pub fn impact_to_bps(pct: Option<&str>) -> u32 {
    pct.map_or(u32::MAX, |raw| {
        raw.parse::<f64>().map_or(u32::MAX, |fraction| {
            let bps = (fraction.abs() * 10_000.0).round();
            if bps.is_finite() && bps <= f64::from(u32::MAX) {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "bounded and non-negative by the checks above"
                )]
                let bps = bps as u32;
                bps
            } else {
                u32::MAX
            }
        })
    })
}

/// Checks a transaction is one the signer will accept.
///
/// Kept, and still correct, though nothing in this module produces a
/// transaction to hand it any more. It is the cheap early form of the check
/// `radar_signer` makes, so a route that cannot be signed is discarded before a
/// decision is built on it. The signer still checks — this is an early exit,
/// never a substitute.
///
/// # Errors
///
/// Returns [`RouteError::Unverifiable`] if the bytes do not decode, or use
/// address lookup tables.
pub fn verify_shape(transaction_base64: &str) -> Result<(), RouteError> {
    let bytes = radar_types::b64::decode(transaction_base64)
        .ok_or_else(|| RouteError::Unverifiable("not base64".to_owned()))?;
    radar_signer::decode(&bytes).map_err(|e| RouteError::Unverifiable(e.to_string()))?;
    Ok(())
}

/// The lamports a notional is worth at a given SOL price.
#[must_use]
pub fn notional_to_lamports(notional: MicroUsd, sol_price: MicroUsd) -> u64 {
    if sol_price.get() == 0 {
        return 0;
    }
    let product = u128::from(notional.get()) * 1_000_000_000u128;
    u64::try_from(product / u128::from(sol_price.get())).unwrap_or(u64::MAX)
}

/// Why this router cannot build a transaction, in the words the caller sees.
const CANNOT_BUILD: &str = "Jupiter's Router returns raw instructions and no transaction, has no \
     asLegacyTransaction parameter, and routed every pair captured on \
     2026-09-09 through address lookup tables. Assembling a legacy \
     transaction from those instructions is not built. Radar's own venue is \
     radar-pumpfun (ADR 0009); this router prices, it does not execute.";

/// The router, as the pipeline sees it — and a refusal.
///
/// **This implementation always fails, and that is the honest answer**, not a
/// stub and not an oversight. The endpoint that returned a signer-readable
/// transaction is deprecated; its replacement returns instructions and lookup
/// tables. Until something assembles a legacy transaction from those, there is
/// no route from here to a signature, and a caller learns that here — at the
/// routing stage, with nothing at stake — rather than at the signer, with a
/// decision already resting on it.
///
/// It is kept rather than deleted because deleting it would leave
/// [`crate::pipeline::Routing`] with no production implementation again and
/// nothing saying so. That is the shape LEARNINGS 10 records. A refusal that
/// explains itself is visible; an absent implementation is not.
impl crate::pipeline::Routing for Router {
    fn build_buy(&self, _: &Address, _: &Address, _: u64) -> Result<Route, RouteError> {
        Err(RouteError::Unverifiable(CANNOT_BUILD.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::Routing;

    /// Not a credential. A literal here is a value Jupiter would reject, and it
    /// is never sent anywhere in these tests.
    const NOT_A_KEY: &str = "test-key-that-authorises-nothing";

    fn credentials() -> Credentials {
        Credentials::from_vars(|k| (k == API_KEY_VAR).then(|| NOT_A_KEY.to_owned()))
            .expect("a supplied key makes credentials")
    }

    #[test]
    fn impact_parses_the_shapes_jupiter_returns() {
        assert_eq!(impact_to_bps(Some("0")), 0);
        assert_eq!(impact_to_bps(Some("0.05")), 500);
        assert_eq!(impact_to_bps(Some("-0.01")), 100);
        // The long-precision decimal the Router actually returned on
        // 2026-09-09, rather than the tidy one a test would invent.
        assert_eq!(impact_to_bps(Some("0.0005106285315228447346440306")), 5);
    }

    #[test]
    fn an_unreadable_impact_is_maximal_rather_than_zero() {
        assert_eq!(impact_to_bps(None), u32::MAX);
        assert_eq!(impact_to_bps(Some("nonsense")), u32::MAX);
    }

    #[test]
    fn a_legacy_transaction_passes_the_shape_check() {
        let mut bytes = vec![1u8];
        bytes.extend_from_slice(&[0u8; 64]);
        bytes.extend_from_slice(&[1, 0, 0, 2]);
        bytes.extend_from_slice(&[0u8; 32]);
        bytes.extend_from_slice(&[1u8; 32]);
        bytes.extend_from_slice(&[0xAA; 32]);
        bytes.push(0);
        assert_eq!(verify_shape(&radar_types::b64::encode(&bytes)), Ok(()));
    }

    #[test]
    fn a_transaction_with_lookup_tables_is_rejected_before_a_decision_rests_on_it() {
        // Why ADR 0003 exists. Discovering it at the signer would mean a
        // decision was already built on a route that can never be executed.
        let mut bytes = vec![0u8, 0x80, 1, 0, 0, 2];
        bytes.extend_from_slice(&[0u8; 32]);
        bytes.extend_from_slice(&[1u8; 32]);
        bytes.extend_from_slice(&[0xAA; 32]);
        bytes.push(0);
        bytes.push(1);
        let err = verify_shape(&radar_types::b64::encode(&bytes)).expect_err("must refuse");
        assert!(
            matches!(err, RouteError::Unverifiable(ref m) if m.contains("lookup")),
            "got {err}"
        );
    }

    #[test]
    fn garbage_from_the_router_is_a_refusal_not_a_panic() {
        assert!(verify_shape("!!!!").is_err());
        assert!(verify_shape("QUJD").is_err());
        assert!(verify_shape("").is_err());
    }

    #[test]
    fn notional_converts_to_lamports_in_integers() {
        let sol = MicroUsd::from_dollars(200.0);
        assert_eq!(
            notional_to_lamports(MicroUsd::from_dollars(200.0), sol),
            1_000_000_000
        );
        assert_eq!(
            notional_to_lamports(MicroUsd::from_dollars(2.0), sol),
            10_000_000
        );
    }

    #[test]
    fn an_unknown_price_sizes_at_nothing_rather_than_at_everything() {
        // Dividing by an absent price is the arithmetic that turns a missing
        // input into an unbounded position.
        assert_eq!(
            notional_to_lamports(MicroUsd::from_dollars(100.0), MicroUsd::ZERO),
            0
        );
    }

    #[test]
    fn a_router_is_built_from_a_key_and_refused_without_one() {
        // The deny-by-default rule, at the level that decides it. `from_env`
        // holds only the reading of the process environment; this holds the
        // decision, and a router that appeared without a key would be a quote
        // Radar could not have paid for.
        assert!(
            Router::from_vars(|k| (k == API_KEY_VAR).then(|| NOT_A_KEY.to_owned())).is_some(),
            "a present key builds a router"
        );
        assert!(
            Router::from_vars(|_| None).is_none(),
            "no key, no router, and therefore no quote"
        );
        assert!(
            Router::from_vars(|_| Some(String::new())).is_none(),
            "a blank key is an absent key, not a key of length zero"
        );
    }

    #[test]
    fn an_absent_or_blank_key_yields_no_credentials_and_so_no_router() {
        assert_eq!(Credentials::from_vars(|_| None), None);
        assert_eq!(Credentials::from_vars(|_| Some(String::new())), None);
        assert_eq!(Credentials::from_vars(|_| Some("   ".to_owned())), None);
        // And the variable's name is the one documented, not a near miss.
        assert_eq!(
            Credentials::from_vars(|k| (k == "JUPITER_API_KEY").then(|| NOT_A_KEY.to_owned())),
            None
        );
    }

    #[test]
    fn the_key_never_reaches_a_formatter() {
        // A derived Debug would put the key into the first panic message that
        // touched a Router. This repository is public.
        let rendered = format!("{:?}", credentials());
        assert!(!rendered.contains(NOT_A_KEY), "leaked: {rendered}");
        assert!(rendered.contains("redacted"), "got {rendered}");

        let router = Router::new(credentials());
        let rendered = format!("{router:?}");
        assert!(!rendered.contains(NOT_A_KEY), "leaked: {rendered}");
    }

    #[test]
    fn native_sol_is_asked_about_under_the_wrapped_mint_but_stays_native() {
        // Jupiter has no name for a lamport balance. Both go on the wire as
        // So111...112; only the Asset that was asked for survives into a Quote.
        assert_eq!(
            QuoteRequest::wire_mint(Asset::Sol),
            Asset::WRAPPED_SOL_MINT,
            "native SOL must be asked about under the wrapped mint"
        );
        assert_eq!(
            QuoteRequest::wire_mint(Asset::WrappedSol),
            Asset::WRAPPED_SOL_MINT
        );
        assert_eq!(QuoteRequest::wire_mint(Asset::Usdc), Asset::USDC_MINT);
        let other = Address::new([0x22; 32]);
        assert_eq!(QuoteRequest::wire_mint(Asset::spl(other)), other);
        assert_eq!(QuoteRequest::wire_mint(Asset::token_2022(other)), other);
    }

    #[test]
    fn the_pipeline_is_told_it_cannot_have_a_transaction_and_why() {
        // Not "no route" and not a panic: a refusal carrying the reason, so an
        // operator reading a journal line learns what to fix rather than
        // concluding the market is thin.
        let router = Router::new(credentials());
        let err = Routing::build_buy(
            &router,
            &Address::new([0x22; 32]),
            &Address::new([0x33; 32]),
            1_000_000,
        )
        .expect_err("the Router API cannot supply a signable transaction");
        assert!(
            matches!(err, RouteError::Unverifiable(ref m) if m.contains("lookup tables")),
            "the refusal must name the reason, got {err}"
        );
    }

    #[test]
    fn a_long_refusal_body_is_clipped_without_splitting_a_character() {
        let long = "é".repeat(REFUSAL_BODY_LIMIT);
        let clipped = truncated(&long);
        assert!(clipped.len() <= REFUSAL_BODY_LIMIT + 4, "{}", clipped.len());
        assert!(clipped.ends_with('…'));
    }
}
