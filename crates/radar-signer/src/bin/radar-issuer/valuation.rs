// SPDX-License-Identifier: Apache-2.0
//! Prices retained effects and optional operator-reviewed acquisition costs.

use radar_journal::{ExecutionBinding, OperationEntry, OperationState, SettlementRecord};
use radar_risk::Policy;
use radar_types::{Asset, Decimals, MicroUsd};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Price {
    version: u8,
    asset: Asset,
    micro_usd_per_sol: String,
    as_of_slot: String,
    as_of_unix_secs: String,
    acquisition_costs: Option<AcquisitionCosts>,
    sale_proceeds: Option<super::sale_proceeds::Breakdown>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AcquisitionCosts {
    version: u8,
    operation: String,
    signed_transaction: String,
    wallet: radar_types::Address,
    mint: radar_types::Address,
    token_program: radar_types::Address,
    decimals: u8,
    net_acquired_raw: String,
    swap_lamports: String,
    rent_lamports: String,
    tip_lamports: String,
    other_cash_flows_absent: bool,
}

fn costs(
    binding: &ExecutionBinding,
    value: &Value,
    input: Option<AcquisitionCosts>,
    debit: u64,
    fee: u64,
    price: u64,
) -> Result<Option<Value>, String> {
    let Some(input) = input else { return Ok(None) };
    let proposal: radar_risk::Proposal = serde_json::from_value(
        binding
            .reviewed_proposal
            .clone()
            .ok_or("acquisition lacks reviewed context")?,
    )
    .map_err(|_| "invalid reviewed acquisition context")?;
    let acquired = &value["wallet_token_acquisition"];
    let raw = input
        .net_acquired_raw
        .parse::<u64>()
        .map_err(|_| "invalid acquired units")?;
    if input.version != 1
        || value["outcome"] != "succeeded"
        || proposal.action != radar_risk::Action::Buy
        || proposal.quote != Asset::Sol
        || value["operation"] != input.operation
        || binding.signed_transaction.as_ref() != Some(&input.signed_transaction)
        || input.wallet != binding.wallet
        || input.mint != proposal.mint
        || acquired["mint"] != input.mint.to_string()
        || acquired["owner"] != input.wallet.to_string()
        || acquired["program_id"] != input.token_program.to_string()
        || acquired["decimals"] != input.decimals
        || Decimals::from_mint_account(input.decimals).is_none()
        || raw == 0
        || integer(acquired, "net_acquired_raw")? != raw
        || !input.other_cash_flows_absent
    {
        return Err("acquisition costs do not bind complete reviewed effects".into());
    }
    let parse = |raw: &str| raw.parse::<u64>().map_err(|_| "invalid acquisition cost");
    let swap = parse(&input.swap_lamports)?;
    let rent = parse(&input.rent_lamports)?;
    let tip = parse(&input.tip_lamports)?;
    let basis = swap
        .checked_add(fee)
        .and_then(|sum| sum.checked_add(tip))
        .ok_or("acquisition cost exceeds native range")?;
    let outlay = basis
        .checked_add(rent)
        .ok_or("acquisition outlay exceeds native range")?;
    if swap == 0 || outlay != debit {
        return Err("acquisition breakdown does not account for exact wallet debit".into());
    }
    // This convention capitalizes swap, network fee and tip. Rent remains
    // separate; a balanced operator breakdown is not independent provenance.
    Ok(Some(json!({"authority":"protected_operator_breakdown",
        "mint":input.mint,"token_program":input.token_program,"decimals":input.decimals,
        "net_acquired_raw":raw.to_string(),"swap_lamports":swap.to_string(),
        "rent_lamports":rent.to_string(),"tip_lamports":tip.to_string(),
        "basis_lamports":basis.to_string(),"trade_notional_micro_usd":dollars(swap,price)?.get().to_string(),
        "position_cost_basis_micro_usd":dollars(basis,price)?.get().to_string(),
        "rent_micro_usd":dollars(rent,price)?.get().to_string()})))
}

fn integer(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "missing or invalid retained valuation integer".into())
}

