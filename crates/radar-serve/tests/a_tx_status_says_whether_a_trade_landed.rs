// SPDX-License-Identifier: Apache-2.0
//! `GET /v1/customer/tx/{signature}?last_valid_block_height=N`. Plan 0014
//! F11.
//!
//! Through the real router and a faked chain transport, exactly as
//! `a_swap_is_priced_and_built_by_radar.rs` does for `/v1/customer/swap` --
//! reusing that file's `FakeChain`/`router`/`trading` shapes rather than a
//! second copy, since Cargo test binaries cannot share `mod` files across
//! `tests/*.rs` without a `tests/common/mod.rs` this crate does not have.
//! No test here ever reaches a real network.

use std::sync::Mutex;

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

/// A well-formed base58 signature: 64 bytes, decodes cleanly, is not any
/// real transaction. `decode_base58`'s own length check is the only thing
/// this route asks of it.
fn signature() -> String {
    bs58::encode([7u8; 64]).into_string()
}

/// A [`Transport`] that answers a fixed queue, in order, and panics if asked
/// for more than it was given -- the same fake `a_swap_is_priced_and_built_by_radar.rs`
/// uses, reproduced here because it is not exported from that file either.
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

/// A `getSignatureStatuses` envelope reporting one status (or none) for the
/// single signature this route ever asks about.
fn status_json(status: Option<(&str, Option<&str>, Option<Value>)>) -> String {
    let value = match status {
        None => "null".to_owned(),
        Some((slot, confirmation, err)) => {
            let confirmation =
                confirmation.map_or_else(|| "null".to_owned(), |c| format!(r#""{c}""#));
            let err = err.map_or_else(|| "null".to_owned(), |v| v.to_string());
            format!(r#"{{"slot":{slot},"confirmationStatus":{confirmation},"err":{err}}}"#)
        }
    };
    format!(r#"{{"jsonrpc":"2.0","id":1,"result":{{"context":{{"slot":1}},"value":[{value}]}}}}"#)
}

/// A `getBlockHeight` envelope.
fn height_json(height: u64) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":1,"result":{height}}}"#)
}

/// A [`Trading`] pointed at a loopback Jupiter double (unused by this
/// route) and a faked chain answering `chain_responses` in call order:
/// `signature_status` first, then `block_height`, per `tx_status_inner`.
fn trading(chain_responses: Vec<String>) -> Trading {
    Trading::new(
        Router::with_endpoint("http://127.0.0.1:1/build".to_owned(), credentials()),
        RpcClient::with_transport("http://test.invalid", FakeChain::boxed(chain_responses)),
    )
}

fn router(trading: Option<Trading>) -> axum::Router {
    app(std::sync::Arc::new(AppState {
        admission: radar_serve::admission::Admission::Open,
        shares: radar_serve::share::Shares::new(radar_serve::share::Allowance::per_day(100)),
        customer_salt: SALT.to_vec(),
        registry: Registry::new(),
        store: Reader::open(std::env::temp_dir().join("radar-tx-status-test")),
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

async fn tx_status(
    router: &axum::Router,
    signature: &str,
    last_valid_block_height: u64,
    bearer: &str,
) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/v1/customer/tx/{signature}?last_valid_block_height={last_valid_block_height}"
                ))
                .header("authorization", format!("Bearer {bearer}"))
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

/// Off is the shipped state, and this route must say so without ever
/// reaching the chain -- no fake transport exists in this test at all.
#[tokio::test(flavor = "multi_thread")]
async fn trading_off_refuses_without_reading_the_chain() {
    let router = router(None);
    let (status, body) = tx_status(&router, &signature(), 100, &session(&wallet(1))).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["reason"], "trading_off", "{body}");
}

/// A signature that is not 64 decoded bytes of base58 is a `bad_request`,
/// never a chain read.
#[tokio::test(flavor = "multi_thread")]
async fn a_malformed_signature_is_a_bad_request() {
    let router = router(Some(trading(vec![])));
    let (status, body) = tx_status(&router, "not-base58!!", 100, &session(&wallet(1))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["reason"], "bad_request", "{body}");

    let (status, body) = tx_status(&router, "abcd", 100, &session(&wallet(1))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["reason"], "bad_request", "{body}");
}

/// `confirmationStatus` of `confirmed` or `finalized` with no `err` is
/// `landed`, and carries the slot it landed in.
#[tokio::test(flavor = "multi_thread")]
async fn a_confirmed_signature_with_no_error_has_landed() {
    let router = router(Some(trading(vec![
        status_json(Some(("42", Some("confirmed"), None))),
        height_json(100),
    ])));
    let (status, body) = tx_status(&router, &signature(), 200, &session(&wallet(1))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["state"], "landed", "{body}");
    assert_eq!(body["slot"], 42, "{body}");
}

/// A status with a non-null `err` is `failed`, with a plain rendering of the
/// error -- regardless of what block height the chain has reached.
#[tokio::test(flavor = "multi_thread")]
async fn a_status_with_an_error_has_failed() {
    let router = router(Some(trading(vec![
        status_json(Some((
            "42",
            Some("confirmed"),
            Some(serde_json::json!({"InstructionError": [1, {"Custom": 6001}]})),
        ))),
        height_json(100),
    ])));
    let (status, body) = tx_status(&router, &signature(), 200, &session(&wallet(1))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["state"], "failed", "{body}");
    assert_eq!(
        body["reason"], "instruction 1 failed with custom error 6001",
        "{body}"
    );
}

/// No status at all, and the chain has already passed the signature's last
/// valid block height: `expired`, and only this combination may say so.
#[tokio::test(flavor = "multi_thread")]
async fn no_status_past_the_last_valid_height_has_expired() {
    let router = router(Some(trading(vec![status_json(None), height_json(101)])));
    let (status, body) = tx_status(&router, &signature(), 100, &session(&wallet(1))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["state"], "expired", "{body}");
}

/// No status yet, but the chain has not passed the last valid height:
/// `pending`, not `expired` -- the transaction may still land.
#[tokio::test(flavor = "multi_thread")]
async fn no_status_before_the_last_valid_height_is_pending() {
    let router = router(Some(trading(vec![status_json(None), height_json(99)])));
    let (status, body) = tx_status(&router, &signature(), 100, &session(&wallet(1))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["state"], "pending", "{body}");
}

/// A status with no confirmation level yet (`processed`, or none at all) is
/// `pending`, never `landed` -- landing means confirmed or finalized.
#[tokio::test(flavor = "multi_thread")]
async fn a_processed_but_not_confirmed_status_is_pending() {
    let router = router(Some(trading(vec![
        status_json(Some(("42", Some("processed"), None))),
        height_json(100),
    ])));
    let (status, body) = tx_status(&router, &signature(), 200, &session(&wallet(1))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["state"], "pending", "{body}");
}

/// Any failure reading the chain is a `chain_unreadable` refusal -- never a
/// claim of `expired`, which is a fact about the chain this route did not
/// actually observe when the read itself failed.
#[tokio::test(flavor = "multi_thread")]
async fn a_chain_read_failure_refuses_chain_unreadable_never_expired() {
    // Malformed JSON: `RpcClient::call` fails to deserialize the envelope
    // and returns `RpcError::Malformed` before either value is ever used.
    let router = router(Some(trading(vec!["not json at all".to_owned()])));
    let (status, body) = tx_status(&router, &signature(), 100, &session(&wallet(1))).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(body["reason"], "chain_unreadable", "{body}");
    assert_ne!(body["state"], "expired", "{body}");
}
