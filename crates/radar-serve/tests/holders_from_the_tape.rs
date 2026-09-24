// SPDX-License-Identifier: Apache-2.0
//! `/v1/market/holders/{mint}` on the free path -- plan 0013, phase B item 2.
//!
//! Without a live feed the collector never gathers raw transfers, so this
//! route used to refuse outright. It now folds the trade tape Radar already
//! stores into net buy-minus-sell per wallet -- modelled on
//! `coin_names_from_launches.rs`'s `state_at`/`get_json` -- and states the
//! gap plainly (`fact: "net_traded_in_window"`, `complete: false`) rather
//! than passing the fold off as a full holder list.
//!
//! **Unlike the tape route, this fold is not windowed to a display slice.**
//! `coin_names_from_launches.rs` explains why its newest trade falls outside
//! `/v1/market/trades`' own window (right-exclusive on the newest trade's own
//! timestamp); a holders fold answers "who holds now", which needs every
//! trade of the mint the snapshot has, so both trades below are counted.

use std::sync::Arc;

use radar_backfill::market_tape::coverage_record;
use radar_instruments::Registry;
use radar_serve::{AppState, app};
use radar_store::{Completion, MarketSide, MarketTrade, Reader, Writer};
use radar_types::{Address, Signature, Slot};
use tower::ServiceExt;

const A_MINT: &str = "5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump";
const UNTRADED_MINT: &str = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";
const WSOL: &str = "So11111111111111111111111111111111111111112";
const WALLET: [u8; 32] = [4u8; 32];

fn market_trade(
    mint: &str,
    ts: &str,
    slot: u64,
    side: MarketSide,
    token_amount: f64,
    trader: Option<Address>,
) -> MarketTrade {
    MarketTrade {
        mint: mint.parse().expect("a mint"),
        ts: ts.to_owned(),
        slot: Slot(slot),
        signature: Signature::new([(slot % 251) as u8; 64]),
        side,
        token_amount,
        quote_amount: Some(2.0),
        quote_mint: Some(WSOL.parse().expect("a mint")),
        price: Some(2.0),
        trader,
        token_destination: None,
    }
}

/// A store with a mint traded by one wallet: a buy of 5, then a sell of 2,
/// netting 3 -- and coverage recorded so the collected-gate passes.
fn store_with_a_net_holder(dir: &std::path::Path, mint: &str, slot: u64) {
    let mut writer = Writer::open(dir, 64).expect("open");
    let wallet = Address::new(WALLET);
    let buy = market_trade(
        mint,
        "2026-09-17 23:59:00",
        slot - 1,
        MarketSide::Buy,
        5.0,
        Some(wallet),
    );
    let sell = market_trade(
        mint,
        "2026-09-18 00:00:00",
        slot,
        MarketSide::Sell,
        2.0,
        Some(wallet),
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

/// A store that has collected (coverage exists) but never traded `A_MINT`.
fn store_without_a_mint_trade(dir: &std::path::Path, slot: u64) {
    let mut writer = Writer::open(dir, 64).expect("open");
    let other = market_trade(
        UNTRADED_MINT,
        "2026-09-18 00:00:00",
        slot,
        MarketSide::Buy,
        5.0,
        Some(Address::new(WALLET)),
    );
    writer
        .append_market_trade(other.clone())
        .expect("append other trade");
    writer
        .append_coverage(coverage_record(Completion::Complete, &[other], Slot(slot)))
        .expect("append coverage");
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
async fn a_net_positive_wallet_shows_up_as_net_traded_in_window() {
    let dir = tempfile::tempdir().expect("tempdir");
    store_with_a_net_holder(dir.path(), A_MINT, 500);
    let state = state_at(dir.path());

    let (status, body) = get(&state, &format!("/v1/market/holders/{A_MINT}")).await;
    assert_eq!(status, axum::http::StatusCode::OK, "body: {body}");
    assert_eq!(body["mint"], A_MINT);
    assert_eq!(body["fold"]["fact"], "net_traded_in_window");
    assert_eq!(body["fold"]["granularity"], "wallet");
    assert_eq!(
        body["fold"]["complete"], false,
        "this fold can never claim to be everyone"
    );
    assert_eq!(body["fold"]["from"], "2026-09-17 23:59:00");
    assert_eq!(body["fold"]["to"], "2026-09-18 00:00:00");
    assert_eq!(body["fold"]["unattributed_trades"], 0);

    let holders = body["fold"]["holders"].as_array().expect("holders array");
    assert_eq!(holders.len(), 1, "one wallet nets positive: {holders:?}");
    let wallet = Address::new(WALLET).to_string();
    assert_eq!(holders[0]["account"], wallet);
    assert!(
        (holders[0]["balance"].as_f64().expect("a balance") - 3.0).abs() < f64::EPSILON,
        "5 bought minus 2 sold nets 3: {holders:?}"
    );
    assert_eq!(holders[0]["pool"], false);
}

#[tokio::test]
async fn an_untraded_mint_is_not_collected_rather_than_an_empty_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    store_without_a_mint_trade(dir.path(), 500);
    let state = state_at(dir.path());

    let (status, body) = get(&state, &format!("/v1/market/holders/{A_MINT}")).await;
    assert_eq!(
        status,
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "an untraded mint must not look like a quiet market: {body}"
    );
    assert_eq!(body["error"], "not_collected");
    assert!(
        body["message"]
            .as_str()
            .expect("a message string")
            .contains("no trades"),
        "the message names the actual gap: {body}"
    );
}
