// SPDX-License-Identifier: Apache-2.0
//! Supplied plain native transfers, not an exhaustive wallet activity scan.

use std::collections::BTreeSet;

use radar_types::{Address, Signature, b64};
use serde_json::{Value, json};

use super::{Config, evidence_integer as integer};

fn balances(value: &Value, field: &str, count: usize) -> Result<Vec<u64>, String> {
    let rows = value[field].as_array().ok_or("transfer balances missing")?;
    if rows.len() != count {
        return Err("transfer balances do not cover exact message accounts".into());
    }
    rows.iter()
        .map(|row| {
            row.as_str()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| "invalid transfer balance".into())
        })
        .collect()
}

fn transaction(value: &Value, wallet: Address) -> Result<Value, String> {
    let signed = value["transaction_base64"]
        .as_str()
        .and_then(b64::decode)
        .ok_or("invalid native transfer transaction")?;
    // Support exactly one writable signer and one readonly unsigned program.
    // Other envelopes remain unknown rather than partially interpreted.
    if signed.len() > 1232 || signed.first() != Some(&1) || signed.get(65..68) != Some(&[1, 0, 1]) {
        return Err("unsupported native transfer envelope".into());
    }
    let message =
        radar_signer::tx::decode(&signed).map_err(|_| "invalid native transfer message")?;
    if message.accounts.last() != Some(&[0; 32])
        || message.accounts.iter().collect::<BTreeSet<_>>().len() != message.accounts.len()
        || message.instructions.is_empty()
    {
        return Err("unsupported native transfer accounts or instructions".into());
    }
    let wallet_index = message
        .accounts
        .iter()
        .position(|account| account == wallet.as_bytes())
        .filter(|index| *index < message.accounts.len() - 1)
        .ok_or("native transfer does not name the configured wallet")?;
    let signature: [u8; 64] = signed[1..65]
        .try_into()
        .map_err(|_| "invalid transfer signature extent")?;
    ed25519_dalek::VerifyingKey::from_bytes(&message.accounts[0])
        .map_err(|_| "invalid native transfer signer")?
        .verify_strict(
            &signed[65..],
            &ed25519_dalek::Signature::from_bytes(&signature),
        )
        .map_err(|_| "native transfer signature verification failed")?;
    let succeeded = match value["outcome"].as_str() {
        Some("succeeded") => true,
        Some("failed") => false,
        _ => return Err("native transfer outcome missing".into()),
    };
    for field in ["pre_token_balances", "post_token_balances"] {
        if !value[field].as_array().is_some_and(Vec::is_empty) {
            return Err("native transfer token effects are not classified".into());
        }
    }
    let pre = balances(value, "pre_balances", message.accounts.len())?;
    let post = balances(value, "post_balances", message.accounts.len())?;
    let fee = integer(value, "network_fee_lamports")?;
    let mut effects = vec![0_i128; message.accounts.len()];
    effects[0] = -i128::from(fee);
    for instruction in &message.instructions {
        if instruction.program_id != [0; 32]
            || instruction.accounts.len() != 2
            || instruction.accounts[0] != message.accounts[0]
            || instruction.data.len() != 12
            || instruction.data[..4] != 2_u32.to_le_bytes()
        {
            return Err("native activity contains an unsupported instruction".into());
        }
        let target = message
            .accounts
            .iter()
            .position(|account| account == &instruction.accounts[1])
            .filter(|index| *index < message.accounts.len() - 1)
            .ok_or("native transfer target is not writable")?;
        let amount = u64::from_le_bytes(
            instruction.data[4..12]
                .try_into()
                .map_err(|_| "invalid transfer amount")?,
        );
        // Wire size bounds the number of u64 amounts far below i128 capacity.
        // Failed execution is atomic: only the fee changes native balances.
        if succeeded {
            // Even a self-transfer must fund its debit before the credit returns.
            if i128::from(amount) > i128::from(pre[0]) + effects[0] {
                return Err("native transfer exceeds intermediate payer balance".into());
            }
            effects[0] -= i128::from(amount);
            effects[target] += i128::from(amount);
        }
    }
    for ((pre, post), effect) in pre.iter().zip(&post).zip(&effects) {
        if i128::from(*pre) + effect != i128::from(*post) {
            return Err(
                "native transfer balances disagree with signed instructions and fee".into(),
            );
        }
    }
    let wallet_fee = if wallet_index == 0 { fee } else { 0 };
    Ok(
        json!({"operation":null,"signature":Signature::new(signature),
        "source":"protected_native_transfer","execution_slot":integer(value,"slot")?.to_string(),
        "outcome":value["outcome"],"pre_lamports":pre[wallet_index].to_string(),
        "post_lamports":post[wallet_index].to_string(),
        "net_change_lamports":effects[wallet_index].to_string(),
        "wallet_transfer_change_lamports":(effects[wallet_index]+i128::from(wallet_fee)).to_string(),
        "wallet_network_fee_lamports":wallet_fee.to_string(),"signature_verified_locally":true}),
    )
}

