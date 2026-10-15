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

struct Opening {
    activity_id: String,
    snapshot: Snapshot,
}

fn opening_plan(log: &BrokerLog, expected: &str, activity_id: &str) -> Result<Opening, String> {
    if activity_id.is_empty()
        || activity_id.len() > 256
        || !activity_id.is_ascii()
        || activity_id.bytes().any(|byte| byte.is_ascii_control())
        || log.entries().map_err(|e| e.to_string())?.next().is_some()
    {
        return Err(
            "opening acknowledgement requires an activity ID and no retained orders".into(),
        );
    }
    let first = log
        .opening_cash()
        .map_err(|e| e.to_string())?
        .ok_or("opening acknowledgement requires retained genesis")?;
    let last = log
        .cash()
        .map_err(|e| e.to_string())?
        .ok_or("opening acknowledgement requires retained cash")?;
    let now = super::wallet_read::now()?;
    let opening = retained_snapshot(&first, expected, now)?;
    let baseline = retained_snapshot(last, expected, now)?;
    if !opening.activities.is_empty()
        || !baseline.activities.is_empty()
        || cash(&opening.account)? != cash(&baseline.account)?
        || !super::alpaca_positions::retained(&first.observation)?.is_empty()
        || !super::alpaca_positions::retained(&last.observation)?.is_empty()
    {
        return Err(
            "opening acknowledgement requires unchanged empty-account opening history".into(),
        );
    }
    Ok(Opening {
        activity_id: activity_id.to_owned(),
        snapshot: opening,
    })
}

