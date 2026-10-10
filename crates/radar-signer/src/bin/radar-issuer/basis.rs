// SPDX-License-Identifier: Apache-2.0
//! FIFO bookkeeping over recorded trades, not complete wallet economics.

use std::collections::{BTreeMap, BTreeSet};

use radar_journal::OpeningInventoryRecord;
use radar_types::Address;
use serde_json::{Value, json};

use super::{evidence_integer as integer, opening};

struct Lot {
    source: Value,
    raw: u64,
    basis: u64,
}

fn address(row: &Value, field: &str) -> Result<Address, String> {
    serde_json::from_value(row[field].clone()).map_err(|_| "invalid basis identity".into())
}

fn consume(lots: &mut [Lot], sale: &Value) -> Result<Value, String> {
    let mut needed = integer(&sale["proceeds"], "net_disposed_raw")?;
    let mut basis = 0_u64;
    let mut allocations = Vec::new();
    for lot in lots {
        let taken = needed.min(lot.raw);
        if taken == 0 {
            continue;
        }
        // Charge the fractional micro-dollar to the sale; retain the exact
        // remainder so successive partial sales never manufacture basis.
        let allocated = u64::try_from(
            (u128::from(lot.basis) * u128::from(taken)).div_ceil(u128::from(lot.raw)),
        )
        .map_err(|_| "disposed basis overflow")?;
        lot.raw -= taken;
        lot.basis -= allocated;
        needed -= taken;
        basis = basis.checked_add(allocated).ok_or("sale basis overflow")?;
        allocations.push(json!({"acquisition_operation":lot.source["operation"],
            "creator":lot.source["creator"],"disposed_raw":taken.to_string(),
            "allocated_cost_basis_micro_usd":allocated.to_string()}));
    }
    if needed != 0 {
        return Err("sale exceeds preceding recorded inventory".into());
    }
    let net = sale["proceeds"]["net_trade_proceeds_micro_usd"]
        .as_str()
        .and_then(|value| value.parse::<i128>().ok())
        .ok_or("invalid net sale proceeds")?;
    let pnl = net
        .checked_sub(i128::from(basis))
        .ok_or("trade PnL overflow")?;
    Ok(
        json!({"operation":sale["operation"],"mint":sale["proceeds"]["mint"],
        "execution_slot":sale["execution_slot"],"allocations":allocations,
        "allocated_cost_basis_micro_usd":basis.to_string(),
        "recorded_trade_pnl_micro_usd":pnl.to_string()}),
    )
}

