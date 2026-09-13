// SPDX-License-Identifier: Apache-2.0
//! The decoder, run on real mainnet transactions.
//!
//! Each fixture is a `getTransaction` response fetched from Solana's public RPC
//! on 2026-09-13, trimmed to the fields the fold reads. It is converted into
//! the protobuf update the feed receives and pushed through the same
//! `Tx::try_from` and `decode` the feed runs, so the JSON is only a carrier.
//!
//! **Where the expected numbers come from.** For the pump.fun trades, from
//! pump.fun's own `TradeEvent` in the transaction's logs, decoded separately
//! and not by this crate: the SOL and token amounts the program itself says
//! moved. Matching them to the base unit is the evidence that reading balances
//! instead of instructions gives the same trade. The PumpSwap figure and the
//! PUMP-quoted figure have no event to check against and are pinned as
//! regressions, read off the pool's balance change by hand.

use prost::Message as _;
use radar_store::MarketSide;
use radar_stream::decode::{Decoded, WSOL, decode};
use radar_stream::proto;
use radar_stream::tx::Tx;
use serde_json::Value;

fn b58(s: &str) -> Vec<u8> {
    bs58::decode(s).into_vec().expect("base58")
}

fn instruction(v: &Value) -> (u32, Vec<u8>, Vec<u8>) {
    let program = u32::try_from(v["programIdIndex"].as_u64().unwrap()).unwrap();
    let accounts = v["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| u8::try_from(a.as_u64().unwrap()).unwrap())
        .collect();
    (program, accounts, b58(v["data"].as_str().unwrap()))
}

fn balances(v: &Value) -> Vec<proto::TokenBalance> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|b| proto::TokenBalance {
            account_index: u32::try_from(b["accountIndex"].as_u64().unwrap()).unwrap(),
            mint: b["mint"].as_str().unwrap().to_owned(),
            ui_token_amount: Some(proto::UiTokenAmount {
                decimals: u32::try_from(b["uiTokenAmount"]["decimals"].as_u64().unwrap()).unwrap(),
                amount: b["uiTokenAmount"]["amount"].as_str().unwrap().to_owned(),
            }),
            owner: b["owner"].as_str().unwrap_or_default().to_owned(),
        })
        .collect()
}

fn keys(v: &Value) -> Vec<Vec<u8>> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|k| b58(k.as_str().unwrap()))
        .collect()
}

fn u64s(v: &Value) -> Vec<u64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap())
        .collect()
}

/// The fixture as the protobuf update a Yellowstone provider would send,
/// round-tripped through its wire encoding.
fn update(name: &str) -> proto::SubscribeUpdateTransaction {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let json: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let message = &json["transaction"]["message"];
    let meta = &json["meta"];

    let update = proto::SubscribeUpdateTransaction {
        slot: json["slot"].as_u64().unwrap(),
        transaction: Some(proto::SubscribeUpdateTransactionInfo {
            signature: b58(json["transaction"]["signatures"][0].as_str().unwrap()),
            is_vote: false,
            index: 0,
            transaction: Some(proto::Transaction {
                signatures: vec![b58(json["transaction"]["signatures"][0].as_str().unwrap())],
                message: Some(proto::Message {
                    account_keys: keys(&message["accountKeys"]),
                    instructions: message["instructions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|i| {
                            let (program_id_index, accounts, data) = instruction(i);
                            proto::CompiledInstruction {
                                program_id_index,
                                accounts,
                                data,
                            }
                        })
                        .collect(),
                }),
            }),
            meta: Some(proto::TransactionStatusMeta {
                err: None,
                fee: meta["fee"].as_u64().unwrap(),
                pre_balances: u64s(&meta["preBalances"]),
                post_balances: u64s(&meta["postBalances"]),
                inner_instructions: meta["innerInstructions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|g| proto::InnerInstructions {
                        index: u32::try_from(g["index"].as_u64().unwrap()).unwrap(),
                        instructions: g["instructions"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|i| {
                                let (program_id_index, accounts, data) = instruction(i);
                                proto::InnerInstruction {
                                    program_id_index,
                                    accounts,
                                    data,
                                }
                            })
                            .collect(),
                    })
                    .collect(),
                pre_token_balances: balances(&meta["preTokenBalances"]),
                post_token_balances: balances(&meta["postTokenBalances"]),
                loaded_writable_addresses: keys(&meta["loadedAddresses"]["writable"]),
                loaded_readonly_addresses: keys(&meta["loadedAddresses"]["readonly"]),
            }),
        }),
    };
    proto::SubscribeUpdateTransaction::decode(update.encode_to_vec().as_slice())
        .expect("the update survives its own wire encoding")
}

