// SPDX-License-Identifier: Apache-2.0
//! Protected operator review of historical effects. Never releases capital.

use radar_journal::{ExecutionBinding, OperationEntry, OperationState};
use radar_types::{Address, Asset, Signature, b64};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TokenBalance {
    account_index: usize,
    mint: Address,
    owner: Address,
    program_id: Address,
    raw_amount: String,
    decimals: u8,
}

fn integer(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "invalid settlement integer".into())
}

fn tokens(value: &Value, field: &str, count: usize) -> Result<Vec<TokenBalance>, String> {
    let entries: Vec<TokenBalance> = serde_json::from_value(value[field].clone())
        .map_err(|_| "invalid settlement token metadata")?;
    let mut indices = std::collections::BTreeSet::new();
    for entry in &entries {
        if entry.account_index >= count
            || !indices.insert(entry.account_index)
            || entry.raw_amount.parse::<u64>().is_err()
        {
            return Err("invalid settlement token index or quantity".into());
        }
    }
    Ok(entries)
}

pub(super) fn unchanged_tokens(value: &Value, count: usize) -> Result<bool, String> {
    let mut before = tokens(value, "pre_token_balances", count)?;
    let mut after = tokens(value, "post_token_balances", count)?;
    before.sort_by_key(|entry| entry.account_index);
    after.sort_by_key(|entry| entry.account_index);
    Ok(before.len() == after.len()
        && before.iter().zip(&after).all(|(pre, post)| {
            pre.account_index == post.account_index
                && pre.mint == post.mint
                && pre.owner == post.owner
                && pre.program_id == post.program_id
                && pre.decimals == post.decimals
                && pre.raw_amount.parse::<u64>().ok() == post.raw_amount.parse::<u64>().ok()
        }))
}

fn acquisition(
    binding: &ExecutionBinding,
    outcome: &str,
    before: &[TokenBalance],
    after: &[TokenBalance],
) -> Option<Value> {
    let proposal: radar_risk::Proposal =
        serde_json::from_value(binding.reviewed_proposal.clone()?).ok()?;
    if outcome != "succeeded"
        || proposal.action != radar_risk::Action::Buy
        || proposal.quote != Asset::Sol
    {
        return None;
    }
    let (program, decimals, pre_total, post_total) =
        paired_totals(binding, &proposal, before, after)?;
    let acquired = post_total
        .checked_sub(pre_total)
        .filter(|amount| *amount > 0)?;
    // Net acquisition is not gross venue fill, price, cost basis or attribution
    // of each transfer. Token-2022 extensions are not interpreted here.
    Some(
        json!({"mint":proposal.mint,"owner":binding.wallet,"program_id":program,
        "decimals":decimals,"pre_raw_amount":pre_total.to_string(),
        "post_raw_amount":post_total.to_string(),"net_acquired_raw":acquired.to_string()}),
    )
}

fn disposal(
    binding: &ExecutionBinding,
    outcome: &str,
    before: &[TokenBalance],
    after: &[TokenBalance],
) -> Option<Value> {
    let proposal: radar_risk::Proposal =
        serde_json::from_value(binding.reviewed_proposal.clone()?).ok()?;
    if outcome != "succeeded"
        || !matches!(
            proposal.action,
            radar_risk::Action::Reduce | radar_risk::Action::Exit
        )
        || proposal.quote != Asset::Sol
    {
        return None;
    }
    let (program, decimals, pre_total, post_total) =
        paired_totals(binding, &proposal, before, after)?;
    let disposed = pre_total
        .checked_sub(post_total)
        .filter(|amount| *amount > 0)?;
    // A net decrease is not proof of venue fill, sale proceeds or realised PnL.
    Some(
        json!({"mint":proposal.mint,"owner":binding.wallet,"program_id":program,
        "decimals":decimals,"pre_raw_amount":pre_total.to_string(),
        "post_raw_amount":post_total.to_string(),"net_disposed_raw":disposed.to_string()}),
    )
}

