// SPDX-License-Identifier: Apache-2.0
//! `/v1/customer/positions`: the signed-in wallet's own on-chain holdings.
//!
//! Through the real router and the real guard, with sessions that really
//! verify and a fake [`radar_onchain::rpc::Transport`] standing in for the
//! chain -- exactly the shape `a_watchlist_is_seen_only_by_its_wallet.rs`
//! uses for the watchlist, because the property under test is the same kind
//! of boundary: what one wallet reads must never be what another gets, and a
//! read that only half completed must never be presented as a whole one.
//!
//! Redesigned 2026-09-24 from a browser-reads-RPC shape (plan 0013 Phase C
//! item 2) because Solana's public node refuses any request carrying a
//! browser `Origin` header.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use radar_customer::session::{LIFETIME_SECONDS, issue};
use radar_instruments::Registry;
use radar_onchain::rpc::{RpcClient, Transport};
use radar_serve::positions::Positions;
use radar_serve::{AppState, app};
use radar_store::Reader;
use radar_types::Address;
use serde_json::Value;
use tower::ServiceExt;

const SALT: [u8; 32] = [7u8; 32];

fn now() -> u64 {
    radar_serve::now_unix()
}

fn wallet(byte: u8) -> Address {
    Address::new([byte; 32])
}

fn session(who: &Address) -> String {
    issue(who, &SALT, now()).expect("a session")
}

/// A transport that answers each call from a fixed queue, in order --
/// exactly `radar_onchain::rpc`'s own private test fake, reproduced here
/// because it is not exported: this crate can only reach `RpcClient` through
/// its public `Transport` seam.
///
/// Panics if asked for more than it was given, rather than returning an
/// error that a bug could quietly turn into an "unreadable_chain" response:
/// a test asserting the cap refuses *without calling the transport* must see
/// a hard failure if that promise is broken, not a response body that merely
/// reads a little differently.
struct Fake(Mutex<Vec<String>>);

impl Fake {
    fn boxed(responses: Vec<String>) -> Box<dyn Transport> {
        Box::new(Self(Mutex::new(responses.into_iter().rev().collect())))
    }
}

impl Transport for Fake {
    fn post(&self, _endpoint: &str, _body: String) -> Result<String, String> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop()
            .unwrap_or_else(|| {
                panic!("the transport was called after its canned responses were exhausted")
            }))
    }
}

fn balance_json(slot: u64, lamports: u64) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":{{"context":{{"slot":{slot}}},"value":{lamports}}}}}"#
    )
}

fn empty_token_accounts_json(slot: u64) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":1,"result":{{"context":{{"slot":{slot}}},"value":[]}}}}"#)
}

fn token_accounts_json(slot: u64, accounts: &[(&str, &str, u8)]) -> String {
    let entries: Vec<String> = accounts
        .iter()
        .map(|(mint, amount, decimals)| {
            format!(
                r#"{{"account":{{"data":{{"parsed":{{"info":{{"mint":"{mint}","tokenAmount":{{"amount":"{amount}","decimals":{decimals}}}}}}}}}}}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"result":{{"context":{{"slot":{slot}}},"value":[{}]}}}}"#,
        entries.join(",")
    )
}

fn node_error_json() -> String {
    r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"the node refused this call"}}"#
        .to_owned()
}

/// One wallet's whole view: a balance read and the two token-program reads,
/// in the order `positions::read_wallet` makes them.
fn view(balance: String, token: String, token_2022: String) -> Vec<String> {
    vec![balance, token, token_2022]
}

fn positions_from(responses: Vec<String>) -> Positions {
    Positions::new(RpcClient::with_transport(
        "http://test.invalid",
        Fake::boxed(responses),
    ))
}

/// An instance as production runs it, apart from Access, which every other
/// fixture in this crate also turns off: it is an operator-identity gate
/// unrelated to a wallet session, and every route this file exercises is
/// reached the same way regardless of it.
fn router(positions: Option<Positions>) -> axum::Router {
    app(Arc::new(AppState {
        admission: radar_serve::admission::Admission::Open,
        shares: radar_serve::share::Shares::new(radar_serve::share::Allowance::per_day(100)),
        customer_salt: SALT.to_vec(),
        registry: Registry::new(),
        store: Reader::open(std::env::temp_dir().join("radar-positions-test")),
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
        positions,
    }))
}

async fn call(
    router: &axum::Router,
    method: Method,
    path: &str,
    bearer: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(token) = bearer {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).expect("a well-formed request"))
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

