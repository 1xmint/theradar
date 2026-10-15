// SPDX-License-Identifier: Apache-2.0
//! Authorship and static query membership only; execution effects remain unknown.

use super::{Config, Snapshot, evidence_integer as integer};
use radar_types::{Address, Signature, b64};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn transaction(row: &Value, reported: &[Address]) -> Result<Value, String> {
    let bytes = row["transaction_base64"]
        .as_str()
        .and_then(b64::decode)
        .ok_or("invalid account activity bytes")?;
    let count = usize::from(*bytes.first().ok_or("empty account activity bytes")?);
    if bytes.len() > 1232 || count == 0 {
        return Err("unsupported account activity signature extent".into());
    }
    // The shared decoder checks the signature extent and every message field;
    // duplicating its minimum-length arithmetic adds no rejection guarantee.
    let message =
        radar_signer::tx::decode(&bytes).map_err(|_| "unsupported account activity message")?;
    if usize::from(message.required_signatures) != count
        || message.accounts.len() < count
        || message.accounts.iter().collect::<BTreeSet<_>>().len() != message.accounts.len()
    {
        return Err("account activity signers or accounts are inconsistent".into());
    }
    for (index, key) in message.accounts.iter().take(count).enumerate() {
        let signature: [u8; 64] = bytes[1 + 64 * index..=64 * (index + 1)]
            .try_into()
            .map_err(|_| "invalid account activity signature")?;
        ed25519_dalek::VerifyingKey::from_bytes(key)
            .map_err(|_| "invalid account activity signer")?
            .verify_strict(
                &bytes[message.message_offset..],
                &ed25519_dalek::Signature::from_bytes(&signature),
            )
            .map_err(|_| "account activity signature verification failed")?;
    }
    let signature = Signature::new(
        bytes[1..65]
            .try_into()
            .map_err(|_| "invalid account activity signature")?,
    );
    if row["signature"] != signature.to_string()
        || reported.is_empty()
        || reported.iter().collect::<BTreeSet<_>>().len() != reported.len()
        || reported
            .iter()
            .any(|address| !message.accounts.contains(address.as_bytes()))
    {
        return Err("account activity signature or query membership differs".into());
    }
    let intents = super::token_intents::review(&message);
    let effects = super::token_effects::review(&message, row, &intents);
    Ok(
        json!({"signature":signature,"reported_slot":row["slot"],"reported_outcome":row["outcome"],
        "reported_for_addresses":reported,"signature_verified_locally":true,"address_membership_verified_locally":true,
        "message_decoded_locally":true,"versioned_message":message.versioned,
        "classification":"unresolved","execution_effects_verified":false,
        "top_level_instruction_intents":intents,"reported_effect_review":effects}),
    )
}

pub(super) fn rows(
    packet: &Value,
    known: &BTreeSet<Address>,
    after: u64,
    through: u64,
) -> Result<Vec<Value>, String> {
    let mut entries = BTreeMap::new();
    for entry in packet["signatures"]
        .as_array()
        .ok_or("activity enumeration missing")?
    {
        let key = entry["signature"]
            .as_str()
            .ok_or("activity signature missing")?;
        if entries.insert(key, entry).is_some() {
            return Err("duplicate activity enumeration".into());
        }
    }
    let rows = packet["transactions"]
        .as_array()
        .ok_or("activity transactions missing")?;
    if rows.len() != entries.len() {
        return Err("activity rows do not cover supplied enumeration".into());
    }
    let mut reviewed = Vec::new();
    for row in rows {
        let key = row["signature"]
            .as_str()
            .ok_or("activity signature missing")?;
        let entry = entries
            .remove(key)
            .ok_or("duplicate or unenumerated activity row")?;
        let slot = integer(row, "slot")?;
        let reported: Vec<Address> =
            serde_json::from_value(entry["reported_for_addresses"].clone())
                .map_err(|_| "activity reporting addresses missing")?;
        if entry["slot"] != row["slot"]
            || entry["outcome"] != row["outcome"]
            || slot <= after
            || slot > through
            || !matches!(row["outcome"].as_str(), Some("succeeded" | "failed"))
            || reported.iter().any(|address| !known.contains(address))
        {
            return Err("activity row differs from supplied enumeration".into());
        }
        reviewed.push(transaction(row, &reported)?);
    }
    reviewed.sort_by(|left, right| left["signature"].as_str().cmp(&right["signature"].as_str()));
    Ok(reviewed)
}

