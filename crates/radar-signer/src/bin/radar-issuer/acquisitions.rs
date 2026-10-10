// SPDX-License-Identifier: Apache-2.0
//! Reviewed buys, sales and failed fees from the owned journal, not wallet reconciliation.

use std::collections::{BTreeMap, BTreeSet};

use radar_journal::{OperationEntry, OperationLog, OperationState};
use radar_types::{Address, Settlement, TokenQuantity};
use serde_json::{Value, json};

use super::{Config, valuation, verified_signed};

fn integer(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_str()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| "invalid retained acquisition integer".into())
}

enum HistoricalEffect {
    Acquisition(Value),
    FailedFee(Value),
    Sale(Value),
}

fn effect(
    log: &OperationLog,
    id: &radar_journal::OperationId,
    entry: &OperationEntry,
    config: &Config,
) -> Result<HistoricalEffect, String> {
    let binding = log
        .execution(id)
        .ok_or("acquisition lacks transaction binding")?;
    let record = log
        .valuation(id)
        .ok_or("submitted operation lacks acquisition valuation")?;
    let signed = radar_types::b64::decode(&record.settlement.signed_transaction)
        .ok_or("invalid retained signed transaction")?;
    verified_signed(binding, config.wallet, &signed)?;
    let signature: [u8; 64] = signed[1..65]
        .try_into()
        .map_err(|_| "invalid signature extent")?;
    if record.settlement.review["signature"] != radar_types::Signature::new(signature).to_string()
        || record.settlement.review["operation"] != id.as_str()
    {
        return Err("retained settlement names another transaction signature".into());
    }
    let value = &record.review;
    let price = historical_price(record, id, config);
    let mut historical = *entry;
    historical.state = OperationState::SubmissionUnknown;
    let mut checked = valuation::review(
        binding,
        &historical,
        &record.settlement,
        price,
        &config.policy,
        config.max_snapshot_age_secs,
    )?;
    checked["operation"] = json!(id.as_str());
    if &checked != value {
        return Err("retained acquisition valuation does not match its reviewed inputs".into());
    }
    if checked["sale_proceeds"].is_object() {
        return sale(entry, binding, id, &checked);
    }
    let debit = TokenQuantity::lamports(integer(value, "wallet_net_debit_lamports")?);
    match entry.state {
        OperationState::SubmissionUnknown => {}
        OperationState::Confirmed(Settlement::Completed(spent))
        | OperationState::Reconciled(Settlement::Completed(spent))
            if spent == debit => {}
        _ => return Err("unsupported acquisition terminal settlement".into()),
    }
    if checked["failed_execution_costs"].is_object() {
        return Ok(HistoricalEffect::FailedFee(json!({"operation":id.as_str(),
            "network_fee_lamports":checked["failed_execution_costs"]["network_fee_lamports"],
            "network_fee_micro_usd":checked["failed_execution_costs"]["network_fee_micro_usd"],
            "execution_slot":checked["execution_slot"],
            "execution_at_unix_secs":checked["execution_at_unix_secs"],
            "valuation_as_of_slot":checked["valuation_as_of_slot"],
            "price_at_unix_secs":checked["price_at_unix_secs"]})));
    }
    acquisition_lot(binding, id, value)
}

fn historical_price(
    record: &radar_journal::ValuationRecord,
    id: &radar_journal::OperationId,
    config: &Config,
) -> Value {
    let value = &record.review;
    let costs = &value["acquisition_costs"];
    // Reconstruct only the previously normalized complete review. Equality with
    // the rerun below verifies every retained price/cost output, not just basis.
    let mut price = json!({"version":1,"asset":"sol",
        "micro_usd_per_sol":value["micro_usd_per_sol"],
        "as_of_slot":value["valuation_as_of_slot"],
        "as_of_unix_secs":value["price_at_unix_secs"]});
    if costs.is_object() {
        price["acquisition_costs"] = json!({"version":1,"operation":id.as_str(),
            "signed_transaction":record.settlement.signed_transaction,
            "wallet":config.wallet,"mint":costs["mint"],
            "token_program":costs["token_program"],"decimals":costs["decimals"],
            "net_acquired_raw":costs["net_acquired_raw"],
            "swap_lamports":costs["swap_lamports"],"rent_lamports":costs["rent_lamports"],
            "tip_lamports":costs["tip_lamports"],"other_cash_flows_absent":true});
    }
    let proceeds = &value["sale_proceeds"];
    if proceeds.is_object() {
        price["sale_proceeds"] = json!({"version":1,"operation":id.as_str(),
            "signed_transaction":record.settlement.signed_transaction,"wallet":config.wallet,
            "mint":proceeds["mint"],"token_program":proceeds["token_program"],"decimals":proceeds["decimals"],
            "net_disposed_raw":proceeds["net_disposed_raw"],"gross_proceeds_lamports":proceeds["gross_proceeds_lamports"],
            "tip_lamports":proceeds["tip_lamports"],"rent_paid_lamports":proceeds["rent_paid_lamports"],
            "rent_refund_lamports":proceeds["rent_refund_lamports"],"other_cash_flows_absent":true});
    }
    price
}