fn paired_totals(
    binding: &ExecutionBinding,
    proposal: &radar_risk::Proposal,
    before: &[TokenBalance],
    after: &[TokenBalance],
) -> Option<(Address, u8, u64, u64)> {
    let owned =
        |balance: &&TokenBalance| balance.mint == proposal.mint && balance.owner == binding.wallet;
    let indices: std::collections::BTreeSet<_> = before
        .iter()
        .chain(after)
        .filter(owned)
        .map(|balance| balance.account_index)
        .collect();
    // Missing one side is not a measured zero, including newly created ATAs.
    let mut identity = None;
    let (mut pre_total, mut post_total) = (0_u64, 0_u64);
    for index in indices {
        let pre = before
            .iter()
            .find(|balance| balance.account_index == index)?;
        let post = after
            .iter()
            .find(|balance| balance.account_index == index)?;
        if !owned(&pre)
            || !owned(&post)
            || pre.program_id != post.program_id
            || pre.decimals != post.decimals
            || radar_types::Decimals::from_mint_account(pre.decimals).is_none()
        {
            return None;
        }
        let units = (pre.program_id, pre.decimals);
        if identity.is_some_and(|previous| previous != units) {
            return None;
        }
        identity = Some(units);
        pre_total = pre_total.checked_add(pre.raw_amount.parse::<u64>().ok()?)?;
        post_total = post_total.checked_add(post.raw_amount.parse::<u64>().ok()?)?;
    }
    let (program, decimals) = identity?;
    Some((program, decimals, pre_total, post_total))
}

fn native_settlement(delta: i128, fee: u64) -> Option<radar_types::Settlement> {
    // A credit does not measure gross spend; a debit must cover the known fee.
    // A candidate does not reconcile USD/exposure/loss or release the claim.
    u64::try_from(-delta)
        .ok()
        .filter(|spent| *spent >= fee)
        .map(|spent| {
            radar_types::Settlement::Completed(radar_types::TokenQuantity::lamports(spent))
        })
}

fn native_effects(accounts: &[Address], pre: &[u64], post: &[u64]) -> Vec<Value> {
    accounts.iter().zip(pre.iter().zip(post)).map(|(account, (pre, post))|
        json!({"account":account.to_string(),"pre_lamports":pre.to_string(),"post_lamports":post.to_string(),
            "net_change_lamports":(i128::from(*post)-i128::from(*pre)).to_string()})).collect()
}

fn block_time(value: &Value, completed: u64) -> Result<Option<u64>, String> {
    match value.get("block_time_unix_secs") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|time| *time <= completed)
            .map(Some)
            .ok_or_else(|| "invalid or future settlement block time".into()),
    }
}

