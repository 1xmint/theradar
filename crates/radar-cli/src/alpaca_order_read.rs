// SPDX-License-Identifier: Apache-2.0
//! Recovery observations by retained client order ID. Never retry or free claims here.

use radar_journal::{BrokerIntent, BrokerLog, BrokerSide};
use serde_json::{Value, json};

use super::alpaca_reconcile::BrokerOrder;

const PAPER_API: &str = "https://paper-api.alpaca.markets";

pub(super) fn query(client_id: &str) -> Result<String, String> {
    // Radar's own retained identifiers use this query-safe subset. This is not
    // a claim about every client ID accepted by the broker.
    if client_id.is_empty()
        || !client_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(
            "client order ID must contain ASCII letters, digits, hyphens or underscores".into(),
        );
    }
    Ok(format!(
        "/v2/orders:by_client_order_id?client_order_id={client_id}"
    ))
}

fn get_optional(
    agent: &ureq::Agent,
    url: &str,
    key: &str,
    secret: &str,
) -> Result<Option<String>, String> {
    let response = agent
        .get(url)
        .header("APCA-API-KEY-ID", key)
        .header("APCA-API-SECRET-KEY", secret)
        .call();
    let mut response = match response {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(_) => return Err("paper order lookup failed".into()),
    };
    if response.status() != 200 {
        return Err("paper order lookup returned an unexpected status".into());
    }
    response
        .body_mut()
        .read_to_string()
        .map(Some)
        .map_err(|_| "paper order response could not be read".into())
}

