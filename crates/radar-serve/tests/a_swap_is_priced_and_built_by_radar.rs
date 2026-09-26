// SPDX-License-Identifier: Apache-2.0
//! `GET /v1/market/quote` and `POST /v1/customer/swap`. Plan 0013 Phase D,
//! ADR 0024.
//!
//! Through the real router, the real rate limiter, and a loopback socket
//! standing in for Jupiter -- the same shape `the_key_reaches_jupiter.rs`
//! uses in `radar-exec` itself, reused here because these routes are the
//! part of that contract an HTTP caller actually sees. A fake
//! [`radar_onchain::rpc::Transport`] stands in for the chain, exactly as
//! `a_wallets_positions_are_read_and_priced_by_radar.rs` does for
//! `/v1/customer/positions`.
//!
//! No test here ever reaches a real network. Every refusal this file
//! exercises is checked against the loopback server's own request count,
//! not just the HTTP status -- a rate limit that let the request through
//! and then discarded the answer would still read as "busy" from the
//! response alone.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use radar_customer::session::issue;
use radar_exec::route::{API_KEY_VAR, Credentials, Router};
use radar_instruments::Registry;
use radar_onchain::rpc::{RpcClient, Transport};
use radar_serve::trade::Trading;
use radar_serve::{AppState, app};
use radar_store::Reader;
use radar_types::Address;
use serde_json::Value;
use tower::ServiceExt;

const SALT: [u8; 32] = [9u8; 32];
const TEST_KEY: &str = "test-key-that-authorises-nothing";
const CRLF_CRLF: &[u8] = b"\r\n\r\n";

fn now() -> u64 {
    radar_serve::now_unix()
}

fn wallet(byte: u8) -> Address {
    Address::new([byte; 32])
}

fn session(who: &Address) -> String {
    issue(who, &SALT, now()).expect("a session")
}

fn credentials() -> Credentials {
    Credentials::from_vars(|k| (k == API_KEY_VAR).then(|| TEST_KEY.to_owned()))
        .expect("a supplied key makes credentials")
}

