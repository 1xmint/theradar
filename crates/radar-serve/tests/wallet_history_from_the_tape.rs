// SPDX-License-Identifier: Apache-2.0
//! `/v1/market/history/{mint}?wallet=` -- plan 0013, phase C item 3.
//!
//! One wallet's trades in one coin, from the recorded tape alone. The tape
//! names the seller on every sell and nobody on four buys in five, so the
//! buys are matched instead by the account they were paid into -- an address
//! derived from the wallet locally, costing no query and reaching no network.
//!
//! The route is public and identity-free like the rest of the module: the
//! wallet is a query parameter, not a session, and the tape is public chain
//! data. Modelled on `holders_from_the_tape.rs`'s `state_at`/`get`.

use std::sync::Arc;

use radar_backfill::market_tape::coverage_record;
use radar_instruments::Registry;
use radar_serve::{AppState, app};
use radar_store::{Completion, MarketSide, MarketTrade, Reader, Writer};
use radar_types::{Address, Signature, Slot};
use tower::ServiceExt;

const A_MINT: &str = "5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump";
const WSOL: &str = "So11111111111111111111111111111111111111112";
const WALLET: [u8; 32] = [5u8; 32];

fn wallet() -> Address {
    Address::new(WALLET)
}

/// The wallet's own associated token account for the mint, derived the way
/// the route derives it.
fn own_account(mint: &Address) -> Address {
    radar_pumpfun::pda::associated_token_account(
        &wallet(),
        mint,
        &radar_pumpfun::token::SPL_TOKEN_PROGRAM,
    )
    .expect("the wallet's associated account derives")
}

fn market_trade(
    mint: &str,
    ts: &str,
    slot: u64,
    side: MarketSide,
    trader: Option<Address>,
    token_destination: Option<Address>,
) -> MarketTrade {
    MarketTrade {
        mint: mint.parse().expect("a mint"),
        ts: ts.to_owned(),
        slot: Slot(slot),
        signature: Signature::new([(slot % 251) as u8; 64]),
        side,
        token_amount: 5.0,
        quote_amount: Some(2.0),
        quote_mint: Some(WSOL.parse().expect("a mint")),
        price: Some(2.0),
        trader,
        token_destination,
    }
}

/// A collected store holding a buy that names no trader but was paid into
/// the wallet's own account, and a sell that names the wallet outright.
fn store_with_one_wallets_trades(dir: &std::path::Path, slot: u64) {
    let mint: Address = A_MINT.parse().expect("a mint");
    let mut writer = Writer::open(dir, 64).expect("open");
    let buy = market_trade(
        A_MINT,
        "2026-09-17 23:59:00",
        slot - 1,
        MarketSide::Buy,
        None,
        Some(own_account(&mint)),
    );
    let sell = market_trade(
        A_MINT,
        "2026-09-18 00:00:00",
        slot,
        MarketSide::Sell,
        Some(wallet()),
        None,
    );
    writer.append_market_trade(buy.clone()).expect("append buy");
    writer
        .append_market_trade(sell.clone())
        .expect("append sell");
    writer
        .append_coverage(coverage_record(
            Completion::Complete,
            &[sell, buy],
            Slot(slot),
        ))
        .expect("append coverage");
    writer.flush().expect("flush");
}

/// A store holding the same trades with **no coverage record at all**: the
/// collector has never reported producing anything here.
fn store_that_never_collected(dir: &std::path::Path, slot: u64) {
    let mut writer = Writer::open(dir, 64).expect("open");
    writer
        .append_market_trade(market_trade(
            A_MINT,
            "2026-09-18 00:00:00",
            slot,
            MarketSide::Sell,
            Some(wallet()),
            None,
        ))
        .expect("append sell");
    writer.flush().expect("flush");
}