pub(super) fn review(
    binding: &ExecutionBinding,
    entry: &OperationEntry,
    value: &Value,
    now: u64,
    max_age: u64,
) -> Result<Value, String> {
    let signed = b64::decode(
        binding
            .signed_transaction
            .as_deref()
            .ok_or("operation is not bound to signed bytes")?,
    )
    .ok_or("invalid signed binding")?;
    // The issuer re-verifies the wallet signature before calling this review.
    let message = radar_signer::tx::decode(&signed).map_err(|_| "invalid signed binding")?;
    let signature: [u8; 64] = signed
        .get(1..65)
        .and_then(|s| s.try_into().ok())
        .ok_or("invalid signature extent")?;
    if entry.state != OperationState::SubmissionUnknown
        || entry.intent.asset != Asset::Sol
        || value["version"] != 1
        || value["authority"] != "read_only"
        || value["commitment"] != "finalized"
        || value["wallet"] != binding.wallet.to_string()
        || value["transaction_base64"] != b64::encode(&signed)
        || value["signature"] != Signature::new(signature).to_string()
        || value.get("signature_verified_locally") != Some(&json!(false))
        || value.get("operation_reconciled") != Some(&json!(false))
        || value.get("usd_value") != Some(&Value::Null)
        || value.get("realised_pnl") != Some(&Value::Null)
    {
        return Err("settlement evidence does not bind the outstanding operation".into());
    }
    let outcome = value["outcome"]
        .as_str()
        .filter(|s| matches!(*s, "succeeded" | "failed"))
        .ok_or("unknown settlement execution outcome")?;
    let slot = integer(value, "slot")?;
    let minimum = integer(value, "minimum_slot")?;
    let started = value["read_started_at_unix_secs"]
        .as_u64()
        .ok_or("missing settlement start time")?;
    let completed = value["read_completed_at_unix_secs"]
        .as_u64()
        .ok_or("missing settlement completion time")?;
    if minimum < entry.intent.at.get()
        || slot < minimum
        || completed < started
        || completed > now
        || !super::snapshot_current(now, started, max_age)
    {
        return Err("settlement context or read window is outside protected bounds".into());
    }
    let accounts: Vec<Address> = serde_json::from_value(value["account_keys"].clone())
        .map_err(|_| "invalid settlement account keys")?;
    if accounts.iter().map(|a| *a.as_bytes()).collect::<Vec<_>>() != message.accounts {
        return Err("settlement accounts do not match signed message".into());
    }
    let balances = |field: &str| -> Result<Vec<u64>, String> {
        let values = value[field]
            .as_array()
            .filter(|v| v.len() == accounts.len())
            .ok_or("settlement balance extent differs from message")?;
        values
            .iter()
            .map(|v| {
                v.as_str()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| "invalid settlement native balance".into())
            })
            .collect()
    };
    let pre = balances("pre_balances_lamports")?;
    let post = balances("post_balances_lamports")?;
    let before = tokens(value, "pre_token_balances", accounts.len())?;
    let after = tokens(value, "post_token_balances", accounts.len())?;
    let fee = integer(value, "network_fee_lamports")?;
    let reserved = entry
        .reserved
        .ok_or("operation has no capital reservation")?;
    let delta = i128::from(*post.first().ok_or("missing wallet account")?)
        - i128::from(*pre.first().ok_or("missing wallet account")?);
    if reserved.decimals() != radar_types::Decimals::NATIVE_SOL
        || fee > reserved.raw()
        || -delta > i128::from(reserved.raw())
    {
        return Err("settlement native effects exceed the recorded reservation".into());
    }
    let effects = native_effects(&accounts, &pre, &post);
    let native_settlement = native_settlement(delta, fee);
    let block_time = block_time(value, completed)?;
    let acquisition = acquisition(binding, outcome, &before, &after);
    let mut report = json!({"version":1,"authority":"protected_file_review","outcome":outcome,
        "wallet":binding.wallet.to_string(),"signature":Signature::new(signature).to_string(),
        "slot":slot.to_string(),"native_account_effects":effects,"wallet_net_change_lamports":delta.to_string(),
        "minimum_slot":minimum.to_string(),"read_started_at_unix_secs":started,"read_completed_at_unix_secs":completed,
        "block_time_unix_secs":block_time.map(|time|time.to_string()),
        "network_fee_lamports":fee.to_string(),"reserved_lamports":reserved.raw().to_string(),
        "pre_token_balances":before,"post_token_balances":after,"wallet_token_acquisition":acquisition,
        "usd_value":null,"realised_pnl":null,
        "native_settlement_candidate":native_settlement,
        "signature_verified_locally":true,"operation_reconciled":false,"reservation_released":false});
    // Preserve the exact review shape of historical buy/failed/unknown records.
    if let Some(disposal) = disposal(binding, outcome, &before, &after) {
        report["wallet_token_disposal"] = disposal;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_journal::Intent;
    use radar_types::{Slot, TokenQuantity};

    fn acquisition_fixture() -> ExecutionBinding {
        let (mut binding, _, _) = fixture();
        binding.reviewed_proposal = Some(json!({"mint":Address::new([2;32]),
            "market":radar_types::Market::PUMP_FUN_BONDING_CURVE,"quote":"sol",
            "creator":Address::new([4;32]),"action":"buy","notional":1,
            "estimated_round_trip_cost":0,"oldest_input_slot":50,"simulated_exit_capacity":1}));
        binding
    }

    fn token(binding: &ExecutionBinding, index: usize, amount: u64) -> TokenBalance {
        TokenBalance {
            account_index: index,
            mint: Address::new([2; 32]),
            owner: binding.wallet,
            program_id: Address::new([3; 32]),
            raw_amount: amount.to_string(),
            decimals: 6,
        }
    }

    #[test]
    fn disposal_measures_only_successful_reviewed_sol_exits_or_reductions() {
        let mut binding = acquisition_fixture();
        let before = [token(&binding, 0, 10), token(&binding, 1, 20)];
        let after = [token(&binding, 1, 25), token(&binding, 0, 0)];
        for action in ["reduce", "exit"] {
            binding.reviewed_proposal.as_mut().expect("proposal")["action"] = json!(action);
            let result = disposal(&binding, "succeeded", &before, &after).expect("disposal");
            assert_eq!(result["net_disposed_raw"], "5");
            assert_eq!(result["pre_raw_amount"], "30");
            assert_eq!(result["post_raw_amount"], "25");
            assert!(disposal(&binding, "failed", &before, &after).is_none());
            assert!(disposal(&binding, "succeeded", &before, &before).is_none());
            assert!(disposal(&binding, "succeeded", &after, &before).is_none());
            assert!(disposal(&binding, "succeeded", &before, &[]).is_none());
            assert!(disposal(&binding, "succeeded", &[], &after).is_none());
            assert_eq!(
                disposal(
                    &binding,
                    "succeeded",
                    &[token(&binding, 0, u64::MAX)],
                    &[token(&binding, 0, 0)]
                )
                .expect("full range")["net_disposed_raw"],
                u64::MAX.to_string()
            );
        }
        for (field, value) in [
            ("action", json!("buy")),
            ("quote", json!("usdc")),
            ("mint", json!(Address::SYSTEM_PROGRAM)),
        ] {
            let mut bad = binding.clone();
            bad.reviewed_proposal.as_mut().expect("proposal")[field] = value;
            assert!(disposal(&bad, "succeeded", &before, &after).is_none());
        }
        binding.reviewed_proposal = None;
        assert!(disposal(&binding, "succeeded", &before, &after).is_none());
    }

    #[test]
    fn settlement_disposal_is_measured_without_inventing_economics_or_changing_old_shape() {
        let (mut binding, entry, mut value) = fixture();
        binding.reviewed_proposal = acquisition_fixture().reviewed_proposal;
        value["pre_token_balances"] = json!([token(&binding, 0, 10)]);
        value["post_token_balances"] = json!([token(&binding, 0, 4)]);
        assert!(
            review(&binding, &entry, &value, 100, 20)
                .expect("buy review")
                .get("wallet_token_disposal")
                .is_none()
        );
        for action in ["reduce", "exit"] {
            binding.reviewed_proposal.as_mut().expect("proposal")["action"] = json!(action);
            let report = review(&binding, &entry, &value, 100, 20).expect("review");
            assert_eq!(report["wallet_token_disposal"]["net_disposed_raw"], "6");
            assert_eq!(report["wallet_token_acquisition"], Value::Null);
            assert_eq!(report["usd_value"], Value::Null);
            assert_eq!(report["realised_pnl"], Value::Null);
            assert_eq!(report["operation_reconciled"], false);
            assert_eq!(report["reservation_released"], false);
        }
        value["outcome"] = json!("failed");
        assert!(
            review(&binding, &entry, &value, 100, 20)
                .expect("failed review")
                .get("wallet_token_disposal")
                .is_none()
        );
    }

    #[test]
    fn net_acquisition_aggregates_paired_accounts_without_counting_internal_transfers() {
        let binding = acquisition_fixture();
        let before = [token(&binding, 0, 10), token(&binding, 1, 20)];
        let after = [token(&binding, 1, 30), token(&binding, 0, 5)];
        let result = acquisition(&binding, "succeeded", &before, &after).expect("net acquisition");
        assert_eq!(result["pre_raw_amount"], "30");
        assert_eq!(result["post_raw_amount"], "35");
        assert_eq!(result["net_acquired_raw"], "5");
        assert_eq!(result["decimals"], 6);
        assert_eq!(result["mint"], Address::new([2; 32]).to_string());
        assert!(acquisition(&binding, "succeeded", &before, &before).is_none());
        assert!(acquisition(&binding, "succeeded", &after, &before).is_none());
        assert!(acquisition(&binding, "failed", &before, &after).is_none());
        let result = acquisition(
            &binding,
            "succeeded",
            &[token(&binding, 0, 0)],
            &[token(&binding, 0, u64::MAX)],
        )
        .expect("full range measured units");
        assert_eq!(result["net_acquired_raw"], u64::MAX.to_string());
    }

    #[test]
    fn missing_or_changed_token_identity_never_supplies_zero_acquisition() {
        let binding = acquisition_fixture();
        let before = [token(&binding, 0, 0)];
        let after = [token(&binding, 0, 1)];
        assert!(acquisition(&binding, "succeeded", &[], &after).is_none());
        assert!(acquisition(&binding, "succeeded", &before, &[]).is_none());
        assert!(acquisition(&binding, "succeeded", &[], &[]).is_none());
        for side in [false, true] {
            for field in [
                "owner",
                "mint",
                "program",
                "decimals",
                "invalid_decimals",
                "index",
                "amount",
            ] {
                let mut changed = token(&binding, 0, u64::from(side));
                match field {
                    "owner" => changed.owner = Address::SYSTEM_PROGRAM,
                    "mint" => changed.mint = Address::SYSTEM_PROGRAM,
                    "program" => changed.program_id = Address::SYSTEM_PROGRAM,
                    "decimals" => changed.decimals = 9,
                    "invalid_decimals" => changed.decimals = u8::MAX,
                    "index" => changed.account_index = 1,
                    _ => changed.raw_amount = "unknown".into(),
                }
                let result = if side {
                    acquisition(&binding, "succeeded", &before, &[changed])
                } else {
                    acquisition(&binding, "succeeded", &[changed], &after)
                };
                assert!(result.is_none(), "changed {field}, after={side}");
            }
        }
        let partial = [token(&binding, 0, 1), token(&binding, 1, 1)];
        assert!(acquisition(&binding, "succeeded", &before, &partial).is_none());
        for decimals in [0, 18, u8::MAX] {
            let mut pre = token(&binding, 0, 0);
            let mut post = token(&binding, 0, 1);
            pre.decimals = decimals;
            post.decimals = decimals;
            assert_eq!(
                acquisition(&binding, "succeeded", &[pre], &[post]).is_some(),
                decimals != u8::MAX
            );
        }
        let mut foreign = token(&binding, 1, u64::MAX);
        foreign.owner = Address::SYSTEM_PROGRAM;
        assert!(
            acquisition(
                &binding,
                "succeeded",
                &before,
                &[token(&binding, 0, 1), foreign]
            )
            .is_some()
        );
    }

    #[test]
    fn acquisition_requires_reviewed_buy_context_consistent_units_and_bounded_totals() {
        let binding = acquisition_fixture();
        let before = [token(&binding, 0, 0), token(&binding, 1, 0)];
        let after = [token(&binding, 0, u64::MAX), token(&binding, 1, 1)];
        assert!(acquisition(&binding, "succeeded", &before, &after).is_none());
        assert!(acquisition(&binding, "succeeded", &after, &after).is_none());
        for program in [false, true] {
            let mut mixed_before = token(&binding, 1, 0);
            let mut mixed_after = token(&binding, 1, 1);
            if program {
                mixed_before.program_id = Address::SYSTEM_PROGRAM;
                mixed_after.program_id = Address::SYSTEM_PROGRAM;
            } else {
                mixed_before.decimals = 9;
                mixed_after.decimals = 9;
            }
            assert!(
                acquisition(
                    &binding,
                    "succeeded",
                    &[token(&binding, 0, 0), mixed_before],
                    &[token(&binding, 0, 1), mixed_after]
                )
                .is_none()
            );
        }
        for context in [None, Some(json!({})), Some(Value::Null)] {
            let mut bad = acquisition_fixture();
            bad.reviewed_proposal = context;
            assert!(
                acquisition(&bad, "succeeded", &before[..1], &[token(&binding, 0, 1)]).is_none()
            );
        }
        for (field, value) in [
            ("action", json!("exit")),
            ("action", json!("reduce")),
            ("quote", json!("usdc")),
            ("mint", json!(Address::SYSTEM_PROGRAM)),
        ] {
            let mut bad = acquisition_fixture();
            bad.reviewed_proposal.as_mut().expect("proposal")[field] = value;
            assert!(
                acquisition(&bad, "succeeded", &before[..1], &[token(&binding, 0, 1)]).is_none()
            );
        }
    }

    fn fixture() -> (ExecutionBinding, OperationEntry, Value) {
        let wallet = Address::new([0x55; 32]);
        let mut signed = vec![1];
        signed.extend_from_slice(&[0xAB; 64]);
        signed.extend_from_slice(&[1, 0, 0, 1]);
        signed.extend_from_slice(wallet.as_bytes());
        signed.extend_from_slice(&[0xAA; 32]);
        signed.push(0);
        let binding = ExecutionBinding {
            wallet,
            transaction: b64::encode(&signed),
            signed_transaction: Some(b64::encode(&signed)),
            reviewed_proposal: None,
        };
        let entry = OperationEntry {
            intent: Intent {
                asset: Asset::Sol,
                amount: TokenQuantity::lamports(u64::MAX),
                at: Slot(50),
            },
            reserved: Some(TokenQuantity::lamports(u64::MAX)),
            state: OperationState::SubmissionUnknown,
        };
        let value = json!({"version":1,"authority":"read_only","commitment":"finalized","wallet":wallet.to_string(),
            "transaction_base64":b64::encode(&signed),"signature":Signature::new([0xAB;64]).to_string(),
            "signature_verified_locally":false,"operation_reconciled":false,"usd_value":null,"realised_pnl":null,
            "outcome":"succeeded","slot":"50","minimum_slot":"50","read_started_at_unix_secs":80,"read_completed_at_unix_secs":100,
            "account_keys":[wallet.to_string()],"pre_balances_lamports":[u64::MAX.to_string()],"post_balances_lamports":["0"],
            "network_fee_lamports":"5000","pre_token_balances":[],"post_token_balances":[]});
        (binding, entry, value)
    }

    #[test]
    fn execution_time_is_retained_or_unknown_and_never_replaced_by_read_time() {
        let (binding, entry, mut value) = fixture();
        for time in [
            None,
            Some(Value::Null),
            Some(json!("0")),
            Some(json!("100")),
        ] {
            value
                .as_object_mut()
                .expect("object")
                .remove("block_time_unix_secs");
            if let Some(time) = time {
                value["block_time_unix_secs"] = time;
            }
            let report = review(&binding, &entry, &value, 100, 20).expect("review");
            assert_eq!(
                report["block_time_unix_secs"],
                value
                    .get("block_time_unix_secs")
                    .cloned()
                    .unwrap_or(Value::Null)
            );
            assert_eq!(report["usd_value"], Value::Null);
            assert_eq!(report["realised_pnl"], Value::Null);
        }
        for bad in [
            json!(100),
            json!(true),
            json!({}),
            json!("-1"),
            json!("101"),
            json!("unknown"),
            json!("18446744073709551616"),
        ] {
            value["block_time_unix_secs"] = bad;
            assert!(review(&binding, &entry, &value, 100, 20).is_err());
        }
        value["block_time_unix_secs"] = json!("00090");
        assert_eq!(
            review(&binding, &entry, &value, 100, 20).expect("canonical time")["block_time_unix_secs"],
            "90"
        );
    }

    #[test]
    fn completed_native_candidates_require_a_measured_debit_covering_the_known_fee() {
        let (binding, entry, mut value) = fixture();
        for (pre, post, fee, candidate) in [
            (5_000, 0, 5_000, Some(5_000)),
            (5_001, 0, 5_000, Some(5_001)),
            (4_999, 0, 5_000, None),
            (0, 0, 0, Some(0)),
            (0, 1, 0, None),
        ] {
            value["pre_balances_lamports"] = json!([pre.to_string()]);
            value["post_balances_lamports"] = json!([post.to_string()]);
            value["network_fee_lamports"] = json!(fee.to_string());
            let report = review(&binding, &entry, &value, 100, 20).expect("review");
            assert_eq!(
                report["native_settlement_candidate"],
                json!(candidate.map(|spent| radar_types::Settlement::Completed(
                    radar_types::TokenQuantity::lamports(spent)
                )))
            );
            assert_eq!(report["reservation_released"], false);
        }
    }

    #[test]
    fn historical_effects_preserve_signed_integer_extremes_and_never_release_claims() {
        let (binding, entry, mut value) = fixture();
        for outcome in ["succeeded", "failed"] {
            value["outcome"] = json!(outcome);
            let report = review(&binding, &entry, &value, 100, 20).expect("review");
            assert_eq!(
                report["wallet_net_change_lamports"],
                (-i128::from(u64::MAX)).to_string()
            );
            assert_eq!(
                report["native_account_effects"][0]["pre_lamports"],
                u64::MAX.to_string()
            );
            assert_eq!(
                report["native_account_effects"][0]["net_change_lamports"],
                (-i128::from(u64::MAX)).to_string()
            );
            assert_eq!(report["network_fee_lamports"], "5000");
            assert_eq!(report["outcome"], outcome);
            assert_eq!(report["reservation_released"], false);
            assert_eq!(report["operation_reconciled"], false);
            assert!(report["usd_value"].is_null());
            assert!(report["realised_pnl"].is_null());
        }
        value["pre_balances_lamports"] = json!(["0"]);
        value["post_balances_lamports"] = json!([u64::MAX.to_string()]);
        value["network_fee_lamports"] = json!("0");
        let report = review(&binding, &entry, &value, 100, 20).expect("historical credit");
        assert_eq!(report["wallet_net_change_lamports"], u64::MAX.to_string());
        assert_eq!(report["network_fee_lamports"], "0");
        let token = json!({"account_index":0,"mint":Address::SYSTEM_PROGRAM.to_string(),"owner":binding.wallet.to_string(),
            "program_id":Address::new([3;32]).to_string(),"raw_amount":u64::MAX.to_string(),"decimals":9});
        value["post_token_balances"] = json!([token]);
        let report = review(&binding, &entry, &value, 100, 20).expect("separate token metadata");
        assert_eq!(report["pre_token_balances"], json!([]));
        assert_eq!(report["post_token_balances"], value["post_token_balances"]);
        for bad in [
            json!([token.clone(), token.clone()]),
            {
                let mut bad = token.clone();
                bad["account_index"] = json!(1);
                json!([bad])
            },
            {
                let mut bad = token.clone();
                bad["raw_amount"] = json!("18446744073709551616");
                json!([bad])
            },
        ] {
            value["post_token_balances"] = bad;
            assert!(review(&binding, &entry, &value, 100, 20).is_err());
        }
        value["post_token_balances"] = json!([]);
        value["pre_balances_lamports"] = json!(["5000"]);
        value["post_balances_lamports"] = json!(["0"]);
        value["network_fee_lamports"] = json!("5000");
        let mut exact = entry;
        exact.reserved = Some(TokenQuantity::lamports(5000));
        assert!(review(&binding, &exact, &value, 100, 20).is_ok());
    }

    #[test]
    fn absent_ambiguous_stale_or_foreign_evidence_never_becomes_a_review() {
        let (binding, entry, value) = fixture();
        for key in value.as_object().expect("object").keys() {
            let mut missing = value.clone();
            missing.as_object_mut().expect("object").remove(key);
            assert!(
                review(&binding, &entry, &missing, 100, 20).is_err(),
                "missing {key}"
            );
        }
        for (key, bad) in [
            ("version", json!(2)),
            ("authority", json!("model")),
            ("commitment", json!("processed")),
            ("wallet", json!(Address::SYSTEM_PROGRAM.to_string())),
            ("transaction_base64", json!("bad")),
            ("signature", json!("bad")),
            ("signature_verified_locally", json!(true)),
            ("operation_reconciled", json!(true)),
            ("usd_value", json!(0)),
            ("realised_pnl", json!(0)),
            ("outcome", json!("unknown")),
            ("slot", json!("49")),
            ("minimum_slot", json!("49")),
            ("read_started_at_unix_secs", json!(79)),
            ("read_started_at_unix_secs", json!(101)),
            ("read_completed_at_unix_secs", json!(79)),
            ("read_completed_at_unix_secs", json!(101)),
            ("account_keys", json!([Address::SYSTEM_PROGRAM.to_string()])),
            ("pre_balances_lamports", json!([])),
            ("post_balances_lamports", json!(["0", "0"])),
            ("pre_balances_lamports", json!([0])),
            ("post_balances_lamports", json!(["18446744073709551616"])),
            ("network_fee_lamports", json!(5000)),
            ("pre_token_balances", Value::Null),
            ("post_token_balances", json!([{}])),
        ] {
            let mut bad_value = value.clone();
            bad_value[key] = bad;
            assert!(
                review(&binding, &entry, &bad_value, 100, 20).is_err(),
                "bad {key}"
            );
        }
        let mut wrong = entry;
        wrong.state = OperationState::Reserved;
        assert!(review(&binding, &wrong, &value, 100, 20).is_err());
        wrong = entry;
        wrong.intent.asset = Asset::Usdc;
        assert!(review(&binding, &wrong, &value, 100, 20).is_err());
        wrong = entry;
        wrong.reserved = None;
        assert!(review(&binding, &wrong, &value, 100, 20).is_err());
        wrong = entry;
        wrong.reserved = Some(TokenQuantity::new(
            u64::MAX,
            radar_types::Decimals::from_mint_account(6).expect("fixture decimals"),
        ));
        assert!(review(&binding, &wrong, &value, 100, 20).is_err());
        wrong = entry;
        wrong.reserved = Some(TokenQuantity::lamports(4999));
        assert!(review(&binding, &wrong, &value, 100, 20).is_err());
        let mut low_debit = value.clone();
        low_debit["network_fee_lamports"] = json!("0");
        assert!(review(&binding, &wrong, &low_debit, 100, 20).is_err());
        let mut missing = binding;
        missing.signed_transaction = None;
        assert!(review(&missing, &entry, &value, 100, 20).is_err());
    }
}