fn fixture(name: &str) -> String {
    // radar-exec's own captured Jupiter bodies -- real wire data, reused
    // rather than hand-written so a swap actually assembles from it.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("radar-exec")
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// The address every captured fixture's `setupInstructions`/`swapInstruction`
/// names as a signer -- the wallet the capture was taken under.
const FIXTURE_TAKER: &str = "CjfBjFVBs6QRvRTpMdKTBxZ7PZuJvHXWQKGRvR7wFbdz";

/// `fixture(name)`, rewritten so its embedded signer is `taker` instead of
/// [`FIXTURE_TAKER`].
///
/// `radar-exec`'s `assemble` refuses (ADR 0024) to compile a transaction that
/// names a signer other than the caller's own wallet -- exactly what a
/// captured fixture used unmodified for a synthetic test wallet would be.
/// Substituting the string stands in for a second real capture taken under a
/// different taker, without hand-writing one.
fn fixture_for(name: &str, taker: &Address) -> String {
    fixture(name).replace(FIXTURE_TAKER, &taker.to_string())
}

/// A loopback server that answers up to `limit` requests, in order, and
/// counts how many it actually saw.
///
/// The count is the point: a rate limit that refuses a caller must never
/// have reached this server for that call, and the count is how a test
/// proves that rather than trusting the response body alone.
fn jupiter(status: u16, reason: &str, body: &str, limit: usize) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("an address").port();
    let seen = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&seen);
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    std::thread::spawn(move || {
        for _ in 0..limit {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut buf = vec![0_u8; 8192];
            let mut n = 0;
            while n < buf.len() {
                let Ok(read) = stream.read(&mut buf[n..]) else {
                    break;
                };
                if read == 0 {
                    break;
                }
                n += read;
                if buf[..n].windows(4).any(|w| w == CRLF_CRLF) {
                    break;
                }
            }
            counted.fetch_add(1, Ordering::SeqCst);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (format!("http://127.0.0.1:{port}/build"), seen)
}

/// A loopback server that answers each connection with the next body from
/// `bodies`, in order, and panics if asked for more connections than it was
/// given bodies -- for tests where two different callers must each see a
/// response built for *them* (a fixture rewritten for their own wallet as
/// signer), not the same body twice.
fn jupiter_sequence(status: u16, reason: &str, bodies: Vec<String>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("an address").port();
    let seen = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&seen);
    let reason = reason.to_owned();

    std::thread::spawn(move || {
        for body in bodies {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut buf = vec![0_u8; 8192];
            let mut n = 0;
            while n < buf.len() {
                let Ok(read) = stream.read(&mut buf[n..]) else {
                    break;
                };
                if read == 0 {
                    break;
                }
                n += read;
                if buf[..n].windows(4).any(|w| w == CRLF_CRLF) {
                    break;
                }
            }
            counted.fetch_add(1, Ordering::SeqCst);
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (format!("http://127.0.0.1:{port}/build"), seen)
}

/// A [`Transport`] that answers a fixed queue, in order, and panics if asked
/// for more than it was given -- the same fake `positions`'s own test uses,
/// reproduced here because it is not exported.
struct FakeChain(Mutex<Vec<String>>);

impl FakeChain {
    fn boxed(responses: Vec<String>) -> Box<dyn Transport> {
        Box::new(Self(Mutex::new(responses.into_iter().rev().collect())))
    }
}

impl Transport for FakeChain {
    fn post(&self, _endpoint: &str, _body: String) -> Result<String, String> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop()
            .unwrap_or_else(|| {
                panic!("the chain transport was called after its canned responses were exhausted")
            }))
    }
}

/// One `getAccountInfo` answer for a Mint account whose `decimals` byte (SPL
/// Token layout offset 44) is `decimals` and everything else is zero --
/// enough for [`Trading`]'s own read, which only ever looks at that byte.
fn mint_account_json(decimals: u8) -> String {
    let mut data = vec![0_u8; 82];
    data[44] = decimals;
    let encoded = radar_types::b64::encode(&data);
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":{{"context":{{"slot":1}},"value":{{"data":["{encoded}","base64"]}}}}}}"#
    )
}

/// A [`Trading`] pointed at a loopback Jupiter double and a faked chain --
/// no real network reachable from either half.
fn trading(endpoint: String, decimals_responses: Vec<String>) -> Trading {
    Trading::new(
        Router::with_endpoint(endpoint, credentials()),
        RpcClient::with_transport("http://test.invalid", FakeChain::boxed(decimals_responses)),
    )
}

/// An instance as production runs it apart from Access, same as every other
/// fixture in this crate -- an operator-identity gate unrelated to what
/// these routes check.
fn router(trading: Option<Trading>) -> axum::Router {
    app(Arc::new(AppState {
        admission: radar_serve::admission::Admission::Open,
        shares: radar_serve::share::Shares::new(radar_serve::share::Allowance::per_day(100)),
        customer_salt: SALT.to_vec(),
        registry: Registry::new(),
        store: Reader::open(std::env::temp_dir().join("radar-trade-test")),
        x402: None,
        chat: None,
        access: radar_serve::access::Mode::Off,
        keys: radar_serve::access::KeyCache::new(),
        customer: radar_serve::customer::Mode::Off,
        customer_keys: radar_serve::customer::KeyCache::preloaded(radar_serve::customer::Keys(
            Vec::new(),
        )),
        privy: None,
        linker: radar_serve::link::Linker::new(),
        scoreboard: radar_serve::cache::Cache::new(),
        token: radar_serve::cache::Cache::new(),
        challenges: None,
        market: radar_serve::market::Market::new(),
        market_snapshot: radar_serve::market::SnapshotCache::new(),
        customers: None,
        positions: None,
        trading,
        ticker: radar_serve::ticker::Ticker::new(),
        market_ticker: radar_serve::ticker::Ticker::new(),
        market_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(
            radar_serve::MARKET_EVENTS_MAX_CONNECTIONS,
        )),
        market_visitors: std::sync::Arc::default(),
    }))
}

