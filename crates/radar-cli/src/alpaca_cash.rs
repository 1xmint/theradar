// SPDX-License-Identifier: Apache-2.0
//! Account-wide cash consistency from retained orders and selected activity deltas.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;

use radar_journal::{Applied, BrokerCash, BrokerIntent, BrokerLog, BrokerProgress};
use serde::Serialize;
use serde_json::{Value, json};

use super::alpaca_activities::{Activity, identity};
use super::alpaca_exposure::nanos;
use super::alpaca_order_read as recovery;
use super::alpaca_read::BrokerAccount;
use super::alpaca_reconcile::BrokerOrder;

const PAPER_API: &str = "https://paper-api.alpaca.markets";
const MAX_BASELINE_BYTES: u64 = 16_777_216;
const SCALE: i128 = 1_000_000_000;

#[derive(Serialize)]
struct Snapshot {
    account: BrokerAccount,
    read_started_unix_secs: u64,
    read_completed_unix_secs: u64,
    activities: Vec<Activity>,
}

fn cash(account: &BrokerAccount) -> Result<u64, String> {
    if account.currency.as_deref() != Some("USD") {
        return Err("paper cash USD currency is unknown".into());
    }
    nanos(
        account
            .cash
            .as_deref()
            .ok_or("paper cash balance is unknown")?,
    )
}

fn snapshot(report: &Value, expected: &str) -> Result<Snapshot, String> {
    if expected.trim().is_empty()
        || report["version"] != 1
        || report["venue"] != "alpaca"
        || report["environment"] != "paper"
        || report["source"] != PAPER_API
        || report["expected_account_id"] != expected
        || report["pagination_exhausted"] != true
        || report["selected_account_fields_unchanged"] != true
        || report["pages_read"].as_u64().is_none_or(|pages| pages == 0)
        || report["account_before"] != report["account_after"]
    {
        return Err("invalid or changing paper cash observation".into());
    }
    let account: BrokerAccount = serde_json::from_value(report["account_before"].clone())
        .map_err(|_| "invalid selected paper cash account")?;
    if account.id != expected {
        return Err("paper cash account identity is unknown".into());
    }
    cash(&account)?;
    let started = report["read_started_unix_secs"]
        .as_u64()
        .ok_or("cash read start is unknown")?;
    let completed = report["read_completed_unix_secs"]
        .as_u64()
        .ok_or("cash read completion is unknown")?;
    if completed < started {
        return Err("paper cash observation clock moved backwards".into());
    }
    let activities: Vec<Activity> = serde_json::from_value(report["activities"].clone())
        .map_err(|_| "invalid selected cash activities")?;
    let mut seen = BTreeSet::new();
    for event in &activities {
        identity(event, expected, &mut seen)?;
    }
    Ok(Snapshot {
        account,
        read_started_unix_secs: started,
        read_completed_unix_secs: completed,
        activities,
    })
}