fn opening_delta(
    baseline: &Snapshot,
    current: &Snapshot,
    opening: &Opening,
) -> Result<Value, String> {
    if baseline.read_completed_unix_secs > current.read_started_unix_secs {
        return Err("opening acknowledgement observations overlap or move backwards".into());
    }
    let [event] = current.activities.as_slice() else {
        return Err("opening acknowledgement requires exactly one cash activity".into());
    };
    let day = event
        .date
        .as_deref()
        .and_then(|date| super::alpaca_quote::timestamp_ns(&format!("{date}T00:00:00Z")))
        .ok_or("opening cash activity date is unknown")?
        / 1_000_000_000
        / 86_400;
    let amount = nanos(
        event
            .net_amount
            .as_deref()
            .ok_or("opening cash amount is unknown")?,
    )?;
    let initial = cash(&opening.snapshot.account)?;
    if event.id != opening.activity_id
        || event.category != "JNLC"
        || event.status.as_deref() != Some("executed")
        || amount == 0
        || amount != initial
        || cash(&baseline.account)? != initial
        || cash(&current.account)? != initial
        || !baseline.activities.is_empty()
        || event.order_id.is_some()
        || event.qty.is_some()
        || event.side.is_some()
        || event.symbol.is_some()
        || event.price.is_some()
        || event.cum_qty.is_some()
        || event.leaves_qty.is_some()
        || event.kind.is_some()
        || day >= u128::from(opening.snapshot.read_started_unix_secs / 86_400)
    {
        return Err("cash event does not match the acknowledged opening balance".into());
    }
    // A date is a coarse eligibility check, not proof of when cash posted.
    // Only the explicit operator acknowledgement selects this interpretation.
    // Keep its first availability NOW and retain the old journal prefix unchanged.
    Ok(
        json!({"new_activities":1,"starting_cash_nanos":initial.to_string(),
        "observed_cash_nanos":initial.to_string(),"expected_cash_floor_nanos":initial.to_string(),
        "expected_cash_ceil_nanos":initial.to_string(),"trade_cash_flow_floor_nanos":"0",
        "trade_cash_flow_ceil_nanos":"0","external_cash_flow_nanos":"0","fee_cash_flow_nanos":"0",
        "observed_cash_consistent":true,"profit_measured":false,
        "opening_cash_acknowledgement":{"activity_id":event.id,"amount_nanos":amount.to_string(),
        "opening_started_unix_secs":opening.snapshot.read_started_unix_secs,
        "operator_acknowledged_already_in_opening_cash":true,"broker_posting_time_verified":false}}),
    )
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
    read: impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<Value, String> {
    collect_checked(log, expected, baseline, max_pages, None, read)
}

fn collect_checked(
    log: &BrokerLog,
    expected: &str,
    baseline: &Snapshot,
    max_pages: u16,
    opening: Option<&Opening>,
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
    let cash = match opening {
        Some(opening) => opening_delta(baseline, &current, opening)?,
        None => delta(baseline, &current, &bindings)?,
    };
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

pub(super) fn checkpoint(
    log: &mut BrokerLog,
    expected: &str,
    max_pages: u16,
    read: impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<Value, String> {
    checkpoint_checked(log, expected, max_pages, None, read)
}

fn checkpoint_checked(
    log: &mut BrokerLog,
    expected: &str,
    max_pages: u16,
    opening: Option<&Opening>,
    mut read: impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<Value, String> {
    let retained = log.cash().map_err(|e| e.to_string())?.cloned();
    let prior_positions = retained
        .as_ref()
        .map(|prior| super::alpaca_positions::retained(&prior.observation))
        .transpose()?;
    if retained.is_none() && log.entries().map_err(|e| e.to_string())?.next().is_some() {
        return Err("cash genesis must precede retained paper orders".into());
    }
    let started = super::wallet_read::now()?;
    let first_account = super::alpaca_reconcile::account(
        &read("/v2/account")?.ok_or("paper account observation is missing")?,
        expected,
    )?;
    let first_positions = super::alpaca_positions::read(
        &read("/v2/positions")?.ok_or("paper position observation is missing")?,
    )?;
    let (mut observation, reconciliation) = if let Some(prior) = &retained {
        let baseline = retained_snapshot(prior, expected, super::wallet_read::now()?)?;
        let report = collect_checked(log, expected, &baseline, max_pages, opening, &mut read)?;
        (report["activity_observations"].clone(), Some(report))
    } else {
        let observation = super::alpaca_activities::collect(expected, max_pages, |path| {
            read(path)?.ok_or_else(|| "paper cash activity observation is missing".into())
        })?;
        (observation, None)
    };
    let positions = super::alpaca_positions::read(
        &read("/v2/positions")?.ok_or("paper position observation is missing")?,
    )?;
    let last_account = super::alpaca_reconcile::account(
        &read("/v2/account")?.ok_or("paper account observation is missing")?,
        expected,
    )?;
    if first_positions != positions
        || first_account != last_account
        || serde_json::to_value(&last_account).map_err(|_| "invalid selected account")?
            != observation["account_before"]
    {
        return Err("selected account or positions changed during paper checkpoint".into());
    }
    let current = snapshot(&observation, expected)?;
    if let (Some(prior), Some(holdings)) = (&retained, &prior_positions) {
        let previous = retained_snapshot(prior, expected, super::wallet_read::now()?)?;
        super::alpaca_positions::reconcile(
            holdings,
            &positions,
            &previous.activities,
            &current.activities,
        )?;
    }
    // The retained watermark covers ALL selected reads, including the position
    // brackets. It must not claim availability at the inner activity-read time.
    observation["read_started_unix_secs"] = json!(started);
    observation["read_completed_unix_secs"] = json!(super::wallet_read::now()?);
    observation["position_quantities"] =
        serde_json::to_value(positions).map_err(|_| "invalid selected positions")?;
    if let Some(acknowledgement) = reconciliation
        .as_ref()
        .and_then(|report| report["cash"].get("opening_cash_acknowledgement"))
    {
        observation["opening_cash_acknowledgement"] = acknowledgement.clone();
    }
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
        "position_delta_consistent":prior_positions.is_some(),
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

pub(super) fn opening_run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let [
        "alpaca-opening-cash-acknowledge",
        "--journal",
        path,
        "--account-id",
        expected,
        "--activity-id",
        activity_id,
        "--max-pages",
        pages,
        "--already-in-opening-balance",
    ] = values.as_slice()
    else {
        return Err("usage: radar alpaca-opening-cash-acknowledge --journal <existing-file> --account-id <paper-id> --activity-id <selected-id> --max-pages <positive-u16> --already-in-opening-balance".into());
    };
    let max_pages = super::alpaca_activities::page_limit(pages)?;
    let mut log = recovery::open_existing(path, expected)?;
    let opening = opening_plan(&log, expected, activity_id)?;
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret =
        super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let report = checkpoint_checked(&mut log, expected, max_pages, Some(&opening), |path| {
        super::alpaca_read::get(&agent, &format!("{PAPER_API}{path}"), &key, &secret).map(Some)
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

    fn opening_event() -> Value {
        json!({"id":"opening-selected","activity_type":"JNLC","net_amount":"50",
            "date":"1970-01-01","status":"executed"})
    }

    #[test]
    fn opening_late_seed_requires_ack_preserves_prefix_and_allows_normal_followup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("opening.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(observation_reader("50", &json!([]), p)))
        })
        .unwrap();
        let original = std::fs::read(&path).unwrap();
        let initial = log.opening_cash().unwrap().unwrap();
        let event = json!([opening_event()]);
        assert!(
            checkpoint(&mut log, "paper-id", 1, |p| {
                Ok(Some(observation_reader("50", &event, p)))
            })
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let opening = opening_plan(&log, "paper-id", "opening-selected").unwrap();
        let result = checkpoint_checked(&mut log, "paper-id", 1, Some(&opening), |p| {
            Ok(Some(observation_reader("50", &event, p)))
        })
        .unwrap();
        assert_eq!(
            result["reconciliation"]["cash"]["external_cash_flow_nanos"],
            "0"
        );
        assert_eq!(
            result["checkpoint"]["observation"]["account_before"]["cash"],
            "50"
        );
        let normalized: Vec<Activity> = serde_json::from_value(event.clone()).unwrap();
        assert_eq!(
            result["checkpoint"]["observation"]["activities"],
            serde_json::to_value(normalized).unwrap()
        );
        assert_eq!(
            result["checkpoint"]["observation"]["opening_cash_acknowledgement"]["activity_id"],
            "opening-selected"
        );
        assert_eq!(
            result["checkpoint"]["observation"]["opening_cash_acknowledgement"]["broker_posting_time_verified"],
            false
        );
        assert!(std::fs::read(&path).unwrap().starts_with(&original));
        for flag in [
            "execution_authority",
            "retry_authorized",
            "claim_release_authorized",
            "profit_measured",
        ] {
            assert_eq!(result[flag], false);
        }
        drop(log);
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        assert_eq!(log.opening_cash().unwrap().unwrap(), initial);
        assert!(opening_plan(&log, "paper-id", "opening-selected").is_err());
        checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(observation_reader("50", &event, p)))
        })
        .unwrap();
        assert_eq!(log.claims().unwrap(), BTreeMap::new());
        let unchanged = std::fs::read(&path).unwrap();
        let mut changed = event;
        changed[0]["net_amount"] = json!("51");
        assert!(
            checkpoint(&mut log, "paper-id", 1, |p| {
                Ok(Some(observation_reader("50", &changed, p)))
            })
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), unchanged);
    }

    fn opening_fixture(balance: &str, events: Value, started: u64, completed: u64) -> Snapshot {
        snapshot(&report(balance, events, started, completed), "paper-id").unwrap()
    }

    #[test]
    fn opening_ack_binds_exact_event_balances_date_status_and_nonoverlapping_reads() {
        let opening = Opening {
            activity_id: "opening-selected".into(),
            snapshot: snapshot(&report("50", json!([]), 172_800, 172_801), "paper-id").unwrap(),
        };
        let baseline = snapshot(&report("50", json!([]), 259_200, 259_201), "paper-id").unwrap();
        for date in ["1970-01-01", "1970-01-02"] {
            let mut event = opening_event();
            event["date"] = json!(date);
            let current =
                snapshot(&report("50", json!([event]), 259_201, 259_201), "paper-id").unwrap();
            let result = opening_delta(&baseline, &current, &opening).unwrap();
            assert_eq!(result["observed_cash_nanos"], "50000000000");
            assert_eq!(
                result["opening_cash_acknowledgement"]["opening_started_unix_secs"],
                172_800
            );
        }
        for (field, value) in [
            ("id", json!("other")),
            ("activity_type", json!("CSD")),
            ("status", json!("pending")),
            ("status", Value::Null),
            ("net_amount", Value::Null),
            ("net_amount", json!("0")),
            ("net_amount", json!("-50")),
            ("net_amount", json!("49.999999999")),
            ("date", Value::Null),
            ("date", json!("1970-01-03")),
            ("date", json!("1970-02-30")),
            ("date", json!("1970-1-01")),
            ("order_id", json!("foreign")),
            ("qty", json!("1")),
            ("side", json!("buy")),
            ("symbol", json!("SPY")),
            ("price", json!("50")),
            ("cum_qty", json!("1")),
            ("leaves_qty", json!("0")),
            ("type", json!("fill")),
        ] {
            let mut event = opening_event();
            event[field] = value;
            let current =
                snapshot(&report("50", json!([event]), 345_600, 345_601), "paper-id").unwrap();
            assert!(
                opening_delta(&baseline, &current, &opening).is_err(),
                "{field}"
            );
        }
        for events in [json!([]), json!([opening_event(), opening_event()])] {
            let value = report("50", events, 345_600, 345_601);
            assert!(
                snapshot(&value, "paper-id").is_err()
                    || opening_delta(&baseline, &snapshot(&value, "paper-id").unwrap(), &opening)
                        .is_err()
            );
        }
        for balance in ["49.999999999", "50.000000001"] {
            let current = opening_fixture(balance, json!([opening_event()]), 345_600, 345_601);
            assert!(opening_delta(&baseline, &current, &opening).is_err());
            let prior =
                snapshot(&report(balance, json!([]), 259_200, 259_201), "paper-id").unwrap();
            let current = opening_fixture("50", json!([opening_event()]), 345_600, 345_601);
            assert!(opening_delta(&prior, &current, &opening).is_err());
        }
        let overlapping = opening_fixture("50", json!([opening_event()]), 259_200, 259_201);
        assert!(opening_delta(&baseline, &overlapping, &opening).is_err());
        let prior = snapshot(
            &report(
                "50",
                json!([{"id":"prior","activity_type":"CSD","net_amount":"50"}]),
                259_200,
                259_201,
            ),
            "paper-id",
        )
        .unwrap();
        let current = opening_fixture("50", json!([opening_event()]), 345_600, 345_601);
        assert!(opening_delta(&prior, &current, &opening).is_err());
        let mut zero_event = opening_event();
        zero_event["net_amount"] = json!("0");
        let zero_opening = Opening {
            activity_id: "opening-selected".into(),
            snapshot: snapshot(&report("0", json!([]), 172_800, 172_801), "paper-id").unwrap(),
        };
        let zero_prior = snapshot(&report("0", json!([]), 259_200, 259_201), "paper-id").unwrap();
        let zero_current = opening_fixture("0", json!([zero_event]), 345_600, 345_601);
        assert!(opening_delta(&zero_prior, &zero_current, &zero_opening).is_err());
    }

    #[test]
    fn opening_plan_requires_retained_empty_unchanged_genesis_and_no_orders() {
        for case in 0..8 {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("opening.jsonl");
            let mut log = BrokerLog::open(&path, "paper-id").unwrap();
            if case != 0 {
                for at in [1, 2] {
                    let events = if (case == 2 && at == 1) || (case == 3 && at == 2) {
                        json!([{"id":"prior", "activity_type":"CSD", "net_amount":"50"}])
                    } else {
                        json!([])
                    };
                    let balance = if case == 1 && at == 2 { "51" } else { "50" };
                    let mut observation = report(balance, events, at, at);
                    observation["position_quantities"] =
                        if (case == 4 && at == 1) || (case == 5 && at == 2) {
                            json!({"SPY":{"asset_id":"asset", "nanoshares":1}})
                        } else {
                            json!({})
                        };
                    if case == 6 {
                        observation["position_quantities"] = Value::Null;
                    }
                    log.observe_cash(
                        BrokerCash {
                            account_id: "paper-id".into(),
                            started: at,
                            completed: at,
                            observation,
                            orders: Some(BTreeMap::new()),
                        },
                        at,
                    )
                    .unwrap();
                }
            }
            if case == 7 {
                log.reserve(intent(BrokerSide::Buy), 3).unwrap();
            }
            assert!(
                opening_plan(&log, "paper-id", "opening-selected").is_err(),
                "{case}"
            );
        }
        let directory = tempfile::tempdir().unwrap();
        let mut log = BrokerLog::open(directory.path().join("valid.jsonl"), "paper-id").unwrap();
        checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(observation_reader("50", &json!([]), p)))
        })
        .unwrap();
        assert!(opening_plan(&log, "paper-id", &"x".repeat(256)).is_ok());
        for id in [
            String::new(),
            "x".repeat(257),
            "line\nbreak".into(),
            "é".into(),
        ] {
            assert!(opening_plan(&log, "paper-id", &id).is_err());
        }
    }

    #[test]
    fn opening_actual_collector_rejects_current_cash_and_position_races_without_append() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("opening.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(observation_reader("50", &json!([]), p)))
        })
        .unwrap();
        let opening = opening_plan(&log, "paper-id", "opening-selected").unwrap();
        let before = std::fs::read(&path).unwrap();
        for changed in ["cash", "positions"] {
            let mut reads = 0;
            let result = checkpoint_checked(&mut log, "paper-id", 1, Some(&opening), |p| {
                if p == "/v2/account" {
                    reads += 1;
                }
                if changed == "cash" && p == "/v2/account" && reads == 4 {
                    return Ok(Some(account("51").to_string()));
                }
                if changed == "positions" && p == "/v2/positions" {
                    return Ok(Some(
                        json!([{"asset_id":"asset", "symbol":"SPY", "asset_class":"us_equity",
                        "qty":"1", "side":"long", "cost_basis":"50"}])
                        .to_string(),
                    ));
                }
                Ok(Some(observation_reader("50", &json!([opening_event()]), p)))
            });
            assert!(result.is_err(), "{changed}");
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }

    fn account(cash: &str) -> Value {
        json!({"id":"paper-id","status":"ACTIVE","currency":"USD","cash":cash,"private":"omit"})
    }

    fn observation_reader(balance: &str, activities: &Value, path: &str) -> String {
        if path == "/v2/account" {
            account(balance).to_string()
        } else if path == "/v2/positions" {
            json!([]).to_string()
        } else {
            activities.to_string()
        }
    }

    fn position(symbol: &str, qty: &str) -> Value {
        json!({"asset_id":format!("asset_{symbol}"),"symbol":symbol,"asset_class":"us_equity",
            "side":"long","qty":qty,"cost_basis":"0","market_value":"0"})
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
                    if p == "/v2/positions" && side == BrokerSide::Sell {
                        return Ok(Some(json!([position("QQQ", "2")]).to_string()));
                    }
                    Ok(Some(observation_reader("500", &json!([prior()]), p)))
                })
                .unwrap();
                let intent = intent(side);
                let id = intent.client_order_id.clone();
                let now = super::super::wallet_read::now().unwrap();
                log.reserve(intent.clone(), now).unwrap();
                log.submit(&id, now, |_| ()).unwrap();
                let completed = status == "filled";
                let quantity = if completed { "1" } else { "0.5" };
                let mut fill = fill(side);
                fill["qty"] = json!(quantity);
                fill["cum_qty"] = json!(quantity);
                fill["leaves_qty"] = json!(if completed { "0" } else { "0.5" });
                fill["price"] = json!("100");
                let mut order = order(side);
                order["status"] = json!(status);
                order["filled_qty"] = json!(quantity);
                let balance = match (side, completed) {
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
                    } else if p == "/v2/positions" {
                        json!([position(
                            &intent.symbol,
                            match (side, completed) {
                                (_, true) => "1",
                                (BrokerSide::Buy, false) => "0.5",
                                (BrokerSide::Sell, false) => "1.5",
                            }
                        )])
                        .to_string()
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
                assert_eq!(result["position_delta_consistent"], true);
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
        for stage in 0..7 {
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
            if p == "/v2/positions" {
                return Ok(Some(json!([position("QQQ", "2")]).to_string()));
            }
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
            } else if p == "/v2/positions" {
                json!([position("SPY", "1"), position("QQQ", "1")]).to_string()
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

    #[test]
    fn position_races_and_unexplained_changes_preserve_checkpoint_across_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("positions.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        let initial = checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(if p == "/v2/positions" {
                json!([position("SPY", "1")]).to_string()
            } else {
                observation_reader("100", &json!([]), p)
            }))
        })
        .unwrap();
        assert_eq!(initial["position_delta_consistent"], false);
        let original = std::fs::read(&path).unwrap();
        drop(log);
        for case in 0..6 {
            let mut log = BrokerLog::open(&path, "paper-id").unwrap();
            let mut position_reads = 0;
            let mut account_reads = 0;
            assert!(
                checkpoint(&mut log, "paper-id", 1, |p| {
                    Ok(Some(if p == "/v2/positions" {
                        position_reads += 1;
                        let mut held = position("SPY", "1");
                        match case {
                            0 => held["qty"] = json!("2"), // Stable but unexplained.
                            1 if position_reads == 2 => held["qty"] = json!("2"),
                            2 => held["asset_id"] = json!("changed"),
                            3 => return Ok(Some("[]".into())),
                            _ => {}
                        }
                        json!([held]).to_string()
                    } else if p == "/v2/account" {
                        account_reads += 1;
                        account(
                            if (case == 4 && account_reads == 1)
                                || (case == 5 && account_reads == 4)
                            {
                                "101"
                            } else {
                                "100"
                            },
                        )
                        .to_string()
                    } else {
                        json!([]).to_string()
                    }))
                })
                .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert_eq!(
                log.cash().unwrap().unwrap().observation["position_quantities"]["SPY"]["nanoshares"],
                1_000_000_000_u64
            );
        }
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        let result = checkpoint(&mut log, "paper-id", 1, |p| {
            Ok(Some(if p == "/v2/positions" {
                json!([position("SPY", "1")]).to_string()
            } else {
                observation_reader("100", &json!([]), p)
            }))
        })
        .unwrap();
        assert_eq!(result["position_delta_consistent"], true);
        assert_eq!(result["execution_authority"], false);
        drop(log);
        let mut log = BrokerLog::open(dir.path().join("legacy.jsonl"), "paper-id").unwrap();
        log.observe_cash(
            BrokerCash {
                account_id: "paper-id".into(),
                started: 1,
                completed: 2,
                observation: report("100", json!([]), 1, 2),
                orders: Some(BTreeMap::new()),
            },
            2,
        )
        .unwrap();
        assert!(
            checkpoint(&mut log, "paper-id", 1, |_| panic!(
                "legacy balance cannot be silently replaced"
            ))
            .is_err()
        );
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