async fn quote(
    router: &axum::Router,
    query: &str,
    visitor: Option<&str>,
) -> (StatusCode, Value, Option<String>) {
    let mut request = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/market/quote?{query}"));
    if let Some(ip) = visitor {
        request = request.header("cf-connecting-ip", ip);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).expect("a well-formed request"))
        .await
        .expect("the router answers");
    let status = response.status();
    let cache_control = response
        .headers()
        .get(axum::http::header::CACHE_CONTROL)
        .map(|v| v.to_str().expect("ascii header").to_owned());
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        cache_control,
    )
}

async fn swap(
    router: &axum::Router,
    body: Value,
    bearer: &str,
) -> (StatusCode, Value, Option<String>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/customer/swap")
                .header("authorization", format!("Bearer {bearer}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("a well-formed request"),
        )
        .await
        .expect("the router answers");
    let status = response.status();
    let cache_control = response
        .headers()
        .get(axum::http::header::CACHE_CONTROL)
        .map(|v| v.to_str().expect("ascii header").to_owned());
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        cache_control,
    )
}

/// GETs `path` with no body and returns its status and parsed JSON body --
/// for `/health`, the one plain unauthenticated `GET` this file exercises
/// outside the two trade routes.
async fn get_json(router: &axum::Router, path: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("a well-formed request"),
        )
        .await
        .expect("the router answers");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// The account-keys prefix of an unsigned v0 transaction: one empty
