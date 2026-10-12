// SPDX-License-Identifier: Apache-2.0
//! Cancel one retained simple paper order; acceptance never frees a capital claim.

use radar_journal::BrokerLog;
use serde_json::{Value, json};

const PAPER_API: &str = "https://paper-api.alpaca.markets";

fn path(id: &str) -> Result<String, String> {
    // The endpoint takes a UUID. Never allow a broker response to choose a host,
    // query or another path, even if its client ID matches our retained intent.
    if id.len() != 36
        || !id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err("unsupported paper broker order identity".into());
    }
    Ok(format!("/v2/orders/{id}"))
}

fn accepted(status: u16) -> Result<(), String> {
    if status != 204 {
        return Err("paper cancellation outcome unknown; reconcile retained client ID".into());
    }
    Ok(())
}

fn execute(
    log: &mut BrokerLog,
    client: &str,
    read: impl FnMut(&str) -> Result<Option<String>, String>,
    send: impl FnOnce(&str) -> Result<(), String>,
) -> Result<Value, String> {
    let intent = super::alpaca_order_read::retained(log, client)?;
    let report =
        super::alpaca_order_read::collect_bound(&intent.account_id, client, Some(&intent), read)?;
    let order: super::alpaca_reconcile::BrokerOrder =
        serde_json::from_value(report["order"].clone())
            .map_err(|_| "paper order not visible; reconcile rather than infer absence")?;
    // Only explicitly supported working orders. Pending cancels and terminal
    // orders need observation/reconciliation, never another DELETE here.
    if !matches!(
        order.status.as_str(),
        "new" | "accepted" | "partially_filled"
    ) {
        return Err("paper order is not supported for first cancellation".into());
    }
    let endpoint = path(&order.id)?;
    log.cancel(client, &order.id, super::wallet_read::now()?, |bound_id| {
        let bound_path = path(bound_id)?;
        if bound_path != endpoint {
            return Err("retained cancellation target changed".into());
        }
        send(&bound_path)
    })
    .map_err(|error| error.to_string())??;
    Ok(
        json!({"venue":"alpaca","environment":"paper","client_order_id":client,
        "broker_order_id":order.id,"cancellation":"request_accepted",
        "cancellation_confirmed":false,"fills_reconciled":false,
        "retry_authorized":false,"claim_release_authorized":false}),
    )
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let (journal, account, client) = match values.as_slice() {
        ["alpaca-paper-cancel", "--journal", journal, "--account-id", account, "--client-order-id", client] => (*journal, *account, *client),
        _ => return Err("usage: radar alpaca-paper-cancel --journal <private-file> --account-id <paper-id> --client-order-id <retained-id>".into()),
    };
    let mut log = super::alpaca_order_read::open_existing(journal, account)?;
    super::alpaca_order_read::retained(&log, client)?;
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret =
        super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let report = execute(
        &mut log,
        client,
        |path| super::alpaca_order_read::read_path(&agent, PAPER_API, path, &key, &secret),
        |path| {
            let response = agent
                .delete(format!("{PAPER_API}{path}"))
                .header("APCA-API-KEY-ID", &key)
                .header("APCA-API-SECRET-KEY", &secret)
                .call()
                .map_err(|_| "paper cancellation outcome unknown; reconcile retained client ID")?;
            accepted(response.status().as_u16())
        },
    )?;
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_journal::{BrokerIntent, BrokerSide, BrokerState};

    const ID: &str = "374d3d8e-63ef-41d1-a1a3-b7921776fbc4";

    fn intent() -> BrokerIntent {
        BrokerIntent {
            account_id: "paper".into(),
            client_order_id: "radar_1".into(),
            symbol: "SPY".into(),
            side: BrokerSide::Buy,
            quantity_nanoshares: 1_000_000_000,
            limit_microusd_per_share: 100_000_000,
            fee_bound_microusd: 7,
        }
    }

    fn order() -> Value {
        json!({"id":ID,"client_order_id":"radar_1","symbol":"SPY",
            "asset_class":"us_equity","side":"buy","type":"limit","order_class":"simple",
            "time_in_force":"day","extended_hours":false,"qty":"1",
            "limit_price":"100","filled_qty":"0","status":"new"})
    }

    fn read(path: &str, order: &Value) -> String {
        if path == "/v2/account" {
            json!({"id":"paper","status":"ACTIVE","cash":"100"}).to_string()
        } else {
            assert_eq!(
                path,
                "/v2/orders:by_client_order_id?client_order_id=radar_1"
            );
            order.to_string()
        }
    }

    #[test]
    fn first_cancel_records_uncertainty_before_effect_and_never_frees_claims() {
        for status in ["new", "accepted", "partially_filled"] {
            for success in [true, false] {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("paper.jsonl");
                let mut log = BrokerLog::open(&path, "paper").unwrap();
                log.reserve(intent(), 1).unwrap();
                log.submit("radar_1", 2, |_| ()).unwrap();
                let claims = log.claims().unwrap();
                let mut value = order();
                value["status"] = json!(status);
                let result = execute(
                    &mut log,
                    "radar_1",
                    |path| Ok(Some(read(path, &value))),
                    |target| {
                        assert_eq!(target, format!("/v2/orders/{ID}"));
                        let events = radar_journal::Journal::open(&path)
                            .unwrap()
                            .events()
                            .unwrap();
                        assert_eq!(events.len(), 3);
                        let entry = events.last().unwrap().broker.as_ref().unwrap();
                        assert_eq!(entry.cancel_unknown_order_id.as_deref(), Some(ID));
                        assert_eq!(entry.state, BrokerState::SubmissionUnknown);
                        if success {
                            Ok(())
                        } else {
                            Err("response lost".into())
                        }
                    },
                );
                if success {
                    assert_eq!(
                        result.unwrap(),
                        json!({"venue":"alpaca","environment":"paper",
                        "client_order_id":"radar_1","broker_order_id":ID,
                        "cancellation":"request_accepted","cancellation_confirmed":false,
                        "fills_reconciled":false,"retry_authorized":false,"claim_release_authorized":false})
                    );
                } else {
                    assert_eq!(result.unwrap_err(), "response lost");
                }
                assert_eq!(log.claims().unwrap(), claims);
                let retained = super::super::alpaca_journal::report(&log, "paper", false).unwrap();
                assert_eq!(retained["claims"][0]["cancel_unknown_order_id"], ID);
                drop(log);
                let mut log = BrokerLog::open(&path, "paper").unwrap();
                assert_eq!(log.claims().unwrap(), claims);
                assert!(
                    execute(
                        &mut log,
                        "radar_1",
                        |path| Ok(Some(read(path, &value))),
                        |_| panic!("no retry")
                    )
                    .is_err()
                );
                assert!(log.submit("radar_1", 4, |_| panic!("no resubmit")).is_err());
            }
        }
    }

    #[test]
    fn unsupported_status_missing_order_payload_or_account_change_refuses_before_effect() {
        for (field, value) in [
            ("status", "pending_cancel"),
            ("status", "filled"),
            ("status", "canceled"),
            ("status", "pending_new"),
            ("status", "unknown"),
            ("symbol", "QQQ"),
            ("qty", "2"),
            ("client_order_id", "other"),
            ("id", "../account"),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("paper.jsonl");
            let mut log = BrokerLog::open(&path, "paper").unwrap();
            log.reserve(intent(), 1).unwrap();
            log.submit("radar_1", 2, |_| ()).unwrap();
            let before = std::fs::read(&path).unwrap();
            let mut order = order();
            order[field] = json!(value);
            assert!(
                execute(
                    &mut log,
                    "radar_1",
                    |path| Ok(Some(read(path, &order))),
                    |_| panic!("refused")
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
        for scenario in 0..3 {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("paper.jsonl");
            let mut log = BrokerLog::open(&path, "paper").unwrap();
            log.reserve(intent(), 1).unwrap();
            if scenario != 2 {
                log.submit("radar_1", 2, |_| ()).unwrap();
            }
            let before = std::fs::read(&path).unwrap();
            let mut accounts = 0;
            assert!(
                execute(
                    &mut log,
                    "radar_1",
                    |path| {
                        if scenario == 0 && path != "/v2/account" {
                            return Ok(None);
                        }
                        let mut response = Some(read(path, &order()));
                        if path == "/v2/account" {
                            accounts += 1;
                            if scenario == 1 && accounts == 2 {
                                response = Some(
                                    json!({"id":"paper","status":"ACTIVE","cash":"99"}).to_string(),
                                );
                            }
                        }
                        Ok(response)
                    },
                    |_| panic!("refused")
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }

    #[test]
    fn only_uuid_target_and_204_acceptance_are_supported() {
        assert_eq!(path(ID).unwrap(), format!("/v2/orders/{ID}"));
        for id in [
            "",
            "broker",
            "/v2/orders",
            "https://api.alpaca.markets",
            "374d3d8e_63ef-41d1-a1a3-b7921776fbc4",
            "374d3d8e-63ef-41d1-a1a3-b7921776fbcZ",
        ] {
            assert!(path(id).is_err());
        }
        accepted(204).unwrap();
        for status in [200, 202, 301, 404, 422, 500] {
            assert!(accepted(status).is_err());
        }
    }
}