fn sale(
    entry: &OperationEntry,
    binding: &radar_journal::ExecutionBinding,
    id: &radar_journal::OperationId,
    value: &Value,
) -> Result<HistoricalEffect, String> {
    // Completed native spend cannot represent sale credits or reconcile basis.
    if entry.state != OperationState::SubmissionUnknown {
        return Err("sale terminal state lacks economic reconciliation".into());
    }
    let proposal: radar_risk::Proposal = serde_json::from_value(
        binding
            .reviewed_proposal
            .clone()
            .ok_or("sale lacks reviewed attribution")?,
    )
    .map_err(|_| "invalid sale attribution")?;
    Ok(HistoricalEffect::Sale(
        json!({"operation":id.as_str(),"creator":proposal.creator,
        "proceeds":value["sale_proceeds"],"execution_slot":value["execution_slot"],
        "execution_at_unix_secs":value["execution_at_unix_secs"],
        "valuation_as_of_slot":value["valuation_as_of_slot"],"price_at_unix_secs":value["price_at_unix_secs"]}),
    ))
}

fn acquisition_lot(
    binding: &radar_journal::ExecutionBinding,
    id: &radar_journal::OperationId,
    value: &Value,
) -> Result<HistoricalEffect, String> {
    let costs = &value["acquisition_costs"];
    let proposal: radar_risk::Proposal = serde_json::from_value(
        binding
            .reviewed_proposal
            .clone()
            .ok_or("acquisition lacks reviewed attribution")?,
    )
    .map_err(|_| "invalid acquisition attribution")?;
    Ok(HistoricalEffect::Acquisition(
        json!({"operation":id.as_str(),"creator":proposal.creator,
        "mint":costs["mint"],"token_program":costs["token_program"],
        "decimals":costs["decimals"],"net_acquired_raw":costs["net_acquired_raw"],
        "position_cost_basis_micro_usd":costs["position_cost_basis_micro_usd"],
        "rent_micro_usd":costs["rent_micro_usd"],
        "execution_slot":value["execution_slot"],
        "execution_at_unix_secs":value["execution_at_unix_secs"],
        "valuation_as_of_slot":value["valuation_as_of_slot"],
        "price_at_unix_secs":value["price_at_unix_secs"]}),
    ))
}

fn fee_totals(fees: &[Value]) -> Result<Value, String> {
    let mut lamports = 0_u64;
    let mut micro_usd = 0_u64;
    for fee in fees {
        lamports = lamports
            .checked_add(integer(fee, "network_fee_lamports")?)
            .ok_or("failed-fee native total overflow")?;
        micro_usd = micro_usd
            .checked_add(integer(fee, "network_fee_micro_usd")?)
            .ok_or("failed-fee dollar total overflow")?;
    }
    Ok(json!({"network_fee_lamports":lamports.to_string(),
        "network_fee_micro_usd":micro_usd.to_string()}))
}

fn aggregate(lots: &[Value]) -> Result<Vec<Value>, String> {
    let mut identities = BTreeMap::<Address, (Address, u8)>::new();
    let mut groups = BTreeMap::<(Address, Address), Value>::new();
    for lot in lots {
        let address = |field: &str| {
            serde_json::from_value::<Address>(lot[field].clone())
                .map_err(|_| "invalid retained acquisition identity")
        };
        let mint = address("mint")?;
        let creator = address("creator")?;
        let program = address("token_program")?;
        let decimals = lot["decimals"]
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .ok_or("invalid retained acquisition decimals")?;
        if identities
            .insert(mint, (program, decimals))
            .is_some_and(|prior| prior != (program, decimals))
        {
            return Err("acquisition mint changed program or units".into());
        }
        let group = groups.entry((mint, creator)).or_insert_with(|| {
            json!({
            "mint":mint,"creator":creator,"token_program":program,"decimals":decimals,
            "net_acquired_raw":"0","position_cost_basis_micro_usd":"0","rent_micro_usd":"0",
            "oldest_valuation_slot":lot["valuation_as_of_slot"]})
        });
        for field in [
            "net_acquired_raw",
            "position_cost_basis_micro_usd",
            "rent_micro_usd",
        ] {
            let total = integer(group, field)?
                .checked_add(integer(lot, field)?)
                .ok_or("acquisition aggregate overflow")?;
            group[field] = json!(total.to_string());
        }
        let oldest =
            integer(group, "oldest_valuation_slot")?.min(integer(lot, "valuation_as_of_slot")?);
        group["oldest_valuation_slot"] = json!(oldest.to_string());
    }
    Ok(groups.into_values().collect())
}