/// signature slot, the version byte, the three header bytes, then the
/// static key array whose first entry is always the fee payer -- the same
/// layout `radar-exec`'s own `assemble` tests decode independently of its
/// encoder, reproduced minimally here for the one field an HTTP caller can
/// check without a full Solana SDK dependency.
fn shortvec(wire: &[u8], pos: &mut usize) -> usize {
    let mut value = 0usize;
    let mut shift = 0;
    loop {
        let byte = wire[*pos];
        *pos += 1;
        value |= usize::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    value
}

/// The fee payer (first static account key) and the message header's
/// `num_required_signatures` byte, decoded independently of `assemble`'s own
/// encoder -- the same cross-check `radar-exec`'s own suite runs one layer
/// down, reproduced here for the two fields an HTTP caller can check without
/// a full Solana SDK dependency.
fn fee_payer_of(wire: &[u8]) -> (Address, u8) {
    let mut pos = 0usize;
    let sig_count = shortvec(wire, &mut pos);
    assert_eq!(
        sig_count, 1,
        "exactly one signature slot, for the fee payer"
    );
    pos += 64; // the zeroed, unsigned slot
    pos += 1; // version byte (0x80: versioned, v0)
    let num_required_signatures = wire[pos];
    pos += 3; // num_required_signatures, num_readonly_signed, num_readonly_unsigned
    let _key_count = shortvec(wire, &mut pos);
    let bytes: [u8; 32] = wire[pos..pos + 32].try_into().expect("32 bytes");
    (Address::new(bytes), num_required_signatures)
}

const BUY_QUERY: &str =
    "mint=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v&side=buy&amount=100000000";

/// Off is the shipped state, and both routes must say so without ever
/// reaching Jupiter -- no fake server exists in this test at all, so any
/// attempt to dial one would fail loudly rather than quietly succeed.
#[tokio::test(flavor = "multi_thread")]
async fn trading_off_refuses_both_routes_without_calling_jupiter() {
    let router = router(None);

    let (status, body, _cache) = quote(&router, BUY_QUERY, None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["reason"], "trading_off", "{body}");

    let (status, body, _cache) = swap(
        &router,
        serde_json::json!({"mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", "side": "buy", "amount": "100000000"}),
        &session(&wallet(1)),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["reason"], "trading_off", "{body}");
}

/// A cap, never a clamp: over the 500bps ceiling is refused before Jupiter
/// is ever asked, regardless of what the caller would have accepted.
#[tokio::test(flavor = "multi_thread")]
async fn slippage_over_the_cap_is_refused_before_any_jupiter_call() {
    let (endpoint, seen) = jupiter(200, "OK", &fixture("jupiter-build-sol-usdc.json"), 1);
    let router = router(Some(trading(endpoint, vec![mint_account_json(6)])));

    let (status, body, _cache) =
        quote(&router, &format!("{BUY_QUERY}&slippage_bps=501"), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["reason"], "slippage_too_wide", "{body}");
    assert_eq!(
        seen.load(Ordering::SeqCst),
        0,
        "Jupiter must not have been asked"
    );
}

/// A thin market reads as `no_route`, not as a broken parser -- the same
/// property `radar-exec`'s own suite checks one layer down, seen here at the
/// HTTP boundary this crate actually exposes.
#[tokio::test(flavor = "multi_thread")]
async fn an_unroutable_pair_becomes_a_404_no_route_refusal() {
    let (endpoint, seen) = jupiter(
        400,
        "Bad Request",
        &fixture("jupiter-build-400-no-routes.json"),
        1,
    );
    let router = router(Some(trading(endpoint, vec![])));

    let (status, body, _cache) = quote(&router, BUY_QUERY, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["reason"], "no_route", "{body}");
    assert_eq!(
        seen.load(Ordering::SeqCst),
        1,
        "the one call that answered no_route"
    );
}

/// The public quote route needs no session at all, carries `Cache-Control:
/// no-store`, and prices from the real captured Jupiter body.
#[tokio::test(flavor = "multi_thread")]
async fn the_public_quote_route_works_without_a_session() {
    let (endpoint, seen) = jupiter(200, "OK", &fixture("jupiter-build-sol-usdc.json"), 1);
    let router = router(Some(trading(endpoint, vec![mint_account_json(6)])));

    let (status, body, cache_control) = quote(&router, BUY_QUERY, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(cache_control.as_deref(), Some("no-store"));
    assert_eq!(body["side"], "buy", "{body}");
    assert_eq!(
        body["out_amount"], "10168783",
        "the fixture's own outAmount"
    );
    assert_eq!(
        body["worst_out"], "10067096",
        "Jupiter's own otherAmountThreshold, not a computed floor"
    );
    assert_eq!(
        body["out_decimals"], 6,
        "read through the faked chain, not hardcoded"
    );
    assert_eq!(body["in_decimals"], 9, "SOL is hardcoded, no chain read");
    assert_eq!(seen.load(Ordering::SeqCst), 1);
}

/// The public route has no identity to charge, so it charges a visitor key.
/// The sixth call in a minute from one visitor still prices; the seventh is
/// `busy` and never reaches Jupiter -- proven by the upstream count, not
/// just the status.
#[tokio::test(flavor = "multi_thread")]
async fn the_visitor_cap_refuses_the_seventh_quote_without_reaching_jupiter() {
    let (endpoint, seen) = jupiter(200, "OK", &fixture("jupiter-build-sol-usdc.json"), 6);
    let router = router(Some(trading(endpoint, vec![mint_account_json(6)])));

    for i in 0..6 {
        let (status, body, _cache) = quote(&router, BUY_QUERY, Some("203.0.113.9")).await;
        assert_eq!(status, StatusCode::OK, "call {i}: {body}");
    }
    let (status, body, _cache) = quote(&router, BUY_QUERY, Some("203.0.113.9")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["reason"], "busy", "{body}");
    assert_eq!(
        seen.load(Ordering::SeqCst),
        6,
        "the seventh call must not have reached Jupiter"
    );
}

/// A signed-in wallet is a real identity and is charged against it directly:
/// the sixth swap in a minute still builds; the seventh is `busy` without a
/// seventh Jupiter call.
#[tokio::test(flavor = "multi_thread")]
async fn the_wallet_cap_refuses_the_seventh_swap_without_reaching_jupiter() {
    let taker = wallet(3);
    let (endpoint, seen) = jupiter(
        200,
        "OK",
        &fixture_for("jupiter-build-sol-usdc.json", &taker),
        6,
    );
    let router = router(Some(trading(endpoint, vec![mint_account_json(6)])));
    let bearer = session(&taker);
    let body = serde_json::json!({"mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", "side": "buy", "amount": "100000000"});

    for i in 0..6 {
        let (status, resp, _cache) = swap(&router, body.clone(), &bearer).await;
        assert_eq!(status, StatusCode::OK, "call {i}: {resp}");
    }
    let (status, resp, _cache) = swap(&router, body, &bearer).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{resp}");
    assert_eq!(resp["reason"], "busy", "{resp}");
    assert_eq!(
        seen.load(Ordering::SeqCst),
        6,
        "the seventh swap must not have reached Jupiter"
    );
}

/// Thirty calls a minute is the ceiling across the quote route and every
/// visitor put together: five visitors each spend their own full six-call
/// share -- exactly at each one's own cap, never over it -- and the
/// thirty-first call, from a sixth visitor with an empty share of their own,
/// is still `busy`. Only the quote route's own global ledger explains that
/// refusal -- see `exhausting_the_quote_cap_does_not_refuse_a_swap` for proof
/// it is that route's own ledger and not one shared with the swap route.
#[tokio::test(flavor = "multi_thread")]
async fn the_global_cap_refuses_the_thirty_first_call_regardless_of_who_asks() {
    let (endpoint, seen) = jupiter(200, "OK", &fixture("jupiter-build-sol-usdc.json"), 30);
    let router = router(Some(trading(endpoint, vec![mint_account_json(6)])));

    for visitor in 0..5 {
        let ip = format!("203.0.113.{visitor}");
        for call in 0..6 {
            let (status, body, _cache) = quote(&router, BUY_QUERY, Some(&ip)).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "visitor {visitor} call {call}: {body}"
            );
        }
    }
    assert_eq!(
        seen.load(Ordering::SeqCst),
        30,
        "thirty calls have now spent the global cap"
    );

    let (status, body, _cache) = quote(&router, BUY_QUERY, Some("203.0.113.99")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["reason"], "busy", "{body}");
    assert_eq!(
        seen.load(Ordering::SeqCst),
        30,
        "a fresh visitor's own room does not reopen a spent global ledger"
    );
}

/// The built transaction names the session wallet as fee payer -- not a
/// placeholder, not the other wallet -- and two different wallets asking
/// for the same swap get two distinct transactions, each naming itself.
#[tokio::test(flavor = "multi_thread")]
async fn two_wallets_each_get_their_own_transaction_naming_themselves_as_fee_payer() {
    let (a, b) = (wallet(11), wallet(22));
    let (endpoint, seen) = jupiter_sequence(
        200,
        "OK",
        vec![
            fixture_for("jupiter-build-sol-usdc.json", &a),
            fixture_for("jupiter-build-sol-usdc.json", &b),
        ],
    );
    let router = router(Some(trading(
        endpoint,
        vec![mint_account_json(6), mint_account_json(6)],
    )));
    let body = serde_json::json!({"mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", "side": "buy", "amount": "100000000"});

    let (status_a, resp_a, cache_a) = swap(&router, body.clone(), &session(&a)).await;
    let (status_b, resp_b, cache_b) = swap(&router, body, &session(&b)).await;
    assert_eq!(status_a, StatusCode::OK, "{resp_a}");
    assert_eq!(status_b, StatusCode::OK, "{resp_b}");

    let tx_a = resp_a["transaction"]
        .as_str()
        .expect("a transaction string");
    let tx_b = resp_b["transaction"]
        .as_str()
        .expect("a transaction string");
    assert_ne!(
        tx_a, tx_b,
        "two wallets must not receive the same transaction"
    );

    let wire_a = radar_types::b64::decode(tx_a).expect("valid base64");
    let wire_b = radar_types::b64::decode(tx_b).expect("valid base64");
    let (fee_payer_a, num_required_a) = fee_payer_of(&wire_a);
    let (fee_payer_b, num_required_b) = fee_payer_of(&wire_b);
    assert_eq!(fee_payer_a, a, "wallet A's transaction must name wallet A");
    assert_eq!(fee_payer_b, b, "wallet B's transaction must name wallet B");
    assert_eq!(
        num_required_a, 1,
        "only the caller's own wallet signs -- never a second name from the fixture"
    );
    assert_eq!(
        num_required_b, 1,
        "only the caller's own wallet signs -- never a second name from the fixture"
    );
    assert_eq!(seen.load(Ordering::SeqCst), 2);

    assert!(
        resp_a["last_valid_block_height"].is_u64(),
        "the wire contract's own field: {resp_a}"
    );
    assert_eq!(
        resp_a["quote"]["mint"],
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
    );
    // `swap_inner` derives "buy"/"sell" itself (separately from `sides_for`,
    // which only decides which asset is which) to label the quote it embeds
    // in the built-transaction response -- assert it here since this is the
    // swap route, not just the quote route which already checks this label.
    assert_eq!(resp_a["quote"]["side"], "buy", "{resp_a}");

    // A wallet-specific unsigned transaction is exactly as cache-unsafe as the
    // public quote it is built from -- see `the_public_quote_route_works_
    // without_a_session`'s own check of the same header on that route.
    assert_eq!(cache_a.as_deref(), Some("no-store"), "{resp_a}");
    assert_eq!(cache_b.as_deref(), Some("no-store"), "{resp_b}");
}

/// One address actually on `crates/radar-serve/data/ofac_sol.txt` -- proof
/// this is wired to the real, shipped list, not a test double. If OFAC ever
/// delists it (rare, but the list only ever grows -- see the file's own
/// header for the refresh procedure), this test starts failing loudly rather
/// than silently testing nothing, which is preferable to hardcoding a
/// synthetic address that would never prove the wiring at all.
const SANCTIONED: &str = "42RLPACwZPx3vYYmxSueqsogfynBDqXK298EDsNoyoHi";

/// A wallet on the shipped OFAC list is refused `sanctioned` at the swap
/// route, before Jupiter is ever asked -- the loopback server sees zero
/// requests. An unlisted wallet is unaffected and reaches Jupiter as usual.
#[tokio::test(flavor = "multi_thread")]
async fn a_sanctioned_wallet_is_refused_before_any_jupiter_call() {
    let sanctioned: Address = SANCTIONED.parse().expect("a real address from the list");
    let clean = wallet(200);
    let (endpoint, seen) = jupiter(
        200,
        "OK",
        &fixture_for("jupiter-build-sol-usdc.json", &clean),
        1,
    );
    let router = router(Some(trading(endpoint, vec![mint_account_json(6)])));
    let body = serde_json::json!({"mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", "side": "buy", "amount": "100000000"});

    let (status, resp, _cache) = swap(&router, body.clone(), &session(&sanctioned)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{resp}");
    assert_eq!(resp["reason"], "sanctioned", "{resp}");
    assert_eq!(
        seen.load(Ordering::SeqCst),
        0,
        "a sanctioned wallet must never reach Jupiter"
    );

    let (status, resp, _cache) = swap(&router, body, &session(&clean)).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an unlisted wallet must pass the check: {resp}"
    );
    assert_eq!(seen.load(Ordering::SeqCst), 1);
}

/// `/health` reports whether this instance builds or prices swaps at all --
/// `false` with `RADAR_TRADE` off (no [`Trading`] at all, the shipped
/// default), `true` once it is configured and on. An operator or `radar
/// brief` reading this instance from outside needs this fact without probing
/// either trade route, one of which sits behind a session it does not have.
#[tokio::test(flavor = "multi_thread")]
async fn health_reports_whether_trading_is_configured() {
    let off = router(None);
    let (status, body) = get_json(&off, "/health").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["trading"], false, "{body}");

    // Never dialed: `/health` reads `state.trading.is_some()`, nothing more.
    let on = router(Some(trading("http://127.0.0.1:1/build".to_owned(), vec![])));
    let (status, body) = get_json(&on, "/health").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["trading"], true, "{body}");
}

