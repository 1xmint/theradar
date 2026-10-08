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
    let effects: Vec<_> = accounts.iter().zip(pre.iter().zip(&post)).map(|(account, (pre, post))|
        json!({"account":account.to_string(),"pre_lamports":pre.to_string(),"post_lamports":post.to_string(),
            "net_change_lamports":(i128::from(*post)-i128::from(*pre)).to_string()})).collect();
    Ok(
        json!({"version":1,"authority":"protected_file_review","outcome":outcome,
        "wallet":binding.wallet.to_string(),"signature":Signature::new(signature).to_string(),
        "slot":slot.to_string(),"native_account_effects":effects,"wallet_net_change_lamports":delta.to_string(),
        "network_fee_lamports":fee.to_string(),"reserved_lamports":reserved.raw().to_string(),
        "pre_token_balances":before,"post_token_balances":after,"usd_value":null,"realised_pnl":null,
        "signature_verified_locally":true,"operation_reconciled":false,"reservation_released":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_journal::Intent;
    use radar_types::{Slot, TokenQuantity};

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
