// SPDX-License-Identifier: Apache-2.0
//! Recovery observations by retained client order ID. Never retry or free claims here.

use serde_json::{Value, json};

use super::alpaca_reconcile::BrokerOrder;

const PAPER_API: &str = "https://paper-api.alpaca.markets";

fn query(client_id: &str) -> Result<String, String> {
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

fn collect(
    expected: &str,
    client_id: &str,
    mut read: impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<Value, String> {
    if expected.trim().is_empty() {
        return Err("expected paper account identity is required".into());
    }
    let path = query(client_id)?;
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
    Ok(
        json!({"version":1,"venue":"alpaca","environment":"paper","source":PAPER_API,
        "expected_account_id":expected,"requested_client_order_id":client_id,
        "read_started_unix_secs":started,"read_completed_unix_secs":super::wallet_read::now()?,
        "selected_account_fields_unchanged":account_before == account_after,
        "account_before":account_before,"account_after":account_after,
        "lookup":if order.is_some() { "found" } else { "not_visible" },"order":order,
        "atomic_snapshot":false,"intent_payload_verified":false,"fills_reconciled":false,
        "retry_authorized":false,"claim_release_authorized":false,"execution_authority":false}),
    )
}

fn read_path(
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

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let [
        "alpaca-order-read",
        "--account-id",
        expected,
        "--client-order-id",
        client_id,
    ] = values.as_slice()
    else {
        return Err("usage: radar alpaca-order-read --account-id <expected-paper-account-id> --client-order-id <retained-id>".into());
    };
    query(client_id)?;
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret =
        super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let report = collect(expected, client_id, |path| {
        read_path(&agent, PAPER_API, path, &key, &secret)
    })?;
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    assert_eq!(response.unwrap_err(), "paper order lookup failed")
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
