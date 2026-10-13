// SPDX-License-Identifier: Apache-2.0
//! Private paper risk caller. Reads provider facts directly, never a model evidence file.

use std::io::Read as _;

use radar_journal::{BrokerIntent, BrokerLog, BrokerSide};
use radar_risk::equity::{self, Order, Policy, State};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

const PAPER_API: &str = "https://paper-api.alpaca.markets";
const SNAPSHOTS: &str = "https://data.alpaca.markets/v2/stocks/snapshots";

pub(super) fn load<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "private paper input unavailable")?
        .take(65_537)
        .read_to_end(&mut bytes)
        .map_err(|_| "private paper input read failed")?;
    if bytes.len() > 65_536 {
        return Err("private paper input exceeds 64 KiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid typed private paper input".into())
}

pub(super) fn order(intent: &BrokerIntent) -> Order {
    Order {
        account_id: intent.account_id.clone(),
        client_order_id: intent.client_order_id.clone(),
        symbol: intent.symbol.clone(),
        side: match intent.side {
            BrokerSide::Buy => equity::Side::Buy,
            BrokerSide::Sell => equity::Side::Sell,
        },
        quantity_nanoshares: intent.quantity_nanoshares,
        limit_microusd_per_share: intent.limit_microusd_per_share,
        fee_bound_microusd: intent.fee_bound_microusd,
    }
}

fn number(value: &Value) -> Result<u64, String> {
    value
        .as_str()
        .ok_or("paper bound is unknown")?
        .parse()
        .map_err(|_| "invalid paper bound".into())
}

fn sum(total: &mut u64, value: u64) -> Result<(), String> {
    *total = total
        .checked_add(value)
        .ok_or("paper risk bound overflow")?;
    Ok(())
}

fn account_bounds(
    log: &BrokerLog,
    intent: &BrokerIntent,
    report: &Value,
) -> Result<(u64, u64, u64, u64), String> {
    if report["selected_account_fields_unchanged"] != true
        || report["selected_open_order_fields_unchanged"] != true
    {
        return Err("paper risk account or open orders changed".into());
    }
    let checkpoint = log
        .cash()
        .map_err(|e| e.to_string())?
        .ok_or("paper risk checkpoint missing")?;
    if report["account_before"] != checkpoint.observation["account_before"] {
        return Err("paper risk account differs from retained checkpoint".into());
    }
    let positions = super::alpaca_positions::read(&report["positions"].to_string())?;
    if positions != super::alpaca_positions::retained(&checkpoint.observation)? {
        return Err("paper risk shares differ from retained checkpoint".into());
    }
    let account = super::alpaca_reconcile::account(
        &report["account_before"].to_string(),
        &intent.account_id,
    )?;
    let raw_positions: Vec<super::alpaca_read::Position> =
        serde_json::from_value(report["positions"].clone())
            .map_err(|_| "invalid risk positions")?;
    // No broker order deduction here: every retained claim is counted once below.
    let bounds = super::alpaca_exposure::summarize(&account, &[], &raw_positions)?;
    let claims = log.claims().map_err(|e| e.to_string())?;
    let entries: std::collections::BTreeMap<_, _> = log
        .entries()
        .map_err(|e| e.to_string())?
        .map(|entry| (entry.intent.client_order_id.clone(), entry))
        .collect();
    open_orders(log, report)?;
    let mut cash_claim = 0;
    let mut share_claim = 0;
    let mut symbol_claim = 0;
    for (id, claim) in &claims {
        sum(&mut cash_claim, claim.cash_microusd)?;
        if entries.get(id).ok_or("unbound paper claim")?.intent.symbol == intent.symbol {
            sum(&mut share_claim, claim.shares_nanoshares)?;
            sum(&mut symbol_claim, claim.cash_microusd)?;
        }
    }
    let funds = |value: Option<&str>| {
        super::alpaca_exposure::nanos(value.ok_or("paper funds unknown")?)
            .map(|value| value / 1_000)
    };
    let cash = funds(account.cash.as_deref())?
        .checked_sub(cash_claim)
        .ok_or("paper claims exceed cash")?
        .min(funds(account.buying_power.as_deref())?)
        .min(funds(account.equity.as_deref())?);
    let shares = positions
        .get(&intent.symbol)
        .map_or(0, |held| held.nanoshares)
        .checked_sub(share_claim)
        .ok_or("paper sell claims exceed owned shares")?;
    let mut committed = number(&bounds["position_exposure_bound_microusd"])?;
    sum(&mut committed, cash_claim)?;
    let selected: Vec<_> = raw_positions
        .into_iter()
        .filter(|position: &super::alpaca_read::Position| position.symbol == intent.symbol)
        .collect();
    let symbol_bounds = super::alpaca_exposure::summarize(&account, &[], &selected)?;
    let mut symbol_committed = number(&symbol_bounds["position_exposure_bound_microusd"])?;
    sum(&mut symbol_committed, symbol_claim)?;
    Ok((cash, shares, committed, symbol_committed))
}

