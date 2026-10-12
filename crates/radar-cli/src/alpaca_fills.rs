// SPDX-License-Identifier: Apache-2.0
//! Connect retained intent, order recovery and activities without releasing claims.

use std::collections::BTreeSet;

use radar_journal::{BrokerIntent, BrokerLog, BrokerSide};
use serde::Serialize;
use serde_json::{Value, json};

use super::alpaca_activities::Activity;
use super::alpaca_exposure::nanos;
use super::alpaca_order_read as recovery;
use super::alpaca_reconcile::BrokerOrder;

const PAPER_API: &str = "https://paper-api.alpaca.markets";

#[derive(Debug, PartialEq, Eq, Serialize)]
struct Executions {
    count: usize,
    filled_nanoshares: u64,
    gross_floor_microusd: u64,
    gross_ceil_microusd: u64,
}

fn amount(value: Option<&str>) -> Result<u64, String> {
    nanos(value.ok_or("execution decimal is unknown")?)
}

fn executions(
    intent: &BrokerIntent,
    order: &BrokerOrder,
    activities: &[Activity],
) -> Result<Executions, String> {
    let filled = amount(order.filled_qty.as_deref())?;
    if filled > intent.quantity_nanoshares
        || !matches!(
            order.status.as_str(),
            "new"
                | "accepted"
                | "pending_new"
                | "partially_filled"
                | "filled"
                | "canceled"
                | "expired"
                | "pending_cancel"
                | "done_for_day"
                | "rejected"
        )
        || (order.status == "filled" && filled != intent.quantity_nanoshares)
    {
        return Err("unsupported or inconsistent paper order fill state".into());
    }
    let side = match intent.side {
        BrokerSide::Buy => "buy",
        BrokerSide::Sell => "sell",
    };
    let limit = intent
        .limit_microusd_per_share
        .checked_mul(1_000)
        .ok_or("retained price exceeds execution decimal range")?;
    let mut seen = BTreeSet::new();
    let mut intervals = Vec::new();
    let mut gross = 0_u128;
    for event in activities {
        if event.category != "FILL" {
            continue;
        }
        // An unattributable fill cannot be declared unrelated to this order.
        let order_id = event
            .order_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
            .ok_or("fill order identity is unknown")?;
        if order_id != order.id {
            continue;
        }
        if event.id.trim().is_empty()
            || !seen.insert(&event.id)
            || event.symbol.as_deref() != Some(intent.symbol.as_str())
            || event.side.as_deref() != Some(side)
            || !matches!(event.kind.as_deref(), Some("fill" | "partial_fill"))
        {
            return Err("fill identity or payload differs from retained order".into());
        }
        let qty = amount(event.qty.as_deref())?;
        let price = amount(event.price.as_deref())?;
        let cumulative = amount(event.cum_qty.as_deref())?;
        let leaves = amount(event.leaves_qty.as_deref())?;
        if qty == 0
            || price == 0
            || cumulative > intent.quantity_nanoshares
            || leaves != intent.quantity_nanoshares - cumulative
            || (side == "buy" && price > limit)
            || (side == "sell" && price < limit)
        {
            return Err("fill quantity, remaining quantity or limit is inconsistent".into());
        }
        let start = cumulative
            .checked_sub(qty)
            .ok_or("fill cumulative quantity underflow")?;
        intervals.push((start, cumulative));
        gross = gross
            .checked_add(u128::from(qty) * u128::from(price))
            .ok_or("gross fill amount overflow")?;
    }
    // Broker response and timestamp order need not establish execution order.
    intervals.sort_unstable();
    let mut end = 0;
    for &(start, cumulative) in &intervals {
        if start != end {
            return Err("fill history has a gap or overlap".into());
        }
        end = cumulative;
    }
    if end != filled {
        return Err("fill history differs from observed order quantity".into());
    }
    // Multiply nanoshares by nano-USD/share, then round the aggregate once.
    // Rounded broker average prices cannot substitute for execution prices.
    let divisor = 1_000_000_000_000;
    Ok(Executions {
        count: intervals.len(),
        filled_nanoshares: filled,
        gross_floor_microusd: u64::try_from(gross / divisor).map_err(|_| "gross USD overflow")?,
        gross_ceil_microusd: u64::try_from(gross.div_ceil(divisor))
            .map_err(|_| "gross USD overflow")?,
    })
}

