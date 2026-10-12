// SPDX-License-Identifier: Apache-2.0
//! Private paper submission boundary. The web/model process never receives credentials.

use radar_journal::{Applied, BrokerIntent, BrokerLog, BrokerSide};
use radar_risk::equity::{self, Order, Policy, State};
use serde::Deserialize;
use serde_json::{Value, json};

// A fixed paper endpoint; no caller-supplied host or live fallback.
const ORDERS: &str = "https://paper-api.alpaca.markets/v2/orders";

fn decimal(value: u64, scale: u64, digits: usize) -> String {
    format!("{}.{:0digits$}", value / scale, value % scale)
}

fn body(intent: &BrokerIntent) -> Value {
    json!({"client_order_id":intent.client_order_id,"symbol":intent.symbol,
        "side":match intent.side { BrokerSide::Buy => "buy", BrokerSide::Sell => "sell" },
        "qty":decimal(intent.quantity_nanoshares, 1_000_000_000, 9),
        "limit_price":decimal(intent.limit_microusd_per_share, 1_000_000, 6),
        "type":"limit","order_class":"simple","time_in_force":"day","extended_hours":false})
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    client_order_id: String,
    symbol: String,
    side: equity::Side,
    qty: String,
    limit_price: String,
    #[serde(rename = "type")]
    kind: String,
    order_class: String,
    time_in_force: String,
    extended_hours: bool,
}

// Re-read the actual serialized request, rather than checking one order and
// sending a different body. Account and fee reserve are local, not API fields.
fn decode(raw: &str, intent: &BrokerIntent) -> Result<Order, String> {
    let payload: Payload = serde_json::from_str(raw).map_err(|_| "invalid paper order payload")?;
    if payload.kind != "limit"
        || payload.order_class != "simple"
        || payload.time_in_force != "day"
        || payload.extended_hours
    {
        return Err("unsupported paper order payload".into());
    }
    let price = super::alpaca_exposure::nanos(&payload.limit_price)?;
    if !price.is_multiple_of(1_000) {
        return Err("paper limit precision exceeds authorization".into());
    }
    Ok(Order {
        account_id: intent.account_id.clone(),
        client_order_id: payload.client_order_id,
        symbol: payload.symbol,
        side: payload.side,
        quantity_nanoshares: super::alpaca_exposure::nanos(&payload.qty)?,
        limit_microusd_per_share: price / 1_000,
        fee_bound_microusd: intent.fee_bound_microusd,
    })
}

pub(super) fn post(
    agent: &ureq::Agent,
    key: &str,
    secret: &str,
    raw: &str,
) -> Result<String, String> {
    let mut response = agent
        .post(ORDERS)
        .header("APCA-API-KEY-ID", key)
        .header("APCA-API-SECRET-KEY", secret)
        .content_type("application/json")
        .send(raw)
        .map_err(|_| "paper submission outcome unknown; reconcile retained client ID")?;
    let status = response.status().as_u16();
    http_response(
        status,
        response.body_mut().read_to_string().map_err(|_| {
            "paper submission response unreadable; reconcile retained client ID".into()
        }),
    )
}

fn http_response(status: u16, body: Result<String, String>) -> Result<String, String> {
    if status != 200 {
        return Err("unexpected paper submission status; reconcile retained client ID".into());
    }
    body
}