pub(super) fn review(log: &OperationLog, config: &Config) -> Result<Value, String> {
    let mut lots = Vec::new();
    let mut artifacts = BTreeSet::new();
    let mut unsubmitted = Vec::new();
    let mut failed_fees = Vec::new();
    let mut sales = Vec::new();
    let mut cash_flows = Vec::new();
    let mut settlements = Vec::new();
    for (id, entry) in log.entries() {
        if matches!(
            entry.state,
            OperationState::Proposed | OperationState::Reserved | OperationState::Failed
        ) {
            unsubmitted.push(id.as_str());
            continue;
        }
        let reviewed = effect(log, id, entry, config)?;
        let artifact = &log
            .valuation(id)
            .ok_or("missing acquisition")?
            .settlement
            .signed_transaction;
        if !artifacts.insert(artifact) {
            return Err("signed transaction occurs under multiple acquisition operations".into());
        }
        cash_flows.push(super::cash::flow(
            &log.valuation(id)
                .ok_or("missing cash flow")?
                .settlement
                .review,
            id.as_str(),
            config.wallet,
        )?);
        settlements.push(
            log.valuation(id)
                .ok_or("missing settlement")?
                .settlement
                .clone(),
        );
        match reviewed {
            HistoricalEffect::Acquisition(lot) => lots.push(lot),
            HistoricalEffect::FailedFee(fee) => failed_fees.push(fee),
            HistoricalEffect::Sale(sale) => sales.push(sale),
        }
    }
    let accounting = if sales.is_empty() {
        None
    } else {
        super::basis::review(&lots, &sales, log.opening_inventory(), config.wallet)?
    };
    let groups = aggregate(&lots)?;
    let fees = fee_totals(&failed_fees)?;
    Ok(
        json!({"version":1,"authority":"protected_operator_acquisition_history",
        "wallet":config.wallet,"accounting_checkpoint":log.checkpoint(),
        "lots":lots,"acquisitions_by_mint_and_creator":groups,
        "failed_execution_fees":failed_fees,"recorded_failed_fee_totals":fees,
        "sales":sales,"recorded_disposal_accounting":accounting,
        "recorded_native_cash_flows":cash_flows,
        "recorded_settlements":settlements,
        "unsubmitted_operations":unsubmitted,
        "wallet_inventory_complete":false,"current_exposure_micro_usd":null,
        "realised_loss_today_micro_usd":null,"portfolio_state_updated":false,
        "economic_reconciliation_complete":false,"reservation_released":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_fee_totals_are_exact_and_refuse_overflow_without_claiming_daily_loss() {
        let first = json!({"network_fee_lamports":"7","network_fee_micro_usd":"11"});
        let second = json!({"network_fee_lamports":"13","network_fee_micro_usd":"17"});
        assert_eq!(
            fee_totals(&[first.clone(), second]).expect("totals"),
            json!({"network_fee_lamports":"20","network_fee_micro_usd":"28"})
        );
        assert_eq!(
            fee_totals(&[]).expect("no recorded fees"),
            json!({"network_fee_lamports":"0","network_fee_micro_usd":"0"})
        );
        for field in ["network_fee_lamports", "network_fee_micro_usd"] {
            let mut overflow = first.clone();
            overflow[field] = json!(u64::MAX.to_string());
            assert!(fee_totals(&[first.clone(), overflow]).is_err());
        }
    }

    fn acquisition(raw: u64, basis: u64, rent: u64, slot: u64) -> Value {
        json!({"mint":Address::new([2;32]),"creator":Address::new([3;32]),
            "token_program":Address::new([4;32]),"decimals":6,
            "net_acquired_raw":raw.to_string(),"position_cost_basis_micro_usd":basis.to_string(),
            "rent_micro_usd":rent.to_string(),"valuation_as_of_slot":slot.to_string()})
    }

    #[test]
    fn acquisitions_aggregate_exact_units_costs_and_oldest_price_by_mint_and_creator() {
        let first = acquisition(7, 19, 2, 99);
        let second = acquisition(11, 23, 3, 95);
        let mut other_creator = acquisition(13, 29, 5, 96);
        other_creator["creator"] = json!(Address::new([5; 32]));
        let mut other_mint = acquisition(17, 31, 7, 97);
        other_mint["mint"] = json!(Address::new([6; 32]));
        let lots = vec![first, second, other_creator, other_mint];
        let groups = aggregate(&lots).expect("groups");
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0]["net_acquired_raw"], "18");
        assert_eq!(groups[0]["position_cost_basis_micro_usd"], "42");
        assert_eq!(groups[0]["rent_micro_usd"], "5");
        assert_eq!(groups[0]["oldest_valuation_slot"], "95");
        assert_eq!(groups[1]["net_acquired_raw"], "13");
        assert_eq!(groups[2]["net_acquired_raw"], "17");
        assert_eq!(
            aggregate(&lots.into_iter().rev().collect::<Vec<_>>()).expect("reverse"),
            groups
        );
    }

    #[test]
    fn aggregation_refuses_changed_units_and_overflow_in_every_component() {
        let first = acquisition(1, 1, 1, 95);
        for field in [
            "token_program",
            "decimals",
            "net_acquired_raw",
            "position_cost_basis_micro_usd",
            "rent_micro_usd",
        ] {
            let mut second = first.clone();
            second[field] = match field {
                "token_program" => json!(Address::new([9; 32])),
                "decimals" => json!(9),
                _ => json!(u64::MAX.to_string()),
            };
            assert!(aggregate(&[first.clone(), second]).is_err(), "{field}");
        }
    }
}