fn decoded(name: &str) -> Decoded {
    decode(&Tx::try_from(&update(name)).expect("a readable transaction"))
}

/// Base units back from an adjusted amount, for an exact comparison.
fn units(amount: f64, decimals: i32) -> u64 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "test rounding"
    )]
    let raw = (amount * 10f64.powi(decimals)).round() as u64;
    raw
}

#[test]
fn a_pumpfun_buy_in_sol_matches_the_programs_own_trade_event() {
    let d = decoded("pumpfun_buy_in_sol");
    assert_eq!(d.fills.len(), 1, "{d:?}");
    let f = &d.fills[0];
    assert_eq!(
        f.mint.to_string(),
        "7dX2JNn1osPpSboYC45tS4SMUDtDmNkPznnkjotmpump"
    );
    assert_eq!(f.side, MarketSide::Buy);
    assert_eq!(
        f.trader.to_string(),
        "E3rU6xViiPQpFUsZsJT2En4RL95F66M8ANnn1X9zzPUg"
    );
    assert_eq!(f.quote_mint, WSOL);
    // TradeEvent: sol_amount 195555555, token_amount 2521151447491.
    assert_eq!(units(f.quote_amount, 9), 195_555_555);
    assert_eq!(units(f.token_amount, 6), 2_521_151_447_491);
    assert_eq!(
        f.pool.to_string(),
        "GCcg6xQoxBcTu44uMDPQXDAep8JczC2gteiFEUw6JZeT"
    );
}

#[test]
fn a_pumpfun_sell_in_sol_matches_the_programs_own_trade_event() {
    let d = decoded("pumpfun_sell_in_sol");
    assert_eq!(d.fills.len(), 1, "{d:?}");
    let f = &d.fills[0];
    assert_eq!(
        f.mint.to_string(),
        "AV5bZdnUrSLKjD6nwryCxBPrkdNqsDwjPYQfKcwtpump"
    );
    assert_eq!(f.side, MarketSide::Sell);
    assert_eq!(
        f.trader.to_string(),
        "CqYJEAT7DstadD8ePZCeDDAZMLrWAz7EKsMPMgH4Zwpa"
    );
    // TradeEvent: sol_amount 1098417567, token_amount 9178065107511.
    assert_eq!(units(f.quote_amount, 9), 1_098_417_567);
    assert_eq!(units(f.token_amount, 6), 9_178_065_107_511);
    assert!((f.price - 1.098_417_567 / 9_178_065.107_511).abs() < 1e-18);
}

#[test]
fn a_coin_quoted_in_another_token_is_priced_in_that_token() {
    // pump.fun curves quoted in PUMP rather than SOL. The transaction also moves
    // USDC and SOL through a route, but not at the pool, so the pool's own
    // payment -- PUMP -- is the price.
    let d = decoded("pumpfun_buy_in_pump");
    assert_eq!(d.fills.len(), 1, "{d:?}");
    let f = &d.fills[0];
    assert_eq!(
        f.mint.to_string(),
        "HP5s4uxwb4dAmLcMnoyzgFreMLSDXgFDthbXh7uTpump"
    );
    assert_eq!(f.side, MarketSide::Buy);
    assert_eq!(
        f.quote_mint.to_string(),
        "pumpCmXqMfrsAkQ5r49WcJnRayYRqmXz6ae8H7H9Dfn"
    );
    assert_eq!(units(f.quote_amount, 6), 16_650_322_136);
    assert_eq!(units(f.token_amount, 6), 9_378_107_052_613);
}