fn state_at(dir: &std::path::Path) -> Arc<AppState> {
    Arc::new(AppState {
        admission: radar_serve::admission::Admission::Open,
        shares: radar_serve::share::Shares::new(radar_serve::share::Allowance::per_day(100)),
        customer_salt: vec![7u8; 32],
        registry: Registry::new(),
        store: Reader::open(dir),
        x402: None,
        chat: None,
        access: radar_serve::access::Mode::Off,
        keys: radar_serve::access::KeyCache::new(),
        customer: radar_serve::customer::Mode::Off,
        customer_keys: radar_serve::customer::KeyCache::new(),
        privy: None,
        linker: radar_serve::link::Linker::new(),
        scoreboard: radar_serve::cache::Cache::new(),
        token: radar_serve::cache::Cache::new(),
        challenges: None,
        market: radar_serve::market::Market::new(),
        market_snapshot: radar_serve::market::SnapshotCache::new(),
        customers: None,
    })
}

async fn get(state: &Arc<AppState>, uri: &str) -> (axum::http::StatusCode, serde_json::Value) {
    let response = app(Arc::clone(state))
        .oneshot(
            axum::http::Request::builder()
                .uri(uri)
                .body(axum::body::Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4_000_000)
        .await
        .expect("body");
    (status, serde_json::from_slice(&bytes).expect("json"))
}

#[tokio::test]
async fn a_traderless_buy_is_found_by_the_account_it_was_paid_into() {
    let dir = tempfile::tempdir().expect("tempdir");
    store_with_one_wallets_trades(dir.path(), 500);
    let state = state_at(dir.path());

    let (status, body) = get(
        &state,
        &format!("/v1/market/history/{A_MINT}?wallet={}", wallet()),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {body}");
    assert_eq!(body["mint"], A_MINT);
    assert_eq!(body["wallet"], wallet().to_string());
    assert_eq!(body["fold"]["fact"], "wallet_trades_in_window");
    assert_eq!(
        body["fold"]["complete"], false,
        "this fold can never claim to be all of a wallet's trades"
    );
    assert_eq!(body["fold"]["unattributable_trades"], 0);

    let trades = body["fold"]["trades"].as_array().expect("trades array");
    assert_eq!(trades.len(), 2, "both halves are this wallet's: {trades:?}");
    assert_eq!(
        trades[0]["matched_by"], "trader",
        "newest first, and the sell names the wallet: {trades:?}"
    );
    assert_eq!(
        trades[1]["matched_by"], "receiving_account",
        "the buy names nobody and is found by the account it was paid into: {trades:?}"
    );
    assert_eq!(trades[1]["side"], "buy");
}

#[tokio::test]
async fn a_store_the_collector_never_reported_on_says_so_rather_than_showing_no_trades() {
    // A wallet that has traded looks identical to a wallet that has not, if
    // this gate is dropped: the trades are on disk, so the fold would happily
    // return them while the collector has never confirmed it produced
    // anything here. "Nothing recorded yet" and "nothing to show" are
    // opposite facts (rule 9).
    let dir = tempfile::tempdir().expect("tempdir");
    store_that_never_collected(dir.path(), 500);
    let state = state_at(dir.path());

    let (status, body) = get(
        &state,
        &format!("/v1/market/history/{A_MINT}?wallet={}", wallet()),
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "an uncollected store must not answer as if it had looked: {body}"
    );
    assert_eq!(body["error"], "not_collected");
    assert!(
        body["message"]
            .as_str()
            .expect("a message string")
            .contains("collector"),
        "the message names the actual gap: {body}"
    );
}

#[tokio::test]
async fn asking_without_a_wallet_is_refused_rather_than_answered_for_everyone() {
    let dir = tempfile::tempdir().expect("tempdir");
    store_with_one_wallets_trades(dir.path(), 500);
    let state = state_at(dir.path());

    let (status, body) = get(&state, &format!("/v1/market/history/{A_MINT}")).await;
    assert_eq!(
        status,
        axum::http::StatusCode::BAD_REQUEST,
        "no wallet means no question to answer: {body}"
    );
}