/// [`SwapBody`] (in `trade.rs`) names only `mint`, `side`, `amount`, and
/// `slippage_bps` -- no `wallet` or `taker` field exists for a caller to
/// supply, so serde's default (non-`deny_unknown_fields`) behavior silently
/// drops any extra field a request adds. This is the behavioral proof: a body
/// naming a second wallet in an extra field still produces a transaction
/// naming the signed-in [`Tenant`]'s own wallet as fee payer, never the
/// name from the extra field.
#[tokio::test(flavor = "multi_thread")]
async fn an_extra_wallet_field_naming_someone_else_is_ignored() {
    let signed_in = wallet(41);
    let someone_else = wallet(42);
    let (endpoint, seen) = jupiter(
        200,
        "OK",
        &fixture_for("jupiter-build-sol-usdc.json", &signed_in),
        1,
    );
    let router = router(Some(trading(endpoint, vec![mint_account_json(6)])));

    let body = serde_json::json!({
        "mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        "side": "buy",
        "amount": "100000000",
        "wallet": someone_else.to_string(),
        "taker": someone_else.to_string(),
    });
    let (status, resp, _cache) = swap(&router, body, &session(&signed_in)).await;
    assert_eq!(status, StatusCode::OK, "{resp}");

    let tx = resp["transaction"].as_str().expect("a transaction string");
    let wire = radar_types::b64::decode(tx).expect("valid base64");
    let (fee_payer, num_required) = fee_payer_of(&wire);
    assert_eq!(
        fee_payer, signed_in,
        "the fee payer is the session's own wallet, never the body's extra field"
    );
    assert_ne!(
        fee_payer, someone_else,
        "the extra wallet/taker field must not have redirected the transaction"
    );
    assert_eq!(
        num_required, 1,
        "only the signed-in wallet signs -- never a second name from the body"
    );
    assert_eq!(seen.load(Ordering::SeqCst), 1);
}

