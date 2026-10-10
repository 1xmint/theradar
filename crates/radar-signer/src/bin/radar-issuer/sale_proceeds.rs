// SPDX-License-Identifier: Apache-2.0
//! Balanced operator sale cash flows, not fill provenance or realised PnL.

use radar_journal::ExecutionBinding;
use radar_risk::{Action, Proposal};
use radar_types::{Address, Asset, Decimals};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Breakdown {
    version: u8,
    operation: String,
    signed_transaction: String,
    wallet: Address,
    mint: Address,
    token_program: Address,
    decimals: u8,
    net_disposed_raw: String,
    gross_proceeds_lamports: String,
    tip_lamports: String,
    rent_paid_lamports: String,
    rent_refund_lamports: String,
    other_cash_flows_absent: bool,
}

fn integer(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_str()
        .and_then(|raw| raw.parse().ok())
        .ok_or_else(|| "invalid retained sale integer".into())
}

fn usd(lamports: u64, price: u64, debit: bool) -> Result<u64, String> {
    let product = u128::from(lamports) * u128::from(price);
    let amount = if debit {
        product.div_ceil(1_000_000_000)
    } else {
        product / 1_000_000_000
    };
    u64::try_from(amount).map_err(|_| "sale valuation exceeds dollar range".into())
}

