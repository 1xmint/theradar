// SPDX-License-Identifier: Apache-2.0
//! Prices retained native cash effects, not fills, cost basis or realised PnL.

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
    Ok(
        json!({"version":1,"authority":"protected_operator_valuation",
        "wallet":binding.wallet.to_string(),"execution_slot":slot.to_string(),
        "execution_at_unix_secs":time.to_string(),"valuation_as_of_slot":price_slot.to_string(),
        "price_at_unix_secs":price_time.to_string(),"micro_usd_per_sol":amount.to_string(),
        "wallet_net_debit_lamports":debit.to_string(),"wallet_net_debit_micro_usd":cash.get().to_string(),
        "network_fee_lamports":fee.to_string(),"network_fee_micro_usd":fee_usd.get().to_string(),
        "trade_notional_micro_usd":null,"position_cost_basis_micro_usd":null,"realised_pnl_micro_usd":null,
        "portfolio_state_updated":false,"operation_reconciled":false,"reservation_released":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_journal::Intent;
    use radar_types::{Address, Slot, SlotDelta, TokenQuantity};

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