fn open_orders(log: &BrokerLog, report: &Value) -> Result<(), String> {
    let claims = log.claims().map_err(|e| e.to_string())?;
    let entries: std::collections::BTreeMap<_, _> = log
        .entries()
        .map_err(|e| e.to_string())?
        .map(|entry| (entry.intent.client_order_id.clone(), entry))
        .collect();
    let open: Vec<super::alpaca_reconcile::BrokerOrder> =
        serde_json::from_value(report["open_orders_before"].clone())
            .map_err(|_| "invalid risk open orders")?;
    for pending in &open {
        if !matches!(
            pending.status.as_str(),
            "new" | "accepted" | "pending_new" | "partially_filled" | "pending_cancel"
        ) {
            return Err("unsupported paper open order status".into());
        }
        let id = pending
            .client_order_id
            .as_ref()
            .ok_or("unidentified paper open order")?;
        let retained = entries
            .get(id)
            .ok_or("paper open order has no retained intent")?;
        super::alpaca_order_read::payload_matches(&retained.intent, pending)?;
        let progress = claims
            .get(id)
            .and_then(|claim| claim.progress.as_ref())
            .ok_or("paper open order lacks reconciled progress")?;
        if progress.terminal
            || progress.broker_order_id != pending.id
            || Some(progress.filled_nanoshares)
                != pending
                    .filled_qty
                    .as_deref()
                    .map(super::alpaca_exposure::nanos)
                    .transpose()?
        {
            return Err("paper open order progressed after checkpoint".into());
        }
    }
    Ok(())
}

fn eligible(report: &Value, account: &Value, intent: &BrokerIntent) -> Result<(), String> {
    if report["account_before"] != *account || report["account_after"] != *account {
        return Err("asset observation account differs from risk checkpoint".into());
    }
    let asset = &report["assets"][0];
    if asset["status"] != "active"
        || asset["tradable"] != true
        || !matches!(
            asset["exchange"].as_str(),
            Some("AMEX" | "ARCA" | "BATS" | "NYSE" | "NASDAQ" | "NYSEARCA")
        )
        || asset["attributes"]
            .as_array()
            .is_none_or(|attributes| !attributes.is_empty())
        || (!intent.quantity_nanoshares.is_multiple_of(1_000_000_000)
            && asset["fractionable"] != true)
    {
        return Err("paper equity eligibility is unknown or unsupported".into());
    }
    Ok(())
}

pub(super) fn now_ms() -> Result<u64, String> {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "host clock before epoch")?
            .as_millis(),
    )
    .map_err(|_| "host timestamp overflow".into())
}

pub(super) fn collect_state(
    log: &mut BrokerLog,
    intent: &BrokerIntent,
    policy: &Policy,
    pages: u16,
    mut read: impl FnMut(&str) -> Result<Option<String>, String>,
    mut clock: impl FnMut() -> Result<u64, String>,
) -> Result<State, String> {
    if !policy.enabled || policy.account_id != intent.account_id {
        return Err("paper handover is closed or account differs".into());
    }
    super::alpaca_session::limit(intent.limit_microusd_per_share)?;
    super::alpaca_market::symbols(&intent.symbol)?;
    super::alpaca_order_read::query(&intent.client_order_id)?;
    if log
        .entries()
        .map_err(|e| e.to_string())?
        .any(|entry| entry.intent.client_order_id == intent.client_order_id)
    {
        return Err(
            "paper preflight identity already retained; reconcile rather than retry".into(),
        );
    }
    let started = clock()?;
    super::alpaca_cash::checkpoint(log, &intent.account_id, pages, |path| {
        read(&format!("{PAPER_API}{path}"))
    })?;
    let reconcile = super::alpaca_reconcile::collect(&intent.account_id, |path| {
        read(&format!("{PAPER_API}{path}"))?.ok_or_else(|| "paper risk response missing".into())
    })?;
    let (cash, shares, committed, symbol_committed) = account_bounds(log, intent, &reconcile)?;
    let assets = super::alpaca_assets::collect(&intent.account_id, &intent.symbol, |path| {
        read(&format!("{PAPER_API}{path}"))?.ok_or_else(|| "paper asset response missing".into())
    })?;
    eligible(&assets, &reconcile["account_before"], intent)?;
    if reconcile["positions"]
        .as_array()
        .ok_or("paper positions unknown")?
        .iter()
        .any(|position| {
            position["symbol"] == intent.symbol && position["asset_id"] != assets["assets"][0]["id"]
        })
    {
        return Err("paper asset identity differs from retained holding".into());
    }
    let raw = read(&format!("{SNAPSHOTS}?symbols={}&feed=iex", intent.symbol))?
        .ok_or("paper quote response missing")?;
    let quote_available = clock()?.checked_add(1).ok_or("paper timestamp overflow")?;
    let quote = super::alpaca_quote::validate(
        &raw,
        &intent.symbol,
        quote_available,
        quote_available,
        policy.max_input_age_ms,
        policy.max_spread_bps,
    )?;
    let final_account = super::alpaca_reconcile::account(
        &read(&format!("{PAPER_API}/v2/account"))?.ok_or("final paper account missing")?,
        &intent.account_id,
    )?;
    if serde_json::to_value(final_account).map_err(|_| "invalid selected final account")?
        != reconcile["account_before"]
    {
        return Err("paper account changed after risk collection".into());
    }
    let available = clock()?.checked_add(1).ok_or("paper timestamp overflow")?;
    let now = clock()?;
    let (session_observed, session_close) =
        super::alpaca_session::validate(&reconcile["market_clock"], now, policy.max_input_age_ms)?;
    let exchange_ms = quote_available
        .checked_sub(number(&quote["age_ns"])?.div_ceil(1_000_000))
        .ok_or("paper quote time underflow")?;
    Ok(State {
        account_id: intent.account_id.clone(),
        now_unix_ms: now,
        oldest_input_unix_ms: started.min(exchange_ms).min(session_observed),
        available_at_unix_ms: available,
        halted: false,
        market_open: reconcile["market_clock"]["is_open"] == true,
        session_close_unix_ms: Some(session_close),
        available_cash_microusd: Some(cash),
        available_nanoshares: Some(shares),
        committed_microusd: Some(committed),
        symbol_committed_microusd: Some(symbol_committed),
        daily_loss_microusd: None,
        bid_microusd_per_share: number(&quote["bid_microusd_per_share"])?,
        ask_microusd_per_share: number(&quote["ask_microusd_per_share"])?,
        // Feed size units and fee/cost adequacy have not yet been
        // verified against this account. Never mint capacity or zero costs.
        capacity_nanoshares: None,
        required_fee_microusd: None,
        round_trip_cost_microusd: None,
    })
}