pub(super) fn review(
    current: &Value,
    history: &Value,
    config: &Config,
    now: u64,
) -> Result<Vec<Value>, String> {
    let Some(packet) = current.get("native_transfers") else {
        return Ok(vec![]);
    };
    if packet["version"] != 1
        || packet["authority"] != "read_only"
        || packet["commitment"] != "finalized"
        || packet["wallet"] != config.wallet.to_string()
    {
        return Err("native transfer evidence identity is inconsistent".into());
    }
    super::read_evidence_expiry(packet, config, now)?;
    let mut seen = BTreeSet::new();
    if let Some(flows) = history["recorded_native_cash_flows"].as_array() {
        for flow in flows {
            seen.insert(
                flow["signature"]
                    .as_str()
                    .ok_or("retained cash signature missing")?
                    .to_owned(),
            );
        }
    }
    let native_slot = integer(&current["native_sol"], "slot")?;
    let rows = packet["transactions"]
        .as_array()
        .ok_or("native transfer transactions missing")?;
    rows.iter()
        .map(|row| {
            let reviewed = transaction(row, config.wallet)?;
            let signature = reviewed["signature"]
                .as_str()
                .ok_or("native transfer signature missing")?;
            if !seen.insert(signature.to_owned())
                || integer(&reviewed, "execution_slot")? > native_slot
            {
                return Err(
                    "native transfer is duplicated or newer than native observation".into(),
                );
            }
            Ok(reviewed)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};

    fn payer() -> Address {
        Address::new(SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes())
    }
    fn packet(transfers: &[(u8, u64)], pre: &[u64], post: &[u64], succeeded: bool) -> Value {
        let key = SigningKey::from_bytes(&[7; 32]);
        let mut bytes = vec![1];
        bytes.extend([0; 64]);
        bytes.extend([1, 0, 1]);
        bytes.push(u8::try_from(pre.len()).unwrap());
        bytes.extend(key.verifying_key().to_bytes());
        for index in 1..pre.len() - 1 {
            bytes.extend([u8::try_from(index + 7).unwrap(); 32]);
        }
        bytes.extend([0; 32]);
        bytes.extend([9; 32]);
        bytes.push(u8::try_from(transfers.len()).unwrap());
        for (target, amount) in transfers {
            bytes.extend([u8::try_from(pre.len() - 1).unwrap(), 2, 0, *target, 12]);
            bytes.extend(2_u32.to_le_bytes());
            bytes.extend(amount.to_le_bytes());
        }
        let signature = key.sign(&bytes[65..]);
        bytes[1..65].copy_from_slice(&signature.to_bytes());
        json!({"transaction_base64":b64::encode(&bytes),"slot":"43","network_fee_lamports":"2",
            "outcome":if succeeded {"succeeded"} else {"failed"},
            "pre_balances":pre.iter().map(u64::to_string).collect::<Vec<_>>(),
            "post_balances":post.iter().map(u64::to_string).collect::<Vec<_>>(),
            "pre_token_balances":[],"post_token_balances":[]})
    }
    fn edit(packet: &Value, mutate: impl FnOnce(&mut Vec<u8>), resign: bool) -> Value {
        let mut bytes = b64::decode(packet["transaction_base64"].as_str().unwrap()).unwrap();
        mutate(&mut bytes);
        if resign {
            let signature = SigningKey::from_bytes(&[7; 32]).sign(&bytes[65..]);
            bytes[1..65].copy_from_slice(&signature.to_bytes());
        }
        let mut result = packet.clone();
        result["transaction_base64"] = json!(b64::encode(&bytes));
        result
    }

    #[test]
    fn native_transfers_separate_deposits_withdrawals_self_transfers_and_failed_fees() {
        let value = packet(&[(1, 7), (1, 11)], &[100, 10, 1], &[80, 28, 1], true);
        for (wallet, delta, transfer, fee, pre, post) in [
            (payer(), "-20", "-18", "2", "100", "80"),
            (Address::new([8; 32]), "18", "18", "0", "10", "28"),
        ] {
            let row = transaction(&value, wallet).unwrap();
            assert_eq!(row["net_change_lamports"], delta);
            assert_eq!(row["wallet_transfer_change_lamports"], transfer);
            assert_eq!(row["wallet_network_fee_lamports"], fee);
            assert_eq!(row["pre_lamports"], pre);
            assert_eq!(row["post_lamports"], post);
            assert_eq!(row["execution_slot"], "43");
            assert!(row["operation"].is_null());
            assert_eq!(row["source"], "protected_native_transfer");
            assert_eq!(row["signature_verified_locally"], true);
            assert!(row["signature"].as_str().is_some());
        }
        for (transfers, succeeded) in [
            (vec![(0, 7)], true),
            (vec![(1, 7)], false),
            (vec![(1, 0)], true),
        ] {
            let value = packet(&transfers, &[100, 10, 1], &[98, 10, 1], succeeded);
            let row = transaction(&value, payer()).unwrap();
            assert_eq!(row["net_change_lamports"], "-2");
            assert_eq!(row["wallet_transfer_change_lamports"], "0");
            assert_eq!(
                row["outcome"],
                if succeeded { "succeeded" } else { "failed" }
            );
            assert_eq!(
                transaction(&value, Address::new([8; 32])).unwrap()["net_change_lamports"],
                "0"
            );
        }
    }

    #[test]
    fn native_transfers_reject_unsigned_changed_unsupported_and_duplicate_messages() {
        let value = packet(&[(1, 7)], &[100, 10, 1], &[91, 17, 1], true);
        for index in [0, 1, 65, 66, 67, 166] {
            let changed = edit(&value, |b| b[index] ^= 1, false);
            assert!(transaction(&changed, payer()).is_err(), "byte {index}");
        }
        for (index, byte) in [(65, 0), (66, 1), (67, 0), (67, 2)] {
            let changed = edit(&value, |b| b[index] = byte, true);
            assert!(
                transaction(&changed, payer()).is_err(),
                "signed header {index}"
            );
        }
        for (index, byte) in [(198, 1), (199, 1), (200, 1), (201, 2), (202, 11), (203, 3)] {
            let changed = edit(&value, |b| b[index] = byte, true);
            assert!(
                transaction(&changed, payer()).is_err(),
                "instruction {index}"
            );
        }
        for changed in [
            edit(
                &value,
                |b| {
                    b[199] = 1;
                    b.remove(201);
                },
                true,
            ),
            edit(
                &value,
                |b| {
                    b[199] = 3;
                    b.insert(202, 1);
                },
                true,
            ),
        ] {
            assert!(transaction(&changed, payer()).is_err());
        }
        for changed in [
            edit(
                &value,
                |b| {
                    b[202] = 13;
                    b.push(0);
                },
                true,
            ),
            edit(
                &value,
                |b| {
                    b[202] = 11;
                    b.pop();
                },
                true,
            ),
            packet(&[(2, 7)], &[100, 10, 1], &[91, 10, 8], true),
        ] {
            assert!(transaction(&changed, payer()).is_err());
        }
        let duplicate = edit(
            &value,
            |b| {
                let key = b[69..101].to_vec();
                b[101..133].copy_from_slice(&key);
            },
            true,
        );
        assert!(transaction(&duplicate, payer()).is_err());
        let wrong_program = edit(&value, |b| b[133..165].fill(10), true);
        assert!(transaction(&wrong_program, payer()).is_err());
        assert!(transaction(&packet(&[], &[100, 10, 1], &[98, 10, 1], false), payer()).is_err());
        assert!(transaction(&value, Address::new([20; 32])).is_err());
        assert!(transaction(&value, Address::new([0; 32])).is_err());
    }

    #[test]
    fn native_transfers_require_all_exact_native_balances_and_empty_token_effects() {
        let value = packet(&[(1, 7)], &[100, 10, 1], &[91, 17, 1], true);
        for (path, bad) in [
            ("/outcome", json!("pending")),
            ("/slot", json!("wrong")),
            ("/network_fee_lamports", json!("wrong")),
            ("/pre_balances", json!([])),
            ("/post_balances", json!([])),
            ("/pre_balances/0", json!("18446744073709551616")),
            ("/post_balances/0", json!("90")),
            ("/post_balances/1", json!("16")),
            ("/post_balances/2", json!("2")),
            ("/pre_token_balances", json!([{}])),
            ("/post_token_balances", json!([{}])),
            ("/pre_token_balances", Value::Null),
        ] {
            let mut changed = value.clone();
            *changed.pointer_mut(path).unwrap() = bad;
            assert!(transaction(&changed, payer()).is_err(), "{path}");
        }
        assert_eq!(
            transaction(
                &packet(&[(0, 98)], &[100, 10, 1], &[98, 10, 1], true),
                payer()
            )
            .unwrap()["net_change_lamports"],
            "-2"
        );
        for value in [
            packet(&[(0, 99)], &[100, 10, 1], &[98, 10, 1], true),
            packet(&[(1, 7), (0, 92)], &[100, 10, 1], &[91, 17, 1], true),
            packet(&[(1, u64::MAX)], &[100, 10, 1], &[91, 17, 1], true),
            packet(&[(1, 1)], &[100, u64::MAX, 1], &[97, u64::MAX, 1], true),
            packet(&[(1, 0)], &[0, 10, 1], &[0, 10, 1], true),
        ] {
            assert!(transaction(&value, payer()).is_err());
        }
        let mut large = packet(&[(1, u64::MAX)], &[u64::MAX, 0, 1], &[0, u64::MAX, 1], true);
        large["network_fee_lamports"] = json!("0");
        assert_eq!(
            transaction(&large, Address::new([8; 32])).unwrap()["net_change_lamports"],
            u64::MAX.to_string()
        );
        assert_eq!(
            transaction(&large, payer()).unwrap()["net_change_lamports"],
            format!("-{}", u64::MAX)
        );
    }

    #[test]
    fn native_transfers_accept_exact_wire_limit_and_two_account_self_transfer() {
        let mut pre = vec![1; 13];
        pre[0] = 100;
        let mut post = pre.clone();
        post[0] = 98;
        let value = packet(&[(1, 0); 42], &pre, &post, true);
        assert_eq!(
            b64::decode(value["transaction_base64"].as_str().unwrap())
                .unwrap()
                .len(),
            1232
        );
        assert_eq!(
            transaction(&value, payer()).unwrap()["net_change_lamports"],
            "-2"
        );
        let too_large = edit(&value, |b| b.push(0), true);
        assert!(transaction(&too_large, payer()).is_err());
        let value = packet(&[(0, 7)], &[100, 1], &[98, 1], true);
        assert_eq!(
            transaction(&value, payer()).unwrap()["net_change_lamports"],
            "-2"
        );
    }

    #[test]
    fn native_transfer_packet_requires_identity_current_reads_and_unique_unrecorded_signatures() {
        let config = Config {
            active: false,
            wallet: payer(),
            app_id: String::new(),
            wallet_id: String::new(),
            policy: radar_risk::Policy::SHIPPED,
            valid_until_unix_secs: 0,
            snapshot_path: std::path::PathBuf::default(),
            history_path: std::path::PathBuf::default(),
            key_path: std::path::PathBuf::default(),
            max_snapshot_age_secs: 10,
            intent_lifetime_secs: 0,
            fee_reserve_lamports: 0,
            programs: vec![],
        };
        let tx = packet(&[(1, 7)], &[100, 10, 1], &[91, 17, 1], true);
        let current = json!({"native_sol":{"slot":"43"},"native_transfers":{
            "version":1,"authority":"read_only","commitment":"finalized","wallet":payer(),
            "read_started_at_unix_secs":1,"read_completed_at_unix_secs":2,"transactions":[tx]}});
        let rows = review(&current, &json!({}), &config, 11).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            review(&json!({}), &Value::Null, &config, 11).unwrap(),
            Vec::<Value>::new()
        );
        assert!(review(&current, &json!({}), &config, 12).is_err());
        assert!(
            review(
                &current,
                &json!({"recorded_native_cash_flows":[{"signature":rows[0]["signature"]}]}),
                &config,
                11
            )
            .is_err()
        );
        for (path, bad) in [
            ("/native_transfers/version", json!(0)),
            ("/native_transfers/authority", json!("model")),
            ("/native_transfers/commitment", json!("processed")),
            ("/native_transfers/wallet", json!(Address::new([8; 32]))),
            ("/native_transfers/read_started_at_unix_secs", json!(0)),
            ("/native_transfers/read_started_at_unix_secs", json!(12)),
            ("/native_transfers/read_completed_at_unix_secs", json!(0)),
            ("/native_transfers/read_completed_at_unix_secs", json!(12)),
            ("/native_transfers/transactions", Value::Null),
            (
                "/native_transfers/transactions",
                json!([tx.clone(), tx.clone()]),
            ),
            ("/native_sol/slot", json!("42")),
        ] {
            let mut changed = current.clone();
            *changed.pointer_mut(path).unwrap() = bad;
            assert!(review(&changed, &json!({}), &config, 11).is_err(), "{path}");
        }
    }
}