/// The swap route's global budget ([`MAX_JUPITER_BUILD_CALLS_PER_MINUTE`],
/// via `Trading::build_calls`) is its own pool, separate from the quote
/// route's ([`MAX_JUPITER_CALLS_PER_MINUTE`], via `Trading::quote_calls`) --
/// see the module doc comment's "Four limits" section. Spending the quote
/// route's entire thirty-call ceiling (proven `busy` for a fresh visitor
/// immediately after) must not cost a signed-in wallet's swap build anything:
/// it still reaches Jupiter and builds, on a ledger the quote traffic never
/// touched.
#[tokio::test(flavor = "multi_thread")]
async fn exhausting_the_quote_cap_does_not_refuse_a_swap() {
    let taker = wallet(77);
    let mut bodies: Vec<String> = vec![fixture("jupiter-build-sol-usdc.json"); 30];
    bodies.push(fixture_for("jupiter-build-sol-usdc.json", &taker));
    let (endpoint, seen) = jupiter_sequence(200, "OK", bodies);
    let router = router(Some(trading(endpoint, vec![mint_account_json(6)])));

    for visitor in 0..5 {
        let ip = format!("203.0.113.{visitor}");
        for call in 0..6 {
            let (status, body, _cache) = quote(&router, BUY_QUERY, Some(&ip)).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "visitor {visitor} call {call}: {body}"
            );
        }
    }
    assert_eq!(
        seen.load(Ordering::SeqCst),
        30,
        "the quote route's own global cap is now fully spent"
    );

    let (status, body, _cache) = quote(&router, BUY_QUERY, Some("203.0.113.99")).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "a fresh visitor confirms the quote route's ledger really is spent: {body}"
    );
    assert_eq!(body["reason"], "busy", "{body}");

    let swap_body = serde_json::json!({"mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", "side": "buy", "amount": "100000000"});
    let (status, resp, _cache) = swap(&router, swap_body, &session(&taker)).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the swap route's own budget was never touched by quote traffic: {resp}"
    );
    assert_eq!(
        seen.load(Ordering::SeqCst),
        31,
        "the swap build reached Jupiter on its own separate ledger"
    );
}