pub(super) fn review(
    snapshot: &Snapshot,
    config: &Config,
    checkpoint: &str,
    opening: Option<&radar_journal::OpeningInventoryRecord>,
    now: u64,
) -> Result<Value, String> {
    if snapshot.wallet != config.wallet
        || snapshot.accounting_checkpoint != checkpoint
        || !super::snapshot_current(
            now,
            snapshot.observed_at_unix_secs,
            config.max_snapshot_age_secs,
        )
    {
        return Err("account activity snapshot does not cover wallet, time and journal".into());
    }
    super::wallet_evidence_expiry(snapshot, config, now)?;
    if let Some(opening) = opening {
        super::opening::lots(opening, config.wallet, &snapshot.wallet_evidence, &[])?;
    }
    let current =
        super::inventory::capture_accounts(&snapshot.wallet_evidence, snapshot.state.now.get())?;
    let comparison = super::account_inventory::review(opening, &current)?;
    let mut known = BTreeSet::from([config.wallet]);
    for row in comparison["accounts"]
        .as_array()
        .ok_or("known accounts missing")?
    {
        known.insert(
            serde_json::from_value::<Address>(row["address"].clone())
                .map_err(|_| "invalid known account")?,
        );
    }
    let packet = &snapshot.wallet_evidence["wallet_activity"];
    let targets: Vec<Address> = serde_json::from_value(packet["queried_addresses"].clone())
        .map_err(|_| "activity query targets missing")?;
    let after = integer(packet, "after_slot_exclusive")?;
    let through = integer(packet, "through_slot_inclusive")?;
    if packet["version"] != 1
        || packet["authority"] != "read_only"
        || packet["commitment"] != "finalized"
        || packet["coverage"] != "provider_reported_known_address_history"
        || packet["wallet"] != config.wallet.to_string()
        || targets.len() > 16
        || targets.len() != known.len()
        || targets.iter().copied().collect::<BTreeSet<_>>() != known
        || after >= through
        || through > integer(&snapshot.wallet_evidence["native_sol"], "slot")?
    {
        return Err("account activity identity, targets or interval differ".into());
    }
    super::read_evidence_expiry(packet, config, now)?;
    let reviewed = rows(packet, &known, after, through)?;
    let token_reconciliation = super::token_reconciliation::review(
        opening,
        &snapshot.wallet_evidence,
        &comparison,
        &reviewed,
    );
    Ok(
        json!({"version":1,"authority":"protected_operator_account_activity_review","wallet":config.wallet,
        "accounting_checkpoint":checkpoint,"transactions":reviewed,"reported_known_token_reconciliation":token_reconciliation,"coverage":"supplied_known_address_transactions",
        "collection_completeness_verified":false,"metadata_verified_independently":false,"wallet_coverage_complete":false,
        "economic_reconciliation_complete":false,"portfolio_state_updated":false,"reservation_released":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};
    fn signed() -> (Value, Address) {
        let keys = [
            SigningKey::from_bytes(&[1; 32]),
            SigningKey::from_bytes(&[2; 32]),
        ];
        let mut message = vec![2, 0, 0, 3];
        for key in &keys {
            message.extend(key.verifying_key().to_bytes());
        }
        let account = Address::new([9; 32]);
        message.extend(account.as_bytes());
        message.extend([4; 32]);
        message.push(0);
        let mut bytes = vec![2];
        for key in &keys {
            bytes.extend(key.sign(&message).to_bytes());
        }
        bytes.extend(message);
        let signature = Signature::new(bytes[1..65].try_into().unwrap());
        (
            json!({"signature":signature,"transaction_base64":b64::encode(&bytes),"slot":"43","outcome":"failed"}),
            account,
        )
    }
    fn edited(row: &Value, edit: impl FnOnce(&mut Vec<u8>)) -> Value {
        let mut bytes = b64::decode(row["transaction_base64"].as_str().unwrap()).unwrap();
        edit(&mut bytes);
        for (index, seed) in [1, 2].into_iter().enumerate() {
            let signature = SigningKey::from_bytes(&[seed; 32]).sign(&bytes[129..]);
            bytes[1 + 64 * index..=64 * (index + 1)].copy_from_slice(&signature.to_bytes());
        }
        let mut result = row.clone();
        result["signature"] = json!(Signature::new(bytes[1..65].try_into().unwrap()));
        result["transaction_base64"] = json!(b64::encode(&bytes));
        result
    }
    #[test]
    fn a_single_signer_account_is_valid_but_truncation_never_is() {
        let key = SigningKey::from_bytes(&[1; 32]);
        let address = Address::new(key.verifying_key().to_bytes());
        let mut message = vec![1, 0, 0, 1];
        message.extend(address.as_bytes());
        message.extend([4; 32]);
        message.push(0);
        let signature = Signature::new(key.sign(&message).to_bytes());
        let mut bytes = vec![1];
        bytes.extend(signature.as_bytes());
        bytes.extend(message);
        let row = json!({"signature":signature,"transaction_base64":b64::encode(&bytes),
            "slot":"43","outcome":"succeeded"});
        let review = transaction(&row, &[address]).unwrap();
        assert_eq!(review["signature_verified_locally"], true);
        assert_eq!(review["execution_effects_verified"], false);
        for length in 0..bytes.len() {
            let mut truncated = row.clone();
            truncated["transaction_base64"] = json!(b64::encode(&bytes[..length]));
            assert!(transaction(&truncated, &[address]).is_err(), "{length}");
        }
        // A valid packet at the exact network limit must be accepted. Use an
        // opaque instruction to reach the boundary without trailing garbage.
        let mut message = bytes[65..].to_vec();
        message.pop();
        message.extend([1, 0, 0, 0xC6, 8]);
        message.extend([0; 1094]);
        let signature = Signature::new(key.sign(&message).to_bytes());
        let mut packet = vec![1];
        packet.extend(signature.as_bytes());
        packet.extend(message);
        assert_eq!(packet.len(), 1232);
        let at_limit = json!({"signature":signature,"transaction_base64":b64::encode(&packet)});
        assert!(transaction(&at_limit, &[address]).is_ok());
    }
    #[test]
    fn every_signer_and_reported_static_address_must_match_exact_bytes() {
        let (row, account) = signed();
        let report = transaction(&row, &[account]).unwrap();
        assert_eq!(report["signature_verified_locally"], true);
        assert_eq!(report["address_membership_verified_locally"], true);
        assert_eq!(report["classification"], "unresolved");
        assert_eq!(report["execution_effects_verified"], false);
        for index in [1, 65, 130] {
            let mut bad = row.clone();
            let mut bytes = b64::decode(row["transaction_base64"].as_str().unwrap()).unwrap();
            bytes[index] ^= 1;
            bad["transaction_base64"] = json!(b64::encode(&bytes));
            assert!(transaction(&bad, &[account]).is_err(), "{index}");
        }
        for reported in [vec![], vec![account, account], vec![Address::new([8; 32])]] {
            assert!(transaction(&row, &reported).is_err());
        }
        let first = Address::new(SigningKey::from_bytes(&[1; 32]).verifying_key().to_bytes());
        let header = edited(&row, |bytes| bytes[129] = 1);
        assert!(transaction(&header, &[account]).is_err());
        let duplicate = edited(&row, |bytes| {
            let first: [u8; 32] = bytes[133..165].try_into().unwrap();
            bytes[197..229].copy_from_slice(&first);
        });
        assert!(transaction(&duplicate, &[first]).is_err());
        let too_few = edited(&row, |bytes| {
            bytes[132] = 1;
            bytes.drain(165..229);
        });
        assert!(transaction(&too_few, &[first]).is_err());
        let oversized = edited(&row, |bytes| {
            bytes.pop();
            bytes.extend([1, 2, 0, 0xE8, 7]);
            bytes.extend([0; 1000]);
        });
        assert!(transaction(&oversized, &[account]).is_err());
        let mut unsigned = vec![0, 0, 0, 0, 2];
        unsigned.extend([9; 32]);
        unsigned.extend([8; 32]);
        unsigned.extend([4; 32]);
        unsigned.push(0);
        let unsigned = json!({"signature":Signature::new(unsigned[1..65].try_into().unwrap()),"transaction_base64":b64::encode(&unsigned)});
        assert!(transaction(&unsigned, &[account]).is_err());
        let mut bad = row;
        bad["signature"] = json!(Signature::new([0; 64]));
        assert!(transaction(&bad, &[account]).is_err());
    }
    #[test]
    fn supplied_enumeration_and_interval_bind_each_reviewed_row_once() {
        let (row, account) = signed();
        let known = BTreeSet::from([account]);
        let entry = json!({"signature":row["signature"],"slot":"43","outcome":"failed","reported_for_addresses":[account]});
        let packet = json!({"signatures":[entry],"transactions":[row]});
        assert_eq!(rows(&packet, &known, 42, 43).unwrap().len(), 1);
        for (pointer, value) in [
            ("/signatures/0/slot", json!("44")),
            ("/signatures/0/outcome", json!("succeeded")),
            (
                "/signatures/0/reported_for_addresses",
                json!([Address::new([8; 32])]),
            ),
            ("/transactions/0/outcome", json!("unknown")),
            ("/transactions/0/slot", json!("42")),
        ] {
            let mut bad = packet.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(rows(&bad, &known, 42, 43).is_err(), "{pointer}");
        }
        let mut unknown = packet.clone();
        unknown["signatures"][0]["outcome"] = json!("unknown");
        unknown["transactions"][0]["outcome"] = json!("unknown");
        assert!(rows(&unknown, &known, 42, 43).is_err());
        assert!(rows(&packet, &BTreeSet::new(), 42, 43).is_err());
        assert!(rows(&packet, &known, 43, 44).is_err());
        assert!(rows(&packet, &known, 41, 42).is_err());
        for field in ["signatures", "transactions"] {
            let mut bad = packet.clone();
            bad[field] = json!([]);
            assert!(rows(&bad, &known, 42, 43).is_err());
            bad[field] = json!([packet[field][0], packet[field][0]]);
            assert!(rows(&bad, &known, 42, 43).is_err());
        }
        let duplicate = json!({"signatures":[packet["signatures"][0],{"signature":"different","slot":"43","outcome":"failed","reported_for_addresses":[account]}],"transactions":[packet["transactions"][0],packet["transactions"][0]]});
        assert!(rows(&duplicate, &known, 42, 43).is_err());
        assert_eq!(
            rows(&json!({"signatures":[],"transactions":[]}), &known, 42, 43).unwrap(),
            Vec::<Value>::new()
        );
    }
}