fn report(intent: &BrokerIntent, state: &State, policy: &Policy) -> Value {
    let proposed = order(intent);
    let refusal = equity::evaluate(&proposed, state, policy).err();
    json!({"version":1,"venue":"alpaca","environment":"paper","intent":intent,
        "risk_refusal":refusal,"provider_state_selected":true,"execution_authority":false,
        "available_cash_microusd":state.available_cash_microusd.map(|v| v.to_string()),
        "available_nanoshares":state.available_nanoshares.map(|v| v.to_string()),
        "committed_microusd":state.committed_microusd.map(|v| v.to_string()),
        "symbol_committed_microusd":state.symbol_committed_microusd.map(|v| v.to_string()),
        "capacity_nanoshares":state.capacity_nanoshares.map(|v| v.to_string()),
        "required_fee_microusd":state.required_fee_microusd.map(|v| v.to_string()),
        "round_trip_cost_microusd":state.round_trip_cost_microusd.map(|v| v.to_string()),
        "oldest_input_unix_ms":state.oldest_input_unix_ms,
        "session_close_unix_ms":state.session_close_unix_ms,
        "available_at_unix_ms":state.available_at_unix_ms,"as_of_unix_ms":state.now_unix_ms})
}

#[cfg(test)]
fn collect(
    log: &mut BrokerLog,
    intent: &BrokerIntent,
    policy: &Policy,
    pages: u16,
    read: impl FnMut(&str) -> Result<Option<String>, String>,
    clock: impl FnMut() -> Result<u64, String>,
) -> Result<Value, String> {
    let state = collect_state(log, intent, policy, pages, read, clock)?;
    Ok(report(intent, &state, policy))
}

struct ExecutionInputs<'a> {
    policy_path: &'a str,
    model_path: Option<&'a str>,
    model: Option<super::alpaca_paper_model::Model>,
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<_> = args.iter().map(String::as_str).collect();
    let (values, model_path) = if let [core @ .., "--paper-model", path] = values.as_slice() {
        (core, Some(*path))
    } else {
        (values.as_slice(), None)
    };
    let [
        command @ ("alpaca-order-preflight" | "alpaca-paper-submit"),
        "--journal",
        path,
        "--intent",
        input,
        "--policy",
        policy_path,
        "--max-pages",
        pages,
    ] = values
    else {
        return Err("usage: radar <alpaca-order-preflight|alpaca-paper-submit> --journal <private-file> --intent <typed-intent> --policy <private-policy> --max-pages <positive-u16> [--paper-model <private-simulation-model>]".into());
    };
    let intent: BrokerIntent = load(input)?;
    let policy: Policy = load(policy_path)?;
    let model: Option<super::alpaca_paper_model::Model> = model_path.map(load).transpose()?;
    if let Some(model) = &model {
        model.validate(&intent.account_id)?;
    }
    let pages = super::alpaca_activities::page_limit(pages)?;
    let mut log = super::alpaca_order_read::open_existing(path, &intent.account_id)?;
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret =
        super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let state = collect_state(
        &mut log,
        &intent,
        &policy,
        pages,
        |url| {
            if url.starts_with("https://paper-api.alpaca.markets/v2/orders:") {
                super::alpaca_order_read::read_path(
                    &agent,
                    PAPER_API,
                    url.strip_prefix(PAPER_API)
                        .ok_or("invalid paper read host")?,
                    &key,
                    &secret,
                )
            } else {
                super::alpaca_read::get(&agent, url, &key, &secret).map(Some)
            }
        },
        now_ms,
    )?;
    let output = dispatch_configured(
        command,
        &mut log,
        &intent,
        state,
        &ExecutionInputs {
            policy_path,
            model_path,
            model,
        },
        now_ms,
        |body| super::alpaca_submit::post(&agent, &key, &secret, body),
    )?;
    println!("{output}");
    Ok(())
}