#[test]
fn a_launch_is_named_and_its_dev_buy_is_left_unpriced_rather_than_priced_with_rent() {
    let d = decoded("pumpfun_launch_with_dev_buy");
    assert_eq!(d.launches.len(), 1, "{d:?}");
    let launch = &d.launches[0];
    assert_eq!(
        launch.mint.to_string(),
        "3wtSZ7gVriBieLKj3XrSn96dv4XDgQ8fEFwA66qSpump"
    );
    assert_eq!(launch.symbol, "NABU");
    assert_eq!(launch.name.trim(), "Nabuchodonosor");
    assert!(
        launch.uri.starts_with("https://ipfs.io/ipfs/"),
        "{}",
        launch.uri
    );
    // The curve account is created in this transaction, so its lamport change
    // is rent plus payment. No price is better than a wrong one.
    assert!(d.fills.is_empty(), "{:?}", d.fills);
    assert_eq!(d.unpriced, 1);
    // The new coin's balances are still recorded: the creator and the curve.
    assert!(
        d.holdings
            .iter()
            .any(|h| h.mint == launch.mint && h.amount > 0)
    );
}

#[test]
fn a_pumpswap_sell_is_read_the_same_way() {
    let d = decoded("pumpswap_sell");
    assert_eq!(d.fills.len(), 1, "{d:?}");
    let f = &d.fills[0];
    assert_eq!(
        f.mint.to_string(),
        "Ai8uA5mWG43jKVoxHrRfzGwifSCQsybSj243PYZupump"
    );
    assert_eq!(f.side, MarketSide::Sell);
    assert_eq!(f.quote_mint, WSOL);
    assert_eq!(units(f.quote_amount, 9), 17_547_099);
    assert_eq!(units(f.token_amount, 6), 7_552_033_678);
}

#[test]
fn a_route_the_fee_payer_is_not_party_to_yields_no_trade() {
    let d = decoded("route_the_payer_is_not_party_to");
    assert!(d.fills.is_empty(), "{:?}", d.fills);
    assert_eq!(d.unpriced, 0, "the payer's coin balance never moved");
}

#[test]
fn a_transaction_that_moved_nothing_yields_nothing() {
    let d = decoded("nothing_moved");
    assert!(d.fills.is_empty());
    assert!(d.launches.is_empty());
}

#[test]
fn lookup_table_addresses_extend_the_account_list_in_order() {
    // Writable loaded addresses follow the message's keys, read-only ones
    // follow those. Balances index into that combined list.
    let u = update("pumpfun_buy_in_pump");
    let tx = Tx::try_from(&u).unwrap();
    let meta = u.transaction.as_ref().unwrap().meta.as_ref().unwrap();
    let message_keys = u
        .transaction
        .as_ref()
        .unwrap()
        .transaction
        .as_ref()
        .unwrap()
        .message
        .as_ref()
        .unwrap()
        .account_keys
        .len();
    assert!(
        !meta.loaded_writable_addresses.is_empty(),
        "the fixture uses a lookup table"
    );
    assert_eq!(
        tx.accounts.len(),
        message_keys + meta.loaded_writable_addresses.len() + meta.loaded_readonly_addresses.len()
    );
    assert_eq!(
        tx.accounts[message_keys].as_bytes().as_slice(),
        meta.loaded_writable_addresses[0].as_slice()
    );
}

#[test]
fn a_failed_transaction_is_refused_not_folded() {
    let mut u = update("pumpfun_buy_in_sol");
    u.transaction.as_mut().unwrap().meta.as_mut().unwrap().err =
        Some(proto::TransactionError { err: vec![1] });
    assert_eq!(Tx::try_from(&u), Err(radar_stream::tx::Unreadable::Failed));
}

#[test]
fn an_unreadable_balance_refuses_the_whole_transaction() {
    let mut u = update("pumpfun_buy_in_sol");
    u.transaction
        .as_mut()
        .unwrap()
        .meta
        .as_mut()
        .unwrap()
        .post_token_balances[0]
        .ui_token_amount
        .as_mut()
        .unwrap()
        .amount = "not a number".into();
    assert!(matches!(
        Tx::try_from(&u),
        Err(radar_stream::tx::Unreadable::Malformed(_))
    ));
}
