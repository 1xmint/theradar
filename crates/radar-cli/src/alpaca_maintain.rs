// SPDX-License-Identifier: Apache-2.0
//! One recovery tick for retained paper orders. Never creates or retries orders.

use radar_journal::BrokerLog;
use serde_json::{Value, json};

const PAPER_API: &str = "https://paper-api.alpaca.markets";

fn maintain(
    log: &mut BrokerLog,
    account: &str,
    cancel_working: bool,
    refresh: impl FnOnce(&mut BrokerLog) -> Result<(), String>,
    cancel: impl FnOnce(&mut BrokerLog, &str) -> Result<Value, String>,
) -> Result<Value, String> {
    // A fresh protected checkpoint must precede selecting a cancellation. A
    // missing order, changing account or incomplete fills stops the entire tick.
    refresh(log)?;
    if log.cash().map_err(|e| e.to_string())?.is_none() {
        return Err("paper maintenance checkpoint missing".into());
    }
    let claims = log.claims().map_err(|e| e.to_string())?;
    let mut working = Vec::new();
    let mut unresolved = Vec::new();
    let mut cancel_pending = Vec::new();
    let mut terminal = Vec::new();
    for entry in log.entries().map_err(|e| e.to_string())? {
        let client = &entry.intent.client_order_id;
        let progress = claims.get(client).and_then(|claim| claim.progress.as_ref());
        if let Some(progress) = progress {
            if progress.terminal {
                terminal.push(client.clone());
            } else if entry.cancel_unknown_order_id.is_some() {
                cancel_pending.push(client.clone());
            } else {
                working.push(client.clone());
            }
        } else {
            unresolved.push(client.clone());
        }
    }
    // One cancellation per tick bounds effects. Existing uncertainty never
    // authorizes another DELETE. The cancel caller performs fresh payload/account
    // checks and durably writes uncertainty before sending its single request.
    let cancellation = if cancel_working && unresolved.is_empty() {
        working
            .first()
            .map(|client| cancel(log, client))
            .transpose()?
    } else {
        None
    };
    Ok(json!({"version":1,"venue":"alpaca","environment":"paper",
        "expected_account_id":account,"checkpoint_refreshed":true,
        "working_before_cancellation":working,"unresolved":unresolved,
        "cancellation_pending_before_tick":cancel_pending,"terminal":terminal,
        "cancellation_attempt":cancellation,
        "claims_after_tick":super::alpaca_journal::report(log, account, false)?,
        "new_order_authority":false,"retry_authorized":false,
        "claim_release_authorized":false,"profit_measured":false}))
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let (journal, account, pages, cancel_working) = match values.as_slice() {
        ["alpaca-paper-maintain", "--journal", journal, "--account-id", account, "--max-pages", pages] => (*journal, *account, *pages, false),
        ["alpaca-paper-maintain", "--journal", journal, "--account-id", account, "--max-pages", pages, "--cancel-working"] => (*journal, *account, *pages, true),
        _ => return Err("usage: radar alpaca-paper-maintain --journal <private-file> --account-id <paper-id> --max-pages <positive-u16> [--cancel-working]".into()),
    };
    let pages = super::alpaca_activities::page_limit(pages)?;
    let mut log = super::alpaca_order_read::open_existing(journal, account)?;
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret =
        super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let read = |path: &str| {
        if path.starts_with("/v2/orders:") {
            super::alpaca_order_read::read_path(&agent, PAPER_API, path, &key, &secret)
        } else {
            super::alpaca_read::get(&agent, &format!("{PAPER_API}{path}"), &key, &secret).map(Some)
        }
    };
    let report = maintain(
        &mut log,
        account,
        cancel_working,
        |log| super::alpaca_cash::checkpoint(log, account, pages, read).map(|_| ()),
        |log, client| {
            super::alpaca_cancel::execute(log, client, read, |path| {
                super::alpaca_cancel::delete(&agent, &key, &secret, path)
            })
        },
    )?;
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_journal::{BrokerCash, BrokerIntent, BrokerProgress, BrokerSide};

    fn cash(at: u64) -> BrokerCash {
        BrokerCash {
            account_id: "paper".into(),
            started: at,
            completed: at,
            observation: json!({}),
            orders: Some(std::collections::BTreeMap::default()),
        }
    }

    fn intent(client: &str) -> BrokerIntent {
        BrokerIntent {
            account_id: "paper".into(),
            client_order_id: client.into(),
            symbol: "SPY".into(),
            side: BrokerSide::Buy,
            quantity_nanoshares: 1_000_000_000,
            limit_microusd_per_share: 100_000_000,
            fee_bound_microusd: 7,
        }
    }

    #[test]
    fn actual_checkpoint_runs_before_maintenance_without_orders_or_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = BrokerLog::open(dir.path().join("paper.jsonl"), "paper").unwrap();
        let mut paths = Vec::new();
        let report = maintain(
            &mut log,
            "paper",
            false,
            |log| {
                super::super::alpaca_cash::checkpoint(log, "paper", 1, |path| {
                    paths.push(path.to_owned());
                    Ok(Some(if path == "/v2/account" {
                        json!({"id":"paper","status":"ACTIVE","currency":"USD","cash":"100"})
                            .to_string()
                    } else {
                        "[]".into()
                    }))
                })
                .map(|_| ())
            },
            |_, _| panic!("observation must never cancel"),
        )
        .unwrap();
        assert!(
            paths
                .iter()
                .any(|path| path.starts_with("/v2/account/activities"))
        );
        assert!(log.cash().unwrap().is_some());
        assert_eq!(report["working_before_cancellation"], json!([]));
        assert_eq!(report["new_order_authority"], false);
        assert!(log.entries().unwrap().next().is_none());
    }

    #[test]
    fn checkpoint_failure_or_absence_stops_before_any_cancellation() {
        for missing in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut log = BrokerLog::open(dir.path().join("paper.jsonl"), "paper").unwrap();
            assert!(
                maintain(
                    &mut log,
                    "paper",
                    true,
                    |_| if missing {
                        Ok(())
                    } else {
                        Err("unknown observation".into())
                    },
                    |_, _| panic!("unverified checkpoint must not cancel")
                )
                .is_err()
            );
        }
    }

    #[test]
    fn stop_selects_only_one_working_order_and_never_retries_pending_cancellation() {
        for stop in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut log = BrokerLog::open(dir.path().join("paper.jsonl"), "paper").unwrap();
            log.observe_cash(cash(1), 1).unwrap();
            for client in ["a_terminal", "b_pending", "c_working", "d_working"] {
                log.reserve(intent(client), 2).unwrap();
                log.submit(client, 3, |_| ()).unwrap();
            }
            let mut observed = cash(4);
            observed.orders = Some(
                ["a_terminal", "b_pending", "c_working", "d_working"]
                    .map(|client| {
                        (
                            client.into(),
                            BrokerProgress {
                                broker_order_id: format!("broker-{client}").replace('_', "-"),
                                filled_nanoshares: 0,
                                terminal: client == "a_terminal",
                            },
                        )
                    })
                    .into(),
            );
            log.observe_cash(observed, 4).unwrap();
            log.cancel("b_pending", "broker-b-pending", 5, |_| ())
                .unwrap();
            let before = log.claims().unwrap();
            let report = maintain(
                &mut log,
                "paper",
                stop,
                |_| Ok(()),
                |log, client| {
                    assert!(stop);
                    assert_eq!(client, "c_working");
                    log.cancel(client, "broker-c-working", 6, |_| ()).unwrap();
                    Ok(json!({"cancellation":"request_accepted"}))
                },
            )
            .unwrap();
            assert_eq!(report["terminal"], json!(["a_terminal"]));
            assert_eq!(
                report["cancellation_pending_before_tick"],
                json!(["b_pending"])
            );
            assert_eq!(
                report["working_before_cancellation"],
                json!(["c_working", "d_working"])
            );
            assert_eq!(!report["cancellation_attempt"].is_null(), stop);
            assert_eq!(log.claims().unwrap(), before);
        }
    }

    #[test]
    fn unknown_claim_prevents_stop_selection_and_cancel_error_is_not_retried() {
        for unknown in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut log = BrokerLog::open(dir.path().join("paper.jsonl"), "paper").unwrap();
            log.observe_cash(cash(1), 1).unwrap();
            log.reserve(intent("a_working"), 2).unwrap();
            log.submit("a_working", 3, |_| ()).unwrap();
            let mut observed = cash(4);
            observed.orders = Some(
                [(
                    "a_working".into(),
                    BrokerProgress {
                        broker_order_id: "broker-a".into(),
                        filled_nanoshares: 0,
                        terminal: false,
                    },
                )]
                .into(),
            );
            log.observe_cash(observed, 4).unwrap();
            if unknown {
                log.reserve(intent("b_unknown"), 5).unwrap();
            }
            let mut calls = 0;
            let result = maintain(
                &mut log,
                "paper",
                true,
                |_| Ok(()),
                |log, client| {
                    calls += 1;
                    log.cancel(client, "broker-a", 6, |_| ()).unwrap();
                    Err("response lost".into())
                },
            );
            if unknown {
                assert_eq!(calls, 0);
                assert_eq!(result.unwrap()["unresolved"], json!(["b_unknown"]));
            } else {
                assert_eq!(calls, 1);
                assert!(result.is_err());
                let result = maintain(
                    &mut log,
                    "paper",
                    true,
                    |_| Ok(()),
                    |_, _| panic!("uncertain cancellation must not retry"),
                )
                .unwrap();
                assert_eq!(
                    result["cancellation_pending_before_tick"],
                    json!(["a_working"])
                );
                assert!(result["cancellation_attempt"].is_null());
            }
        }
    }
}