fn collect(
    log: &BrokerLog,
    expected: &str,
    client_id: &str,
    max_pages: u16,
    mut read: impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<Value, String> {
    if max_pages == 0 {
        return Err("positive activity page limit is required".into());
    }
    let intent = recovery::retained(log, client_id)?;
    let before = recovery::collect_bound(expected, client_id, Some(&intent), &mut read)?;
    if before["intent_payload_verified"] != true {
        return Err("retained paper order is not visible; reconciliation unresolved".into());
    }
    let activity = super::alpaca_activities::collect(expected, max_pages, |path| {
        read(path)?.ok_or_else(|| "paper activity observation is missing".into())
    })?;
    let after = recovery::collect_bound(expected, client_id, Some(&intent), &mut read)?;
    if before["order"] != after["order"]
        || [
            &before["account_after"],
            &activity["account_before"],
            &activity["account_after"],
            &after["account_before"],
            &after["account_after"],
        ]
        .iter()
        .any(|account| **account != before["account_before"])
    {
        return Err("paper order or account changed across execution reconciliation".into());
    }
    let order: BrokerOrder = serde_json::from_value(before["order"].clone())
        .map_err(|_| "selected paper order is invalid")?;
    let events: Vec<Activity> = serde_json::from_value(activity["activities"].clone())
        .map_err(|_| "selected paper activities are invalid")?;
    let matched = executions(&intent, &order, &events)?;
    let cash_claim = intent.cash_claim_microusd().map_err(|e| e.to_string())?;
    let share_claim = match intent.side {
        BrokerSide::Buy => 0,
        BrokerSide::Sell => intent.quantity_nanoshares,
    };
    Ok(
        json!({"version":1,"venue":"alpaca","environment":"paper","source":PAPER_API,
        "intent":intent,"order_before":before,"activity_observations":activity,"order_after":after,
        "execution_quantities_matched":true,"executions":matched,
        "held_cash_claim_microusd":cash_claim.to_string(),
        "held_share_claim_nanoshares":share_claim.to_string(),
        "atomic_snapshot":false,"fills_reconciled":false,"average_price_reconciled":false,
        "economic_history_complete":false,"fees_final":false,"cash_reconciled":false,
        "retry_authorized":false,"claim_release_authorized":false,"execution_authority":false}),
    )
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let [
        "alpaca-order-reconcile",
        "--journal",
        path,
        "--account-id",
        expected,
        "--client-order-id",
        client_id,
        "--max-pages",
        pages,
    ] = values.as_slice()
    else {
        return Err("usage: radar alpaca-order-reconcile --journal <private-file> --account-id <paper-id> --client-order-id <retained-id> --max-pages <positive-u16>".into());
    };
    let max_pages = super::alpaca_activities::page_limit(pages)?;
    recovery::query(client_id)?;
    let log = recovery::open_existing(path, expected)?;
    recovery::retained(&log, client_id)?;
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret =
        super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let report = collect(&log, expected, client_id, max_pages, |path| {
        if path.starts_with("/v2/orders:") {
            recovery::read_path(&agent, PAPER_API, path, &key, &secret)
        } else {
            super::alpaca_read::get(&agent, &format!("{PAPER_API}{path}"), &key, &secret).map(Some)
        }
    })?;
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent() -> BrokerIntent {
        BrokerIntent {
            account_id: "paper-id".into(),
            client_order_id: "retained_1".into(),
            symbol: "SPY".into(),
            side: BrokerSide::Buy,
            quantity_nanoshares: 3_000_000_000,
            limit_microusd_per_share: 100_000_000,
            fee_bound_microusd: 7,
        }
    }

    fn order() -> Value {
        json!({"id":"broker-id","client_order_id":"retained_1","symbol":"SPY",
            "asset_class":"us_equity","side":"buy","type":"limit","order_class":"simple",
            "time_in_force":"day","extended_hours":false,"qty":"3","limit_price":"100",
            "filled_qty":"3","filled_avg_price":"99.996667","status":"filled"})
    }

    fn events() -> Value {
        json!([{"id":"b","activity_type":"FILL","order_id":"broker-id","symbol":"SPY",
            "side":"buy","type":"fill","qty":"2","price":"100","cum_qty":"3","leaves_qty":"0"},
            {"id":"a","activity_type":"FILL","order_id":"broker-id","symbol":"SPY",
            "side":"buy","type":"partial_fill","qty":"1","price":"99.99","cum_qty":"1","leaves_qty":"2"}])
    }

    fn matched(intent: &BrokerIntent, order: Value, events: Value) -> Result<Executions, String> {
        executions(
            intent,
            &serde_json::from_value(order).unwrap(),
            &serde_json::from_value::<Vec<Activity>>(events).unwrap(),
        )
    }

    fn account() -> String {
        json!({"id":"paper-id","cash":"50","private":"omit"}).to_string()
    }

    fn response(path: &str) -> Option<String> {
        Some(if path == "/v2/account" {
            account()
        } else if path.starts_with("/v2/orders:") {
            order().to_string()
        } else {
            events().to_string()
        })
    }

    #[test]
    fn execution_intervals_ignore_response_order_and_match_both_sides_and_partial_states() {
        for side in [BrokerSide::Buy, BrokerSide::Sell] {
            let mut intent = intent();
            intent.side = side;
            let mut order = order();
            let mut events = events();
            if side == BrokerSide::Sell {
                order["side"] = json!("sell");
                for event in events.as_array_mut().unwrap() {
                    event["side"] = json!("sell");
                }
                events[1]["price"] = json!("100.01");
            }
            for reverse in [false, true] {
                if reverse {
                    events.as_array_mut().unwrap().reverse();
                }
                let result = matched(&intent, order.clone(), events.clone()).unwrap();
                assert_eq!(
                    result,
                    Executions {
                        count: 2,
                        filled_nanoshares: 3_000_000_000,
                        gross_floor_microusd: if side == BrokerSide::Buy {
                            299_990_000
                        } else {
                            300_010_000
                        },
                        gross_ceil_microusd: if side == BrokerSide::Buy {
                            299_990_000
                        } else {
                            300_010_000
                        }
                    }
                );
            }
        }
        let mut partial = order();
        partial["status"] = json!("partially_filled");
        partial["filled_qty"] = json!("1");
        let result = matched(&intent(), partial.clone(), json!([events()[1]])).unwrap();
        assert_eq!(result.count, 1);
        assert_eq!(result.filled_nanoshares, 1_000_000_000);
        assert_eq!(result.gross_floor_microusd, 99_990_000);
        for status in [
            "new",
            "accepted",
            "pending_new",
            "canceled",
            "expired",
            "pending_cancel",
            "done_for_day",
            "rejected",
        ] {
            partial["status"] = json!(status);
            partial["filled_qty"] = json!("0");
            assert_eq!(
                matched(&intent(), partial.clone(), json!([])).unwrap(),
                Executions {
                    count: 0,
                    filled_nanoshares: 0,
                    gross_floor_microusd: 0,
                    gross_ceil_microusd: 0
                }
            );
        }
    }

    #[test]
    fn gross_rounding_happens_once_after_exact_products_and_refuses_unsupported_ranges() {
        let mut intent = intent();
        intent.quantity_nanoshares = 2;
        let mut order = order();
        order["filled_qty"] = json!("0.000000002");
        let mut events = events();
        for (index, cumulative) in ["0.000000001", "0.000000002"].iter().enumerate() {
            events[index]["qty"] = json!("0.000000001");
            events[index]["price"] = json!("100");
            events[index]["cum_qty"] = json!(cumulative);
            events[index]["leaves_qty"] = json!(if index == 0 { "0.000000001" } else { "0" });
        }
        assert_eq!(
            matched(&intent, order.clone(), events.clone()).unwrap(),
            Executions {
                count: 2,
                filled_nanoshares: 2,
                gross_floor_microusd: 0,
                gross_ceil_microusd: 1
            }
        );
        intent.limit_microusd_per_share = u64::MAX;
        assert!(matched(&intent, order.clone(), events.clone()).is_err());
        intent.quantity_nanoshares = u64::MAX;
        intent.limit_microusd_per_share = 18_000_000_000_000_000;
        order["filled_qty"] = json!("18446744073.709551615");
        let mut event = events[0].clone();
        event["qty"] = order["filled_qty"].clone();
        event["cum_qty"] = order["filled_qty"].clone();
        event["leaves_qty"] = json!("0");
        event["price"] = json!("18000000000");
        assert_eq!(
            matched(&intent, order, json!([event])).unwrap_err(),
            "gross USD overflow"
        );
    }

    #[test]
    fn gaps_overlaps_duplicates_overfills_and_limit_breaches_refuse() {
        let mut gap = events();
        gap[0]["qty"] = json!("1");
        let mut overlap = events();
        overlap[0]["qty"] = json!("3");
        let mut duplicate = events();
        duplicate[0]["id"] = duplicate[1]["id"].clone();
        for value in [
            gap,
            overlap,
            duplicate,
            json!([events()[0]]),
            json!([events()[1]]),
            json!([]),
        ] {
            assert!(matched(&intent(), order(), value).is_err());
        }
        for (field, value) in [
            ("qty", "0"),
            ("qty", "4"),
            ("price", "0"),
            ("price", "100.000000001"),
            ("cum_qty", "4"),
            ("cum_qty", "1"),
            ("leaves_qty", "1"),
        ] {
            let mut events = events();
            events[0][field] = json!(value);
            assert!(
                matched(&intent(), order(), events).is_err(),
                "{field}={value}"
            );
        }
        let mut intent = intent();
        intent.side = BrokerSide::Sell;
        let mut events = events();
        for event in events.as_array_mut().unwrap() {
            event["side"] = json!("sell");
        }
        assert!(matched(&intent, order(), events).is_err());
        for (status, qty) in [("future-status", "3"), ("filled", "2"), ("new", "4")] {
            let mut order = order();
            order["status"] = json!(status);
            order["filled_qty"] = json!(qty);
            assert!(matched(&intent, order, json!([])).is_err());
        }
    }

    #[test]
    fn unknown_execution_fields_cannot_be_zero_or_unrelated_fills() {
        for field in [
            "id",
            "order_id",
            "symbol",
            "side",
            "type",
            "qty",
            "price",
            "cum_qty",
            "leaves_qty",
        ] {
            for value in [
                Value::Null,
                json!(""),
                json!(" "),
                json!(if field == "id" { "a" } else { "other" }),
            ] {
                let mut events = events();
                events[0][field] = value;
                let parsed = serde_json::from_value::<Vec<Activity>>(events);
                assert!(
                    parsed.is_err()
                        || executions(
                            &intent(),
                            &serde_json::from_value(order()).unwrap(),
                            &parsed.unwrap()
                        )
                        .is_err(),
                    "{field}"
                );
            }
        }
        for field in ["qty", "price", "cum_qty", "leaves_qty"] {
            for value in ["-1", "1e0", "0.0000000001", "18446744074", "1."] {
                let mut events = events();
                events[0][field] = json!(value);
                assert!(matched(&intent(), order(), events).is_err());
            }
        }
        let mut observed = order();
        for value in [Value::Null, json!("bad")] {
            observed["filled_qty"] = value;
            assert!(matched(&intent(), observed.clone(), events()).is_err());
        }
        let mut additional = events();
        additional.as_array_mut().unwrap().extend([
            json!({"id":"fee","activity_type":"FEE","net_amount":"-0.01"}),
            json!({"id":"foreign","activity_type":"FILL","order_id":"another-order"}),
        ]);
        assert_eq!(matched(&intent(), order(), additional).unwrap().count, 2);
    }

    #[test]
    fn actual_collection_brackets_activities_and_keeps_entire_claims_and_journal_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("claims.jsonl");
        for side in [BrokerSide::Buy, BrokerSide::Sell] {
            let mut intent = intent();
            intent.side = side;
            intent.client_order_id = format!("retained_{side:?}");
            let mut log = BrokerLog::open(&path, "paper-id").unwrap();
            log.reserve(intent.clone(), 1).unwrap();
            let prior = std::fs::read(&path).unwrap();
            let mut paths = Vec::new();
            let report = collect(&log, "paper-id", &intent.client_order_id, 1, |path| {
                paths.push(path.to_owned());
                let mut value: Value = serde_json::from_str(&response(path).unwrap()).unwrap();
                if path.starts_with("/v2/orders:") {
                    value["client_order_id"] = json!(intent.client_order_id);
                    value["side"] = json!(if side == BrokerSide::Buy {
                        "buy"
                    } else {
                        "sell"
                    });
                } else if path.starts_with("/v2/account/activities") && side == BrokerSide::Sell {
                    for event in value.as_array_mut().unwrap() {
                        event["side"] = json!("sell");
                    }
                    value[1]["price"] = json!("100.01");
                }
                Ok(Some(value.to_string()))
            })
            .unwrap();
            let lookup = recovery::query(&intent.client_order_id).unwrap();
            assert_eq!(
                paths,
                [
                    "/v2/account",
                    &lookup,
                    "/v2/account",
                    "/v2/account",
                    "/v2/account/activities?direction=asc&page_size=100",
                    "/v2/account",
                    "/v2/account",
                    &lookup,
                    "/v2/account"
                ]
            );
            assert_eq!(report["execution_quantities_matched"], true);
            assert_eq!(report["executions"]["count"], 2);
            assert_eq!(
                report["held_cash_claim_microusd"],
                if side == BrokerSide::Buy {
                    "300000007"
                } else {
                    "7"
                }
            );
            assert_eq!(
                report["held_share_claim_nanoshares"],
                if side == BrokerSide::Buy {
                    "0"
                } else {
                    "3000000000"
                }
            );
            for flag in [
                "atomic_snapshot",
                "fills_reconciled",
                "average_price_reconciled",
                "economic_history_complete",
                "fees_final",
                "cash_reconciled",
                "retry_authorized",
                "claim_release_authorized",
                "execution_authority",
            ] {
                assert_eq!(report[flag], false);
            }
            assert_eq!(std::fs::read(&path).unwrap(), prior);
            assert!(report["order_before"]["account_before"]["private"].is_null());
        }
    }

    #[test]
    fn every_failed_missing_malformed_or_changed_observation_refuses_partial_reconciliation() {
        let directory = tempfile::tempdir().unwrap();
        let mut log = BrokerLog::open(directory.path().join("claims.jsonl"), "paper-id").unwrap();
        log.reserve(intent(), 1).unwrap();
        for stage in 0..9 {
            for failure in 0..4 {
                let mut index = 0;
                let result = collect(&log, "paper-id", "retained_1", 1, |path| {
                    let current = index;
                    index += 1;
                    if current == stage {
                        return match failure {
                            0 => Err("transport failed".into()),
                            1 => Ok(None),
                            2 => Ok(Some("null".into())),
                            _ => {
                                let mut value: Value =
                                    serde_json::from_str(&response(path).unwrap()).unwrap();
                                if path == "/v2/account" {
                                    value["cash"] = json!("49");
                                } else if path.starts_with("/v2/orders:") {
                                    value["updated_at"] = json!("changed");
                                } else {
                                    value[0]["qty"] = json!("1");
                                }
                                Ok(Some(value.to_string()))
                            }
                        };
                    }
                    Ok(response(path))
                });
                assert!(result.is_err(), "stage={stage}, failure={failure}");
            }
        }
        for (expected, client, pages) in [
            ("paper-id", "missing", 1),
            ("other", "retained_1", 1),
            ("paper-id", "retained_1", 0),
        ] {
            assert!(
                collect(&log, expected, client, pages, |_| panic!(
                    "invalid input must not read"
                ))
                .is_err()
            );
        }
    }
}
