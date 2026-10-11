// SPDX-License-Identifier: Apache-2.0
//! Bracket paper positions with account and open-order observations.
//! Stable selected fields do not establish an atomic or economically complete risk state.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::alpaca_read::{BrokerAccount, Clock, Position};

const PAPER_API: &str = "https://paper-api.alpaca.markets";
const OPEN_ORDERS: &str = "/v2/orders?status=open&limit=500&direction=asc&nested=false";

#[derive(Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct BrokerOrder {
    pub(super) id: String,
    pub(super) status: String,
    pub(super) client_order_id: Option<String>,
    pub(super) symbol: Option<String>,
    pub(super) asset_class: Option<String>,
    pub(super) side: Option<String>,
    #[serde(rename = "type")]
    pub(super) kind: Option<String>,
    pub(super) order_class: Option<String>,
    pub(super) qty: Option<String>,
    pub(super) notional: Option<String>,
    pub(super) filled_qty: Option<String>,
    filled_avg_price: Option<String>,
    submitted_at: Option<String>,
    updated_at: Option<String>,
    filled_at: Option<String>,
    canceled_at: Option<String>,
    expired_at: Option<String>,
    failed_at: Option<String>,
    pub(super) limit_price: Option<String>,
    pub(super) stop_price: Option<String>,
    pub(super) replaces: Option<String>,
    pub(super) replaced_by: Option<String>,
    pub(super) legs: Option<Vec<BrokerOrder>>,
}

fn orders(raw: &str) -> Result<Vec<BrokerOrder>, String> {
    let mut orders: Vec<BrokerOrder> =
        serde_json::from_str(raw).map_err(|_| "invalid paper open orders response")?;
    // The API's maximum response size is 500. A full page cannot prove that
    // another pending order was not omitted; do not turn truncation into cash.
    if orders.len() >= 500 {
        return Err(
            "paper open orders reached the response limit; reconciliation incomplete".into(),
        );
    }
    let mut seen = BTreeSet::new();
    for order in &orders {
        if order.id.trim().is_empty() || !seen.insert(&order.id) {
            return Err("paper open orders have missing or duplicate identities".into());
        }
    }
    orders.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(orders)
}

pub(super) fn account(raw: &str, expected: &str) -> Result<BrokerAccount, String> {
    let account: BrokerAccount =
        serde_json::from_str(raw).map_err(|_| "invalid paper account response")?;
    if account.id != expected {
        return Err("paper account identity differs from configured account".into());
    }
    Ok(account)
}