pub(super) fn review(
    binding: &ExecutionBinding,
    value: &Value,
    input: &Breakdown,
    price: u64,
    reserved: u64,
) -> Result<Value, String> {
    let proposal: Proposal = serde_json::from_value(
        binding
            .reviewed_proposal
            .clone()
            .ok_or("sale lacks reviewed context")?,
    )
    .map_err(|_| "invalid reviewed sale context")?;
    let disposed = &value["wallet_token_disposal"];
    let raw = input
        .net_disposed_raw
        .parse::<u64>()
        .map_err(|_| "invalid disposed units")?;
    if input.version != 1
        || value["outcome"] != "succeeded"
        || !matches!(proposal.action, Action::Reduce | Action::Exit)
        || proposal.quote != Asset::Sol
        || value["operation"] != input.operation
        || binding.signed_transaction.as_ref() != Some(&input.signed_transaction)
        || input.wallet != binding.wallet
        || input.mint != proposal.mint
        || disposed["mint"] != input.mint.to_string()
        || disposed["owner"] != input.wallet.to_string()
        || disposed["program_id"] != input.token_program.to_string()
        || disposed["decimals"] != input.decimals
        || Decimals::from_mint_account(input.decimals).is_none()
        || raw == 0
        || integer(disposed, "net_disposed_raw")? != raw
        || !input.other_cash_flows_absent
    {
        return Err("sale breakdown does not bind complete reviewed disposal".into());
    }
    let parse = |raw: &str| raw.parse::<u64>().map_err(|_| "invalid sale cash flow");
    let gross = parse(&input.gross_proceeds_lamports)?;
    let tip = parse(&input.tip_lamports)?;
    let rent = parse(&input.rent_paid_lamports)?;
    let refund = parse(&input.rent_refund_lamports)?;
    let fee = integer(value, "network_fee_lamports")?;
    let credits = gross
        .checked_add(refund)
        .ok_or("sale credits exceed native range")?;
    let trade_cost = fee
        .checked_add(tip)
        .ok_or("sale costs exceed native range")?;
    let debits = trade_cost
        .checked_add(rent)
        .ok_or("sale debits exceed native range")?;
    let delta = value["wallet_net_change_lamports"]
        .as_str()
        .and_then(|raw| raw.parse::<i128>().ok())
        .ok_or("invalid retained sale wallet effect")?;
    if gross == 0
        || delta != i128::from(credits) - i128::from(debits)
        || fee > reserved
        || delta < -i128::from(reserved)
    {
        return Err("sale breakdown does not account for exact wallet effect".into());
    }
    let gross_usd = usd(gross, price, false)?;
    let fee_usd = usd(fee, price, true)?;
    let tip_usd = usd(tip, price, true)?;
    // Credits round down and costs up; rent is separate from trade proceeds.
    // A balanced operator assertion does not authenticate venue attribution.
    Ok(
        json!({"authority":"protected_operator_sale_breakdown","mint":input.mint,
        "token_program":input.token_program,"decimals":input.decimals,"net_disposed_raw":raw.to_string(),
        "gross_proceeds_lamports":gross.to_string(),"network_fee_lamports":fee.to_string(),"tip_lamports":tip.to_string(),
        "rent_paid_lamports":rent.to_string(),"rent_refund_lamports":refund.to_string(),
        "wallet_net_change_lamports":delta.to_string(),"net_trade_proceeds_lamports":(i128::from(gross)-i128::from(trade_cost)).to_string(),
        "gross_proceeds_micro_usd":gross_usd.to_string(),"network_fee_micro_usd":fee_usd.to_string(),"tip_micro_usd":tip_usd.to_string(),
        "net_trade_proceeds_micro_usd":(i128::from(gross_usd)-i128::from(fee_usd)-i128::from(tip_usd)).to_string(),
        "rent_paid_micro_usd":usd(rent,price,true)?.to_string(),"rent_refund_micro_usd":usd(refund,price,false)?.to_string()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (ExecutionBinding, Value, Value) {
        let wallet = Address::new([1; 32]);
        let mint = Address::new([2; 32]);
        let program = Address::new([3; 32]);
        let binding = ExecutionBinding {
            wallet,
            transaction: "unsigned".into(),
            signed_transaction: Some("signed".into()),
            reviewed_proposal: Some(
                json!({"mint":mint,"creator":wallet,"market":radar_types::Market::PUMP_FUN_BONDING_CURVE,
                "quote":"sol","action":"exit","notional":1,"estimated_round_trip_cost":0,
                "oldest_input_slot":1,"simulated_exit_capacity":1}),
            ),
        };
        let value = json!({"operation":"op","outcome":"succeeded","network_fee_lamports":"5",
            "wallet_net_change_lamports":"90","wallet_token_disposal":{
            "mint":mint,"owner":wallet,"program_id":program,"decimals":6,"net_disposed_raw":"10"}});
        let input = json!({"version":1,"operation":"op","signed_transaction":"signed","wallet":wallet,
            "mint":mint,"token_program":program,"decimals":6,"net_disposed_raw":"10","gross_proceeds_lamports":"100",
            "tip_lamports":"5","rent_paid_lamports":"0","rent_refund_lamports":"0","other_cash_flows_absent":true});
        (binding, value, input)
    }

    #[test]
    fn sale_native_sides_are_bounded_even_when_overflow_would_cancel() {
        let (binding, value, input) = fixture();
        for (gross, tip, rent, refund, valid) in [
            (u64::MAX, 0, u64::MAX - 5, 0, true),
            (u64::MAX, 0, u64::MAX - 5, 1, false),
            (u64::MAX, u64::MAX, 0, 0, false),
            (u64::MAX, 5, u64::MAX, 0, false),
        ] {
            let mut adjusted = input.clone();
            for (field, amount) in [
                ("gross_proceeds_lamports", gross),
                ("tip_lamports", tip),
                ("rent_paid_lamports", rent),
                ("rent_refund_lamports", refund),
            ] {
                adjusted[field] = json!(amount.to_string());
            }
            let adjusted: Breakdown = serde_json::from_value(adjusted).expect("range");
            let mut changed = value.clone();
            changed["wallet_net_change_lamports"] = json!("0");
            assert_eq!(
                review(&binding, &changed, &adjusted, 1, u64::MAX).is_ok(),
                valid,
                "range {gross}/{tip}/{rent}/{refund}"
            );
        }
    }

    #[test]
    fn sale_requires_successful_reviewed_exit_context_and_exact_disposed_units() {
        let (mut binding, value, input) = fixture();
        let wallet = binding.wallet;
        let mint = &value["wallet_token_disposal"]["mint"];
        let breakdown: Breakdown = serde_json::from_value(input.clone()).expect("breakdown");
        assert!(review(&binding, &value, &breakdown, 1, u64::MAX).is_ok());
        for decimals in [0, 18, u8::MAX] {
            let mut changed = value.clone();
            changed["wallet_token_disposal"]["decimals"] = json!(decimals);
            let mut adjusted = input.clone();
            adjusted["decimals"] = json!(decimals);
            let adjusted: Breakdown = serde_json::from_value(adjusted).expect("units");
            assert_eq!(
                review(&binding, &changed, &adjusted, 1, u64::MAX).is_ok(),
                decimals != u8::MAX
            );
        }
        for zero in ["net_disposed_raw", "gross_proceeds_lamports"] {
            let mut changed = value.clone();
            let mut adjusted = input.clone();
            adjusted[zero] = json!("0");
            if zero == "net_disposed_raw" {
                changed["wallet_token_disposal"][zero] = json!("0");
            } else {
                changed["wallet_net_change_lamports"] = json!("-10");
            }
            let adjusted: Breakdown = serde_json::from_value(adjusted).expect("zero");
            assert!(review(&binding, &changed, &adjusted, 1, u64::MAX).is_err());
        }
        for (field, bad) in [
            ("action", json!("buy")),
            ("quote", json!("usdc")),
            ("mint", json!(wallet)),
        ] {
            let mut changed = binding.clone();
            changed.reviewed_proposal.as_mut().expect("proposal")[field] = bad;
            assert!(
                review(&changed, &value, &breakdown, 1, u64::MAX).is_err(),
                "context {field}"
            );
        }
        for context in [None, Some(Value::Null), Some(json!({}))] {
            let mut changed = binding.clone();
            changed.reviewed_proposal = context;
            assert!(review(&changed, &value, &breakdown, 1, u64::MAX).is_err());
        }
        for (field, bad) in [
            ("outcome", json!("failed")),
            ("wallet_token_disposal", Value::Null),
            ("wallet_net_change_lamports", json!("unknown")),
            ("wallet_net_change_lamports", json!("91")),
        ] {
            let mut changed = value.clone();
            changed[field] = bad;
            assert!(
                review(&binding, &changed, &breakdown, 1, u64::MAX).is_err(),
                "effect {field}"
            );
        }
        for (field, bad) in [
            ("mint", json!(wallet)),
            ("owner", json!(mint)),
            ("program_id", json!(wallet)),
            ("decimals", json!(9)),
            ("net_disposed_raw", json!("9")),
            ("net_disposed_raw", json!("unknown")),
        ] {
            let mut changed = value.clone();
            changed["wallet_token_disposal"][field] = bad;
            assert!(
                review(&binding, &changed, &breakdown, 1, u64::MAX).is_err(),
                "disposal {field}"
            );
        }
        binding.signed_transaction = None;
        assert!(review(&binding, &value, &breakdown, 1, u64::MAX).is_err());
    }

    #[test]
    fn credits_round_down_costs_round_up_and_overflow_never_wraps() {
        assert_eq!(usd(1, 1, false), Ok(0));
        assert_eq!(usd(1, 1, true), Ok(1));
        assert_eq!(usd(1_000_000_000, 123, false), Ok(123));
        assert_eq!(usd(1_000_000_000, 123, true), Ok(123));
        assert_eq!(usd(0, u64::MAX, true), Ok(0));
        for debit in [false, true] {
            assert!(usd(u64::MAX, u64::MAX, debit).is_err());
        }
    }

    #[test]
    fn retained_sale_fee_and_net_debit_must_fit_the_reservation() {
        let (binding, mut value, mut input) = fixture();
        let breakdown: Breakdown = serde_json::from_value(input.clone()).expect("input");
        assert!(review(&binding, &value, &breakdown, 1, 5).is_ok());
        assert!(review(&binding, &value, &breakdown, 1, 4).is_err());
        input["gross_proceeds_lamports"] = json!("1");
        value["wallet_net_change_lamports"] = json!("-9");
        let breakdown: Breakdown = serde_json::from_value(input).expect("input");
        assert!(review(&binding, &value, &breakdown, 1, 9).is_ok());
        assert!(review(&binding, &value, &breakdown, 1, 8).is_err());
    }
}