fn dispatch_configured(
    command: &str,
    log: &mut BrokerLog,
    intent: &BrokerIntent,
    mut state: State,
    inputs: &ExecutionInputs<'_>,
    clock: impl FnMut() -> Result<u64, String>,
    send: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Value, String> {
    let scenario = inputs
        .model
        .as_ref()
        .map(|model| model.apply(intent, &mut state))
        .transpose()?;
    let mut output = dispatch(
        command,
        log,
        intent,
        &state,
        || {
            super::alpaca_paper_model::unchanged(inputs.model.as_ref(), inputs.model_path)?;
            load(inputs.policy_path)
        },
        clock,
        send,
    )?;
    output["paper_execution_model"] = scenario.unwrap_or(Value::Null);
    Ok(output)
}

fn dispatch(
    command: &str,
    log: &mut BrokerLog,
    intent: &BrokerIntent,
    state: &State,
    mut policy: impl FnMut() -> Result<Policy, String>,
    clock: impl FnMut() -> Result<u64, String>,
    send: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Value, String> {
    if command == "alpaca-paper-submit" {
        super::alpaca_submit::execute(log, intent, state, policy, clock, send)
    } else if command == "alpaca-order-preflight" {
        Ok(report(intent, state, &policy()?))
    } else {
        Err("unsupported paper command".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(side: BrokerSide) -> BrokerIntent {
        BrokerIntent {
            account_id: "paper".into(),
            client_order_id: format!("radar_{side:?}"),
            symbol: "SPY".into(),
            side,
            quantity_nanoshares: 1_000_000_000,
            limit_microusd_per_share: 100_000_000,
            fee_bound_microusd: 7,
        }
    }

    fn policy() -> Policy {
        Policy {
            enabled: true,
            account_id: "paper".into(),
            symbols: ["SPY".into()].into(),
            max_capital_microusd: 500_000_000,
            max_order_microusd: 100_000_000,
            max_position_microusd: 500_000_000,
            max_daily_loss_microusd: None,
            max_input_age_ms: 10_000,
            authorization_lifetime_ms: 1_000,
            max_spread_bps: 100,
            max_limit_distance_bps: 100,
            max_round_trip_cost_bps: 100,
        }
    }

    fn fixture(url: &str, seconds: u64) -> Value {
        if url.ends_with("/v2/account") {
            json!({"id":"paper","status":"ACTIVE","currency":"USD","cash":"500","buying_power":"400","equity":"710",
                "trading_blocked":false,"account_blocked":false,"trade_suspended_by_user":false})
        } else if url.ends_with("/v2/positions") {
            json!([{"asset_id":"asset","symbol":"SPY","asset_class":"us_equity","qty":"2","side":"long","market_value":"210","cost_basis":"200"}])
        } else if url.contains("/v2/orders?") || url.contains("/v2/account/activities?") {
            json!([])
        } else if url.ends_with("/v2/clock") {
            json!({"timestamp":radar_types::civil::timestamp_from_seconds(seconds),"is_open":true,
                "next_open":radar_types::civil::timestamp_from_seconds(seconds + 86400),
                "next_close":radar_types::civil::timestamp_from_seconds(seconds + 3600)})
        } else if url.ends_with("/v2/assets/SPY") {
            json!({"id":"asset","symbol":"SPY","class":"us_equity","status":"active","tradable":true,"fractionable":true,"attributes":[],"exchange":"ARCA"})
        } else if url.starts_with(SNAPSHOTS) {
            json!({"SPY":{"latestQuote":{"t":radar_types::civil::timestamp_from_seconds(seconds),"bp":100,"ap":100,"bs":10,"as":10,"bx":"V","ax":"V","c":["R"],"z":"B"}}})
        } else {
            panic!("unexpected fixture URL {url}")
        }
    }

    fn paper_scenario(capacity: u64, fee: u64) -> super::super::alpaca_paper_model::Model {
        serde_json::from_value(
            json!({"version":"alpaca-paper-stress-v1","account_id":"paper",
            "max_quantity_nanoshares":capacity,"fee_per_side_microusd":fee,
            "slippage_bps_per_side":10}),
        )
        .unwrap()
    }

    fn paper_state(log: &mut BrokerLog, intent: &BrokerIntent) -> (State, u64) {
        let seconds = super::super::wallet_read::now().unwrap();
        let mut tick = seconds * 1_000;
        let state = collect_state(
            log,
            intent,
            &policy(),
            1,
            |url| Ok(Some(fixture(url, seconds).to_string())),
            || {
                tick += 10;
                Ok(tick)
            },
        )
        .unwrap();
        (state, tick)
    }

    #[test]
    fn paper_model_preflight_reports_assumptions_and_keeps_policy_capacity_fee_and_cash_gates() {
        for side in [BrokerSide::Buy, BrokerSide::Sell] {
            for case in ["valid", "capacity", "fee", "disabled", "cash"] {
                let directory = tempfile::tempdir().unwrap();
                let mut log =
                    BrokerLog::open(directory.path().join("orders.jsonl"), "paper").unwrap();
                let proposal = intent(side);
                let (mut state, _) = paper_state(&mut log, &proposal);
                if case == "cash" {
                    state.available_cash_microusd = Some(0);
                }
                let model = paper_scenario(
                    if case == "capacity" { 1 } else { 2_000_000_000 },
                    if case == "fee" { 8 } else { 7 },
                );
                let model_path = directory.path().join("model.json");
                let policy_path = directory.path().join("policy.json");
                let mut selected = policy();
                if case == "disabled" {
                    selected.enabled = false;
                }
                std::fs::write(&model_path, serde_json::to_vec(&model).unwrap()).unwrap();
                std::fs::write(&policy_path, serde_json::to_vec(&selected).unwrap()).unwrap();
                let inputs = ExecutionInputs {
                    policy_path: policy_path.to_str().unwrap(),
                    model_path: Some(model_path.to_str().unwrap()),
                    model: Some(model),
                };
                let report = dispatch_configured(
                    "alpaca-order-preflight",
                    &mut log,
                    &proposal,
                    state,
                    &inputs,
                    || panic!("preflight has no send clock"),
                    |_| panic!("preflight cannot send"),
                )
                .unwrap();
                assert_eq!(report["risk_refusal"].is_null(), case == "valid", "{case}");
                assert_eq!(report["execution_authority"], false);
                assert_eq!(
                    report["paper_execution_model"]["basis"],
                    "operator_stress_scenario"
                );
                assert_eq!(
                    report["paper_execution_model"]["live_capacity_verified"],
                    false
                );
                assert_eq!(
                    report["paper_execution_model"]["live_costs_verified"],
                    false
                );
                assert_eq!(report["paper_execution_model"]["profit_measured"], false);
                if case == "valid" {
                    assert_eq!(report["capacity_nanoshares"], "2000000000");
                    assert_eq!(report["required_fee_microusd"], "7");
                    assert_eq!(report["round_trip_cost_microusd"], "200014");
                }
                assert!(log.entries().unwrap().next().is_none());
            }
        }
    }

    #[test]
    fn paper_model_submit_reloads_before_risk_and_after_durable_uncertainty() {
        for case in ["absent", "same", "before", "after"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("orders.jsonl");
            let mut log = BrokerLog::open(&path, "paper").unwrap();
            let proposal = intent(BrokerSide::Buy);
            let (state, tick) = paper_state(&mut log, &proposal);
            let model_path = directory.path().join("model.json");
            let policy_path = directory.path().join("policy.json");
            let model = paper_scenario(2_000_000_000, 7);
            std::fs::write(&model_path, serde_json::to_vec(&model).unwrap()).unwrap();
            std::fs::write(&policy_path, serde_json::to_vec(&policy()).unwrap()).unwrap();
            let inputs = ExecutionInputs {
                policy_path: policy_path.to_str().unwrap(),
                model_path: if case == "absent" {
                    None
                } else {
                    Some(model_path.to_str().unwrap())
                },
                model: if case == "absent" { None } else { Some(model) },
            };
            let changed = serde_json::to_vec(&paper_scenario(2_000_000_000, 8)).unwrap();
            if case == "before" {
                std::fs::write(&model_path, &changed).unwrap();
            }
            let sent = std::cell::Cell::new(false);
            let result = dispatch_configured(
                "alpaca-paper-submit",
                &mut log,
                &proposal,
                state,
                &inputs,
                || {
                    if case == "after" {
                        std::fs::write(&model_path, &changed).unwrap();
                    }
                    Ok(tick)
                },
                |body| {
                    assert!(
                        std::fs::read_to_string(&path)
                            .unwrap()
                            .contains("submission_unknown")
                    );
                    sent.set(true);
                    let mut response: Value = serde_json::from_str(body).unwrap();
                    response["id"] = json!("broker");
                    response["asset_class"] = json!("us_equity");
                    response["status"] = json!("new");
                    Ok(response.to_string())
                },
            );
            assert_eq!(result.is_ok(), case == "same", "{case}");
            assert_eq!(sent.get(), case == "same", "{case}");
            let entries: Vec<_> = log.entries().unwrap().collect();
            if case == "same" || case == "after" {
                assert_eq!(entries.len(), 1);
                assert_eq!(
                    entries[0].state,
                    radar_journal::BrokerState::SubmissionUnknown
                );
            } else {
                assert_eq!(entries.len(), 0);
            }
        }
    }

    #[test]
    fn paper_model_cannot_override_evidence_or_partially_update_invalid_state() {
        let directory = tempfile::tempdir().unwrap();
        let mut log = BrokerLog::open(directory.path().join("orders.jsonl"), "paper").unwrap();
        let proposal = intent(BrokerSide::Buy);
        let (state, _) = paper_state(&mut log, &proposal);
        for case in ["capacity", "fee", "cost", "account", "quote", "zero"] {
            let mut current = state.clone();
            let mut order = proposal.clone();
            match case {
                "capacity" => current.capacity_nanoshares = Some(0),
                "fee" => current.required_fee_microusd = Some(0),
                "cost" => current.round_trip_cost_microusd = Some(0),
                "account" => current.account_id = "other".into(),
                "quote" => current.ask_microusd_per_share = current.bid_microusd_per_share - 1,
                _ => order.quantity_nanoshares = 0,
            }
            let original = (
                current.capacity_nanoshares,
                current.required_fee_microusd,
                current.round_trip_cost_microusd,
            );
            assert!(
                paper_scenario(2_000_000_000, 7)
                    .apply(&order, &mut current)
                    .is_err(),
                "{case}"
            );
            assert_eq!(
                (
                    current.capacity_nanoshares,
                    current.required_fee_microusd,
                    current.round_trip_cost_microusd
                ),
                original
            );
        }
    }

    #[test]
    fn actual_preflight_reads_fixed_hosts_and_preserves_unknown_capacity_and_costs() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = BrokerLog::open(dir.path().join("paper.jsonl"), "paper").unwrap();
        let seconds = super::super::wallet_read::now().unwrap();
        let observed_ms = now_ms().unwrap();
        assert!(observed_ms >= seconds * 1_000);
        assert!(observed_ms < (super::super::wallet_read::now().unwrap() + 1) * 1_000);
        let mut tick = seconds * 1_000;
        let mut urls = Vec::new();
        let report = collect(
            &mut log,
            &intent(BrokerSide::Buy),
            &policy(),
            1,
            |url| {
                urls.push(url.to_owned());
                Ok(Some(fixture(url, seconds).to_string()))
            },
            || {
                tick += 10;
                Ok(tick)
            },
        )
        .unwrap();
        assert_eq!(report["risk_refusal"], "liquidity");
        assert_eq!(report["available_cash_microusd"], "400000000");
        assert_eq!(report["available_nanoshares"], "2000000000");
        assert_eq!(report["committed_microusd"], "210000000");
        assert_eq!(report["symbol_committed_microusd"], "210000000");
        for field in [
            "capacity_nanoshares",
            "required_fee_microusd",
            "round_trip_cost_microusd",
        ] {
            assert!(report[field].is_null());
        }
        assert_eq!(report["execution_authority"], false);
        assert!(log.entries().unwrap().next().is_none());
        assert!(urls.iter().all(
            |url| url.starts_with("https://paper-api.alpaca.markets/v2/")
                || url == "https://data.alpaca.markets/v2/stocks/snapshots?symbols=SPY&feed=iex"
        ));
        assert_eq!(
            urls.iter()
                .filter(|url| url.ends_with("/v2/positions"))
                .count(),
            3
        );
        assert!(
            collect(
                &mut log,
                &intent(BrokerSide::Buy),
                &policy(),
                1,
                |url| {
                    let mut value = fixture(url, seconds);
                    if url.ends_with("/v2/assets/SPY") {
                        value["id"] = json!("changed");
                    }
                    Ok(Some(value.to_string()))
                },
                || {
                    tick += 10;
                    Ok(tick)
                }
            )
            .is_err()
        );
        assert!(log.entries().unwrap().next().is_none());
        let mut closed = policy();
        closed.enabled = false;
        assert!(
            collect(
                &mut log,
                &intent(BrokerSide::Buy),
                &closed,
                1,
                |_| panic!("closed handover must not read"),
                || Ok(tick)
            )
            .is_err()
        );
        log.reserve(intent(BrokerSide::Buy), seconds).unwrap();
        assert!(
            collect(
                &mut log,
                &intent(BrokerSide::Buy),
                &policy(),
                1,
                |_| panic!("retained ID is not a retry"),
                || Ok(tick)
            )
            .is_err()
        );
    }

    #[test]
    fn actual_collector_rejects_invalid_clock_and_limit_before_any_submission() {
        let seconds = 1_791_813_600;
        for (field, bad) in [
            ("timestamp", json!("2026-10-12T13:59:00Z")),
            ("timestamp", json!("2026-10-12T14:00:01Z")),
            ("next_close", json!("2026-10-12T14:00:00Z")),
            ("next_open", json!("2026-10-12T14:01:00Z")),
            ("is_open", json!(false)),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut log = BrokerLog::open(dir.path().join("paper.jsonl"), "paper").unwrap();
            let result = collect_state(
                &mut log,
                &intent(BrokerSide::Buy),
                &policy(),
                1,
                |url| {
                    let mut value = fixture(url, seconds);
                    if url.ends_with("/v2/clock") {
                        value[field] = bad.clone();
                    }
                    Ok(Some(value.to_string()))
                },
                || Ok(seconds * 1_000 + 10),
            );
            assert_eq!(
                result.unwrap_err(),
                "paper session is closed, stale or inconsistent"
            );
            assert!(log.entries().unwrap().next().is_none());
        }
        let dir = tempfile::tempdir().unwrap();
        let mut log = BrokerLog::open(dir.path().join("paper.jsonl"), "paper").unwrap();
        let mut invalid = intent(BrokerSide::Buy);
        invalid.limit_microusd_per_share += 1;
        assert!(
            collect_state(
                &mut log,
                &invalid,
                &policy(),
                1,
                |_| panic!("invalid increment must refuse before reads"),
                || Ok(0)
            )
            .is_err()
        );
        assert!(log.cash().unwrap().is_none());
    }

    #[test]
    fn actual_preflight_rejects_unsupported_quote_metadata_before_authorization() {
        for (field, bad) in [
            ("bx", json!("Q")),
            ("ax", json!("Q")),
            ("c", json!(["H"])),
            ("z", json!("D")),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut log = BrokerLog::open(dir.path().join("paper.jsonl"), "paper").unwrap();
            let seconds = 1_791_813_600;
            let result = collect_state(
                &mut log,
                &intent(BrokerSide::Buy),
                &policy(),
                1,
                |url| {
                    let mut value = fixture(url, seconds);
                    if url.starts_with(SNAPSHOTS) {
                        value["SPY"]["latestQuote"][field] = bad.clone();
                    }
                    Ok(Some(value.to_string()))
                },
                || Ok(seconds * 1_000),
            );
            assert_eq!(
                result.unwrap_err(),
                "unsupported IEX quote exchange, conditions or tape"
            );
            assert!(log.entries().unwrap().next().is_none());
        }
    }

    #[test]
    fn actual_unknown_provider_state_refuses_submit_before_reserving() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = BrokerLog::open(dir.path().join("paper.jsonl"), "paper").unwrap();
        let seconds = super::super::wallet_read::now().unwrap();
        let mut tick = seconds * 1_000;
        let state = collect_state(
            &mut log,
            &intent(BrokerSide::Buy),
            &policy(),
            1,
            |url| Ok(Some(fixture(url, seconds).to_string())),
            || {
                tick += 10;
                Ok(tick)
            },
        )
        .unwrap();
        assert!(
            dispatch(
                "alpaca-paper-submit",
                &mut log,
                &intent(BrokerSide::Buy),
                &state,
                || Ok(policy()),
                || Ok(tick),
                |_| panic!("actual unknown evidence must not submit"),
            )
            .is_err()
        );
        assert!(log.entries().unwrap().next().is_none());
        let report = dispatch(
            "alpaca-order-preflight",
            &mut log,
            &intent(BrokerSide::Buy),
            &state,
            || Ok(policy()),
            || panic!("preflight never submits"),
            |_| panic!("preflight never submits"),
        )
        .unwrap();
        assert_eq!(report["risk_refusal"], "liquidity");
        assert_eq!(report["execution_authority"], false);
        assert!(
            dispatch(
                "unknown",
                &mut log,
                &intent(BrokerSide::Buy),
                &state,
                || panic!("unknown command"),
                || panic!("unknown command"),
                |_| panic!("unknown command")
            )
            .is_err()
        );
    }

    #[test]
    fn local_cash_and_share_liabilities_are_counted_once_and_unbound_open_orders_refuse() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = BrokerLog::open(dir.path().join("paper.jsonl"), "paper").unwrap();
        let seconds = super::super::wallet_read::now().unwrap();
        super::super::alpaca_cash::checkpoint(&mut log, "paper", 1, |path| {
            Ok(Some(
                fixture(&format!("{PAPER_API}{path}"), seconds).to_string(),
            ))
        })
        .unwrap();
        let report = super::super::alpaca_reconcile::collect("paper", |path| {
            Ok(fixture(&format!("{PAPER_API}{path}"), seconds).to_string())
        })
        .unwrap();
        log.reserve(intent(BrokerSide::Buy), seconds).unwrap();
        log.reserve(intent(BrokerSide::Sell), seconds).unwrap();
        assert_eq!(
            account_bounds(&log, &intent(BrokerSide::Buy), &report).unwrap(),
            (399_999_986, 1_000_000_000, 310_000_014, 310_000_014)
        );
        for case in 0..5 {
            let mut wrong = report.clone();
            match case {
                0 => wrong["selected_account_fields_unchanged"] = json!(false),
                1 => wrong["selected_open_order_fields_unchanged"] = json!(false),
                2 => wrong["account_before"]["cash"] = json!("501"),
                3 => wrong["positions"][0]["qty"] = json!("1"),
                _ => {
                    wrong["open_orders_before"] =
                        json!([{"id":"external","status":"new","client_order_id":"external"}]);
                }
            }
            assert!(account_bounds(&log, &intent(BrokerSide::Buy), &wrong).is_err());
        }
        let mut changed = intent(BrokerSide::Sell);
        changed.quantity_nanoshares = 3_000_000_000;
        changed.client_order_id = "excess".into();
        log.reserve(changed, seconds).unwrap();
        assert!(account_bounds(&log, &intent(BrokerSide::Buy), &report).is_err());
    }

    #[test]
    fn asset_restrictions_and_private_input_size_fail_without_echoing_bodies() {
        let intent = intent(BrokerSide::Buy);
        let account = fixture(&format!("{PAPER_API}/v2/account"), 0);
        let asset = fixture(&format!("{PAPER_API}/v2/assets/SPY"), 0);
        let good = json!({"account_before":account,"account_after":account,"assets":[asset]});
        assert!(eligible(&good, &account, &intent).is_ok());
        for (field, value) in [
            ("status", json!("inactive")),
            ("tradable", json!(false)),
            ("exchange", json!("OTC")),
            ("attributes", Value::Null),
            ("attributes", json!(["restricted"])),
        ] {
            let mut wrong = good.clone();
            wrong["assets"][0][field] = value;
            assert!(eligible(&wrong, &account, &intent).is_err());
        }
        let mut fractional = intent.clone();
        fractional.quantity_nanoshares = 1;
        let mut wrong = good.clone();
        wrong["assets"][0]["fractionable"] = json!(false);
        assert!(eligible(&wrong, &account, &intent).is_ok());
        assert!(eligible(&wrong, &account, &fractional).is_err());
        wrong["account_after"] = Value::Null;
        assert!(eligible(&wrong, &account, &intent).is_err());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private.json");
        let body = serde_json::to_string(&intent).unwrap();
        std::fs::write(&path, format!("{}{body}", " ".repeat(65_536 - body.len()))).unwrap();
        assert_eq!(
            load::<BrokerIntent>(path.to_str().unwrap()).unwrap(),
            intent
        );
        std::fs::write(&path, format!("{}{body}", " ".repeat(65_537 - body.len()))).unwrap();
        assert_eq!(
            load::<BrokerIntent>(path.to_str().unwrap()).unwrap_err(),
            "private paper input exceeds 64 KiB"
        );
        std::fs::write(&path, "private malformed sentinel").unwrap();
        assert!(
            !load::<BrokerIntent>(path.to_str().unwrap())
                .unwrap_err()
                .contains("sentinel")
        );
    }

    #[test]
    fn open_orders_must_match_retained_payload_and_current_nonterminal_progress() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = BrokerLog::open(dir.path().join("paper.jsonl"), "paper").unwrap();
        let seconds = super::super::wallet_read::now().unwrap();
        super::super::alpaca_cash::checkpoint(&mut log, "paper", 1, |path| {
            Ok(Some(
                fixture(&format!("{PAPER_API}{path}"), seconds).to_string(),
            ))
        })
        .unwrap();
        let intent = intent(BrokerSide::Buy);
        log.reserve(intent.clone(), seconds).unwrap();
        let pending = json!({"id":"broker","client_order_id":intent.client_order_id,"symbol":"SPY","status":"new",
            "asset_class":"us_equity","side":"buy","type":"limit","order_class":"simple","time_in_force":"day",
            "extended_hours":false,"qty":"1","filled_qty":"0","limit_price":"100"});
        let report = json!({"open_orders_before":[pending]});
        assert!(open_orders(&log, &report).is_err());
        log.submit(&intent.client_order_id, seconds, |_| ())
            .unwrap();
        let mut cash = log.cash().unwrap().unwrap().clone();
        cash.orders = Some(
            [(
                intent.client_order_id.clone(),
                radar_journal::BrokerProgress {
                    broker_order_id: "broker".into(),
                    filled_nanoshares: 0,
                    terminal: false,
                },
            )]
            .into(),
        );
        log.observe_cash(cash.clone(), seconds).unwrap();
        assert!(open_orders(&log, &report).is_ok());
        for (field, value) in [
            ("id", json!("other")),
            ("client_order_id", Value::Null),
            ("client_order_id", json!("external")),
            ("filled_qty", Value::Null),
            ("filled_qty", json!("0.1")),
            ("status", json!("unknown")),
            ("qty", json!("2")),
        ] {
            let mut wrong = report.clone();
            wrong["open_orders_before"][0][field] = value;
            assert!(open_orders(&log, &wrong).is_err());
        }
        cash.orders
            .as_mut()
            .unwrap()
            .get_mut(&intent.client_order_id)
            .unwrap()
            .terminal = true;
        log.observe_cash(cash, seconds).unwrap();
        assert!(open_orders(&log, &report).is_err());
    }
}