fn collect(
    expected: &str,
    mut read: impl FnMut(&str) -> Result<String, String>,
) -> Result<Value, String> {
    if expected.trim().is_empty() {
        return Err("expected paper account identity is required".into());
    }
    let started = super::wallet_read::now()?;
    let first_account = account(&read("/v2/account")?, expected)?;
    let first_orders = orders(&read(OPEN_ORDERS)?)?;
    let positions: Vec<Position> = serde_json::from_str(&read("/v2/positions")?)
        .map_err(|_| "invalid paper positions response")?;
    let clock: Clock = serde_json::from_str(&read("/v2/clock")?)
        .map_err(|_| "invalid paper market clock response")?;
    let last_orders = orders(&read(OPEN_ORDERS)?)?;
    let last_account = account(&read("/v2/account")?, expected)?;
    let exposure = if first_account == last_account && first_orders == last_orders {
        super::alpaca_exposure::summarize(&last_account, &last_orders, &positions)
    } else {
        Err("account or pending orders changed during observation".into())
    };
    let exposure = match exposure {
        Ok(bounds) => json!({"supported":true,"bounds":bounds}),
        Err(reason) => json!({"supported":false,"bounds":null,"reason":reason}),
    };
    Ok(
        json!({"version":1,"venue":"alpaca","environment":"paper","source":PAPER_API,
        "expected_account_id":expected,
        "read_started_unix_secs":started,"read_completed_unix_secs":super::wallet_read::now()?,
        "selected_account_fields_unchanged":first_account == last_account,
        "selected_open_order_fields_unchanged":first_orders == last_orders,
        "account_before":first_account,"account_after":last_account,
        "open_orders_before":first_orders,"open_orders_after":last_orders,
        "positions":positions,"market_clock":clock,
        "conservative_exposure":exposure,
        "open_order_responses_below_limit":true,"atomic_snapshot":false,
        "pending_order_accounting_complete":false,"risk_state_complete":false,"execution_authority":false}),
    )
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let ["alpaca-reconcile", "--account-id", expected] = values.as_slice() else {
        return Err(
            "usage: radar alpaca-reconcile --account-id <expected-paper-account-id>".into(),
        );
    };
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret =
        super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let report = collect(expected, |path| {
        super::alpaca_read::get(&agent, &format!("{PAPER_API}{path}"), &key, &secret)
    })?;
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        match path {
            "/v2/account" => json!({"id":"paper-id","account_number":"private-number","status":"ACTIVE",
                "currency":"USD","cash":"50.01","equity":"52.34","buying_power":"104.68",
                "trading_blocked":false,"account_blocked":false,"trade_suspended_by_user":false}),
            OPEN_ORDERS => json!([{"id":"order-b","status":"pending_cancel","symbol":"SPY",
                "side":"buy","qty":"0.1","filled_qty":"0.04","limit_price":"100.01",
                "replaced_by":"replacement-id","legs":[{"id":"leg-id","status":"held"}]},
                {"id":"order-a","status":"partially_filled","notional":"5.00","filled_qty":"0.01"}]),
            "/v2/positions" => json!([]),
            "/v2/clock" => json!({"timestamp":"2026-10-21T10:00:00-04:00","is_open":true,
                "next_open":"2026-10-22T09:30:00-04:00","next_close":"2026-10-21T16:00:00-04:00"}),
            _ => panic!("unexpected path {path}"),
        }.to_string()
    }

    #[test]
    fn pending_and_partially_filled_orders_survive_both_bracket_reads() {
        let mut paths = Vec::new();
        let mut order_reads = 0;
        let report = collect("paper-id", |path| {
            paths.push(path.to_owned());
            let mut body: Value = serde_json::from_str(&fixture(path)).unwrap();
            if path == OPEN_ORDERS {
                order_reads += 1;
                if order_reads == 2 {
                    body.as_array_mut().unwrap().reverse();
                }
            }
            Ok(body.to_string())
        })
        .unwrap();
        assert_eq!(
            paths,
            [
                "/v2/account",
                OPEN_ORDERS,
                "/v2/positions",
                "/v2/clock",
                OPEN_ORDERS,
                "/v2/account"
            ]
        );
        assert_eq!(report["selected_account_fields_unchanged"], true);
        assert_eq!(report["selected_open_order_fields_unchanged"], true);
        assert_eq!(report["open_orders_before"], report["open_orders_after"]);
        assert_eq!(report["open_orders_after"][0]["id"], "order-a");
        assert_eq!(report["open_orders_after"][0]["notional"], "5.00");
        assert_eq!(report["open_orders_after"][1]["status"], "pending_cancel");
        assert_eq!(report["open_orders_after"][1]["qty"], "0.1");
        assert_eq!(report["open_orders_after"][1]["filled_qty"], "0.04");
        assert_eq!(report["open_orders_after"][1]["legs"][0]["status"], "held");
        assert!(report["account_after"]["account_number"].is_null());
        for flag in [
            "atomic_snapshot",
            "pending_order_accounting_complete",
            "risk_state_complete",
            "execution_authority",
        ] {
            assert_eq!(report[flag], false);
        }
        assert_eq!(report["open_order_responses_below_limit"], true);
        assert_eq!(report["expected_account_id"], "paper-id");
        assert_eq!(report["positions"], json!([]));
        assert_eq!(report["market_clock"]["is_open"], true);
        assert_eq!(report["conservative_exposure"]["supported"], false);
        assert!(report["conservative_exposure"]["bounds"].is_null());
    }

    #[test]
    fn changing_account_and_order_fields_are_visible_without_claiming_atomicity() {
        for (change_account, change_orders) in [(true, false), (false, true), (true, true)] {
            let mut account_reads = 0;
            let mut order_reads = 0;
            let report = collect("paper-id", |path| {
                let mut body: Value = serde_json::from_str(&fixture(path)).unwrap();
                if path == "/v2/account" {
                    account_reads += 1;
                    if account_reads == 2 && change_account {
                        body["cash"] = json!("40.00");
                    }
                }
                if path == OPEN_ORDERS {
                    order_reads += 1;
                    if order_reads == 2 && change_orders {
                        body[0]["filled_qty"] = json!("0.09");
                    }
                }
                Ok(body.to_string())
            })
            .unwrap();
            assert_eq!(report["selected_account_fields_unchanged"], !change_account);
            assert_eq!(
                report["selected_open_order_fields_unchanged"],
                !change_orders
            );
            assert_eq!(report["account_before"]["cash"], "50.01");
            assert_eq!(
                report["account_after"]["cash"],
                if change_account { "40.00" } else { "50.01" }
            );
            assert_eq!(
                report["open_orders_after"][1]["filled_qty"],
                if change_orders { "0.09" } else { "0.04" }
            );
            assert_eq!(report["conservative_exposure"]["supported"], false);
            assert_eq!(
                report["conservative_exposure"]["reason"],
                "account or pending orders changed during observation"
            );
        }
    }

    #[test]
    fn supported_exposure_is_attached_to_the_actual_reconciliation_caller() {
        let report = collect("paper-id", |path| {
            Ok(if path == OPEN_ORDERS {
                json!([{"id":"buy-id","status":"pending_cancel","symbol":"SPY",
                "asset_class":"us_equity","side":"buy","type":"limit","order_class":"simple",
                "qty":"0.001","filled_qty":"0.0005","limit_price":"100"}])
                .to_string()
            } else {
                fixture(path)
            })
        })
        .unwrap();
        let exposure = &report["conservative_exposure"];
        assert_eq!(exposure["supported"], true);
        assert_eq!(exposure["bounds"]["pending_buy_bound_microusd"], "100000");
        assert_eq!(exposure["bounds"]["cash_bound_microusd"], "49910000");
        assert_eq!(exposure["bounds"]["local_claims_reconciled"], false);
        assert_eq!(report["risk_state_complete"], false);
        assert_eq!(report["execution_authority"], false);
    }

    #[test]
    fn identity_is_pinned_before_and_after_and_read_failures_have_no_partial_report() {
        assert_eq!(
            collect(" ", |_| panic!("must not read")).unwrap_err(),
            "expected paper account identity is required"
        );
        for stage in [0, 5] {
            let mut index = 0;
            let result = collect("paper-id", |path| {
                let current = index;
                index += 1;
                Ok(if current == stage {
                    fixture(path).replace("paper-id", "other-id")
                } else {
                    fixture(path)
                })
            });
            assert_eq!(
                result.unwrap_err(),
                "paper account identity differs from configured account"
            );
        }
        for stage in 0..6 {
            for malformed in [false, true] {
                let mut index = 0;
                let result = collect("paper-id", |path| {
                    let current = index;
                    index += 1;
                    if current == stage {
                        if malformed {
                            Ok("null".into())
                        } else {
                            Err("transport refused".into())
                        }
                    } else {
                        Ok(fixture(path))
                    }
                });
                assert!(result.is_err(), "stage {stage}, malformed {malformed}");
            }
        }
    }

    #[test]
    fn full_order_pages_and_ambiguous_identities_are_not_complete_observations() {
        assert_eq!(
            serde_json::to_value(orders("[]").unwrap()).unwrap(),
            json!([])
        );
        for count in [499, 500, 501] {
            let values: Vec<Value> = (0..count)
                .map(|id| json!({"id":id.to_string(),"status":"new"}))
                .collect();
            let result = orders(&json!(values).to_string());
            assert_eq!(result.is_ok(), count == 499);
        }
        for body in [
            r#"[{"id":"","status":"new"}]"#,
            r#"[{"id":" ","status":"new"}]"#,
            r#"[{"id":"a","status":"new"},{"id":"a","status":"new"}]"#,
            r#"[{"id":"a"}]"#,
        ] {
            assert!(orders(body).is_err(), "{body}");
        }
    }
}
