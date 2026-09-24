// SPDX-License-Identifier: Apache-2.0
//! A wallet's watchlist is read by that wallet and by nobody else.
//!
//! Through the real router and the real guard, with sessions that really
//! verify, because the property is the boundary between two genuine customers:
//! a request that fails to sign in proves nothing about whether a signed-in one
//! can reach past its own folder.
//!
//! Plan 0012 task 9-11-0011's rubric, items 3, 4, 5 and 7, and the owner's
//! decision of 2026-09-23 that the operator cannot read a list either.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use radar_customer::session::{LIFETIME_SECONDS, issue};
use radar_instruments::Registry;
use radar_serve::customer::{KeyCache, Keys, Mode};
use radar_serve::tenant::Customers;
use radar_serve::{AppState, app};
use radar_store::Reader;
use radar_types::Address;
use serde_json::Value;
use tower::ServiceExt;

const SALT: [u8; 32] = [7u8; 32];
/// A real mint's shape -- wrapped SOL -- so the coin parses.
const COIN: &str = "So11111111111111111111111111111111111111112";

fn now() -> u64 {
    radar_serve::now_unix()
}

fn wallet(byte: u8) -> Address {
    Address::new([byte; 32])
}

fn session(who: &Address) -> String {
    issue(who, &SALT, now()).expect("a session")
}

/// An instance as production runs it: Access enforced, no Privy, every wallet
/// admitted, and a real folder for lists.
fn router(customers: Option<Customers>, access: radar_serve::access::Mode) -> axum::Router {
    app(Arc::new(AppState {
        admission: radar_serve::admission::Admission::Open,
        shares: radar_serve::share::Shares::new(radar_serve::share::Allowance::per_day(100)),
        customer_salt: SALT.to_vec(),
        registry: Registry::new(),
        store: Reader::open(std::env::temp_dir().join("radar-watchlist-test")),
        x402: None,
        chat: None,
        access,
        keys: radar_serve::access::KeyCache::new(),
        customer: Mode::Off,
        customer_keys: KeyCache::preloaded(Keys(Vec::new())),
        linker: radar_serve::link::Linker::new(),
        scoreboard: radar_serve::cache::Cache::new(),
        token: radar_serve::cache::Cache::new(),
        challenges: None,
        market: radar_serve::market::Market::new(),
        market_snapshot: radar_serve::market::SnapshotCache::new(),
        customers,
        privy: None,
    }))
}

fn enforced() -> radar_serve::access::Mode {
    radar_serve::access::Mode::Enforce(radar_serve::access::Config {
        team_domain: "radar-test.invalid".to_owned(),
        aud: "radar-aud-tag".to_owned(),
    })
}

fn with_lists() -> (tempfile::TempDir, axum::Router) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let customers = Customers::at(&dir.path().join("customers")).expect("writable");
    (dir, router(Some(customers), enforced()))
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

