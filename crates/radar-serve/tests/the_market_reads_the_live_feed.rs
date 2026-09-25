// SPDX-License-Identifier: Apache-2.0
//! With a live feed configured, every market route answers from it, in the
//! shape the screen already reads, and says plainly when it has nothing yet.
//!
//! The tape here is filled through `Tape::apply`, the call the feed makes, so
//! these requests exercise everything below the network: the routes, the
//! window arithmetic, the roll-up and the JSON.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use radar_instruments::Registry;
use radar_serve::access::{KeyCache, Mode};
use radar_serve::{AppState, app};
use radar_store::{MarketSide, Reader};
use radar_stream::Live;
use radar_stream::decode::{Decoded, Fill, Holding, LaunchSeen, WSOL};
use radar_types::{Address, Signature};
use serde_json::Value;
use tower::ServiceExt;

const COIN: Address = Address::new([1; 32]);
const QUIET: Address = Address::new([2; 32]);
const TRADER: Address = Address::new([3; 32]);
const POOL: Address = Address::new([4; 32]);
const T0: i64 = 1_800_000_000;

fn router(live: Arc<Live>) -> axum::Router {
    app(Arc::new(AppState {
        admission: radar_serve::admission::Admission::Open,
        shares: radar_serve::share::Shares::new(radar_serve::share::Allowance::per_day(100)),
        customer_salt: vec![7u8; 32],
        registry: Registry::new(),
        store: Reader::open(std::env::temp_dir().join("radar-live-feed-test-empty-store")),
        x402: None,
        chat: None,
        access: Mode::Off,
        keys: KeyCache::new(),
        customer: radar_serve::customer::Mode::Off,
        customer_keys: radar_serve::customer::KeyCache::new(),
        privy: None,
        linker: radar_serve::link::Linker::new(),
        scoreboard: radar_serve::cache::Cache::new(),
        token: radar_serve::cache::Cache::new(),
        challenges: None,
        market: radar_serve::market::Market::with_live(live),
        market_snapshot: radar_serve::market::SnapshotCache::new(),
        customers: None,

        positions: None,
        trading: None,
        ticker: radar_serve::ticker::Ticker::new(),
    }))
}

fn fill(mint: Address, price: f64, side: MarketSide) -> Fill {
    Fill {
        mint,
        side,
        token_amount: 1_000.0,
        quote_amount: 1_000.0 * price,
        quote_mint: WSOL,
        price,
        trader: TRADER,
        pool: POOL,
    }
}

/// A coin launched at T0, traded three times over two minutes, with two holders.
fn filled() -> Arc<Live> {
    let live = Arc::new(Live::new(usize::MAX));
    {
        let mut tape = live.tape();
        tape.apply(
            1,
            Signature::new([1; 64]),
            T0,
            Decoded {
                launches: vec![LaunchSeen {
                    mint: COIN,
                    name: "Radar Test ".into(),
                    symbol: "RDR".into(),
                    uri: "https://example.invalid".into(),
                    creator: TRADER,
                }],
                ..Decoded::default()
            },
        );
        tape.apply(
            2,
            Signature::new([2; 64]),
            T0 + 10,
            Decoded {
                fills: vec![fill(COIN, 0.001, MarketSide::Buy)],
                holdings: vec![
                    Holding {
                        mint: COIN,
                        account: Address::new([10; 32]),
                        owner: Some(TRADER),
                        amount: 1_000_000_000,
                        decimals: 6,
                    },
                    Holding {
                        mint: COIN,
                        account: Address::new([11; 32]),
                        owner: Some(POOL),
                        amount: 9_000_000_000,
                        decimals: 6,
                    },
                ],
                ..Decoded::default()
            },
        );
        tape.apply(
            3,
            Signature::new([3; 64]),
            T0 + 20,
            Decoded {
                fills: vec![fill(COIN, 0.003, MarketSide::Buy)],
                ..Decoded::default()
            },
        );
        tape.apply(
            4,
            Signature::new([4; 64]),
            T0 + 70,
            Decoded {
                fills: vec![fill(COIN, 0.002, MarketSide::Sell)],
                ..Decoded::default()
            },
        );
        tape.apply(
            5,
            Signature::new([5; 64]),
            T0 + 75,
            Decoded {
                fills: vec![fill(QUIET, 5.0, MarketSide::Buy)],
                ..Decoded::default()
            },
        );
    }
    live
}