pub(super) fn review(
    buys: &[Value],
    sales: &[Value],
    baseline: Option<&OpeningInventoryRecord>,
    wallet: Address,
) -> Result<Option<Value>, String> {
    let Some(baseline) = baseline else {
        return Ok(None);
    };
    let floor = opening::floor(baseline, wallet)?.get();
    let mut sold = BTreeSet::new();
    for sale in sales {
        sold.insert(address(&sale["proceeds"], "mint")?);
    }
    if baseline
        .holdings
        .iter()
        .any(|holding| holding.raw_amount > 0 && sold.contains(&holding.mint))
    {
        return Ok(None);
    }
    let mut events = Vec::new();
    for (rows, is_sale) in [(buys, false), (sales, true)] {
        for row in rows {
            let slot = integer(row, "execution_slot")?;
            if slot <= floor {
                return Err("trade does not follow opening inventory".into());
            }
            events.push((slot, is_sale, row));
        }
    }
    events.sort_by_key(|event| event.0);
    let mut identities: BTreeMap<_, _> = baseline
        .holdings
        .iter()
        .map(|holding| {
            (
                holding.mint,
                (holding.token_program, json!(holding.decimals)),
            )
        })
        .collect();
    let mut slots = BTreeSet::new();
    let mut retained = BTreeMap::<Address, Vec<Lot>>::new();
    let mut disposals = Vec::new();
    for (slot, is_sale, row) in events {
        let units = if is_sale { &row["proceeds"] } else { row };
        let mint = address(units, "mint")?;
        let identity = (address(units, "token_program")?, units["decimals"].clone());
        if identities
            .insert(mint, identity.clone())
            .is_some_and(|prior| prior != identity)
        {
            return Err("basis mint changed program or units".into());
        }
        if !slots.insert((mint, slot)) {
            return Err("same-mint trade order is ambiguous within one slot".into());
        }
        let lots = retained.entry(mint).or_default();
        if is_sale {
            disposals.push(consume(lots, row)?);
        } else {
            let raw = integer(row, "net_acquired_raw")?;
            if raw == 0 {
                return Err("zero acquisition cannot supply basis".into());
            }
            lots.push(Lot {
                source: row.clone(),
                raw,
                basis: integer(row, "position_cost_basis_micro_usd")?,
            });
        }
    }
    let remaining: Vec<_> = retained
        .into_values()
        .flatten()
        .map(|lot| {
            let mut row = lot.source;
            row["remaining_raw"] = json!(lot.raw.to_string());
            row["remaining_cost_basis_micro_usd"] = json!(lot.basis.to_string());
            row
        })
        .collect();
    Ok(Some(
        json!({"method":"fifo_conservative_micro_usd","coverage":"recorded_trades_only",
        "disposals":disposals,"remaining_lots":remaining,
        "external_cash_flows_complete":false,"realised_loss_today_micro_usd":null,
        "economic_reconciliation_complete":false,"portfolio_state_updated":false}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_types::Slot;
    fn baseline() -> OpeningInventoryRecord {
        OpeningInventoryRecord {
            wallet: Address::new([9; 32]),
            native_lamports: 100,
            native_slot: Slot(1),
            token_program_slot: Slot(1),
            token_2022_slot: Slot(1),
            raw_token_slot: None,
            read_started_at_unix_secs: 1,
            read_completed_at_unix_secs: 1,
            accounts: None,
            holdings: vec![],
        }
    }
    fn buy(slot: u64, raw: u64, basis: u64) -> Value {
        json!({"operation":slot.to_string(),"creator":Address::new([u8::try_from(slot).expect("fixture slot");32]),
            "mint":Address::new([2;32]),"token_program":Address::new([3;32]),"decimals":6,
            "execution_slot":slot.to_string(),"net_acquired_raw":raw.to_string(),"position_cost_basis_micro_usd":basis.to_string()})
    }
    fn sale(slot: u64, raw: u64, net: i128) -> Value {
        json!({"operation":slot.to_string(),"execution_slot":slot.to_string(),"proceeds":{
            "mint":Address::new([2;32]),"token_program":Address::new([3;32]),"decimals":6,
            "net_disposed_raw":raw.to_string(),"net_trade_proceeds_micro_usd":net.to_string()}})
    }
    fn report(buys: &[Value], sales: &[Value]) -> Result<Value, String> {
        let opening = baseline();
        review(buys, sales, Some(&opening), opening.wallet)?.ok_or("unknown basis".into())
    }
    #[test]
    fn fifo_uses_oldest_lots_and_conserves_partial_basis_across_sales() {
        let buys = [buy(2, 3, 10), buy(4, 5, 20)];
        let sales = [sale(3, 1, 7), sale(5, 4, 12), sale(6, 3, -1)];
        let value = report(&buys, &sales).expect("FIFO");
        let disposals = &value["disposals"];
        assert_eq!(disposals[0]["allocated_cost_basis_micro_usd"], "4");
        assert_eq!(disposals[0]["recorded_trade_pnl_micro_usd"], "3");
        assert_eq!(disposals[1]["allocated_cost_basis_micro_usd"], "14");
        assert_eq!(disposals[1]["recorded_trade_pnl_micro_usd"], "-2");
        assert_eq!(disposals[1]["allocations"][0]["acquisition_operation"], "2");
        assert_eq!(disposals[1]["allocations"][1]["acquisition_operation"], "4");
        assert_eq!(disposals[2]["allocated_cost_basis_micro_usd"], "12");
        assert_eq!(disposals[2]["recorded_trade_pnl_micro_usd"], "-13");
        for row in value["remaining_lots"].as_array().expect("remaining") {
            assert_eq!(row["remaining_raw"], "0");
            assert_eq!(row["remaining_cost_basis_micro_usd"], "0");
        }
        assert_eq!(
            report(
                &[buys[1].clone(), buys[0].clone()],
                &[sales[2].clone(), sales[1].clone(), sales[0].clone()]
            )
            .expect("order"),
            value
        );
        let partial = report(&buys, &sales[..1]).expect("partial");
        assert_eq!(partial["remaining_lots"][0]["remaining_raw"], "2");
        assert_eq!(
            partial["remaining_lots"][0]["remaining_cost_basis_micro_usd"],
            "6"
        );
    }
    #[test]
    fn missing_or_nonzero_opening_sold_inventory_keeps_basis_unknown() {
        let buys = [buy(2, 3, 10)];
        let sales = [sale(3, 1, 7)];
        let mut opening = baseline();
        assert!(
            review(&buys, &sales, None, opening.wallet)
                .expect("absent")
                .is_none()
        );
        opening.holdings.push(radar_journal::OpeningTokenHolding {
            mint: Address::new([2; 32]),
            token_program: Address::new([3; 32]),
            decimals: 6,
            raw_amount: 1,
        });
        opening.raw_token_slot = Some(Slot(1));
        assert!(
            review(&buys, &sales, Some(&opening), opening.wallet)
                .expect("unknown purchase costs")
                .is_none()
        );
        opening.holdings[0].raw_amount = 0;
        assert!(
            review(&buys, &sales, Some(&opening), opening.wallet)
                .expect("zero measured")
                .is_some()
        );
        opening.holdings[0].token_program = Address::new([8; 32]);
        assert!(review(&buys, &sales, Some(&opening), opening.wallet).is_err());
        assert!(review(&buys, &sales, Some(&baseline()), Address::new([8; 32])).is_err());
    }
    #[test]
    fn fifo_refuses_future_inventory_tied_slots_and_changed_sale_units() {
        assert!(report(&[buy(4, 3, 10)], &[sale(3, 1, 7)]).is_err());
        assert!(report(&[buy(2, 3, 10)], &[sale(3, 4, 7)]).is_err());
        assert!(report(&[buy(2, 3, 10)], &[sale(2, 1, 7)]).is_err());
        assert!(report(&[buy(1, 3, 10)], &[sale(3, 1, 7)]).is_err());
        assert!(report(&[buy(2, 0, 10), buy(3, 3, 10)], &[sale(4, 1, 7)]).is_err());
        for field in ["mint", "token_program", "decimals"] {
            let mut changed = sale(3, 1, 7);
            changed["proceeds"][field] = if field == "decimals" {
                json!(9)
            } else {
                json!(Address::new([8; 32]))
            };
            assert!(report(&[buy(2, 3, 10)], &[changed]).is_err(), "{field}");
        }
        assert!(report(&[buy(2, 3, 10)], &[sale(3, 1, i128::MIN)]).is_err());
    }
    #[test]
    fn fifo_preserves_large_products_zero_basis_and_refuses_sum_overflow() {
        let value = report(&[buy(2, u64::MAX, u64::MAX)], &[sale(3, u64::MAX - 1, 0)])
            .expect("wide product");
        assert_eq!(
            value["disposals"][0]["allocated_cost_basis_micro_usd"],
            (u64::MAX - 1).to_string()
        );
        assert_eq!(
            value["remaining_lots"][0]["remaining_cost_basis_micro_usd"],
            "1"
        );
        let value = report(&[buy(2, 1, 0)], &[sale(3, 1, 0)]).expect("zero measured basis");
        assert_eq!(value["disposals"][0]["recorded_trade_pnl_micro_usd"], "0");
        assert!(report(&[buy(2, 1, u64::MAX), buy(3, 1, 1)], &[sale(4, 2, 0)]).is_err());
    }
}
