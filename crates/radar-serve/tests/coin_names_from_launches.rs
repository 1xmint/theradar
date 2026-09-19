// SPDX-License-Identifier: Apache-2.0
//! `/v1/market/coins` and `/v1/market/token/{mint}` carrying a coin's real
//! name, symbol and metadata uri, joined in from Radar's own recorded
//! pump.fun launches rather than a CryptoHouse query -- plan 0013, phase B.
//!
//! A mint with no recorded launch keeps `null` and an honest
//! `metadata_reason` (rule 9: absent is not zero). The join is built once per
//! watermark and cached -- [`the_launches_table_is_not_read_per_request`]
//! proves that by deleting the launches partition between two requests at the
//! same watermark and checking the second answer is unaffected.

use std::sync::Arc;

use radar_backfill::market_tape::coverage_record;
use radar_instruments::Registry;
use radar_serve::{AppState, app};
use radar_store::{Completion, MarketSide, MarketTrade, Reader, Writer};
use radar_types::{Address, Signature, Slot};
use tower::ServiceExt;

const A_MINT: &str = "5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump";
const WSOL: &str = "So11111111111111111111111111111111111111112";

fn launch(mint: &str, slot: u64, name: &str, symbol: &str, uri: &str) -> radar_store::Event {
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
        uri: uri.to_owned(),
        dev_buy_lamports: None,
    }))
}

fn market_trade(mint: &str, slot: u64, ts: &str) -> MarketTrade {
    MarketTrade {
        mint: mint.parse().expect("a mint"),
        ts: ts.to_owned(),
        slot: Slot(slot),
        signature: Signature::new([9u8; 64]),
        side: MarketSide::Unknown,
        token_amount: 1.0,
        quote_amount: Some(2.0),
        quote_mint: Some(WSOL.parse().expect("a mint")),
        price: Some(2.0),
        trader: None,
    }
}

/// A store with one traded, launch-recorded mint, ready for `market_tape`'s
/// coverage gate. `slot` is the watermark this store settles at.
fn store_with_launch(
    dir: &std::path::Path,
    mint: &str,
    slot: u64,
    name: &str,
    symbol: &str,
    uri: &str,
) {
    let mut writer = Writer::open(dir, 64).expect("open");
    writer
        .append(launch(mint, slot, name, symbol, uri))
        .expect("append launch");
    // Two trades: the window's `to` is the *newest* trade's own timestamp and
    // `within_window` is right-exclusive (`ts < to`), so the newest trade
    // never appears in its own window. The earlier one is what the coin list
    // and token tape actually see.
    let newest = market_trade(mint, slot, "2026-09-18 00:00:00");
    let earlier = market_trade(mint, slot - 1, "2026-09-17 23:59:59");
    writer
        .append_market_trade(newest.clone())
        .expect("append newest trade");
    writer
        .append_market_trade(earlier.clone())
        .expect("append earlier trade");
    writer
        .append_coverage(coverage_record(
            Completion::Complete,
            &[newest, earlier],
            Slot(slot),
        ))
        .expect("append coverage");
    writer.flush().expect("flush");
}

/// A store with a traded mint that has no recorded launch.
fn store_without_launch(dir: &std::path::Path, mint: &str, slot: u64) {
    let mut writer = Writer::open(dir, 64).expect("open");
    let newest = market_trade(mint, slot, "2026-09-18 00:00:00");
    let earlier = market_trade(mint, slot - 1, "2026-09-17 23:59:59");
    writer
        .append_market_trade(newest.clone())
        .expect("append newest trade");
    writer
        .append_market_trade(earlier.clone())
        .expect("append earlier trade");
    writer
        .append_coverage(coverage_record(
            Completion::Complete,
            &[newest, earlier],
            Slot(slot),
        ))
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
        launches: radar_serve::cache::Cache::new(),
    })
}