fn dollars(lamports: u64, price: u64) -> Result<MicroUsd, String> {
    // Round debits upward. Neither floats nor a wrapped dollar amount may
    // understate a measured cost. The u64 product fits in u128.
    u64::try_from((u128::from(lamports) * u128::from(price)).div_ceil(1_000_000_000))
        .map(MicroUsd)
        .map_err(|_| "native valuation exceeds dollar range".into())
}

fn failed_costs(
    binding: &ExecutionBinding,
    value: &Value,
    debit: u64,
    fee: u64,
    fee_usd: MicroUsd,
) -> Result<Option<Value>, String> {
    if value["outcome"] != "failed" {
        return Ok(None);
    }
    let effects = value["native_account_effects"]
        .as_array()
        .filter(|effects| !effects.is_empty())
        .ok_or("failed execution lacks native effects")?;
    let mut accounts = std::collections::BTreeSet::new();
    for (index, effect) in effects.iter().enumerate() {
        let account: radar_types::Address = serde_json::from_value(effect["account"].clone())
            .map_err(|_| "invalid failed execution account")?;
        let pre = integer(effect, "pre_lamports")?;
        let post = integer(effect, "post_lamports")?;
        let delta = i128::from(post) - i128::from(pre);
        let expected = if index == 0 { -i128::from(fee) } else { 0 };
        if !accounts.insert(account)
            || (index == 0 && account != binding.wallet)
            || delta != expected
            || effect["net_change_lamports"] != delta.to_string()
        {
            return Err("failed execution has effects beyond the wallet fee".into());
        }
    }
    if debit != fee
        || !super::settlement::unchanged_tokens(value, effects.len())?
        || !value["wallet_token_acquisition"].is_null()
    {
        return Err("failed execution is not a measured fee-only debit".into());
    }
    // A classified historical cost is not total realised PnL or daily loss.
    Ok(Some(json!({"authority":"protected_failed_fee_review",
        "network_fee_lamports":fee.to_string(),
        "network_fee_micro_usd":fee_usd.get().to_string()})))
}