#[tokio::test(flavor = "multi_thread")]
async fn one_wallets_holdings_are_never_served_to_another() {
    // Re-apply a cache keyed on something other than the verified address --
    // a session id, a request-scoped counter, nothing at all -- and this
    // fails: B would either see A's holdings or trigger no read of its own.
    let (a, b) = (wallet(1), wallet(2));
    let responses = [
        view(
            balance_json(10, 5_000_000_000),
            empty_token_accounts_json(10),
            empty_token_accounts_json(10),
        ),
        view(
            balance_json(20, 7_000_000_000),
            empty_token_accounts_json(20),
            empty_token_accounts_json(20),
        ),
    ]
    .concat();
    let router = router(Some(positions_from(responses)));

    let (status, body) = call(
        &router,
        Method::GET,
        "/v1/customer/positions",
        Some(&session(&a)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["wallet"], a.to_string());
    assert_eq!(body["sol"]["lamports"], 5_000_000_000_u64);

    let (status, body) = call(
        &router,
        Method::GET,
        "/v1/customer/positions",
        Some(&session(&b)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["wallet"], b.to_string(), "B is answered as B");
    assert_eq!(
        body["sol"]["lamports"], 7_000_000_000_u64,
        "B must read its own balance, not A's cached one"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_query_string_is_refused_as_unscoped() {
    // This route takes no parameter at all -- the wallet is always the
    // caller's own, from the session. A `?wallet=` naming a different one
    // must not be silently ignored, the same as the watchlist.
    let router = router(Some(positions_from(Vec::new())));
    let path = format!("/v1/customer/positions?wallet={}", wallet(9));
    let (status, body) = call(&router, Method::GET, &path, Some(&session(&wallet(1)))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["reason"], "unscoped");
    assert!(body.get("tokens").is_none(), "no answer at all: {body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn no_session_an_expired_one_a_forged_one_and_a_failed_email_login_each_say_which() {
    // Rubric items shared with the watchlist test of the same shape: four
    // refusals that must not read alike, and none of them may carry a list.
    let router = router(Some(positions_from(Vec::new())));
    let who = wallet(1);

    let (status, none) = call(&router, Method::GET, "/v1/customer/positions", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(none["reason"], "no_session");

    let stale = issue(&who, &SALT, now() - LIFETIME_SECONDS - 60).expect("a session");
    let (status, expired) =
        call(&router, Method::GET, "/v1/customer/positions", Some(&stale)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(expired["reason"], "session_expired");

    let genuine = session(&who);
    let mut forged: Vec<char> = genuine.chars().collect();
    forged[0] = if forged[0] == 'A' { 'B' } else { 'A' };
    let forged: String = forged.into_iter().collect();
    let (status, bad) = call(
        &router,
        Method::GET,
        "/v1/customer/positions",
        Some(&forged),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(bad["reason"], "session_invalid");

    // A Privy (email-login) token has three parts, a wallet session two: one
    // that fails on an instance without Privy must not be blamed on a wallet
    // that never signed anything.
    let (status, email) = call(
        &router,
        Method::GET,
        "/v1/customer/positions",
        Some("eyJhbGciOiJFUzI1NiJ9.eyJzdWIiOiJkaWQ6cHJpdnk6eCJ9.c2ln"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{email}");
    assert_eq!(email["reason"], "no_session", "{email}");

    let errors = [&none["error"], &expired["error"], &bad["error"]];
    for (i, x) in errors.iter().enumerate() {
        for y in &errors[i + 1..] {
            assert_ne!(x, y, "two different refusals read the same");
        }
    }
    for body in [&none, &expired, &bad, &email] {
        assert!(
            body.get("tokens").is_none(),
            "a refusal carries no holdings: {body}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn one_programs_failed_read_refuses_the_whole_answer() {
    // Rule 9's shape: the Token-2022 read fails after the Token-program read
    // already succeeded. Re-apply a fold that returns whatever programs did
    // answer and this passes with a one-program list instead of a failure.
    let who = wallet(1);
    let responses = view(
        balance_json(5, 1_000_000_000),
        token_accounts_json(5, &[("MintOne", "1000", 6)]),
        node_error_json(),
    );
    let router = router(Some(positions_from(responses)));

    let (status, body) = call(
        &router,
        Method::GET,
        "/v1/customer/positions",
        Some(&session(&who)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(body["reason"], "unreadable_chain");
    assert!(
        body.get("tokens").is_none(),
        "the Token program's list must not be presented as the whole answer: {body}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cached_answer_keeps_its_slot_while_its_age_grows() {
    let who = wallet(1);
    let responses = view(
        balance_json(42, 2_000_000_000),
        empty_token_accounts_json(42),
        empty_token_accounts_json(42),
    );
    let router = router(Some(positions_from(responses)));
    let token = session(&who);

    let (status, first) = call(&router, Method::GET, "/v1/customer/positions", Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["slot"], 42);
    let first_read_at = first["read_at"].as_u64().expect("read_at");

    tokio::time::sleep(Duration::from_millis(1_100)).await;

    // Served from cache: the fake transport has nothing left to give, so a
    // second live read would panic, not merely answer wrong.
    let (status, second) = call(&router, Method::GET, "/v1/customer/positions", Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(
        second["slot"], 42,
        "a cached answer keeps its original slot"
    );
    assert_eq!(
        second["read_at"], first_read_at,
        "a cached answer keeps its original read_at"
    );
    let first_age = first["age_seconds"].as_u64().expect("age_seconds");
    let second_age = second["age_seconds"].as_u64().expect("age_seconds");
    assert!(
        second_age > first_age,
        "a cached answer's age must grow, not stay pinned at the first read: {first_age} -> {second_age}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cap_refuses_busy_without_calling_the_transport() {
    // 60 calls/minute, 3 per view: 20 wallets exhaust it exactly, and the
    // 21st is refused before spending a fourth. Distinct wallets, so the
    // per-wallet cache cannot be what answers any of them.
    let mut responses = Vec::new();
    for _ in 0..20 {
        responses.extend(view(
            balance_json(1, 0),
            empty_token_accounts_json(1),
            empty_token_accounts_json(1),
        ));
    }
    let router = router(Some(positions_from(responses)));

    for i in 0..20u8 {
        let (status, body) = call(
            &router,
            Method::GET,
            "/v1/customer/positions",
            Some(&session(&wallet(i))),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "wallet {i}: {body}");
    }

    // The transport has nothing left: if this reserved and called anyway, it
    // would panic inside the handler rather than answer `busy`.
    let (status, body) = call(
        &router,
        Method::GET,
        "/v1/customer/positions",
        Some(&session(&wallet(20))),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["reason"], "busy");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mint_radar_does_not_track_is_priced_false_never_zero() {
    // No market feed and no snapshot built: `market::price_of` returns
    // `None` for anything, which is the same state a mint outside Radar's
    // coverage looks like -- this holding must say so, not print `0`.
    let who = wallet(1);
    let responses = view(
        balance_json(1, 0),
        token_accounts_json(1, &[("UntrackedMint111111111111111111111111111", "500", 6)]),
        empty_token_accounts_json(1),
    );
    let router = router(Some(positions_from(responses)));

    let (status, body) = call(
        &router,
        Method::GET,
        "/v1/customer/positions",
        Some(&session(&who)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tokens = body["tokens"].as_array().expect("a token list");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0]["priced"], false);
    assert!(tokens[0]["price_usd"].is_null(), "{body}");
    assert!(tokens[0]["value_usd"].is_null(), "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn same_mint_accounts_are_summed_and_zero_balances_are_dropped() {
    let who = wallet(1);
    let responses = view(
        balance_json(1, 0),
        token_accounts_json(
            1,
            &[
                ("SameMint11111111111111111111111111111111", "1000", 6),
                ("SameMint11111111111111111111111111111111", "2000", 6),
                ("ZeroMint111111111111111111111111111111111", "0", 6),
            ],
        ),
        empty_token_accounts_json(1),
    );
    let router = router(Some(positions_from(responses)));

    let (status, body) = call(
        &router,
        Method::GET,
        "/v1/customer/positions",
        Some(&session(&who)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tokens = body["tokens"].as_array().expect("a token list");
    assert_eq!(
        tokens.len(),
        1,
        "the zero-balance mint must be dropped: {body}"
    );
    assert_eq!(
        tokens[0]["mint"],
        "SameMint11111111111111111111111111111111"
    );
    assert_eq!(tokens[0]["amount"], "3000", "two accounts of one mint sum");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unconfigured_instance_refuses_rather_than_forges_an_empty_answer() {
    let router = router(None);
    let (status, body) = call(
        &router,
        Method::GET,
        "/v1/customer/positions",
        Some(&session(&wallet(1))),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["reason"], "not_configured");
}