fn load(path: &str, expected: &str, now: u64) -> Result<Snapshot, String> {
    let file = std::fs::File::open(path).map_err(|_| "paper cash baseline unavailable")?;
    let mut bytes = Vec::new();
    file.take(MAX_BASELINE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "paper cash baseline read failed")?;
    if u64::try_from(bytes.len()).map_err(|_| "baseline size overflow")? > MAX_BASELINE_BYTES {
        return Err("paper cash baseline exceeds 16 MiB".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "invalid paper cash baseline JSON")?;
    let baseline = snapshot(&value, expected)?;
    if baseline.read_completed_unix_secs > now {
        return Err("paper cash baseline is in the future".into());
    }
    Ok(baseline)
}

fn signed(text: &str) -> Result<i128, String> {
    if let Some(magnitude) = text.strip_prefix('-') {
        Ok(-i128::from(nanos(magnitude)?))
    } else {
        Ok(i128::from(nanos(text)?))
    }
}

fn add(total: &mut i128, value: i128) -> Result<(), String> {
    *total = total
        .checked_add(value)
        .ok_or("paper cash arithmetic overflow")?;
    Ok(())
}

fn bounds(scaled: i128) -> (i128, i128) {
    let floor = scaled.div_euclid(SCALE);
    (floor, floor + i128::from(scaled.rem_euclid(SCALE) != 0))
}

fn delta(
    baseline: &Snapshot,
    current: &Snapshot,
    bindings: &BTreeMap<String, BrokerIntent>,
) -> Result<Value, String> {
    if baseline.read_completed_unix_secs > current.read_started_unix_secs {
        return Err("paper cash baseline overlaps or follows current observation".into());
    }
    let prior: BTreeMap<_, _> = baseline
        .activities
        .iter()
        .map(|event| (&event.id, event))
        .collect();
    let observed: BTreeMap<_, _> = current
        .activities
        .iter()
        .map(|event| (&event.id, event))
        .collect();
    for (id, event) in &prior {
        if observed.get(id).copied() != Some(*event) {
            return Err("baseline activity disappeared or changed; cash history unresolved".into());
        }
    }
    let mut trade_scaled = 0;
    let mut external_nanos = 0;
    let mut fee_nanos = 0;
    let mut count = 0;
    for (id, event) in observed {
        if prior.contains_key(id) {
            continue;
        }
        count += 1;
        match event.category.as_str() {
            "FILL" => {
                let order_id = event
                    .order_id
                    .as_ref()
                    .ok_or("cash fill order identity is unknown")?;
                if !bindings.contains_key(order_id) {
                    return Err("cash fill has no bound retained order".into());
                }
                // The actual caller has already matched all execution payloads
                // and intervals. Retain each exact product for account-wide sums.
                let qty = nanos(
                    event
                        .qty
                        .as_deref()
                        .ok_or("cash fill quantity is unknown")?,
                )?;
                let price = nanos(event.price.as_deref().ok_or("cash fill price is unknown")?)?;
                let product = i128::try_from(u128::from(qty) * u128::from(price))
                    .map_err(|_| "paper cash fill product overflow")?;
                let amount = match event.side.as_deref() {
                    Some("buy") => -product,
                    Some("sell") => product,
                    _ => return Err("cash fill side is unknown".into()),
                };
                add(&mut trade_scaled, amount)?;
            }
            "CSD" | "CSW" | "FEE" => {
                let amount = signed(
                    event
                        .net_amount
                        .as_deref()
                        .ok_or("nontrade cash amount is unknown")?,
                )?;
                match event.category.as_str() {
                    "CSD" if amount < 0 => return Err("deposit has negative cash amount".into()),
                    "CSW" if amount > 0 => return Err("withdrawal has positive cash amount".into()),
                    "FEE" => add(&mut fee_nanos, amount)?,
                    _ => add(&mut external_nanos, amount)?,
                }
            }
            _ => return Err("unsupported new cash activity; reconciliation unresolved".into()),
        }
    }
    let starting = cash(&baseline.account)?;
    let actual = cash(&current.account)?;
    let mut expected = i128::from(starting) * SCALE;
    add(&mut expected, trade_scaled)?;
    add(
        &mut expected,
        external_nanos
            .checked_mul(SCALE)
            .ok_or("external cash overflow")?,
    )?;
    add(
        &mut expected,
        fee_nanos.checked_mul(SCALE).ok_or("fee cash overflow")?,
    )?;
    let (floor, ceil) = bounds(expected);
    if floor < 0 || i128::from(actual) < floor || i128::from(actual) > ceil {
        return Err("observed paper cash differs from selected activity delta".into());
    }
    let (trade_floor, trade_ceil) = bounds(trade_scaled);
    Ok(
        json!({"new_activities":count,"starting_cash_nanos":starting.to_string(),
        "observed_cash_nanos":actual.to_string(),"expected_cash_floor_nanos":floor.to_string(),
        "expected_cash_ceil_nanos":ceil.to_string(),"trade_cash_flow_floor_nanos":trade_floor.to_string(),
        "trade_cash_flow_ceil_nanos":trade_ceil.to_string(),
        "external_cash_flow_nanos":external_nanos.to_string(),"fee_cash_flow_nanos":fee_nanos.to_string(),
        "observed_cash_consistent":true,"profit_measured":false}),
    )
}

fn collect(
    log: &BrokerLog,
    expected: &str,
    baseline: &Snapshot,
    max_pages: u16,
    mut read: impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<Value, String> {
    if max_pages == 0 || baseline.account.id != expected {
        return Err("cash reconciliation account or page limit is invalid".into());
    }
    let intents: Vec<_> = log
        .entries()
        .map_err(|e| e.to_string())?
        .map(|entry| entry.intent.clone())
        .collect();
    let mut before = Vec::new();
    for intent in &intents {
        let report =
            recovery::collect_bound(expected, &intent.client_order_id, Some(intent), &mut read)?;
        if report["intent_payload_verified"] != true {
            return Err("retained cash order is not visible; reconciliation unresolved".into());
        }
        before.push(report);
    }
    let activity = super::alpaca_activities::collect(expected, max_pages, |path| {
        read(path)?.ok_or_else(|| "paper cash activity observation is missing".into())
    })?;
    let current = snapshot(&activity, expected)?;
    let mut orders = Vec::new();
    let mut bindings = BTreeMap::new();
    let mut progress = BTreeMap::new();
    for (intent, first) in intents.iter().zip(before) {
        let last =
            recovery::collect_bound(expected, &intent.client_order_id, Some(intent), &mut read)?;
        if first["order"] != last["order"]
            || [
                &first["account_before"],
                &first["account_after"],
                &last["account_before"],
                &last["account_after"],
            ]
            .iter()
            .any(|account| **account != activity["account_before"])
        {
            return Err("cash order or selected account changed during reconciliation".into());
        }
        let order: BrokerOrder = serde_json::from_value(first["order"].clone())
            .map_err(|_| "invalid selected cash order")?;
        let executions = super::alpaca_fills::executions(intent, &order, &current.activities)?;
        progress.insert(
            intent.client_order_id.clone(),
            BrokerProgress {
                broker_order_id: order.id.clone(),
                filled_nanoshares: executions.filled_nanoshares,
                terminal: matches!(
                    order.status.as_str(),
                    "filled" | "canceled" | "expired" | "rejected"
                ),
            },
        );
        if bindings.insert(order.id, intent.clone()).is_some() {
            return Err("broker cash order identity is repeated".into());
        }
        orders.push(json!({"intent":intent,"before":first,"after":last,"executions":executions}));
    }
    let cash = delta(baseline, &current, &bindings)?;
    let claims = super::alpaca_journal::report(log, expected, false)?;
    Ok(
        json!({"version":1,"venue":"alpaca","environment":"paper","source":PAPER_API,
        "baseline":baseline,"activity_observations":activity,"orders":orders,"cash":cash,"claims":claims,
        "order_progress":progress,
        "atomic_snapshot":false,"baseline_provenance_verified":false,"economic_history_complete":false,
        "cash_reconciled":false,"fees_final":false,"claim_release_authorized":false,
        "retry_authorized":false,"execution_authority":false}),
    )
}

fn retained_snapshot(prior: &BrokerCash, expected: &str, now: u64) -> Result<Snapshot, String> {
    let baseline = snapshot(&prior.observation, expected)?;
    if baseline.read_started_unix_secs != prior.started
        || baseline.read_completed_unix_secs != prior.completed
        || prior.completed > now
    {
        return Err("durable cash baseline times are invalid".into());
    }
    Ok(baseline)
}

fn checkpoint(
    log: &mut BrokerLog,
    expected: &str,
    max_pages: u16,
    mut read: impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<Value, String> {
    let retained = log.cash().map_err(|e| e.to_string())?.cloned();
    let (observation, reconciliation) = if let Some(prior) = retained {
        let baseline = retained_snapshot(&prior, expected, super::wallet_read::now()?)?;
        let report = collect(log, expected, &baseline, max_pages, read)?;
        (report["activity_observations"].clone(), Some(report))
    } else {
        if log.entries().map_err(|e| e.to_string())?.next().is_some() {
            return Err("cash genesis must precede retained paper orders".into());
        }
        let observation = super::alpaca_activities::collect(expected, max_pages, |path| {
            read(path)?.ok_or_else(|| "paper cash activity observation is missing".into())
        })?;
        (observation, None)
    };
    let selected = snapshot(&observation, expected)?;
    let cash = BrokerCash {
        account_id: expected.to_owned(),
        started: selected.read_started_unix_secs,
        completed: selected.read_completed_unix_secs,
        observation,
        orders: Some(match &reconciliation {
            Some(report) => serde_json::from_value(report["order_progress"].clone())
                .map_err(|_| "invalid selected paper order progress")?,
            None => BTreeMap::new(),
        }),
    };
    let applied = log
        .observe_cash(cash, super::wallet_read::now()?)
        .map_err(|e| e.to_string())?;
    Ok(json!({"version":1,"venue":"alpaca","environment":"paper",
        "expected_account_id":expected,"checkpoint_written":applied == Applied::Advanced,
        "checkpoint":log.cash().map_err(|e| e.to_string())?,"reconciliation":reconciliation,
        "claims":super::alpaca_journal::report(log, expected, false)?,
        "economic_history_complete":false,"fees_final":false,"profit_measured":false,
        "claim_release_authorized":false,"retry_authorized":false,"execution_authority":false}))
}

pub(super) fn checkpoint_run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let [
        "alpaca-cash-checkpoint",
        "--journal",
        path,
        "--account-id",
        expected,
        "--max-pages",
        pages,
    ] = values.as_slice()
    else {
        return Err("usage: radar alpaca-cash-checkpoint --journal <private-file> --account-id <paper-id> --max-pages <positive-u16>".into());
    };
    let max_pages = super::alpaca_activities::page_limit(pages)?;
    let mut log = BrokerLog::open(*path, expected).map_err(|e| e.to_string())?;
    if log.cash().map_err(|e| e.to_string())?.is_none()
        && log.entries().map_err(|e| e.to_string())?.next().is_some()
    {
        return Err("cash genesis must precede retained paper orders".into());
    }
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret =
        super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let report = checkpoint(&mut log, expected, max_pages, |path| {
        if path.starts_with("/v2/orders:") {
            recovery::read_path(&agent, PAPER_API, path, &key, &secret)
        } else {
            super::alpaca_read::get(&agent, &format!("{PAPER_API}{path}"), &key, &secret).map(Some)
        }
    })?;
    println!("{report}");
    Ok(())
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let [
        "alpaca-cash-reconcile",
        "--journal",
        path,
        "--account-id",
        expected,
        "--baseline",
        input,
        "--max-pages",
        pages,
    ] = values.as_slice()
    else {
        return Err("usage: radar alpaca-cash-reconcile --journal <private-file> --account-id <paper-id> --baseline <activity-report> --max-pages <positive-u16>".into());
    };
    let max_pages = super::alpaca_activities::page_limit(pages)?;
    let log = recovery::open_existing(path, expected)?;
    let baseline = load(input, expected, super::wallet_read::now()?)?;
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret =
        super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let report = collect(&log, expected, &baseline, max_pages, |path| {
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
    use radar_journal::BrokerSide;

    fn account(cash: &str) -> Value {
        json!({"id":"paper-id","status":"ACTIVE","currency":"USD","cash":cash,"private":"omit"})
    }

    fn observation_reader(balance: &str, activities: &Value, path: &str) -> String {
        if path == "/v2/account" {
            account(balance).to_string()
        } else {
            activities.to_string()
        }
    }

    #[test]
    fn cash_checkpoint_derives_residual_claims_from_verified_fills_and_order_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        for side in [BrokerSide::Buy, BrokerSide::Sell] {
            for status in [
                "new",
                "accepted",
                "pending_new",
                "partially_filled",
                "pending_cancel",
                "done_for_day",
                "canceled",
                "expired",
                "rejected",
                "filled",
            ] {
                let path = dir.path().join(format!("{side:?}-{status}.jsonl"));
                let mut log = BrokerLog::open(&path, "paper-id").unwrap();
                checkpoint(&mut log, "paper-id", 1, |p| {
                    Ok(Some(observation_reader("500", &json!([prior()]), p)))
                })
                .unwrap();
                let intent = intent(side);
                let id = intent.client_order_id.clone();
                let now = super::super::wallet_read::now().unwrap();
                log.reserve(intent.clone(), now).unwrap();
                log.submit(&id, now, |_| ()).unwrap();
                let full = status == "filled";
                let quantity = if full { "1" } else { "0.5" };
                let mut fill = fill(side);
                fill["qty"] = json!(quantity);
                fill["cum_qty"] = json!(quantity);
                fill["leaves_qty"] = json!(if full { "0" } else { "0.5" });
                fill["price"] = json!("100");
                let mut order = order(side);
                order["status"] = json!(status);
                order["filled_qty"] = json!(quantity);
                let balance = match (side, full) {
                    (BrokerSide::Buy, true) => "400",
                    (BrokerSide::Buy, false) => "450",
                    (BrokerSide::Sell, true) => "600",
                    (BrokerSide::Sell, false) => "550",
                };
                let result = checkpoint(&mut log, "paper-id", 1, |p| {
                    Ok(Some(if p == "/v2/account" {
                        account(balance).to_string()
                    } else if p.starts_with("/v2/orders:") {
                        order.to_string()
                    } else {
                        json!([prior(), fill]).to_string()
                    }))
                })
                .unwrap();
                let terminal = matches!(status, "canceled" | "expired" | "rejected" | "filled");
                let cash = if side == BrokerSide::Buy && !terminal {
                    50_000_007
                } else {
                    7
                };
                let shares = if side == BrokerSide::Sell && !terminal {
                    500_000_000
                } else {
                    0
                };
                assert_eq!(
                    result["claims"]["claims"][0]["observed_remaining_cash_claim_microusd"],
                    cash.to_string()
                );
                assert_eq!(
                    result["claims"]["claims"][0]["observed_remaining_share_claim_nanoshares"],
                    shares.to_string()
                );
                assert_eq!(result["claim_release_authorized"], false);
                drop(log);
                let mut log = BrokerLog::open(&path, "paper-id").unwrap();
                let residual = log.claims().unwrap().remove(&id).unwrap();
                assert_eq!(residual.cash_microusd, cash);
                assert_eq!(residual.shares_nanoshares, shares);
                assert_eq!(residual.progress.unwrap().terminal, terminal);
                assert_eq!(log.entries().unwrap().next().unwrap().intent, intent);
                assert!(
                    log.submit(&id, now, |_| panic!("reconciled does not mean retry"))
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn durable_checkpoint_bootstraps_replays_and_accounts_for_deposits_and_late_fees() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cash.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        let boot = checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(observation_reader("100", &json!([]), p)))
        })
        .unwrap();
        assert_eq!(boot["checkpoint_written"], true);
        assert_eq!(boot["reconciliation"], Value::Null);
        assert!(boot["checkpoint"]["observation"]["account_before"]["private"].is_null());
        drop(log);
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        let deposit = json!([{"id":"deposit","activity_type":"CSD","net_amount":"10"}]);
        let result = checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(observation_reader("110", &deposit, p)))
        })
        .unwrap();
        assert_eq!(
            result["reconciliation"]["cash"]["external_cash_flow_nanos"],
            "10000000000"
        );
        drop(log);
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        let late = json!([deposit[0], {"id":"fee","activity_type":"FEE","net_amount":"-0.01"}]);
        let result = checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(observation_reader("109.99", &late, p)))
        })
        .unwrap();
        assert_eq!(
            result["reconciliation"]["cash"]["fee_cash_flow_nanos"],
            "-10000000"
        );
        for flag in [
            "economic_history_complete",
            "fees_final",
            "profit_measured",
            "claim_release_authorized",
            "retry_authorized",
            "execution_authority",
        ] {
            assert_eq!(result[flag], false);
        }
        drop(log);
        let log = BrokerLog::open(&path, "paper-id").unwrap();
        assert_eq!(
            log.cash().unwrap().unwrap().observation["account_before"]["cash"],
            "109.99"
        );
        assert_eq!(
            radar_journal::Journal::open(&path)
                .unwrap()
                .events()
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn failed_observations_and_cash_corrections_keep_the_durable_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cash.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        let deposit = json!([{"id":"deposit","activity_type":"CSD","net_amount":"10"}]);
        checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(observation_reader("110", &deposit, p)))
        })
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        for stage in 0..3 {
            for failure in 0..3 {
                let mut index = 0;
                assert!(
                    checkpoint(&mut log, "paper-id", 1, |p| {
                        let current = index;
                        index += 1;
                        if current == stage {
                            return match failure {
                                0 => Err("transport".into()),
                                1 => Ok(None),
                                _ => Ok(Some("null".into())),
                            };
                        }
                        Ok(Some(observation_reader("110", &deposit, p)))
                    })
                    .is_err()
                );
                assert!(index > stage);
                assert_eq!(std::fs::read(&path).unwrap(), bytes);
            }
        }
        for (balance, activities) in [
            ("111", deposit.clone()),
            ("110", json!([])),
            (
                "110",
                json!([{"id":"deposit","activity_type":"CSD","net_amount":"11"}]),
            ),
            (
                "110",
                json!([deposit[0], {"id":"unknown","activity_type":"DIV","net_amount":"0"}]),
            ),
        ] {
            assert!(
                checkpoint(&mut log, "paper-id", 1, |p| Ok(Some(observation_reader(
                    balance,
                    &activities,
                    p
                ))))
                .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        // A successful unchanged observation remains usable after failures.
        checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(observation_reader("110", &deposit, p)))
        })
        .unwrap();
    }

    #[test]
    fn checkpoint_revalidates_retained_selected_evidence_and_never_bootstraps_over_orders() {
        let dir = tempfile::tempdir().unwrap();
        let valid = BrokerCash {
            account_id: "paper-id".into(),
            started: 1,
            completed: 2,
            observation: report("100", json!([]), 1, 2),
            orders: None,
        };
        assert!(retained_snapshot(&valid, "paper-id", 2).is_ok());
        assert!(retained_snapshot(&valid, "paper-id", 3).is_ok());
        assert!(retained_snapshot(&valid, "paper-id", 1).is_err());
        for case in 0..4 {
            let path = dir.path().join(format!("case-{case}.jsonl"));
            let mut log = BrokerLog::open(&path, "paper-id").unwrap();
            let mut observation = report("100", json!([]), 1, 2);
            match case {
                0 => observation["pagination_exhausted"] = json!(false),
                1 => observation["read_started_unix_secs"] = json!(0),
                2 => observation["read_completed_unix_secs"] = json!(1),
                _ => {}
            }
            if case == 3 {
                log.reserve(intent(BrokerSide::Buy), 1).unwrap();
            } else {
                log.observe_cash(
                    BrokerCash {
                        account_id: "paper-id".into(),
                        started: 1,
                        completed: 2,
                        observation,
                        orders: None,
                    },
                    2,
                )
                .unwrap();
            }
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                checkpoint(&mut log, "paper-id", 1, |_| panic!(
                    "invalid baseline must not read"
                ))
                .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn durable_cash_updates_bind_retained_orders_and_preserve_unknown_submission_claims() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cash.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(observation_reader("500", &json!([prior()]), p)))
        })
        .unwrap();
        for side in [BrokerSide::Buy, BrokerSide::Sell] {
            let intent = intent(side);
            let id = intent.client_order_id.clone();
            log.reserve(intent, super::super::wallet_read::now().unwrap())
                .unwrap();
            log.submit(&id, super::super::wallet_read::now().unwrap(), |_| ())
                .unwrap();
        }
        let result = checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(if p == "/v2/account" {
                account("457.99").to_string()
            } else if p.contains("retained_Buy") {
                order(BrokerSide::Buy).to_string()
            } else if p.contains("retained_Sell") {
                order(BrokerSide::Sell).to_string()
            } else {
                events().to_string()
            }))
        })
        .unwrap();
        assert_eq!(
            result["reconciliation"]["cash"]["observed_cash_consistent"],
            true
        );
        assert_eq!(result["claims"]["claims"].as_array().unwrap().len(), 2);
        drop(log);
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        assert_eq!(
            log.cash().unwrap().unwrap().observation["account_before"]["cash"],
            "457.99"
        );
        for side in [BrokerSide::Buy, BrokerSide::Sell] {
            let intent = intent(side);
            let entry = log
                .entries()
                .unwrap()
                .find(|entry| entry.intent == intent)
                .unwrap();
            assert_eq!(entry.state, radar_journal::BrokerState::SubmissionUnknown);
            assert!(
                log.submit(&intent.client_order_id, 10, |_| panic!("no retry"))
                    .is_err()
            );
        }
    }

    fn report(cash: &str, activities: Value, started: u64, completed: u64) -> Value {
        let mut report = json!({"version":1,"venue":"alpaca","environment":"paper","source":PAPER_API,
            "expected_account_id":"paper-id","pagination_exhausted":true,"pages_read":1,
            "selected_account_fields_unchanged":true,"account_before":account(cash),"account_after":account(cash),
            "read_started_unix_secs":started,"read_completed_unix_secs":completed});
        report["activities"] = activities;
        report
    }

    fn prior() -> Value {
        json!({"id":"prior","activity_type":"CSD","net_amount":"500"})
    }

    fn intent(side: BrokerSide) -> BrokerIntent {
        BrokerIntent {
            account_id: "paper-id".into(),
            client_order_id: format!("retained_{side:?}"),
            symbol: if side == BrokerSide::Buy {
                "SPY"
            } else {
                "QQQ"
            }
            .into(),
            side,
            quantity_nanoshares: 1_000_000_000,
            limit_microusd_per_share: if side == BrokerSide::Buy {
                100_000_000
            } else {
                50_000_000
            },
            fee_bound_microusd: 7,
        }
    }

    fn order(side: BrokerSide) -> Value {
        let intent = intent(side);
        json!({"id":format!("broker_{side:?}"),"client_order_id":intent.client_order_id,"symbol":intent.symbol,
            "asset_class":"us_equity","side":if side == BrokerSide::Buy {"buy"} else {"sell"},
            "type":"limit","order_class":"simple","time_in_force":"day","extended_hours":false,
            "qty":"1","limit_price":if side == BrokerSide::Buy {"100"} else {"50"},
            "filled_qty":"1","status":"filled"})
    }

    fn fill(side: BrokerSide) -> Value {
        let order = order(side);
        json!({"id":format!("fill_{side:?}"),"activity_type":"FILL","order_id":order["id"],
            "symbol":order["symbol"],"side":order["side"],"type":"fill","qty":"1",
            "price":order["limit_price"],"cum_qty":"1","leaves_qty":"0"})
    }

    fn events() -> Value {
        json!([prior(),fill(BrokerSide::Sell),{"id":"deposit","activity_type":"CSD","net_amount":"10"},
            {"id":"withdrawal","activity_type":"CSW","net_amount":"-2"},fill(BrokerSide::Buy),
            {"id":"fee","activity_type":"FEE","net_amount":"-0.01"}])
    }

    fn baseline() -> Snapshot {
        snapshot(&report("500", json!([prior()]), 1, 2), "paper-id").unwrap()
    }

    fn bindings() -> BTreeMap<String, BrokerIntent> {
        [BrokerSide::Buy, BrokerSide::Sell]
            .into_iter()
            .map(|side| (format!("broker_{side:?}"), intent(side)))
            .collect()
    }

    #[test]
    fn cash_delta_separates_buys_sells_external_funds_and_fees_without_claiming_profit() {
        for reverse in [false, true] {
            let mut activities = events();
            if reverse {
                activities.as_array_mut().unwrap().reverse();
            }
            let current = snapshot(&report("457.99", activities, 3, 4), "paper-id").unwrap();
            let result = delta(&baseline(), &current, &bindings()).unwrap();
            assert_eq!(result["starting_cash_nanos"], "500000000000");
            assert_eq!(result["observed_cash_nanos"], "457990000000");
            assert_eq!(result["expected_cash_floor_nanos"], "457990000000");
            assert_eq!(result["expected_cash_ceil_nanos"], "457990000000");
            assert_eq!(result["trade_cash_flow_floor_nanos"], "-50000000000");
            assert_eq!(result["trade_cash_flow_ceil_nanos"], "-50000000000");
            assert_eq!(result["external_cash_flow_nanos"], "8000000000");
            assert_eq!(result["fee_cash_flow_nanos"], "-10000000");
            assert_eq!(result["new_activities"], 5);
            assert_eq!(result["observed_cash_consistent"], true);
            assert_eq!(result["profit_measured"], false);
        }
        let current = snapshot(&report("500", json!([prior()]), 2, 2), "paper-id").unwrap();
        let unchanged = delta(&baseline(), &current, &BTreeMap::new()).unwrap();
        assert_eq!(unchanged["new_activities"], 0);
        assert_eq!(unchanged["trade_cash_flow_floor_nanos"], "0");
    }

    #[test]
    fn signed_fractional_cash_bounds_round_outward_once_and_never_add_a_tolerance() {
        assert_eq!(bounds(0), (0, 0));
        assert_eq!(bounds(1), (0, 1));
        assert_eq!(bounds(-1), (-1, 0));
        assert_eq!(bounds(SCALE), (1, 1));
        assert_eq!(bounds(-SCALE), (-1, -1));
        let empty = snapshot(&report("0", json!([]), 1, 1), "paper-id").unwrap();
        let zero = snapshot(&report("0", json!([]), 2, 2), "paper-id").unwrap();
        assert_eq!(
            delta(&empty, &zero, &BTreeMap::new()).unwrap()["observed_cash_nanos"],
            "0"
        );
        let mut tiny_buy = fill(BrokerSide::Buy);
        tiny_buy["qty"] = json!("0.000000001");
        tiny_buy["price"] = json!("0.1");
        let negative_fraction =
            snapshot(&report("0", json!([tiny_buy]), 2, 2), "paper-id").unwrap();
        assert!(delta(&empty, &negative_fraction, &bindings()).is_err());
        let base = snapshot(&report("1", json!([]), 1, 1), "paper-id").unwrap();
        for side in [BrokerSide::Buy, BrokerSide::Sell] {
            let mut event = fill(side);
            event["qty"] = json!("0.000000001");
            event["price"] = json!("0.1");
            for cash in if side == BrokerSide::Buy {
                ["0.999999999", "1"]
            } else {
                ["1", "1.000000001"]
            } {
                let current = snapshot(&report(cash, json!([event]), 2, 2), "paper-id").unwrap();
                let result = delta(&base, &current, &bindings()).unwrap();
                assert_eq!(
                    result["trade_cash_flow_floor_nanos"],
                    if side == BrokerSide::Buy { "-1" } else { "0" }
                );
                assert_eq!(
                    result["trade_cash_flow_ceil_nanos"],
                    if side == BrokerSide::Buy { "0" } else { "1" }
                );
            }
            let cash = if side == BrokerSide::Buy {
                "0.999999998"
            } else {
                "1.000000002"
            };
            let current = snapshot(&report(cash, json!([event]), 2, 2), "paper-id").unwrap();
            assert!(delta(&base, &current, &bindings()).is_err());
        }
        let current = snapshot(
            &report(
                "1.000000001",
                json!([
            {"id":"a","activity_type":"FEE","net_amount":"0.000000001"},
            {"id":"b","activity_type":"CSD","net_amount":"0"},
            {"id":"c","activity_type":"CSW","net_amount":"-0"}]),
                2,
                2,
            ),
            "paper-id",
        )
        .unwrap();
        assert_eq!(
            delta(&base, &current, &BTreeMap::new()).unwrap()["fee_cash_flow_nanos"],
            "1"
        );
    }

    #[test]
    fn changed_removed_unknown_or_unbound_cash_history_cannot_reconcile() {
        let mut changed = events();
        changed[0]["net_amount"] = json!("499");
        let mut removed = events();
        removed.as_array_mut().unwrap().remove(0);
        for activities in [changed, removed] {
            let current = snapshot(&report("457.99", activities, 3, 4), "paper-id").unwrap();
            assert!(delta(&baseline(), &current, &bindings()).is_err());
        }
        let empty = snapshot(&report("0", json!([]), 1, 2), "paper-id").unwrap();
        for event in [
            json!({"id":"a","activity_type":"DIV","net_amount":"0"}),
            json!({"id":"a","activity_type":"CSD","net_amount":"-1"}),
            json!({"id":"a","activity_type":"CSW","net_amount":"1"}),
            json!({"id":"a","activity_type":"FEE"}),
            json!({"id":"a","activity_type":"FILL","order_id":"foreign"}),
            json!({"id":"a","activity_type":"FILL"}),
        ] {
            let current = snapshot(&report("0", json!([event]), 3, 4), "paper-id").unwrap();
            assert!(delta(&empty, &current, &bindings()).is_err());
        }
        for cash in ["457.989999999", "457.990000001", "0", "500"] {
            let current = snapshot(&report(cash, events(), 3, 4), "paper-id").unwrap();
            assert!(delta(&baseline(), &current, &bindings()).is_err());
        }
        let current = snapshot(&report("500", json!([prior()]), 1, 2), "paper-id").unwrap();
        assert!(delta(&baseline(), &current, &BTreeMap::new()).is_err());
        for (category, amount, cash) in [("CSD", "-1", "499"), ("CSW", "1", "501")] {
            let current = snapshot(
                &report(
                    cash,
                    json!([prior(),
                {"id":"wrong-sign","activity_type":category,"net_amount":amount}]),
                    3,
                    4,
                ),
                "paper-id",
            )
            .unwrap();
            assert!(delta(&baseline(), &current, &BTreeMap::new()).is_err());
        }
        let negative = snapshot(
            &report(
                "0",
                json!([{"id":"fee","activity_type":"FEE","net_amount":"-1"}]),
                3,
                4,
            ),
            "paper-id",
        )
        .unwrap();
        assert!(delta(&empty, &negative, &BTreeMap::new()).is_err());
    }

    #[test]
    fn invalid_baseline_envelopes_accounts_clocks_and_identities_refuse() {
        let valid = report("500", json!([prior()]), 1, 2);
        for (field, value) in [
            ("version", json!(2)),
            ("venue", json!("other")),
            ("environment", json!("live")),
            ("source", json!("https://api.alpaca.markets")),
            ("expected_account_id", json!("other")),
            ("pagination_exhausted", json!(false)),
            ("selected_account_fields_unchanged", json!(false)),
            ("pages_read", json!(0)),
            ("read_started_unix_secs", json!(3)),
            ("activities", json!([prior(), prior()])),
        ] {
            for replacement in [value, Value::Null] {
                let mut value = valid.clone();
                value[field] = replacement;
                assert!(snapshot(&value, "paper-id").is_err(), "{field}");
            }
        }
        for field in ["read_started_unix_secs", "read_completed_unix_secs"] {
            let mut value = valid.clone();
            value[field] = Value::Null;
            assert!(snapshot(&value, "paper-id").is_err());
        }
        for (field, value) in [
            ("id", json!("other")),
            ("currency", json!("EUR")),
            ("cash", json!("-1")),
            ("cash", json!("1e0")),
            ("cash", Value::Null),
            ("currency", Value::Null),
        ] {
            let mut report = valid.clone();
            report["account_before"][field] = value.clone();
            report["account_after"][field] = value;
            assert!(snapshot(&report, "paper-id").is_err(), "{field}");
        }
        let mut changed = valid.clone();
        changed["account_after"]["cash"] = json!("499");
        assert!(snapshot(&changed, "paper-id").is_err());
        for event in [
            json!({"id":"","activity_type":"CSD"}),
            json!({"id":"a","activity_type":" "}),
            json!({"id":"a","activity_type":"CSD","account_id":"other"}),
        ] {
            let mut report = valid.clone();
            report["activities"] = json!([event]);
            assert!(snapshot(&report, "paper-id").is_err());
        }
        assert!(snapshot(&valid, " ").is_err());
        for text in ["--1", "+1", "-", "1.0000000001", "1e0", "18446744074"] {
            assert!(signed(text).is_err());
        }
        assert_eq!(signed("-0.000000001").unwrap(), -1);
        let mut total = i128::MAX;
        assert!(add(&mut total, 1).is_err());
        let mut total = i128::MIN;
        assert!(add(&mut total, -1).is_err());
    }

    #[test]
    fn cash_effects_require_exact_known_amounts_and_refuse_product_overflow() {
        let empty = snapshot(&report("0", json!([]), 1, 2), "paper-id").unwrap();
        for field in ["qty", "price", "side"] {
            for value in [Value::Null, json!("invalid")] {
                let mut event = fill(BrokerSide::Buy);
                event[field] = value;
                let current = snapshot(&report("0", json!([event]), 3, 4), "paper-id").unwrap();
                assert!(delta(&empty, &current, &bindings()).is_err());
            }
        }
        for amount in ["-", "1e0", "1.0000000001", "18446744074"] {
            let current = snapshot(
                &report(
                    "0",
                    json!([{"id":"a","activity_type":"FEE","net_amount":amount}]),
                    3,
                    4,
                ),
                "paper-id",
            )
            .unwrap();
            assert!(delta(&empty, &current, &bindings()).is_err());
        }
        let mut event = fill(BrokerSide::Buy);
        event["qty"] = json!("18446744073.709551615");
        event["price"] = json!("18446744073.709551615");
        let current = snapshot(&report("0", json!([event]), 3, 4), "paper-id").unwrap();
        assert_eq!(
            delta(&empty, &current, &bindings()).unwrap_err(),
            "paper cash fill product overflow"
        );
    }

    #[test]
    fn account_wide_caller_binds_multiple_orders_keeps_claims_and_records_cash_components() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("claims.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        for side in [BrokerSide::Buy, BrokerSide::Sell] {
            log.reserve(intent(side), 1).unwrap();
        }
        let prior = std::fs::read(&path).unwrap();
        let mut paths = Vec::new();
        let result = collect(&log, "paper-id", &baseline(), 1, |path| {
            paths.push(path.to_owned());
            Ok(Some(if path == "/v2/account" {
                account("457.99").to_string()
            } else if path.contains("retained_Buy") {
                order(BrokerSide::Buy).to_string()
            } else if path.contains("retained_Sell") {
                order(BrokerSide::Sell).to_string()
            } else {
                events().to_string()
            }))
        })
        .unwrap();
        assert_eq!(paths.len(), 15);
        assert_eq!(
            paths
                .iter()
                .filter(|path| path.starts_with("/v2/orders:"))
                .count(),
            4
        );
        assert_eq!(result["orders"].as_array().unwrap().len(), 2);
        assert_eq!(result["cash"]["observed_cash_consistent"], true);
        assert_eq!(
            result["claims"]["claims"][0]["cash_claim_microusd"],
            "100000007"
        );
        assert_eq!(result["claims"]["claims"][1]["cash_claim_microusd"], "7");
        assert_eq!(
            result["claims"]["claims"][1]["share_claim_nanoshares"],
            "1000000000"
        );
        for flag in [
            "atomic_snapshot",
            "baseline_provenance_verified",
            "economic_history_complete",
            "cash_reconciled",
            "fees_final",
            "claim_release_authorized",
            "retry_authorized",
            "execution_authority",
        ] {
            assert_eq!(result[flag], false);
        }
        assert!(result["baseline"]["account"]["private"].is_null());
        assert_eq!(std::fs::read(&path).unwrap(), prior);
        let duplicate = collect(&log, "paper-id", &baseline(), 1, |path| {
            Ok(Some(if path == "/v2/account" {
                account("500").to_string()
            } else if path.starts_with("/v2/orders:") {
                let mut order = order(if path.contains("retained_Buy") {
                    BrokerSide::Buy
                } else {
                    BrokerSide::Sell
                });
                order["id"] = json!("repeated-broker-id");
                order["status"] = json!("new");
                order["filled_qty"] = json!("0");
                order.to_string()
            } else {
                json!([super::tests::prior()]).to_string()
            }))
        });
        assert_eq!(
            duplicate.unwrap_err(),
            "broker cash order identity is repeated"
        );
    }

    #[test]
    fn every_failed_or_changed_read_refuses_and_empty_journals_still_account_for_external_cash() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("claims.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        let empty = collect(&log, "paper-id", &baseline(), 1, |path| {
            Ok(Some(if path == "/v2/account" {
                account("501").to_string()
            } else {
                json!([prior(),{"id":"new","activity_type":"CSD","net_amount":"1"}]).to_string()
            }))
        })
        .unwrap();
        assert_eq!(empty["orders"], json!([]));
        assert_eq!(empty["cash"]["external_cash_flow_nanos"], "1000000000");
        log.reserve(intent(BrokerSide::Buy), 1).unwrap();
        let base = baseline();
        for stage in 0..9 {
            for failure in 0..4 {
                let mut index = 0;
                let result = collect(&log, "paper-id", &base, 1, |path| {
                    let current = index;
                    index += 1;
                    let mut body = if path == "/v2/account" {
                        account("400")
                    } else if path.starts_with("/v2/orders:") {
                        order(BrokerSide::Buy)
                    } else {
                        json!([prior(), fill(BrokerSide::Buy)])
                    };
                    if current == stage {
                        match failure {
                            0 => return Err("transport failed".into()),
                            1 => return Ok(None),
                            2 => return Ok(Some("null".into())),
                            _ => {
                                if path == "/v2/account" {
                                    body["cash"] = json!("399");
                                } else if path.starts_with("/v2/orders:") {
                                    body["updated_at"] = json!("changed");
                                } else {
                                    body[1]["qty"] = json!("0.5");
                                }
                            }
                        }
                    }
                    Ok(Some(body.to_string()))
                });
                assert!(index > stage, "must reach the injected read stage");
                assert!(result.is_err(), "stage={stage}, failure={failure}");
            }
        }
        for (expected, pages) in [("other", 1), ("paper-id", 0)] {
            assert!(
                collect(&log, expected, &base, pages, |_| panic!(
                    "invalid inputs must not read"
                ))
                .is_err()
            );
        }
    }

    #[test]
    fn baseline_files_are_bounded_and_malformed_inputs_do_not_echo_contents() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("baseline.json");
        let name = path.to_str().unwrap();
        assert!(load(name, "paper-id", 2).is_err());
        std::fs::write(&path, "sentinel-private-secret").unwrap();
        assert_eq!(
            load(name, "paper-id", 2).err().unwrap(),
            "invalid paper cash baseline JSON"
        );
        let valid = report("500", json!([prior()]), 1, 2).to_string();
        std::fs::write(&path, &valid).unwrap();
        assert!(load(name, "paper-id", 2).is_ok());
        assert!(load(name, "paper-id", 3).is_ok());
        assert_eq!(
            load(name, "paper-id", 1).err().unwrap(),
            "paper cash baseline is in the future"
        );
        let padding = usize::try_from(MAX_BASELINE_BYTES).unwrap() - valid.len();
        std::fs::write(&path, format!("{valid}{}", " ".repeat(padding))).unwrap();
        assert!(load(name, "paper-id", 2).is_ok());
        std::fs::write(&path, format!("{valid}{}", " ".repeat(padding + 1))).unwrap();
        assert_eq!(
            load(name, "paper-id", 2).err().unwrap(),
            "paper cash baseline exceeds 16 MiB"
        );
    }
}