pub(super) fn execute(
    log: &mut BrokerLog,
    intent: &BrokerIntent,
    state: &State,
    mut policy: impl FnMut() -> Result<Policy, String>,
    mut clock: impl FnMut() -> Result<u64, String>,
    send: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Value, String> {
    let order = super::alpaca_preflight::order(intent);
    let authorization = equity::evaluate(&order, state, &policy()?)
        .map_err(|_| "paper risk refused; no submission released")?;
    let raw = body(intent).to_string();
    let decoded = decode(&raw, intent)?;
    let at = clock()?;
    if !authorization.permits(&decoded, at) {
        return Err("paper payload or authorization time differs".into());
    }
    if log
        .reserve(intent.clone(), at / 1_000)
        .map_err(|e| e.to_string())?
        != Applied::Advanced
    {
        return Err("paper client ID already retained; reconcile rather than retry".into());
    }
    // The journal persists SubmissionUnknown before this closure runs. Even a
    // pre-send refusal keeps that conservative state: never infer a safe retry.
    let result = log
        .submit(&intent.client_order_id, at / 1_000, |retained| {
            let latest = policy()?;
            let now = clock()?;
            let decoded = decode(&raw, retained)?;
            if !authorization.permits(&decoded, now) {
                return Err("paper permission expired or retained payload differs".into());
            }
            let mut current = state.clone();
            current.now_unix_ms = now;
            equity::evaluate(&decoded, &current, &latest)
                .map_err(|_| "paper policy changed or inputs expired; submission refused")?;
            send(&raw)
        })
        .map_err(|e| e.to_string())??;
    let response: super::alpaca_reconcile::BrokerOrder = serde_json::from_str(&result)
        .map_err(|_| "paper response invalid; reconcile retained client ID")?;
    super::alpaca_order_read::payload_matches(intent, &response)?;
    Ok(
        json!({"venue":"alpaca","environment":"paper", "client_order_id":intent.client_order_id,
        "broker_order_id":response.id,"submission":"response_payload_matched",
        "fills_reconciled":false,"retry_authorized":false,"claim_release_authorized":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_journal::BrokerState;

    fn fixture() -> (BrokerIntent, State, Policy) {
        let intent = BrokerIntent {
            account_id: "paper".into(),
            client_order_id: "radar_1".into(),
            symbol: "SPY".into(),
            side: BrokerSide::Buy,
            quantity_nanoshares: 1_000_000_001,
            limit_microusd_per_share: 100_000_001,
            fee_bound_microusd: 7,
        };
        let state = State {
            account_id: "paper".into(),
            now_unix_ms: 1_000,
            oldest_input_unix_ms: 999,
            available_at_unix_ms: 1_000,
            halted: false,
            market_open: true,
            session_close_unix_ms: Some(10_000),
            available_cash_microusd: Some(500_000_000),
            available_nanoshares: Some(2_000_000_000),
            committed_microusd: Some(0),
            symbol_committed_microusd: Some(0),
            daily_loss_microusd: None,
            bid_microusd_per_share: 100_000_000,
            ask_microusd_per_share: 100_000_000,
            capacity_nanoshares: Some(2_000_000_000),
            required_fee_microusd: Some(7),
            round_trip_cost_microusd: Some(10_000),
        };
        let policy = Policy {
            enabled: true,
            account_id: "paper".into(),
            symbols: ["SPY".into()].into(),
            max_capital_microusd: 500_000_000,
            max_order_microusd: 110_000_000,
            max_position_microusd: 500_000_000,
            max_daily_loss_microusd: None,
            max_input_age_ms: 100,
            authorization_lifetime_ms: 10,
            max_spread_bps: 100,
            max_limit_distance_bps: 100,
            max_round_trip_cost_bps: 100,
        };
        (intent, state, policy)
    }

    fn response(intent: &BrokerIntent) -> String {
        let mut value = body(intent);
        value["id"] = json!("broker");
        value["asset_class"] = json!("us_equity");
        value["status"] = json!("new");
        value.to_string()
    }

    #[test]
    fn exact_wire_digits_and_payload_redecode_cover_both_sides_and_limits() {
        let (mut intent, _, _) = fixture();
        let wire = body(&intent);
        assert_eq!(wire["qty"], "1.000000001");
        assert_eq!(wire["limit_price"], "100.000001");
        for side in [BrokerSide::Buy, BrokerSide::Sell] {
            intent.side = side;
            intent.quantity_nanoshares = u64::MAX;
            intent.limit_microusd_per_share = u64::MAX / 1_000;
            assert_eq!(
                decode(&body(&intent).to_string(), &intent).unwrap(),
                super::super::alpaca_preflight::order(&intent)
            );
        }
        intent.limit_microusd_per_share = u64::MAX;
        assert!(decode(&body(&intent).to_string(), &intent).is_err());
        let (intent, _, _) = fixture();
        for (field, value) in [
            ("type", json!("market")),
            ("order_class", json!("bracket")),
            ("time_in_force", json!("gtc")),
            ("extended_hours", json!(true)),
            ("qty", json!("-1")),
            ("limit_price", json!("1.000000001")),
            ("notional", json!("100")),
        ] {
            let mut bad = body(&intent);
            bad[field] = value;
            assert!(decode(&bad.to_string(), &intent).is_err(), "{field}");
        }
        for (field, value) in [
            ("symbol", json!("QQQ")),
            ("side", json!("sell")),
            ("client_order_id", json!("other")),
            ("qty", json!("2")),
            ("limit_price", json!("100.01")),
        ] {
            let mut bad = body(&intent);
            bad[field] = value;
            assert_ne!(
                decode(&bad.to_string(), &intent).unwrap(),
                super::super::alpaca_preflight::order(&intent)
            );
        }
    }

    #[test]
    fn successful_submission_retains_unknown_and_never_retries_after_restart() {
        let (intent, state, policy) = fixture();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("orders.jsonl");
        let mut log = BrokerLog::open(&path, "paper").unwrap();
        let report = execute(
            &mut log,
            &intent,
            &state,
            || Ok(policy.clone()),
            || Ok(1_000),
            |raw| {
                assert_eq!(
                    decode(raw, &intent).unwrap(),
                    super::super::alpaca_preflight::order(&intent)
                );
                let events = radar_journal::Journal::open(&path)
                    .unwrap()
                    .events()
                    .unwrap();
                assert_eq!(events.len(), 2);
                assert_eq!(events[0].at, 1);
                assert_eq!(events[1].at, 1);
                assert_eq!(
                    events[1].broker.as_ref().unwrap().state,
                    BrokerState::SubmissionUnknown
                );
                http_response(200, Ok(response(&intent)))
            },
        )
        .unwrap();
        assert_eq!(report["broker_order_id"], "broker");
        assert_eq!(report["retry_authorized"], false);
        drop(log);
        let mut reopened = BrokerLog::open(&path, "paper").unwrap();
        assert_eq!(
            reopened.entries().unwrap().next().unwrap().state,
            BrokerState::SubmissionUnknown
        );
        assert!(
            execute(
                &mut reopened,
                &intent,
                &state,
                || Ok(policy.clone()),
                || Ok(1_000),
                |_| panic!("must not retry")
            )
            .is_err()
        );
        assert_eq!(reopened.entries().unwrap().count(), 1);
    }

    #[test]
    fn initial_unknowns_and_expired_permissions_never_reserve_or_send() {
        let (intent, state, policy) = fixture();
        for scenario in 0..6 {
            let dir = tempfile::tempdir().unwrap();
            let mut log = BrokerLog::open(dir.path().join("orders.jsonl"), "paper").unwrap();
            let mut current = state.clone();
            let mut limits = policy.clone();
            match scenario {
                0 => current.capacity_nanoshares = None,
                1 => current.required_fee_microusd = None,
                2 => current.round_trip_cost_microusd = None,
                3 => limits.enabled = false,
                _ => (),
            }
            let at = match scenario {
                4 => 999,
                5 => 1_011,
                _ => 1_000,
            };
            assert!(
                execute(
                    &mut log,
                    &intent,
                    &current,
                    || Ok(limits.clone()),
                    || Ok(at),
                    |_| panic!("refusal must not send")
                )
                .is_err()
            );
            assert!(log.entries().unwrap().next().is_none());
        }
    }

    #[test]
    fn session_closing_after_durable_submission_refuses_network_effect() {
        let (intent, mut state, policy) = fixture();
        state.session_close_unix_ms = Some(1_001);
        let dir = tempfile::tempdir().unwrap();
        let mut log = BrokerLog::open(dir.path().join("orders.jsonl"), "paper").unwrap();
        let mut calls = 0;
        assert!(execute(&mut log, &intent, &state, || Ok(policy.clone()), || {
            calls += 1;
            Ok(if calls == 1 { 1_000 } else { 1_001 })
        }, |_| panic!("closed session must not send")).is_err());
        assert_eq!(log.entries().unwrap().next().unwrap().state, BrokerState::SubmissionUnknown);
    }

    #[test]
    fn policy_reload_and_final_time_refusals_hold_uncertainty_without_effect() {
        let (intent, state, policy) = fixture();
        for scenario in 0..7 {
            let dir = tempfile::tempdir().unwrap();
            let mut log = BrokerLog::open(dir.path().join("orders.jsonl"), "paper").unwrap();
            let mut calls = 0;
            let mut clocks = 0;
            assert!(
                execute(
                    &mut log,
                    &intent,
                    &state,
                    || {
                        calls += 1;
                        let mut latest = policy.clone();
                        if calls == 2 {
                            match scenario {
                                0 => latest.enabled = false,
                                1 => latest.max_order_microusd = 1,
                                2 => return Err("policy missing".into()),
                                3 => latest.account_id = "other".into(),
                                4 => latest.max_input_age_ms = 0,
                                _ => (),
                            }
                        }
                        Ok(latest)
                    },
                    || {
                        clocks += 1;
                        Ok(if clocks == 2 {
                            match scenario {
                                5 => 999,
                                6 => 1_011,
                                _ => 1_000,
                            }
                        } else {
                            1_000
                        })
                    },
                    |_| panic!("revoked/expired must not send")
                )
                .is_err()
            );
            assert_eq!(calls, 2);
            assert_eq!(
                log.entries().unwrap().next().unwrap().state,
                BrokerState::SubmissionUnknown
            );
        }
    }

    #[test]
    fn ambiguous_and_mismatched_responses_never_release_claims() {
        let (intent, state, policy) = fixture();
        for scenario in 0..5 {
            let dir = tempfile::tempdir().unwrap();
            let mut log = BrokerLog::open(dir.path().join("orders.jsonl"), "paper").unwrap();
            assert!(
                execute(
                    &mut log,
                    &intent,
                    &state,
                    || Ok(policy.clone()),
                    || Ok(1_000),
                    |_| {
                        match scenario {
                            0 => Err("timeout".into()),
                            1 => Ok("bad JSON".into()),
                            2 => {
                                let mut bad: Value =
                                    serde_json::from_str(&response(&intent)).unwrap();
                                bad["qty"] = json!("2");
                                Ok(bad.to_string())
                            }
                            _ => http_response(
                                if scenario == 3 { 201 } else { 500 },
                                Ok(response(&intent)),
                            ),
                        }
                    }
                )
                .is_err()
            );
            assert_eq!(
                log.entries().unwrap().next().unwrap().state,
                BrokerState::SubmissionUnknown
            );
            assert_eq!(
                log.claims().unwrap()[&intent.client_order_id].cash_microusd,
                intent.cash_claim_microusd().unwrap()
            );
        }
    }
}