pub(super) fn review(
    binding: &ExecutionBinding,
    entry: &OperationEntry,
    record: &SettlementRecord,
    price: Value,
    policy: &Policy,
    max_age_secs: u64,
) -> Result<Value, String> {
    let value = &record.review;
    let reserved = entry.reserved.ok_or("operation has no reservation")?;
    if entry.state != OperationState::SubmissionUnknown
        || entry.intent.asset != Asset::Sol
        || reserved.decimals() != Decimals::NATIVE_SOL
        || binding.signed_transaction.as_ref() != Some(&record.signed_transaction)
        || value["version"] != 1
        || value["authority"] != "protected_file_review"
        || value["wallet"] != binding.wallet.to_string()
        || value["signature_verified_locally"] != true
        || integer(value, "reserved_lamports")? != reserved.raw()
    {
        return Err("retained settlement does not bind the operation for valuation".into());
    }
    let price: Price = serde_json::from_value(price).map_err(|_| "invalid protected SOL price")?;
    let amount = price
        .micro_usd_per_sol
        .parse::<u64>()
        .map_err(|_| "invalid SOL price")?;
    let price_slot = price
        .as_of_slot
        .parse::<u64>()
        .map_err(|_| "invalid price slot")?;
    let price_time = price
        .as_of_unix_secs
        .parse::<u64>()
        .map_err(|_| "invalid price time")?;
    let slot = integer(value, "slot")?;
    let time = integer(value, "block_time_unix_secs")?;
    let slots_old = slot
        .checked_sub(price_slot)
        .ok_or("price is from after execution")?;
    let seconds_old = time
        .checked_sub(price_time)
        .ok_or("price is from after execution")?;
    if price.version != 1
        || price.asset != Asset::Sol
        || amount == 0
        || slot < entry.intent.at.get()
        || slots_old > policy.max_input_staleness.get()
        || seconds_old > max_age_secs
    {
        return Err("price is unpriced, foreign or stale for execution".into());
    }
    if let Some(input) = price.sale_proceeds {
        if price.acquisition_costs.is_some() {
            return Err("sale and acquisition breakdowns cannot be combined".into());
        }
        let proceeds =
            super::sale_proceeds::review(binding, value, &input, amount, reserved.raw())?;
        return Ok(
            json!({"version":1,"authority":"protected_operator_valuation",
            "wallet":binding.wallet,"execution_slot":slot.to_string(),
            "execution_at_unix_secs":time.to_string(),"valuation_as_of_slot":price_slot.to_string(),
            "price_at_unix_secs":price_time.to_string(),"micro_usd_per_sol":amount.to_string(),
            "sale_proceeds":proceeds,"position_cost_basis_micro_usd":null,"realised_pnl_micro_usd":null,
            "portfolio_state_updated":false,"operation_reconciled":false,"reservation_released":false}),
        );
    }
    let delta = value["wallet_net_change_lamports"]
        .as_str()
        .and_then(|s| s.parse::<i128>().ok())
        .ok_or("invalid retained native effect")?;
    let debit = delta
        .checked_neg()
        .and_then(|n| u64::try_from(n).ok())
        .ok_or("native credit or effect outside debit range cannot value a cash debit")?;
    let fee = integer(value, "network_fee_lamports")?;
    if debit > reserved.raw() || fee > debit {
        return Err("retained native debit or fee exceeds its bounds".into());
    }
    let cash = dollars(debit, amount)?;
    let fee_usd = dollars(fee, amount)?;
    let failed = failed_costs(binding, value, debit, fee, fee_usd)?;
    let costs = costs(binding, value, price.acquisition_costs, debit, fee, amount)?;
    let notional = costs
        .as_ref()
        .map(|value| value["trade_notional_micro_usd"].clone());
    let basis = costs
        .as_ref()
        .map(|value| value["position_cost_basis_micro_usd"].clone());
    let mut report = json!({"version":1,"authority":"protected_operator_valuation",
        "wallet":binding.wallet.to_string(),"execution_slot":slot.to_string(),
        "execution_at_unix_secs":time.to_string(),"valuation_as_of_slot":price_slot.to_string(),
        "price_at_unix_secs":price_time.to_string(),"micro_usd_per_sol":amount.to_string(),
        "wallet_net_debit_lamports":debit.to_string(),"wallet_net_debit_micro_usd":cash.get().to_string(),
        "network_fee_lamports":fee.to_string(),"network_fee_micro_usd":fee_usd.get().to_string(),
        "acquisition_costs":costs,"trade_notional_micro_usd":notional,
        "position_cost_basis_micro_usd":basis,"realised_pnl_micro_usd":null,
        "portfolio_state_updated":false,"operation_reconciled":false,"reservation_released":false});
    // Preserve the shape of older successful acquisition reviews during replay.
    if let Some(failed) = failed {
        report["failed_execution_costs"] = failed;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_journal::Intent;
    use radar_types::{Address, Slot, SlotDelta, TokenQuantity};

    #[test]
    fn failed_fee_classification_requires_exact_native_and_paired_token_effects() {
        let (binding, _, _, _, _) = fixture();
        let token = json!({"account_index":1,"mint":Address::new([2;32]),
            "owner":binding.wallet,"program_id":Address::new([3;32]),
            "raw_amount":"10","decimals":6});
        let value = json!({"outcome":"failed","wallet_token_acquisition":null,
            "native_account_effects":[
                {"account":binding.wallet,"pre_lamports":"6000","post_lamports":"1000","net_change_lamports":"-5000"},
                {"account":Address::new([2;32]),"pre_lamports":"42","post_lamports":"42","net_change_lamports":"0"}],
            "pre_token_balances":[token.clone()],"post_token_balances":[token]});
        let cost = failed_costs(&binding, &value, 5000, 5000, MicroUsd(1001))
            .expect("fee only")
            .expect("classified");
        assert_eq!(cost["network_fee_micro_usd"], "1001");
        assert_eq!(cost["network_fee_lamports"], "5000");
        for (field, bad) in [
            ("account", json!(Address::SYSTEM_PROGRAM)),
            ("pre_lamports", json!("6001")),
            ("post_lamports", json!("999")),
            ("net_change_lamports", json!("-4999")),
        ] {
            let mut bad_value = value.clone();
            bad_value["native_account_effects"][0][field] = bad;
            assert!(failed_costs(&binding, &bad_value, 5000, 5000, MicroUsd(1001)).is_err());
        }
        for (field, bad) in [
            ("account", json!(binding.wallet)),
            ("post_lamports", json!("43")),
            ("net_change_lamports", json!("1")),
        ] {
            let mut bad_value = value.clone();
            bad_value["native_account_effects"][1][field] = bad;
            assert!(failed_costs(&binding, &bad_value, 5000, 5000, MicroUsd(1001)).is_err());
        }
        for (field, bad) in [
            ("account_index", json!(0)),
            ("mint", json!(Address::SYSTEM_PROGRAM)),
            ("owner", json!(Address::SYSTEM_PROGRAM)),
            ("program_id", json!(Address::SYSTEM_PROGRAM)),
            ("decimals", json!(9)),
            ("raw_amount", json!("11")),
        ] {
            let mut bad_value = value.clone();
            bad_value["post_token_balances"][0][field] = bad;
            assert!(failed_costs(&binding, &bad_value, 5000, 5000, MicroUsd(1001)).is_err());
        }
        for (field, bad) in [
            ("native_account_effects", json!([])),
            ("pre_token_balances", json!([])),
            ("post_token_balances", Value::Null),
            ("wallet_token_acquisition", json!({})),
        ] {
            let mut bad_value = value.clone();
            bad_value[field] = bad;
            assert!(failed_costs(&binding, &bad_value, 5000, 5000, MicroUsd(1001)).is_err());
        }
        assert!(failed_costs(&binding, &value, 5001, 5000, MicroUsd(1001)).is_err());
        let mut success = value.clone();
        success["outcome"] = json!("succeeded");
        assert!(
            failed_costs(&binding, &success, 5000, 5000, MicroUsd(1001))
                .expect("success")
                .is_none()
        );
        let mut zero = value;
        zero["native_account_effects"][0]["post_lamports"] = json!("6000");
        zero["native_account_effects"][0]["net_change_lamports"] = json!("0");
        zero["post_token_balances"][0]["raw_amount"] = json!("0010");
        assert!(
            failed_costs(&binding, &zero, 0, 0, MicroUsd::ZERO)
                .expect("measured zero")
                .is_some()
        );
        let mut empty = zero.clone();
        empty["native_account_effects"] = json!([]);
        empty["pre_token_balances"] = json!([]);
        empty["post_token_balances"] = json!([]);
        assert!(failed_costs(&binding, &empty, 0, 0, MicroUsd::ZERO).is_err());
        let mut reordered = zero;
        let mut second = reordered["pre_token_balances"][0].clone();
        second["account_index"] = json!(0);
        reordered["pre_token_balances"]
            .as_array_mut()
            .expect("tokens")
            .push(second.clone());
        reordered["post_token_balances"]
            .as_array_mut()
            .expect("tokens")
            .insert(0, second);
        assert!(
            failed_costs(&binding, &reordered, 0, 0, MicroUsd::ZERO)
                .expect("token order is immaterial")
                .is_some()
        );
    }

    fn fixture() -> (
        ExecutionBinding,
        OperationEntry,
        SettlementRecord,
        Value,
        Policy,
    ) {
        let wallet = Address::new([7; 32]);
        let binding = ExecutionBinding {
            wallet,
            transaction: "approved".into(),
            signed_transaction: Some("signed".into()),
            reviewed_proposal: None,
        };
        let entry = OperationEntry {
            intent: Intent {
                asset: Asset::Sol,
                amount: TokenQuantity::lamports(u64::MAX),
                at: Slot(90),
            },
            reserved: Some(TokenQuantity::lamports(u64::MAX)),
            state: OperationState::SubmissionUnknown,
        };
        let record = SettlementRecord {
            signed_transaction: "signed".into(),
            review: json!({"version":1,"authority":"protected_file_review","wallet":wallet.to_string(),
                "signature_verified_locally":true,"reserved_lamports":u64::MAX.to_string(),
                "slot":"100","block_time_unix_secs":"1000","wallet_net_change_lamports":"-250005000",
                "network_fee_lamports":"5000"}),
        };
        let price = json!({"version":1,"asset":"sol","micro_usd_per_sol":"200000003",
            "as_of_slot":"90","as_of_unix_secs":"980"});
        let policy = Policy {
            max_input_staleness: SlotDelta(10),
            ..Policy::CLOSED
        };
        (binding, entry, record, price, policy)
    }

    fn acquisition_fixture() -> (
        ExecutionBinding,
        OperationEntry,
        SettlementRecord,
        Value,
        Policy,
    ) {
        let (mut binding, entry, mut record, mut price, policy) = fixture();
        let mint = Address::new([2; 32]);
        let program = Address::new([3; 32]);
        binding.reviewed_proposal = Some(
            json!({"mint":mint,"market":radar_types::Market::PUMP_FUN_BONDING_CURVE,
            "quote":"sol","creator":Address::new([4;32]),"action":"buy","notional":1,
            "estimated_round_trip_cost":0,"oldest_input_slot":90,"simulated_exit_capacity":1}),
        );
        record.review["operation"] = json!("operation");
        record.review["outcome"] = json!("succeeded");
        record.review["wallet_net_change_lamports"] = json!("-251006000");
        record.review["wallet_token_acquisition"] = json!({"mint":mint,"owner":binding.wallet,
            "program_id":program,"decimals":6,"net_acquired_raw":"1000"});
        price["acquisition_costs"] = json!({"version":1,"operation":"operation",
            "signed_transaction":"signed","wallet":binding.wallet,"mint":mint,
            "token_program":program,"decimals":6,"net_acquired_raw":"1000",
            "swap_lamports":"250000000","rent_lamports":"1000000","tip_lamports":"1000",
            "other_cash_flows_absent":true});
        (binding, entry, record, price, policy)
    }

    #[test]
    fn sale_dispatch_passes_the_recorded_reservation_into_cash_flow_review() {
        let (mut binding, mut entry, mut record, mut price, policy) = acquisition_fixture();
        binding.reviewed_proposal.as_mut().expect("context")["action"] = json!("exit");
        record.review["wallet_token_disposal"] = json!({"mint":Address::new([2;32]),"owner":binding.wallet,
            "program_id":Address::new([3;32]),"decimals":6,"net_disposed_raw":"1000"});
        price
            .as_object_mut()
            .expect("price")
            .remove("acquisition_costs");
        price["sale_proceeds"] = json!({"version":1,"operation":"operation","signed_transaction":"signed",
            "wallet":binding.wallet,"mint":Address::new([2;32]),"token_program":Address::new([3;32]),
            "decimals":6,"net_disposed_raw":"1000","gross_proceeds_lamports":"6000","tip_lamports":"0",
            "rent_paid_lamports":"0","rent_refund_lamports":"0","other_cash_flows_absent":true});
        for (reserved, delta, gross, tip, valid) in [
            (5000, "1000", "6000", "0", true),
            (4999, "1000", "6000", "0", false),
            (5001, "-5001", "1", "2", true),
            (5000, "-5001", "1", "2", false),
        ] {
            entry.reserved = Some(TokenQuantity::lamports(reserved));
            record.review["reserved_lamports"] = json!(reserved.to_string());
            record.review["wallet_net_change_lamports"] = json!(delta);
            price["sale_proceeds"]["gross_proceeds_lamports"] = json!(gross);
            price["sale_proceeds"]["tip_lamports"] = json!(tip);
            assert_eq!(
                review(&binding, &entry, &record, price.clone(), &policy, 20).is_ok(),
                valid
            );
        }
    }

    #[test]
    fn bound_acquisition_costs_price_exact_components_without_capitalizing_rent() {
        let (binding, entry, record, price, policy) = acquisition_fixture();
        let report =
            review(&binding, &entry, &record, price.clone(), &policy, 20).expect("bound costs");
        assert_eq!(report["trade_notional_micro_usd"], "50000001");
        assert_eq!(report["position_cost_basis_micro_usd"], "50001201");
        assert_eq!(report["acquisition_costs"]["basis_lamports"], "250006000");
        assert_eq!(report["acquisition_costs"]["rent_micro_usd"], "200001");
        assert_eq!(report["acquisition_costs"]["net_acquired_raw"], "1000");
        assert_eq!(
            report["acquisition_costs"]["authority"],
            "protected_operator_breakdown"
        );
        assert_eq!(report["realised_pnl_micro_usd"], Value::Null);
        assert_eq!(report["portfolio_state_updated"], false);
        assert_eq!(report["reservation_released"], false);
        for omitted in [false, true] {
            let mut absent = price.clone();
            if omitted {
                absent
                    .as_object_mut()
                    .expect("price")
                    .remove("acquisition_costs");
            } else {
                absent["acquisition_costs"] = Value::Null;
            }
            let report = review(&binding, &entry, &record, absent, &policy, 20)
                .expect("cash valuation only");
            assert_eq!(report["trade_notional_micro_usd"], Value::Null);
            assert_eq!(report["position_cost_basis_micro_usd"], Value::Null);
        }
        let mut zero_costs = price;
        zero_costs["acquisition_costs"]["rent_lamports"] = json!("0");
        zero_costs["acquisition_costs"]["tip_lamports"] = json!("0");
        let mut exact = record;
        exact.review["wallet_net_change_lamports"] = json!("-250005000");
        assert!(review(&binding, &entry, &exact, zero_costs, &policy, 20).is_ok());
    }

    #[test]
    fn incomplete_foreign_or_unbalanced_breakdowns_never_become_cost_basis() {
        let (binding, entry, record, price, policy) = acquisition_fixture();
        for key in price["acquisition_costs"]
            .as_object()
            .expect("costs")
            .keys()
        {
            let mut bad = price.clone();
            bad["acquisition_costs"]
                .as_object_mut()
                .expect("costs")
                .remove(key);
            assert!(
                review(&binding, &entry, &record, bad, &policy, 20).is_err(),
                "missing {key}"
            );
        }
        for (field, value) in [
            ("version", json!(2)),
            ("operation", json!("foreign")),
            ("signed_transaction", json!("foreign")),
            ("wallet", json!(Address::SYSTEM_PROGRAM)),
            ("mint", json!(Address::SYSTEM_PROGRAM)),
            ("token_program", json!(Address::SYSTEM_PROGRAM)),
            ("decimals", json!(9)),
            ("net_acquired_raw", json!("0")),
            ("net_acquired_raw", json!("999")),
            ("other_cash_flows_absent", json!(false)),
            ("swap_lamports", json!("0")),
            ("swap_lamports", json!("250000001")),
            ("swap_lamports", json!("249999999")),
            ("rent_lamports", json!("1000001")),
            ("tip_lamports", json!("1001")),
            ("swap_lamports", json!(u64::MAX.to_string())),
            ("rent_lamports", json!(u64::MAX.to_string())),
            ("tip_lamports", json!(u64::MAX.to_string())),
            ("rent_lamports", json!("unknown")),
            ("tip_lamports", json!(-1)),
            ("provider_body", json!("unreviewed")),
        ] {
            let mut bad = price.clone();
            bad["acquisition_costs"][field] = value;
            assert!(
                review(&binding, &entry, &record, bad, &policy, 20).is_err(),
                "bad {field}"
            );
        }
        for (field, value) in [
            ("outcome", json!("failed")),
            ("operation", Value::Null),
            ("wallet_token_acquisition", Value::Null),
        ] {
            let mut bad = record.clone();
            bad.review[field] = value;
            assert!(review(&binding, &entry, &bad, price.clone(), &policy, 20).is_err());
        }
        for (field, value) in [
            ("mint", json!(Address::SYSTEM_PROGRAM)),
            ("owner", json!(Address::SYSTEM_PROGRAM)),
            ("program_id", json!(Address::SYSTEM_PROGRAM)),
            ("decimals", json!(9)),
            ("net_acquired_raw", Value::Null),
        ] {
            let mut bad = record.clone();
            bad.review["wallet_token_acquisition"][field] = value;
            assert!(review(&binding, &entry, &bad, price.clone(), &policy, 20).is_err());
        }
        for context in [None, Some(json!({}))] {
            let mut bad = binding.clone();
            bad.reviewed_proposal = context;
            assert!(review(&bad, &entry, &record, price.clone(), &policy, 20).is_err());
        }
        for (field, value) in [
            ("action", json!("exit")),
            ("action", json!("reduce")),
            ("quote", json!("usdc")),
            ("mint", json!(Address::SYSTEM_PROGRAM)),
        ] {
            let mut bad = binding.clone();
            bad.reviewed_proposal.as_mut().expect("proposal")[field] = value;
            assert!(review(&bad, &entry, &record, price.clone(), &policy, 20).is_err());
        }
    }

    #[test]
    fn acquisition_unit_bounds_and_retained_identity_fields_are_required() {
        let (binding, entry, record, price, policy) = acquisition_fixture();
        for (input_field, retained_field, value) in [
            ("wallet", "owner", json!(Address::SYSTEM_PROGRAM)),
            ("mint", "mint", json!(Address::SYSTEM_PROGRAM)),
            ("net_acquired_raw", "net_acquired_raw", json!("0")),
        ] {
            let mut record = record.clone();
            let mut price = price.clone();
            record.review["wallet_token_acquisition"][retained_field] = value.clone();
            price["acquisition_costs"][input_field] = value;
            assert!(review(&binding, &entry, &record, price, &policy, 20).is_err());
        }
        let mut zero_swap = price.clone();
        zero_swap["acquisition_costs"]["swap_lamports"] = json!("0");
        zero_swap["acquisition_costs"]["rent_lamports"] = json!("251000000");
        assert!(review(&binding, &entry, &record, zero_swap, &policy, 20).is_err());
        for decimals in [0, 18, u8::MAX] {
            let mut record = record.clone();
            let mut price = price.clone();
            record.review["wallet_token_acquisition"]["decimals"] = json!(decimals);
            price["acquisition_costs"]["decimals"] = json!(decimals);
            assert_eq!(
                review(&binding, &entry, &record, price, &policy, 20).is_ok(),
                decimals != u8::MAX
            );
        }
        for field in [
            "mint",
            "owner",
            "program_id",
            "decimals",
            "net_acquired_raw",
        ] {
            let mut record = record.clone();
            record.review["wallet_token_acquisition"]
                .as_object_mut()
                .expect("acquisition")
                .remove(field);
            assert!(
                review(&binding, &entry, &record, price.clone(), &policy, 20).is_err(),
                "missing {field}"
            );
        }
    }

    #[test]
    fn debit_valuation_rounds_up_preserves_zero_and_refuses_overflow() {
        for (lamports, price, expected) in [
            (0, 1, 0),
            (1, 1, 1),
            (1_000_000_000, 1, 1),
            (1_000_000_001, 1, 2),
            (u64::MAX, 1_000_000_000, u64::MAX),
        ] {
            assert_eq!(
                dollars(lamports, price).expect("valuation"),
                MicroUsd(expected)
            );
        }
        assert!(dollars(u64::MAX, 1_000_000_001).is_err());
        assert!(dollars(u64::MAX, u64::MAX).is_err());
        let (binding, entry, record, price, policy) = fixture();
        let report = review(&binding, &entry, &record, price, &policy, 20).expect("boundary price");
        assert_eq!(report["wallet_net_debit_micro_usd"], "50001001");
        assert_eq!(report["network_fee_micro_usd"], "1001");
        assert_eq!(report["valuation_as_of_slot"], "90");
        assert_eq!(report["execution_slot"], "100");
        let mut boundary = entry;
        boundary.intent.at = radar_types::Slot(100);
        boundary.reserved = Some(radar_types::TokenQuantity::lamports(250_005_000));
        let mut exact = record;
        exact.review["reserved_lamports"] = json!("250005000");
        assert!(
            review(
                &binding,
                &boundary,
                &exact,
                json!({"version":1,"asset":"sol","micro_usd_per_sol":"200000003",
                    "as_of_slot":"90","as_of_unix_secs":"980"}),
                &policy,
                20,
            )
            .is_ok()
        );
        for field in [
            "trade_notional_micro_usd",
            "position_cost_basis_micro_usd",
            "realised_pnl_micro_usd",
        ] {
            assert_eq!(report[field], Value::Null);
        }
        for field in [
            "portfolio_state_updated",
            "operation_reconciled",
            "reservation_released",
        ] {
            assert_eq!(report[field], false);
        }
    }

    #[test]
    fn historical_prices_cannot_be_missing_foreign_future_stale_or_unpriced() {
        let (binding, entry, record, price, policy) = fixture();
        for key in price.as_object().expect("price").keys() {
            let mut bad = price.clone();
            bad.as_object_mut().expect("object").remove(key);
            assert!(review(&binding, &entry, &record, bad, &policy, 20).is_err());
        }
        for (field, value) in [
            ("version", json!(2)),
            ("asset", json!("usdc")),
            ("micro_usd_per_sol", json!("0")),
            ("micro_usd_per_sol", json!(1)),
            ("micro_usd_per_sol", json!("18446744073709551616")),
            ("as_of_slot", json!("89")),
            ("as_of_slot", json!("101")),
            ("as_of_unix_secs", json!("979")),
            ("as_of_unix_secs", json!("1001")),
            ("as_of_unix_secs", json!("unknown")),
            ("provider_body", json!("not accepted")),
        ] {
            let mut bad = price.clone();
            bad[field] = value;
            assert!(
                review(&binding, &entry, &record, bad, &policy, 20).is_err(),
                "bad {field}"
            );
        }
        let mut old = record.clone();
        old.review["slot"] = json!("89");
        let mut price = price;
        price["as_of_slot"] = json!("89");
        assert!(review(&binding, &entry, &old, price, &policy, 20).is_err());
    }

    #[test]
    fn absent_unbound_or_out_of_range_native_effects_do_not_become_zero_dollars() {
        let (binding, entry, record, price, policy) = fixture();
        for (field, value) in [
            ("version", json!(2)),
            ("authority", json!("read_only")),
            ("wallet", json!(Address::SYSTEM_PROGRAM.to_string())),
            ("signature_verified_locally", json!(false)),
            ("reserved_lamports", json!("0")),
            ("block_time_unix_secs", Value::Null),
            ("wallet_net_change_lamports", Value::Null),
            ("wallet_net_change_lamports", json!("1")),
            ("wallet_net_change_lamports", json!(i128::MIN.to_string())),
            ("wallet_net_change_lamports", json!("-18446744073709551616")),
            ("network_fee_lamports", Value::Null),
            ("network_fee_lamports", json!("250005001")),
        ] {
            let mut bad = record.clone();
            bad.review[field] = value;
            assert!(
                review(&binding, &entry, &bad, price.clone(), &policy, 20).is_err(),
                "bad {field}"
            );
        }
        let mut bad = record.clone();
        bad.signed_transaction = "different".into();
        assert!(review(&binding, &entry, &bad, price.clone(), &policy, 20).is_err());
        let mut bad = record.clone();
        bad.review["reserved_lamports"] = json!("5000");
        let mut small = entry;
        small.reserved = Some(TokenQuantity::lamports(5000));
        assert!(review(&binding, &small, &bad, price.clone(), &policy, 20).is_err());
        for mut bad in [entry; 3].into_iter().enumerate() {
            match bad.0 {
                0 => bad.1.reserved = None,
                1 => bad.1.state = OperationState::Reserved,
                _ => bad.1.intent.asset = Asset::Usdc,
            }
            assert!(review(&binding, &bad.1, &record, price.clone(), &policy, 20).is_err());
        }
        let mut wrong_units = entry;
        wrong_units.reserved = Some(TokenQuantity::new(
            u64::MAX,
            Decimals::from_mint_account(6).expect("decimals"),
        ));
        assert!(review(&binding, &wrong_units, &record, price, &policy, 20).is_err());
    }
}
