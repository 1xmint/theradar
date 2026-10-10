// SPDX-License-Identifier: Apache-2.0
//! Minimum recorded risk bounds, not complete live portfolio reconstruction.

use std::collections::BTreeMap;

use radar_journal::OpeningInventoryRecord;
use radar_risk::PortfolioState;
use radar_types::Address;
use serde_json::Value;

use super::evidence_integer as integer;

#[derive(Default)]
struct Floors {
    deployed: u64,
    creators: BTreeMap<Address, u64>,
    loss: u64,
    failures: u32,
}

fn add(total: &mut u64, amount: u64) -> Result<(), String> {
    *total = total
        .checked_add(amount)
        .ok_or("recorded risk bound overflow")?;
    Ok(())
}

fn rows<'a>(history: &'a Value, field: &str) -> Result<&'a [Value], String> {
    history[field]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| "recorded risk history missing".into())
}

fn exposure(history: &Value, floors: &mut Floors) -> Result<(), String> {
    let sales = rows(history, "sales")?;
    let (lots, raw_field, basis_field) = if sales.is_empty() {
        (
            rows(history, "lots")?,
            "net_acquired_raw",
            "position_cost_basis_micro_usd",
        )
    } else {
        (
            rows(&history["recorded_disposal_accounting"], "remaining_lots")?,
            "remaining_raw",
            "remaining_cost_basis_micro_usd",
        )
    };
    for lot in lots {
        let raw = integer(lot, raw_field)?;
        let basis = integer(lot, basis_field)?;
        if raw == 0 && basis != 0 {
            return Err("empty recorded lot retains basis".into());
        }
        let creator: Address = serde_json::from_value(lot["creator"].clone())
            .map_err(|_| "recorded risk creator missing")?;
        add(&mut floors.deployed, basis)?;
        add(floors.creators.entry(creator).or_default(), basis)?;
    }
    Ok(())
}

fn outcomes(
    history: &Value,
    state: &PortfolioState,
    now: u64,
    floors: &mut Floors,
) -> Result<(), String> {
    let mut ordered = BTreeMap::new();
    for (field, failed) in [
        ("lots", false),
        ("sales", false),
        ("failed_execution_fees", true),
    ] {
        for row in rows(history, field)? {
            let slot = integer(row, "execution_slot")?;
            let time = integer(row, "execution_at_unix_secs")?;
            if slot > state.now.get() || time > now || ordered.insert(slot, failed).is_some() {
                return Err("recorded execution order or time unknown".into());
            }
            if failed && time / 86_400 == now / 86_400 {
                add(&mut floors.loss, integer(row, "network_fee_micro_usd")?)?;
            }
        }
    }
    for failed in ordered.values() {
        floors.failures = if *failed {
            floors
                .failures
                .checked_add(1)
                .ok_or("recorded failure count overflow")?
        } else {
            0
        };
    }
    Ok(())
}

fn disposal_losses(history: &Value, now: u64, floors: &mut Floors) -> Result<(), String> {
    let sales = rows(history, "sales")?;
    if sales.is_empty() {
        return Ok(());
    }
    let mut disposals = BTreeMap::new();
    for row in rows(&history["recorded_disposal_accounting"], "disposals")? {
        let id = row["operation"]
            .as_str()
            .ok_or("disposal operation missing")?;
        if disposals.insert(id, row).is_some() {
            return Err("duplicate recorded disposal".into());
        }
    }
    for sale in sales {
        let id = sale["operation"].as_str().ok_or("sale operation missing")?;
        let row = disposals.remove(id).ok_or("recorded sale basis unknown")?;
        let pnl = row["recorded_trade_pnl_micro_usd"]
            .as_str()
            .and_then(|value| value.parse::<i128>().ok())
            .ok_or("recorded PnL unknown")?;
        if pnl < 0 && integer(sale, "execution_at_unix_secs")? / 86_400 == now / 86_400 {
            let loss = pnl
                .checked_neg()
                .and_then(|value| u64::try_from(value).ok())
                .ok_or("recorded loss overflow")?;
            // Do not let a winning trade erase already incurred loss stops.
            add(&mut floors.loss, loss)?;
        }
    }
    if !disposals.is_empty() {
        return Err("unassociated recorded disposal".into());
    }
    Ok(())
}