fn coins(body: &Value) -> Vec<String> {
    body["coins"]
        .as_array()
        .expect("a list of coins")
        .iter()
        .map(|c| c.as_str().expect("a coin").to_owned())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn one_wallets_list_is_invisible_to_another() {
    // Rubric item 7, across the wire. Re-apply by keying every folder on one
    // fixed name, or by reading the wallet from a request parameter.
    let (_dir, router) = with_lists();
    let (a, b) = (wallet(1), wallet(2));
    let (a_session, b_session) = (session(&a), session(&b));
    let one_coin = format!("/v1/customer/watchlist/{COIN}");

    let (status, body) = call(&router, Method::PUT, &one_coin, Some(&a_session)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(coins(&body), [COIN]);
    assert_eq!(body["wallet"], a.to_string());
    assert_eq!(body["limit"], 100);

    let (status, body) = call(
        &router,
        Method::GET,
        "/v1/customer/watchlist",
        Some(&b_session),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["wallet"], b.to_string(), "B is answered as B");
    assert!(coins(&body).is_empty(), "B must not see A's coin: {body}");

    // B removing the same coin touches B's (empty) list, not A's.
    let (status, _) = call(&router, Method::DELETE, &one_coin, Some(&b_session)).await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = call(
        &router,
        Method::GET,
        "/v1/customer/watchlist",
        Some(&a_session),
    )
    .await;
    assert_eq!(coins(&body), [COIN], "A's list must survive B's delete");

    // And A's own delete works.
    let (_, body) = call(&router, Method::DELETE, &one_coin, Some(&a_session)).await;
    assert!(coins(&body).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn naming_another_wallet_in_the_request_is_refused_out_loud() {
    // An address guess. Ignoring the parameter and answering with B's own list
    // would be safe and would look to B like it worked -- so it is refused.
    let (_dir, router) = with_lists();
    let (a, b) = (wallet(1), wallet(2));
    let path = format!("/v1/customer/watchlist?wallet={a}");
    let (status, body) = call(&router, Method::GET, &path, Some(&session(&b))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["reason"], "unscoped");
    assert!(body.get("coins").is_none(), "no list at all: {body}");

    let path = format!("/v1/customer/watchlist/{COIN}?wallet={a}");
    let (status, _) = call(&router, Method::PUT, &path, Some(&session(&b))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread")]
async fn no_session_an_expired_one_and_a_forged_one_each_say_which() {
    // Rubric items 3, 4 and 5. Three refusals that want three different
    // responses, so they must not read alike.
    let (_dir, router) = with_lists();
    let who = wallet(1);

    let (status, none) = call(&router, Method::GET, "/v1/customer/watchlist", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(none["reason"], "no_session");

    // Issued a lifetime and a minute ago: well formed, genuinely signed, stale.
    let stale = issue(&who, &SALT, now() - LIFETIME_SECONDS - 60).expect("a session");
    let (status, expired) =
        call(&router, Method::GET, "/v1/customer/watchlist", Some(&stale)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(expired["reason"], "session_expired");
    assert!(
        expired["error"]
            .as_str()
            .is_some_and(|e| e.contains("expired")),
        "{expired}"
    );

    // One character of the payload changed: the tag no longer matches.
    let genuine = session(&who);
    let mut forged: Vec<char> = genuine.chars().collect();
    forged[0] = if forged[0] == 'A' { 'B' } else { 'A' };
    let forged: String = forged.into_iter().collect();
    let (status, bad) = call(
        &router,
        Method::GET,
        "/v1/customer/watchlist",
        Some(&forged),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(bad["reason"], "session_invalid");

    // Signed with some other instance's secret: the same answer, and the answer
    // does not say which half was wrong.
    let elsewhere = issue(&who, &[8u8; 32], now()).expect("a session");
    let (status, other) = call(
        &router,
        Method::GET,
        "/v1/customer/watchlist",
        Some(&elsewhere),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(other["error"], bad["error"]);

    let errors = [&none["error"], &expired["error"], &bad["error"]];
    for (i, x) in errors.iter().enumerate() {
        for y in &errors[i + 1..] {
            assert_ne!(x, y, "two different refusals read the same");
        }
    }
    for body in [&none, &expired, &bad] {
        assert!(
            body.get("coins").is_none(),
            "a refusal carries no list: {body}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_customer_route_names_the_wallet_session_and_an_operator_route_does_not() {
    // With nothing offered, a customer is told to sign in with a wallet, which
    // is a door they can open. The operator's surface still names Cloudflare.
    let (_dir, router) = with_lists();
    let (_, wallet_route) = call(&router, Method::GET, "/v1/customer/wallet", None).await;
    assert_eq!(wallet_route["reason"], "no_session");
    let (status, store) = call(&router, Method::GET, "/v1/store", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(store["error"], "no Cloudflare Access assertion");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_expired_session_beside_a_bad_operator_login_says_expired() {
    // A browser can carry both: a stale wallet session and a Cloudflare cookie
    // that does not verify here. The wallet's problem is the one the customer
    // can fix, so it is the one named.
    let (_dir, router) = with_lists();
    let stale = issue(&wallet(1), &SALT, now() - LIFETIME_SECONDS - 60).expect("a session");
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/customer/watchlist")
                .header("authorization", format!("Bearer {stale}"))
                .header(radar_serve::access::ASSERTION_HEADER, "not-a-jwt")
                .body(Body::empty())
                .expect("a well-formed request"),
        )
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    let body: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(body["reason"], "session_expired", "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_let_in_without_a_wallet_reads_no_list() {
    // The operator's case. With the operator check off -- the state in which a
    // request certainly reaches the handler -- nothing names a wallet, so there
    // is no list to read, however the request got in. With Access on, an
    // operator's login reaches this same handler the same way: no `Tenant`.
    let dir = tempfile::tempdir().expect("a temporary directory");
    let customers = Customers::at(&dir.path().join("customers")).expect("writable");
    let router = router(Some(customers), radar_serve::access::Mode::Off);

    let (status, body) = call(&router, Method::GET, "/v1/customer/watchlist", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["reason"], "no_session");

    // And the reason a session failed still reaches the handler.
    let stale = issue(&wallet(1), &SALT, now() - LIFETIME_SECONDS - 60).expect("a session");
    let (status, body) = call(&router, Method::GET, "/v1/customer/watchlist", Some(&stale)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["reason"], "session_expired");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_instance_keeping_no_lists_refuses_rather_than_forgets() {
    let router = router(None, enforced());
    let (status, body) = call(
        &router,
        Method::GET,
        "/v1/customer/watchlist",
        Some(&session(&wallet(1))),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["reason"], "not_configured");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bad_coin_and_a_damaged_list_are_refused_not_emptied() {
    let (dir, router) = with_lists();
    let who = wallet(1);
    let token = session(&who);

    let (status, body) = call(
        &router,
        Method::PUT,
        "/v1/customer/watchlist/not-a-coin",
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["reason"], "not_a_coin");

    // Rule 9: "could not read" is not "watching nothing".
    let folder = dir.path().join("customers").join(who.to_string());
    std::fs::create_dir_all(&folder).expect("a folder");
    std::fs::write(folder.join("watchlist.json"), b"{ damaged").expect("damaged");
    let (status, body) = call(&router, Method::GET, "/v1/customer/watchlist", Some(&token)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["reason"], "unreadable");
    assert!(body.get("coins").is_none(), "{body}");
}