#[cfg(test)]
fn collect(
    expected: &str,
    client_id: &str,
    read: impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<Value, String> {
    collect_bound(expected, client_id, None, read)
}

pub(super) fn payload_matches(intent: &BrokerIntent, order: &BrokerOrder) -> Result<(), String> {
    let side = match intent.side {
        BrokerSide::Buy => "buy",
        BrokerSide::Sell => "sell",
    };
    if order.id.trim().is_empty()
        || order.client_order_id.as_deref() != Some(intent.client_order_id.as_str())
        || order.symbol.as_deref() != Some(intent.symbol.as_str())
        || order.asset_class.as_deref() != Some("us_equity")
        || order.side.as_deref() != Some(side)
        || order.kind.as_deref() != Some("limit")
        || order.order_class.as_deref() != Some("simple")
        || order.time_in_force.as_deref() != Some("day")
        || order.extended_hours != Some(false)
        || order.notional.is_some()
        || order.stop_price.is_some()
        || order.replaces.is_some()
        || order.replaced_by.is_some()
        || order.legs.as_ref().is_some_and(|legs| !legs.is_empty())
    {
        return Err("broker order does not match the retained simple day equity intent".into());
    }
    let quantity = super::alpaca_exposure::nanos(
        order
            .qty
            .as_deref()
            .ok_or("broker order quantity is unknown")?,
    )?;
    let price = super::alpaca_exposure::nanos(
        order
            .limit_price
            .as_deref()
            .ok_or("broker limit price is unknown")?,
    )?;
    let expected_price = intent
        .limit_microusd_per_share
        .checked_mul(1_000)
        .ok_or("retained limit exceeds supported broker decimal range")?;
    if quantity != intent.quantity_nanoshares || price != expected_price {
        return Err("broker quantity or limit differs from retained intent".into());
    }
    Ok(())
}

pub(super) fn retained(log: &BrokerLog, client_id: &str) -> Result<BrokerIntent, String> {
    log.entries()
        .map_err(|e| e.to_string())?
        .find(|entry| entry.intent.client_order_id == client_id)
        .map(|entry| entry.intent.clone())
        .ok_or_else(|| "client order ID has no retained journal intent".into())
}

pub(super) fn collect_bound(
    expected: &str,
    client_id: &str,
    intent: Option<&BrokerIntent>,
    mut read: impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<Value, String> {
    if expected.trim().is_empty() {
        return Err("expected paper account identity is required".into());
    }
    let path = query(client_id)?;
    if let Some(intent) = intent
        && (intent.account_id != expected || intent.client_order_id != client_id)
    {
        return Err("retained intent differs from requested account or client ID".into());
    }
    let started = super::wallet_read::now()?;
    let account_before = super::alpaca_reconcile::account(
        &read("/v2/account")?.ok_or("paper account observation is missing")?,
        expected,
    )?;
    let order: Option<BrokerOrder> = read(&path)?
        .map(|raw| {
            let order: BrokerOrder =
                serde_json::from_str(&raw).map_err(|_| "invalid paper order lookup response")?;
            if order.id.trim().is_empty() || order.client_order_id.as_deref() != Some(client_id) {
                return Err("paper order identity differs from requested client ID");
            }
            Ok(order)
        })
        .transpose()?;
    let account_after = super::alpaca_reconcile::account(
        &read("/v2/account")?.ok_or("paper account observation is missing")?,
        expected,
    )?;
    let mut payload_verified = false;
    if let (Some(intent), Some(order)) = (intent, order.as_ref()) {
        payload_matches(intent, order)?;
        // Bracket equality is still not an atomic snapshot. A changed account
        // cannot support even this bounded recovery check.
        if account_before != account_after {
            return Err("paper account changed during intent binding".into());
        }
        payload_verified = true;
    }
    Ok(
        json!({"version":1,"venue":"alpaca","environment":"paper","source":PAPER_API,
        "expected_account_id":expected,"requested_client_order_id":client_id,
        "read_started_unix_secs":started,"read_completed_unix_secs":super::wallet_read::now()?,
        "selected_account_fields_unchanged":account_before == account_after,
        "account_before":account_before,"account_after":account_after,
        "lookup":if order.is_some() { "found" } else { "not_visible" },"order":order,
        "atomic_snapshot":false,"intent_payload_verified":payload_verified,"fills_reconciled":false,
        "retry_authorized":false,"claim_release_authorized":false,"execution_authority":false}),
    )
}

pub(super) fn read_path(
    agent: &ureq::Agent,
    base: &str,
    path: &str,
    key: &str,
    secret: &str,
) -> Result<Option<String>, String> {
    let url = format!("{base}{path}");
    // Missing accounts always refuse; only the order lookup may be not visible.
    if path == "/v2/account" {
        super::alpaca_read::get(agent, &url, key, secret).map(Some)
    } else {
        get_optional(agent, &url, key, secret)
    }
}

pub(super) fn open_existing(path: &str, expected: &str) -> Result<BrokerLog, String> {
    if !std::path::Path::new(path).is_file() {
        return Err("paper order journal is missing".into());
    }
    BrokerLog::open(path, expected).map_err(|e| e.to_string())
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let (expected, client_id, journal_path) = match values.as_slice() {
        ["alpaca-order-read", "--account-id", expected, "--client-order-id", client_id] => (*expected, *client_id, None),
        ["alpaca-order-read", "--account-id", expected, "--client-order-id", client_id, "--journal", path] => (*expected, *client_id, Some(*path)),
        _ => return Err("usage: radar alpaca-order-read --account-id <expected-paper-account-id> --client-order-id <retained-id> [--journal <private-file>]".into()),
    };
    query(client_id)?;
    // Hold the cooperative journal owner across the broker reads. Never create
    // a missing log and turn it into a claim of no outstanding intent.
    let log = journal_path
        .map(|path| open_existing(path, expected))
        .transpose()?;
    let intent = log
        .as_ref()
        .map(|log| retained(log, client_id))
        .transpose()?;
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret =
        super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let report = collect_bound(expected, client_id, intent.as_ref(), |path| {
        read_path(&agent, PAPER_API, path, &key, &secret)
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
            quantity_nanoshares: 12_345_678,
            limit_microusd_per_share: 100_000_001,
            fee_bound_microusd: 7,
        }
    }

    fn matching_order() -> Value {
        json!({"id":"broker-id","client_order_id":"retained_1","symbol":"SPY",
            "asset_class":"us_equity","side":"buy","type":"limit","order_class":"simple",
            "time_in_force":"day","extended_hours":false,"qty":"0.012345678",
            "limit_price":"100.000001","filled_qty":"0","status":"new"})
    }

    #[test]
    fn payload_binding_compares_exact_units_and_both_sides_without_releasing_claims() {
        for side in [BrokerSide::Buy, BrokerSide::Sell] {
            let mut intent = intent();
            intent.side = side;
            let mut value = matching_order();
            value["side"] = json!(if side == BrokerSide::Buy {
                "buy"
            } else {
                "sell"
            });
            for price in ["100.000001", "0100.000001000"] {
                value["limit_price"] = json!(price);
                for legs in [Value::Null, json!([])] {
                    value["legs"] = legs;
                    let order = serde_json::from_value(value.clone()).unwrap();
                    payload_matches(&intent, &order).unwrap();
                }
            }
        }
        for (field, value) in [
            ("qty", "0.012345677"),
            ("qty", "0.012345679"),
            ("qty", "0"),
            ("qty", "-0.012345678"),
            ("qty", "1.2345678e-2"),
            ("qty", "0.0123456780"),
            ("qty", "18446744074"),
            ("limit_price", "100.000000999"),
            ("limit_price", "100.000001001"),
            ("limit_price", "100.000002"),
            ("limit_price", "0"),
            ("limit_price", "100.0000010000"),
            ("limit_price", "100."),
        ] {
            let mut order = matching_order();
            order[field] = json!(value);
            assert!(
                payload_matches(&intent(), &serde_json::from_value(order).unwrap()).is_err(),
                "{field}={value}"
            );
        }
        let mut overflow = intent();
        overflow.limit_microusd_per_share = u64::MAX;
        assert!(
            payload_matches(
                &overflow,
                &serde_json::from_value(matching_order()).unwrap()
            )
            .is_err()
        );
    }

    #[test]
    fn missing_changed_and_complex_order_payloads_never_match_retained_intent() {
        for (field, changed) in [
            ("id", json!(" ")),
            ("client_order_id", json!("other")),
            ("symbol", json!("QQQ")),
            ("asset_class", json!("crypto")),
            ("side", json!("sell")),
            ("type", json!("market")),
            ("order_class", json!("bracket")),
            ("time_in_force", json!("gtc")),
            ("extended_hours", json!(true)),
            ("qty", json!("1")),
            ("limit_price", json!("99")),
        ] {
            for replacement in [changed, Value::Null] {
                let mut value = matching_order();
                value[field] = replacement;
                let result: Result<BrokerOrder, _> = serde_json::from_value(value);
                assert!(
                    result.is_err() || payload_matches(&intent(), &result.unwrap()).is_err(),
                    "{field}"
                );
            }
            let mut value = matching_order();
            value.as_object_mut().unwrap().remove(field);
            let result: Result<BrokerOrder, _> = serde_json::from_value(value);
            assert!(
                result.is_err() || payload_matches(&intent(), &result.unwrap()).is_err(),
                "missing {field}"
            );
        }
        for (field, changed) in [
            ("notional", json!("0")),
            ("stop_price", json!("0")),
            ("replaces", json!("")),
            ("replaced_by", json!("other")),
            ("legs", json!([{"id":"leg","status":"held"}])),
        ] {
            let mut value = matching_order();
            value[field] = changed;
            assert!(
                payload_matches(&intent(), &serde_json::from_value(value).unwrap()).is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn bound_recovery_checks_account_and_payload_without_treating_absence_as_success() {
        for found in [false, true] {
            let report = collect_bound("paper-id", "retained_1", Some(&intent()), |path| {
                Ok(if path == "/v2/account" {
                    Some(account())
                } else if found {
                    Some(matching_order().to_string())
                } else {
                    None
                })
            })
            .unwrap();
            assert_eq!(report["intent_payload_verified"], found);
            for flag in [
                "atomic_snapshot",
                "fills_reconciled",
                "retry_authorized",
                "claim_release_authorized",
                "execution_authority",
            ] {
                assert_eq!(report[flag], false);
            }
        }
        for field in ["account_id", "client_order_id"] {
            let mut value = serde_json::to_value(intent()).unwrap();
            value[field] = json!("other");
            let intent = serde_json::from_value(value).unwrap();
            assert!(
                collect_bound("paper-id", "retained_1", Some(&intent), |_| panic!(
                    "mismatched intent must refuse before reads"
                ))
                .is_err()
            );
        }
        for change_account in [false, true] {
            let mut accounts = 0;
            assert!(
                collect_bound("paper-id", "retained_1", Some(&intent()), |path| {
                    if path == "/v2/account" {
                        accounts += 1;
                        Ok(Some(if change_account && accounts == 2 {
                            account().replace("50", "49")
                        } else {
                            account()
                        }))
                    } else {
                        let mut value = matching_order();
                        if !change_account {
                            value["symbol"] = json!("QQQ");
                        }
                        Ok(Some(value.to_string()))
                    }
                })
                .is_err()
            );
        }
    }

    #[test]
    fn retained_lookup_uses_verified_journal_intent_and_preserves_unknown_submission() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("paper.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        log.reserve(intent(), 1).unwrap();
        let mut second = intent();
        second.client_order_id = "another".into();
        second.symbol = "QQQ".into();
        log.reserve(second, 2).unwrap();
        log.submit("retained_1", 3, |_| ()).unwrap();
        drop(log);
        let before = std::fs::read(&path).unwrap();
        let log = BrokerLog::open(&path, "paper-id").unwrap();
        assert_eq!(retained(&log, "retained_1").unwrap(), intent());
        assert!(retained(&log, "missing").is_err());
        assert_eq!(retained(&log, "another").unwrap().symbol, "QQQ");
        assert_eq!(
            log.entries()
                .unwrap()
                .find(|e| e.intent.client_order_id == "retained_1")
                .unwrap()
                .state,
            radar_journal::BrokerState::SubmissionUnknown
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    fn account() -> String {
        json!({"id":"paper-id","status":"ACTIVE","account_number":"do-not-echo","cash":"50"})
            .to_string()
    }

    fn order(status: &str) -> Value {
        json!({"id":"broker-id","client_order_id":"retained_1","status":status,"symbol":"SPY",
            "filled_qty":"0.012345678","filled_avg_price":"100.000001","filled_at":"2026-10-21T15:00:00Z",
            "submitted_at":"2026-10-21T14:00:00Z","updated_at":"2026-10-21T15:00:00Z",
            "canceled_at":"cancel-time","expired_at":"expire-time","failed_at":"fail-time",
            "legs":[{"id":"leg-id","status":"held"}],"unknown_secret":"do-not-echo"})
    }

    fn flags(report: &Value) {
        for flag in [
            "atomic_snapshot",
            "intent_payload_verified",
            "fills_reconciled",
            "retry_authorized",
            "claim_release_authorized",
            "execution_authority",
        ] {
            assert_eq!(report[flag], false);
        }
    }

    #[test]
    fn retained_identifiers_cannot_inject_query_parameters_or_paths() {
        for id in ["retained_1", "ABC-123", "0"] {
            assert_eq!(
                query(id).unwrap(),
                format!("/v2/orders:by_client_order_id?client_order_id={id}")
            );
        }
        for id in [
            "", " ", "a&x=1", "a?x=1", "a/b", "a#b", "a%26b", "a.b", "é", "a\nb",
        ] {
            assert!(query(id).is_err(), "{id}");
            assert!(collect("paper-id", id, |_| panic!("invalid ID must not read")).is_err());
        }
        assert!(
            collect(" ", "retained_1", |_| panic!(
                "missing account must not read"
            ))
            .is_err()
        );
    }

    #[test]
    fn found_orders_preserve_terminal_and_pending_observations_without_releasing_claims() {
        for status in [
            "partially_filled",
            "pending_cancel",
            "filled",
            "canceled",
            "expired",
            "rejected",
            "unknown-status",
        ] {
            let mut paths = Vec::new();
            let report = collect("paper-id", "retained_1", |path| {
                paths.push(path.to_owned());
                Ok(Some(if path == "/v2/account" {
                    account()
                } else {
                    order(status).to_string()
                }))
            })
            .unwrap();
            assert_eq!(
                paths,
                [
                    "/v2/account",
                    "/v2/orders:by_client_order_id?client_order_id=retained_1",
                    "/v2/account"
                ]
            );
            assert_eq!(report["lookup"], "found");
            assert_eq!(report["order"]["status"], status);
            assert_eq!(report["order"]["id"], "broker-id");
            assert_eq!(report["order"]["client_order_id"], "retained_1");
            for field in [
                "filled_qty",
                "filled_avg_price",
                "submitted_at",
                "updated_at",
                "filled_at",
                "canceled_at",
                "expired_at",
                "failed_at",
            ] {
                assert_eq!(report["order"][field], order(status)[field], "{field}");
            }
            assert_eq!(report["order"]["legs"].as_array().unwrap().len(), 1);
            assert_eq!(report["order"]["legs"][0]["id"], "leg-id");
            assert_eq!(report["order"]["legs"][0]["status"], "held");
            assert_eq!(report["selected_account_fields_unchanged"], true);
            assert!(report["account_after"]["account_number"].is_null());
            assert!(report["order"]["unknown_secret"].is_null());
            flags(&report);
        }
    }

    #[test]
    fn not_visible_remains_unresolved_and_account_changes_stay_visible() {
        let mut accounts = 0;
        let report = collect("paper-id", "retained_1", |path| {
            if path != "/v2/account" {
                return Ok(None);
            }
            accounts += 1;
            Ok(Some(if accounts == 2 {
                account().replace("50", "49")
            } else {
                account()
            }))
        })
        .unwrap();
        assert_eq!(report["lookup"], "not_visible");
        assert!(report["order"].is_null());
        assert_eq!(report["selected_account_fields_unchanged"], false);
        assert_eq!(report["account_before"]["cash"], "50");
        assert_eq!(report["account_after"]["cash"], "49");
        flags(&report);
    }

    #[test]
    fn account_binding_and_response_failures_never_emit_a_partial_report() {
        for stage in 0..3 {
            for malformed in [false, true] {
                let mut index = 0;
                let result = collect("paper-id", "retained_1", |path| {
                    let current = index;
                    index += 1;
                    if current == stage {
                        return if malformed {
                            Ok(Some("null".into()))
                        } else {
                            Err("transport failed".into())
                        };
                    }
                    Ok(Some(if path == "/v2/account" {
                        account()
                    } else {
                        order("new").to_string()
                    }))
                });
                assert!(result.is_err(), "stage {stage}");
            }
        }
        for stage in [0, 2] {
            for missing in [false, true] {
                let mut index = 0;
                assert!(
                    collect("paper-id", "retained_1", |path| {
                        let current = index;
                        index += 1;
                        if current == stage {
                            return Ok(if missing {
                                None
                            } else {
                                Some(account().replace("paper-id", "other-id"))
                            });
                        }
                        Ok(Some(if path == "/v2/account" {
                            account()
                        } else {
                            order("new").to_string()
                        }))
                    })
                    .is_err()
                );
            }
        }
        for (field, value) in [
            ("id", json!("")),
            ("id", json!(" ")),
            ("client_order_id", json!("other-id")),
            ("client_order_id", Value::Null),
        ] {
            let mut wrong = order("new");
            wrong[field] = value;
            assert!(
                collect("paper-id", "retained_1", |path| Ok(Some(
                    if path == "/v2/account" {
                        account()
                    } else {
                        wrong.to_string()
                    }
                )))
                .is_err()
            );
        }
    }

    #[test]
    fn only_http_404_is_not_visible_other_statuses_and_redirects_refuse() {
        use std::io::{Read as _, Write as _};
        for (path, status) in [200, 302, 400, 401, 404, 500]
            .into_iter()
            .flat_map(|status| [("/v2/account", status), ("/lookup", status)])
        {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let body = "private-response";
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nLocation: http://{address}/redirected\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                String::from_utf8(request).unwrap()
            });
            let response = read_path(
                &super::super::alpaca_read::agent(),
                &format!("http://{address}"),
                path,
                "test-key",
                "test-secret",
            );
            match status {
                200 => assert_eq!(response.unwrap(), Some("private-response".into())),
                404 if path == "/lookup" => assert_eq!(response.unwrap(), None),
                302 if path == "/lookup" => assert_eq!(
                    response.unwrap_err(),
                    "paper order lookup returned an unexpected status"
                ),
                _ if path == "/lookup" => {
                    assert_eq!(response.unwrap_err(), "paper order lookup failed");
                }
                _ => {
                    assert!(response.is_err(), "missing or failed accounts must refuse");
                }
            }
            let request = server.join().unwrap().to_ascii_lowercase();
            assert!(request.starts_with(&format!("get {path} http/1.1")));
            assert!(request.contains("apca-api-key-id: test-key"));
            assert!(request.contains("apca-api-secret-key: test-secret"));
        }
    }
}