fn derive(
    history: &Value,
    opening: Option<&OpeningInventoryRecord>,
    state: &PortfolioState,
    now: u64,
) -> Result<Floors, String> {
    if let Some(record) = opening {
        let wallet = serde_json::from_value(history["wallet"].clone())
            .map_err(|_| "risk history wallet missing")?;
        if super::opening::floor(record, wallet)?.get() > state.now.get()
            || record.read_completed_at_unix_secs > now
        {
            return Err("opening inventory is from after risk state".into());
        }
    }
    let has_trades = !rows(history, "lots")?.is_empty() || !rows(history, "sales")?.is_empty();
    if has_trades && opening.is_none()
        || opening
            .is_some_and(|record| record.holdings.iter().any(|holding| holding.raw_amount > 0))
    {
        return Err("opening inventory risk basis unknown".into());
    }
    let mut floors = Floors::default();
    exposure(history, &mut floors)?;
    outcomes(history, state, now, &mut floors)?;
    disposal_losses(history, now, &mut floors)?;
    Ok(floors)
}

pub(super) fn verify(
    history: &Value,
    opening: Option<&OpeningInventoryRecord>,
    state: &PortfolioState,
    now: u64,
) -> Result<(), String> {
    let floors = derive(history, opening, state, now)?;
    if state.deployed.get() < floors.deployed
        || state.realised_loss_today.get() < floors.loss
        || state.consecutive_failures < floors.failures
        || floors
            .creators
            .iter()
            .any(|(creator, bound)| state.creator_exposure(creator).get() < *bound)
    {
        return Err("snapshot risk state understates retained history".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_types::{MicroUsd, Slot};
    use serde_json::json;

    fn fixture() -> (Value, OpeningInventoryRecord, PortfolioState) {
        let wallet = Address::new([1; 32]);
        let creator = Address::new([2; 32]);
        let opening = OpeningInventoryRecord {
            wallet,
            native_lamports: 0,
            native_slot: Slot(0),
            token_program_slot: Slot(0),
            token_2022_slot: Slot(0),
            raw_token_slot: None,
            read_started_at_unix_secs: 1,
            read_completed_at_unix_secs: 1,
            holdings: vec![],
            accounts: Some(vec![]),
        };
        let history = json!({"wallet":wallet,
            "lots":[{"creator":creator,"net_acquired_raw":"10","position_cost_basis_micro_usd":"100",
                "execution_slot":"10","execution_at_unix_secs":"172801"}],
            "sales":[{"operation":"loss","execution_slot":"20","execution_at_unix_secs":"172802"},
                {"operation":"win","execution_slot":"30","execution_at_unix_secs":"172803"}],
            "failed_execution_fees":[{"execution_slot":"5","execution_at_unix_secs":"172799","network_fee_micro_usd":"99"},
                {"execution_slot":"40","execution_at_unix_secs":"172804","network_fee_micro_usd":"7"},
                {"execution_slot":"50","execution_at_unix_secs":"172805","network_fee_micro_usd":"9"}],
            "recorded_disposal_accounting":{"remaining_lots":[{"creator":creator,"remaining_raw":"5","remaining_cost_basis_micro_usd":"50"}],
                "disposals":[{"operation":"loss","recorded_trade_pnl_micro_usd":"-13"},
                    {"operation":"win","recorded_trade_pnl_micro_usd":"100"}]}});
        let mut state = PortfolioState::flat(Slot(50));
        state.deployed = MicroUsd(50);
        state.per_creator.insert(creator, MicroUsd(50));
        state.realised_loss_today = MicroUsd(29);
        state.consecutive_failures = 2;
        (history, opening, state)
    }

    #[test]
    fn recorded_floors_count_remaining_basis_losses_and_ordered_failures_once() {
        let (history, opening, state) = fixture();
        let floors = derive(&history, Some(&opening), &state, 172_810).unwrap();
        assert_eq!(floors.deployed, 50);
        assert_eq!(floors.loss, 29);
        assert_eq!(floors.failures, 2);
        assert!(verify(&history, Some(&opening), &state, 172_810).is_ok());
        for kind in 0..4 {
            let mut understated = state.clone();
            match kind {
                0 => understated.deployed = MicroUsd(49),
                1 => understated.per_creator.clear(),
                2 => understated.realised_loss_today = MicroUsd(28),
                _ => understated.consecutive_failures = 1,
            }
            assert_eq!(
                verify(&history, Some(&opening), &understated, 172_810).unwrap_err(),
                "snapshot risk state understates retained history"
            );
        }
        let mut next_day = state.clone();
        next_day.realised_loss_today = MicroUsd::ZERO;
        assert!(verify(&history, Some(&opening), &next_day, 259_200).is_ok());
        let mut higher = state.clone();
        higher.deployed = MicroUsd(51);
        higher.realised_loss_today = MicroUsd(30);
        higher.consecutive_failures = 3;
        assert!(verify(&history, Some(&opening), &higher, 172_810).is_ok());
        // Exact watermark/time boundaries are permitted; a success resets a
        // previous streak, while a UTC day change alone does not.
        assert!(verify(&history, Some(&opening), &state, 172_805).is_ok());
        let mut reset = history.clone();
        reset["failed_execution_fees"] = json!([reset["failed_execution_fees"][0]]);
        let mut reset_state = state.clone();
        reset_state.realised_loss_today = MicroUsd(13);
        reset_state.consecutive_failures = 0;
        assert!(verify(&reset, Some(&opening), &reset_state, 172_810).is_ok());
        for (raw, basis) in [("0", "0"), ("5", "0")] {
            let mut zero = history.clone();
            zero["recorded_disposal_accounting"]["remaining_lots"][0]["remaining_raw"] = json!(raw);
            zero["recorded_disposal_accounting"]["remaining_lots"][0]["remaining_cost_basis_micro_usd"] =
                json!(basis);
            reset_state.deployed = MicroUsd::ZERO;
            reset_state.per_creator.clear();
            reset_state.realised_loss_today = state.realised_loss_today;
            reset_state.consecutive_failures = state.consecutive_failures;
            assert!(verify(&zero, Some(&opening), &reset_state, 172_810).is_ok());
        }
    }

    #[test]
    fn missing_basis_future_events_ambiguous_order_and_overflow_refuse() {
        let (history, opening, state) = fixture();
        assert!(verify(&history, None, &state, 172_810).is_err());
        for (pointer, value) in [
            ("/recorded_disposal_accounting", Value::Null),
            (
                "/recorded_disposal_accounting/disposals/0/recorded_trade_pnl_micro_usd",
                json!(i128::MIN.to_string()),
            ),
            ("/sales/0/operation", json!("unknown")),
            ("/failed_execution_fees/0/execution_slot", json!("10")),
            ("/lots/0/execution_slot", json!("51")),
            ("/lots/0/execution_at_unix_secs", json!("172811")),
            (
                "/recorded_disposal_accounting/remaining_lots/0/remaining_raw",
                json!("0"),
            ),
            (
                "/failed_execution_fees/1/network_fee_micro_usd",
                json!(u64::MAX.to_string()),
            ),
            (
                "/recorded_disposal_accounting/remaining_lots/0/creator",
                Value::Null,
            ),
        ] {
            let mut changed = history.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            assert!(
                verify(&changed, Some(&opening), &state, 172_810).is_err(),
                "{pointer}"
            );
        }
        let mut changed = opening.clone();
        changed.wallet = Address::new([9; 32]);
        assert!(verify(&history, Some(&changed), &state, 172_810).is_err());
        for id in ["loss", "extra"] {
            let mut changed = history.clone();
            changed["recorded_disposal_accounting"]["disposals"]
                .as_array_mut()
                .unwrap()
                .push(json!({"operation":id,"recorded_trade_pnl_micro_usd":"0"}));
            assert!(verify(&changed, Some(&opening), &state, 172_810).is_err());
        }
        let mut future = opening.clone();
        future.native_slot = Slot(51);
        assert!(verify(&history, Some(&future), &state, 172_810).is_err());
        future = opening.clone();
        future.read_completed_at_unix_secs = 172_811;
        assert!(verify(&history, Some(&future), &state, 172_810).is_err());
        future.read_completed_at_unix_secs = 172_810;
        assert!(verify(&history, Some(&future), &state, 172_810).is_ok());
        future = opening.clone();
        future.holdings.push(radar_journal::OpeningTokenHolding {
            mint: Address::new([3; 32]),
            token_program: Address::new([4; 32]),
            decimals: 6,
            raw_amount: 1,
        });
        future.accounts = None;
        future.raw_token_slot = Some(Slot(0));
        assert_eq!(
            verify(&history, Some(&future), &state, 172_810).unwrap_err(),
            "opening inventory risk basis unknown"
        );
        let mut empty = json!({"lots":[],"sales":[],"failed_execution_fees":[]});
        assert!(verify(&empty, None, &PortfolioState::flat(Slot(1)), 1).is_ok());
        empty["lots"] = json!([{"creator":Address::new([2;32]),"net_acquired_raw":"2",
            "position_cost_basis_micro_usd":"1","execution_slot":"1","execution_at_unix_secs":"1"}]);
        empty["wallet"] = json!(opening.wallet);
        assert!(verify(&empty, Some(&opening), &state, 172_810).is_ok());
    }
}