async fn get_json(state: &Arc<AppState>, uri: &str) -> serde_json::Value {
    let response = app(Arc::clone(state))
        .oneshot(
            axum::http::Request::builder()
                .uri(uri)
                .body(axum::body::Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "the route answered"
    );
    let bytes = axum::body::to_bytes(response.into_body(), 4_000_000)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("json")
}

#[tokio::test]
async fn a_recorded_launch_names_a_coin_in_coins_and_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    store_with_launch(
        dir.path(),
        A_MINT,
        500,
        "Radar Coin",
        "RADAR",
        "https://example.test/meta.json",
    );
    let state = state_at(dir.path());

    let coins = get_json(&state, "/v1/market/coins").await;
    let coin = coins["coins"]
        .as_array()
        .expect("coins array")
        .iter()
        .find(|c| c["mint"] == A_MINT)
        .unwrap_or_else(|| panic!("the traded mint is in the list: {coins}"));
    assert_eq!(coin["name"], "Radar Coin");
    assert_eq!(coin["symbol"], "RADAR");
    assert_eq!(coin["uri"], "https://example.test/meta.json");

    let token = get_json(&state, &format!("/v1/market/token/{A_MINT}")).await;
    assert_eq!(token["name"], "Radar Coin");
    assert_eq!(token["symbol"], "RADAR");
    assert_eq!(token["uri"], "https://example.test/meta.json");
    assert!(
        token["metadata_reason"]
            .as_str()
            .expect("a reason string")
            .contains("recorded"),
        "a found launch still explains what is and is not read: {}",
        token["metadata_reason"]
    );
}

#[tokio::test]
async fn a_mint_with_no_recorded_launch_keeps_nulls_and_an_honest_reason() {
    let dir = tempfile::tempdir().expect("tempdir");
    store_without_launch(dir.path(), A_MINT, 500);
    let state = state_at(dir.path());

    let token = get_json(&state, &format!("/v1/market/token/{A_MINT}")).await;
    assert!(token["name"].is_null());
    assert!(token["symbol"].is_null());
    assert!(token["uri"].is_null());
    let reason = token["metadata_reason"].as_str().expect("a reason string");
    assert!(
        reason.contains("no pump.fun launch"),
        "the reason names the actual gap, not the old blanket claim: {reason}"
    );

    let coins = get_json(&state, "/v1/market/coins").await;
    let coin = coins["coins"]
        .as_array()
        .expect("coins array")
        .iter()
        .find(|c| c["mint"] == A_MINT)
        .expect("the traded mint is in the list");
    assert!(coin["name"].is_null());
    assert!(coin["symbol"].is_null());
    assert!(coin["uri"].is_null());
}

#[tokio::test]
async fn an_overlong_name_is_capped_in_the_response() {
    let dir = tempfile::tempdir().expect("tempdir");
    let long_name = "n".repeat(500);
    store_with_launch(
        dir.path(),
        A_MINT,
        500,
        &long_name,
        "S",
        "https://example.test/x.json",
    );
    let state = state_at(dir.path());

    let token = get_json(&state, &format!("/v1/market/token/{A_MINT}")).await;
    let name = token["name"].as_str().expect("a name string");
    assert_eq!(
        name.chars().count(),
        200,
        "an attacker-controlled name is bounded before it leaves the server, not passed through whole"
    );
    assert!(
        long_name.starts_with(name),
        "the cap truncates rather than mangling"
    );
}

#[tokio::test]
async fn the_launches_table_is_not_read_per_request() {
    let dir = tempfile::tempdir().expect("tempdir");
    store_with_launch(
        dir.path(),
        A_MINT,
        500,
        "Radar Coin",
        "RADAR",
        "https://example.test/meta.json",
    );
    let state = state_at(dir.path());

    // First request builds and caches the launch index for watermark 500.
    let first = get_json(&state, &format!("/v1/market/token/{A_MINT}")).await;
    assert_eq!(first["name"], "Radar Coin");

    // Delete only the launches partition, leaving trades and coverage intact.
    // A handler that reads the launches table per request would find nothing
    // here and answer with a null name on the very next call.
    std::fs::remove_dir_all(dir.path().join("launches")).expect("remove the launches partition");

    let second = get_json(&state, &format!("/v1/market/token/{A_MINT}")).await;
    assert_eq!(
        second["name"], "Radar Coin",
        "the cached launch index answers this request, not a fresh scan of a table that is now gone"
    );
}
