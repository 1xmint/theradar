// SPDX-License-Identifier: Apache-2.0
//! `/v1/market/launches` -- plan 0013, phase B item 3 (the "newly launched"
//! list).
//!
//! **Zero CryptoHouse queries.** This route reads only the cached
//! [`radar_serve::market::LaunchIndex`] a [`radar_serve::market::Snapshot`]
//! already builds from [`radar_store::Table::Launches`] -- the same table
//! `coin_names_from_launches.rs` covers for `/v1/market/coins` and
//! `/v1/market/token/{mint}`. No market trade and no coverage record is
//! written by any fixture below, because this route depends on neither.

use std::sync::Arc;

use radar_instruments::Registry;
use radar_serve::{AppState, app};
use radar_store::{MarketSide, MarketTrade, Reader, Writer};
use radar_types::{Address, Signature, Slot};
use tower::ServiceExt;

const MINT_A: &str = "5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump";
const MINT_B: &str = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";
const MINT_C: &str = "BLoS31jkH1nRfCDvxNbFbz1r9SKUB6EshCmfcbxaMbNq";

fn launch(mint: &str, slot: u64, name: &str, symbol: &str) -> radar_store::Event {
    radar_store::Event::Launch(Box::new(radar_store::Launch {
        envelope: radar_store::Envelope {
            slot: Slot(slot),
            signature: Signature::new([(slot % 251) as u8; 64]),
            tx_index: Some(0),
            instruction_index: 0,
            parent_index: None,
            success: Some(true),
        },
        origin: radar_store::Origin::known(Address::new([3u8; 32]), "create_v2"),
        mint: mint.parse().expect("a mint"),
        creator: Address::new([2u8; 32]),
        name: name.to_owned(),
        symbol: symbol.to_owned(),
        uri: format!("ipfs://{name}"),
        dev_buy_lamports: None,
    }))
}

/// Three launches at three different slots, all inside the lookback window
/// of `watermark`, written in an order that is neither slot-ascending nor
/// slot-descending -- so a test that passes only because it happened to read
/// them back in storage order would be a false negative.
fn store_with_three_launches(dir: &std::path::Path, watermark: u64) {
    let mut writer = Writer::open(dir, 64).expect("open");
    writer
        .append(launch(MINT_B, watermark - 100, "Bee", "BEE"))
        .expect("append B");
    writer
        .append(launch(MINT_A, watermark, "Ay", "AY"))
        .expect("append A, the newest");
    writer
        .append(launch(MINT_C, watermark - 200, "Cee", "CEE"))
        .expect("append C, the oldest");
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

        positions: None,
        trading: None,
        ticker: radar_serve::ticker::Ticker::new(),
        market_ticker: radar_serve::ticker::Ticker::new(),
        market_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(
            radar_serve::MARKET_EVENTS_MAX_CONNECTIONS,
        )),
        market_visitors: std::sync::Arc::default(),
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
async fn the_newest_launch_leads_and_the_oldest_trails() {
    let dir = tempfile::tempdir().expect("tempdir");
    store_with_three_launches(dir.path(), 500);
    let state = state_at(dir.path());

    let (status, body) = get(&state, "/v1/market/launches").await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {body}");

    let launches = body["launches"].as_array().expect("launches array");
    assert_eq!(
        launches.len(),
        3,
        "all three recorded launches: {launches:?}"
    );
    // Newest first (A at watermark), then B, then C the oldest. A reversed
    // sort, or one that ignored slot entirely and fell back to insertion
    // order, would put B or C first here.
    assert_eq!(launches[0]["mint"], MINT_A);
    assert_eq!(launches[0]["symbol"], "AY");
    assert_eq!(launches[1]["mint"], MINT_B);
    assert_eq!(launches[2]["mint"], MINT_C);
    assert_eq!(
        body["complete"], false,
        "this list never claims to be everyone"
    );
}

#[tokio::test]
async fn a_limit_of_one_returns_only_the_newest() {
    let dir = tempfile::tempdir().expect("tempdir");
    store_with_three_launches(dir.path(), 500);
    let state = state_at(dir.path());

    let (status, body) = get(&state, "/v1/market/launches?limit=1").await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {body}");
    let launches = body["launches"].as_array().expect("launches array");
    assert_eq!(
        launches.len(),
        1,
        "the limit must actually cap the response, not just the display: {launches:?}"
    );
    assert_eq!(launches[0]["mint"], MINT_A);
}

#[tokio::test]
async fn no_recorded_launches_is_not_collected_rather_than_an_empty_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    // A store that has a watermark -- one unrelated market trade, so
    // `watermark_of` succeeds -- but no `Launches` row at all. The gap this
    // test proves is specifically the launch index being empty, not the
    // store being empty outright.
    let mut writer = Writer::open(dir.path(), 64).expect("open");
    writer
        .append_market_trade(MarketTrade {
            mint: MINT_A.parse().expect("a mint"),
            ts: "2026-09-18 00:00:00".to_owned(),
            slot: Slot(500),
            signature: Signature::new([9u8; 64]),
            side: MarketSide::Unknown,
            token_amount: 1.0,
            quote_amount: None,
            quote_mint: None,
            price: None,
            trader: None,
            token_destination: None,
        })
        .expect("append trade");
    writer.flush().expect("flush");
    let state = state_at(dir.path());

    let (status, body) = get(&state, "/v1/market/launches").await;
    assert_eq!(
        status,
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "an empty index must not look like a quiet launch window: {body}"
    );
    assert_eq!(body["error"], "not_collected");
    assert!(
        body["message"]
            .as_str()
            .expect("a message string")
            .contains("launch"),
        "the message names the actual gap: {body}"
    );
}