async fn get(live: Arc<Live>, path: &str) -> (StatusCode, Value) {
    let response = router(live)
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn the_tape_reads_the_feed_newest_first_including_the_newest_second() {
    let (status, body) = get(filled(), &format!("/v1/market/trades/{COIN}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let trades = body["trades"].as_array().unwrap();
    assert_eq!(trades.len(), 3, "{body}");
    assert_eq!(trades[0]["side"], "sell");
    assert_eq!(trades[0]["price"], 0.002);
    assert_eq!(trades[0]["trader"], TRADER.to_string());
    assert_eq!(
        body["window"]["complete"], true,
        "the feed saw this coin launch"
    );
}

#[tokio::test]
async fn candles_roll_minutes_up_to_the_interval_asked_for() {
    let live = filled();
    let (status, body) = get(
        Arc::clone(&live),
        &format!("/v1/market/candles/{COIN}?interval=1m"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let candles = body["candles"].as_array().unwrap();
    assert_eq!(candles.len(), 2, "{body}");
    assert_eq!(candles[0]["time"], T0);
    assert_eq!(
        (candles[0]["open"].as_f64(), candles[0]["close"].as_f64()),
        (Some(0.001), Some(0.003))
    );
    assert_eq!(candles[0]["trade_count"], 2);

    let (_, body) = get(live, &format!("/v1/market/candles/{COIN}?interval=5m")).await;
    let candles = body["candles"].as_array().unwrap();
    assert_eq!(candles.len(), 1, "{body}");
    assert_eq!(candles[0]["high"], 0.003);
    assert_eq!(candles[0]["close"], 0.002);
    assert_eq!(candles[0]["trade_count"], 3);
}

#[tokio::test]
async fn the_coin_list_carries_names_for_launches_the_feed_saw() {
    let (status, body) = get(filled(), "/v1/market/coins").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let coins = body["coins"].as_array().unwrap();
    assert_eq!(coins.len(), 2, "{body}");
    assert_eq!(coins[0]["mint"], COIN.to_string(), "the busier coin first");
    assert_eq!(coins[0]["symbol"], "RDR");
    assert_eq!(coins[0]["name"], "Radar Test", "creator whitespace trimmed");
    assert_eq!(coins[0]["tx_count"], 3);
    assert_eq!(coins[0]["price"], 0.002);
    assert_eq!(
        coins[1]["name"],
        Value::Null,
        "an unseen launch has no name, not a guess"
    );
}

#[tokio::test]
async fn the_header_names_the_coin_and_prices_it_from_the_newest_trade() {
    let (status, body) = get(filled(), &format!("/v1/market/token/{COIN}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["symbol"], "RDR");
    assert_eq!(body["creator"], TRADER.to_string());
    assert_eq!(body["metadata_reason"], Value::Null);
    assert_eq!(body["price"], 0.002);

    let (_, body) = get(filled(), &format!("/v1/market/token/{QUIET}")).await;
    assert_eq!(body["name"], Value::Null);
    assert!(
        body["metadata_reason"]
            .as_str()
            .unwrap()
            .contains("did not see this coin launch")
    );
}

#[tokio::test]
async fn holders_are_by_wallet_with_the_pool_marked() {
    let (status, body) = get(filled(), &format!("/v1/market/holders/{COIN}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let fold = &body["fold"];
    assert_eq!(fold["granularity"], "wallet");
    assert_eq!(fold["fact"], "balances_since_launch");
    let holders = fold["holders"].as_array().unwrap();
    assert_eq!(holders[0]["account"], POOL.to_string());
    assert_eq!(holders[0]["pool"], true);
    assert_eq!(holders[0]["balance"], 9_000.0);
    assert_eq!(holders[1]["pool"], false);
}

#[tokio::test]
async fn a_feed_with_nothing_yet_says_so_instead_of_an_empty_market() {
    let empty = Arc::new(Live::new(usize::MAX));
    for path in [
        format!("/v1/market/trades/{COIN}"),
        format!("/v1/market/candles/{COIN}"),
        "/v1/market/coins".to_owned(),
        format!("/v1/market/holders/{COIN}"),
    ] {
        let (status, body) = get(Arc::clone(&empty), &path).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{path}: {body}");
        assert_eq!(body["error"], "not_collected", "{path}");
    }
}
