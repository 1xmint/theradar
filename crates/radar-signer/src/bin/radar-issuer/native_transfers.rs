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

// Query targets describe collection, not ownership or complete wallet coverage.
// Reuse the account-history verifier before recognizing any retained operation.
fn known_queries(packet: &Value, after: u64, through: u64) -> Result<(), String> {
    let wallet: Address =
        serde_json::from_value(packet["wallet"].clone()).map_err(|_| "invalid activity wallet")?;
    let targets: Vec<Address> = serde_json::from_value(packet["queried_addresses"].clone())
        .map_err(|_| "activity query targets missing")?;
    let known: BTreeSet<_> = targets.iter().copied().collect();
    if targets.len() > 16 || targets.len() != known.len() || !known.contains(&wallet) {
        return Err("activity query targets must be unique and include wallet".into());
    }
    super::account_activity::rows(packet, &known, after, through)?;
    Ok(())
}

// Collection completion is only about supplied address scans, never wallet coverage.
fn collected(packet: &Value, native_slot: u64) -> Result<Value, String> {
    let after = integer(packet, "after_slot_exclusive")?;
    let through = integer(packet, "through_slot_inclusive")?;
    match packet["coverage"].as_str() {
        Some("provider_reported_address_history") => {}
        Some("provider_reported_known_address_history") => known_queries(packet, after, through)?,
        _ => return Err("unsupported collected activity coverage".into()),
    }
    if packet["signature_scan_finished"] != true
        || packet["transaction_fetch_finished"] != true
        || after >= through
        || through > native_slot
    {
        return Err("collected activity is incomplete or outside the native observation".into());
    }
    let entries = packet["signatures"]
        .as_array()
        .ok_or("activity entries missing")?;
    let rows = packet["transactions"]
        .as_array()
        .ok_or("activity transactions missing")?;
    if entries.len() != rows.len() {
        return Err("activity transactions do not cover enumerated entries".into());
    }
    let transactions = entries
        .iter()
        .zip(rows)
        .map(|(entry, row)| {
            let slot = integer(row, "slot")?;
            let signature = row["signature"]
                .as_str()
                .ok_or("activity signature missing")?;
            if slot <= after
                || slot > through
                || entry["slot"] != row["slot"]
                || entry["signature"] != signature
                || entry["outcome"] != row["outcome"]
            {
                return Err("activity row differs from interval or enumeration".into());
            }
            let meta = &row["raw_metadata"];
            let succeeded = match meta.get("err") {
                Some(Value::Null) => true,
                Some(Value::Object(_) | Value::String(_)) => false,
                _ => return Err("activity metadata outcome missing".into()),
            };
            if row["outcome"] != if succeeded { "succeeded" } else { "failed" }
                || meta["fee"].as_u64().map(|fee| fee.to_string()).as_deref()
                    != row["network_fee_lamports"].as_str()
            {
                return Err("activity metadata outcome or fee differs".into());
            }
            let mut normalized = row.clone();
            for (raw, field) in [
                ("preBalances", "pre_balances"),
                ("postBalances", "post_balances"),
            ] {
                let balances = meta[raw]
                    .as_array()
                    .ok_or("activity native balances missing")?
                    .iter()
                    .map(|value| {
                        value
                            .as_u64()
                            .map(|n| Value::String(n.to_string()))
                            .ok_or("activity native balance is not an integer")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                normalized[field] = json!(balances);
            }
            normalized["pre_token_balances"] = meta["preTokenBalances"].clone();
            normalized["post_token_balances"] = meta["postTokenBalances"].clone();
            normalized["collected_signature"] = json!(signature);
            Ok(normalized)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut normalized = packet.clone();
    normalized["transactions"] = json!(transactions);
    Ok(normalized)
}

// History is produced by acquisitions::review after replaying each retained
// valuation and verifying its signed binding. Recognizing an exact recorded
// operation avoids treating a swap as an external plain transfer a second time.
fn recorded(row: &Value, history: &Value) -> Result<bool, String> {
    let Some(signature) = row.get("collected_signature") else {
        return Ok(false);
    };
    let Some(records) = history["recorded_settlements"].as_array() else {
        return Ok(false);
    };
    let Some(record) = records
        .iter()
        .find(|record| record["review"]["signature"] == *signature)
    else {
        return Ok(false);
    };
    let review = &record["review"];
    if row["transaction_base64"] != record["signed_transaction"]
        || row["slot"] != review["slot"]
        || row["outcome"] != review["outcome"]
        || row["network_fee_lamports"] != review["network_fee_lamports"]
    {
        return Err("collected operation differs from retained settlement".into());
    }
    let effects = review["native_account_effects"]
        .as_array()
        .ok_or("recorded native effects missing")?;
    for (field, retained) in [
        ("pre_balances", "pre_lamports"),
        ("post_balances", "post_lamports"),
    ] {
        let expected: Vec<_> = effects
            .iter()
            .map(|effect| effect[retained].clone())
            .collect();
        if row[field] != json!(expected) {
            return Err(
                "collected operation native balances differ from retained settlement".into(),
            );
        }
    }
    for field in ["pre_token_balances", "post_token_balances"] {
        let tokens = row[field]
            .as_array()
            .ok_or("collected operation token balances missing")?;
        let normalized: Vec<_> = tokens
            .iter()
            .map(|token| {
                json!({
            "account_index":token["accountIndex"],"mint":token["mint"],"owner":token["owner"],
            "program_id":token["programId"],"raw_amount":token["uiTokenAmount"]["amount"],
            "decimals":token["uiTokenAmount"]["decimals"]})
            })
            .collect();
        if json!(normalized) != review[field] {
            return Err(
                "collected operation token balances differ from retained settlement".into(),
            );
        }
    }
    Ok(true)
}

pub(super) fn capture(
    current: &Value,
    history: &Value,
    config: &Config,
    now: u64,
) -> Result<Vec<radar_journal::NativeTransferRecord>, String> {
    let native_slot = integer(&current["native_sol"], "slot");
    let normalized;
    let packet = match (
        current.get("native_transfers"),
        current.get("wallet_activity"),
    ) {
        (Some(_), Some(_)) => {
            return Err("choose supplied transfers or collected activity, not both".into());
        }
        (Some(packet), None) => packet,
        (None, Some(packet)) => {
            normalized = collected(packet, native_slot.clone()?)?;
            &normalized
        }
        (None, None) => return Ok(vec![]),
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
    let mut owned = BTreeSet::new();
    if let Some(flows) = history["recorded_native_cash_flows"].as_array() {
        for flow in flows {
            owned.insert(
                flow["signature"]
                    .as_str()
                    .ok_or("retained cash signature missing")?
                    .to_owned(),
            );
        }
    }
    let native_slot = native_slot?;
    let rows = packet["transactions"]
        .as_array()
        .ok_or("native transfer transactions missing")?;
    let mut result = Vec::new();
    for row in rows {
        if recorded(row, history)? {
            let signature = row["collected_signature"]
                .as_str()
                .ok_or("recorded signature missing")?;
            if !owned.contains(signature) || !seen.insert(signature.to_owned()) {
                return Err("recorded activity is duplicated or lacks cash history".into());
            }
            continue;
        }
        let reviewed = transaction(row, config.wallet)?;
        let signature = reviewed["signature"]
            .as_str()
            .ok_or("native transfer signature missing")?;
        if row
            .get("collected_signature")
            .is_some_and(|value| value != signature)
        {
            return Err("collected signature differs from verified wire signature".into());
        }
        if owned.contains(signature)
            || !seen.insert(signature.to_owned())
            || integer(&reviewed, "execution_slot")? > native_slot
        {
            return Err("native transfer is duplicated or newer than native observation".into());
        }
        let mut evidence = json!({});
        for field in [
            "transaction_base64",
            "slot",
            "outcome",
            "network_fee_lamports",
            "pre_balances",
            "post_balances",
            "pre_token_balances",
            "post_token_balances",
        ] {
            evidence[field] = row[field].clone();
        }
        let signed = b64::encode(
            &b64::decode(
                row["transaction_base64"]
                    .as_str()
                    .ok_or("native transaction missing")?,
            )
            .ok_or("native transaction base64 invalid")?,
        );
        evidence["transaction_base64"] = json!(signed);
        result.push(radar_journal::NativeTransferRecord {
            wallet: config.wallet,
            signed_transaction: signed,
            evidence,
            review: reviewed,
        });
    }
    Ok(result)
}

#[cfg(test)]
pub(super) fn review(
    current: &Value,
    history: &Value,
    config: &Config,
    now: u64,
) -> Result<Vec<Value>, String> {
    Ok(capture(current, history, config, now)?
        .into_iter()
        .map(|r| r.review)
        .collect())
}

pub(super) fn combine(
    current: &Value,
    history: &Value,
    config: &Config,
    now: u64,
    retained: &[radar_journal::NativeTransferRecord],
) -> Result<Vec<Value>, String> {
    let mut records = std::collections::BTreeMap::new();
    for record in retained
        .iter()
        .cloned()
        .chain(capture(current, history, config, now)?)
    {
        let reviewed = transaction(&record.evidence, config.wallet)?;
        let signature = reviewed["signature"]
            .as_str()
            .ok_or("retained transfer signature missing")?
            .to_owned();
        if record.wallet != config.wallet
            || record.evidence["transaction_base64"] != record.signed_transaction
            || reviewed != record.review
            || integer(&reviewed, "execution_slot")? > integer(&current["native_sol"], "slot")?
            || history["recorded_native_cash_flows"]
                .as_array()
                .is_some_and(|flows| flows.iter().any(|flow| flow["signature"] == signature))
            || records
                .get(&signature)
                .is_some_and(|previous| previous != &record)
        {
            return Err(
                "retained transfer is changed, foreign, duplicated or newer than observation"
                    .into(),
            );
        }
        records.insert(signature, record);
    }
    Ok(records.into_values().map(|record| record.review).collect())
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

    fn config() -> Config {
        Config {
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
        }
    }

    fn activity(succeeded: bool) -> Value {
        let tx = packet(
            &[(1, 7)],
            &[100, 10, 1],
            if succeeded {
                &[91, 17, 1]
            } else {
                &[98, 10, 1]
            },
            succeeded,
        );
        let bytes = b64::decode(tx["transaction_base64"].as_str().unwrap()).unwrap();
        let signature = Signature::new(bytes[1..65].try_into().unwrap()).to_string();
        let balances = |field: &str| {
            tx[field]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().parse::<u64>().unwrap())
                .collect::<Vec<_>>()
        };
        let row = json!({"signature":signature,"slot":"43","outcome":tx["outcome"],
            "transaction_base64":tx["transaction_base64"],"network_fee_lamports":"2",
            "raw_metadata":{"err":if succeeded {Value::Null} else {json!({"InstructionError":[0,1]})},
            "fee":2,"preBalances":balances("pre_balances"),"postBalances":balances("post_balances"),
            "preTokenBalances":[],"postTokenBalances":[]}});
        json!({"native_sol":{"slot":"44"},"wallet_activity":{
            "version":1,"authority":"read_only","commitment":"finalized","wallet":payer(),
            "read_started_at_unix_secs":1,"read_completed_at_unix_secs":2,
            "coverage":"provider_reported_address_history","signature_scan_finished":true,
            "transaction_fetch_finished":true,"after_slot_exclusive":"42","through_slot_inclusive":"44",
            "signatures":[{"signature":signature,"slot":"43","outcome":tx["outcome"]}],"transactions":[row]}})
    }

    #[test]
    fn known_address_history_retains_native_checks_and_signed_query_membership() {
        let mut current = activity(true);
        let packet = &mut current["wallet_activity"];
        packet["coverage"] = json!("provider_reported_known_address_history");
        packet["queried_addresses"] = json!([payer(), Address::new([8; 32])]);
        packet["signatures"][0]["reported_for_addresses"] = json!([Address::new([8; 32])]);
        let expected = review(&activity(true), &json!({}), &config(), 11).unwrap();
        assert_eq!(
            review(&current, &json!({}), &config(), 11).unwrap(),
            expected
        );
        for (path, bad) in [
            ("/wallet_activity/queried_addresses", Value::Null),
            ("/wallet_activity/queried_addresses", json!([])),
            (
                "/wallet_activity/queried_addresses",
                json!([Address::new([8; 32])]),
            ),
            (
                "/wallet_activity/queried_addresses",
                json!([payer(), payer()]),
            ),
            ("/wallet_activity/wallet", json!("invalid")),
            (
                "/wallet_activity/signatures/0/reported_for_addresses",
                Value::Null,
            ),
            (
                "/wallet_activity/signatures/0/reported_for_addresses",
                json!([]),
            ),
            (
                "/wallet_activity/signatures/0/reported_for_addresses",
                json!([payer(), payer()]),
            ),
            (
                "/wallet_activity/signatures/0/reported_for_addresses",
                json!([Address::new([9; 32])]),
            ),
            ("/wallet_activity/signature_scan_finished", json!(false)),
            ("/wallet_activity/transaction_fetch_finished", json!(false)),
            (
                "/wallet_activity/transactions/0/raw_metadata/postBalances/0",
                json!(92),
            ),
        ] {
            let mut changed = current.clone();
            *changed.pointer_mut(path).unwrap() = bad;
            assert!(
                review(&changed, &json!({}), &config(), 11).is_err(),
                "{path}"
            );
        }
        // A queried address must actually occur in the signed message, even
        // when the outer enumeration and claimed targets agree with each other.
        let mut forged = current.clone();
        forged["wallet_activity"]["queried_addresses"] = json!([payer(), Address::new([9; 32])]);
        forged["wallet_activity"]["signatures"][0]["reported_for_addresses"] =
            json!([Address::new([9; 32])]);
        assert!(review(&forged, &json!({}), &config(), 11).is_err());
        // Query count has the same exact bound as the collector. Extra empty
        // scans grant no ownership, coverage, or transaction classification.
        let mut targets = vec![payer()];
        targets.extend((8..23).map(|n| Address::new([n; 32])));
        current["wallet_activity"]["queried_addresses"] = json!(targets);
        assert_eq!(
            review(&current, &json!({}), &config(), 11).unwrap(),
            expected
        );
        targets.push(Address::new([23; 32]));
        current["wallet_activity"]["queried_addresses"] = json!(targets);
        assert!(review(&current, &json!({}), &config(), 11).is_err());
    }

    #[test]
    fn collected_recorded_operations_are_not_external_transfers() {
        let mut current = activity(true);
        let row = &mut current["wallet_activity"]["transactions"][0];
        let opaque = edit(row, |bytes| bytes[133] = 6, true);
        row["transaction_base64"] = opaque["transaction_base64"].clone();
        let bytes = b64::decode(row["transaction_base64"].as_str().unwrap()).unwrap();
        let signature = json!(Signature::new(bytes[1..65].try_into().unwrap()));
        row["signature"] = signature.clone();
        let history = json!({"recorded_native_cash_flows":[{"signature":signature}],
            "recorded_settlements":[{"signed_transaction":row["transaction_base64"],"review":{
            "signature":signature,"slot":"43","outcome":"succeeded","network_fee_lamports":"2",
            "native_account_effects":[{"pre_lamports":"100","post_lamports":"91"},
            {"pre_lamports":"10","post_lamports":"17"},{"pre_lamports":"1","post_lamports":"1"}],
            "pre_token_balances":[],"post_token_balances":[]}}]});
        current["wallet_activity"]["signatures"][0]["signature"] = signature;
        assert!(capture(&current, &json!({}), &config(), 11).is_err());
        assert_eq!(capture(&current, &history, &config(), 11).unwrap().len(), 0);
        let mut missing = history.clone();
        missing["recorded_native_cash_flows"] = json!([]);
        assert!(capture(&current, &missing, &config(), 11).is_err());
        let mut mixed = current.clone();
        let mut extra = activity(false);
        extra["wallet_activity"]["transactions"][0]["slot"] = json!("44");
        extra["wallet_activity"]["signatures"][0]["slot"] = json!("44");
        mixed["wallet_activity"]["signatures"]
            .as_array_mut()
            .unwrap()
            .push(extra["wallet_activity"]["signatures"][0].clone());
        mixed["wallet_activity"]["transactions"]
            .as_array_mut()
            .unwrap()
            .push(extra["wallet_activity"]["transactions"][0].clone());
        let external = capture(&mixed, &history, &config(), 11).unwrap();
        assert_eq!(external.len(), 1);
        assert_eq!(external[0].review["execution_slot"], "44");
        assert_eq!(external[0].review["outcome"], "failed");
        assert_eq!(external[0].review["net_change_lamports"], "-2");
        let entry = current["wallet_activity"]["signatures"][0].clone();
        let row = current["wallet_activity"]["transactions"][0].clone();
        current["wallet_activity"]["signatures"]
            .as_array_mut()
            .unwrap()
            .push(entry);
        current["wallet_activity"]["transactions"]
            .as_array_mut()
            .unwrap()
            .push(row);
        assert!(capture(&current, &history, &config(), 11).is_err());
    }

    #[test]
    fn recorded_activity_requires_exact_wire_outcome_fee_and_every_balance() {
        let current = activity(true);
        let normalized = collected(&current["wallet_activity"], 44).unwrap();
        let mut row = normalized["transactions"][0].clone();
        let token = json!({"accountIndex":1,"mint":payer(),"owner":payer(),"programId":payer(),
            "uiTokenAmount":{"amount":"7","decimals":6}});
        row["pre_token_balances"] = json!([token]);
        row["post_token_balances"] = json!([token]);
        let retained_token = json!({"account_index":1,"mint":payer(),"owner":payer(),
            "program_id":payer(),"raw_amount":"7","decimals":6});
        let history = json!({"recorded_settlements":[{"signed_transaction":row["transaction_base64"],"review":{
            "signature":row["collected_signature"],"slot":"43","outcome":"succeeded","network_fee_lamports":"2",
            "native_account_effects":[{"pre_lamports":"100","post_lamports":"91"},
            {"pre_lamports":"10","post_lamports":"17"},{"pre_lamports":"1","post_lamports":"1"}],
            "pre_token_balances":[retained_token],"post_token_balances":[retained_token]}}]});
        assert!(recorded(&row, &history).unwrap());
        for (path, bad) in [
            ("/transaction_base64", json!("changed")),
            ("/slot", json!("44")),
            ("/outcome", json!("failed")),
            ("/network_fee_lamports", json!("3")),
            ("/pre_balances/1", json!("11")),
            ("/post_balances/2", json!("2")),
            (
                "/pre_token_balances/0/owner",
                json!(Address::SYSTEM_PROGRAM),
            ),
            ("/post_token_balances/0/uiTokenAmount/amount", json!("8")),
            ("/pre_token_balances", Value::Null),
            ("/post_token_balances", json!([])),
        ] {
            let mut bad_row = row.clone();
            *bad_row.pointer_mut(path).unwrap() = bad;
            assert!(recorded(&bad_row, &history).is_err(), "{path}");
        }
        let mut unknown = row.clone();
        unknown["collected_signature"] = json!("unknown");
        assert!(!recorded(&unknown, &history).unwrap());
        unknown
            .as_object_mut()
            .unwrap()
            .remove("collected_signature");
        assert!(!recorded(&unknown, &history).unwrap());
        assert!(!recorded(&row, &json!({})).unwrap());
        let mut missing = history.clone();
        missing["recorded_settlements"][0]["review"]["native_account_effects"] = Value::Null;
        assert!(recorded(&row, &missing).is_err());
    }

    #[test]
    fn retained_native_transfers_are_reverified_and_combined_exactly_once() {
        let current = activity(true);
        let mut records = capture(&current, &json!({}), &config(), 11).unwrap();
        assert!(records[0].evidence.get("raw_metadata").is_none());
        let expected = vec![records[0].review.clone()];
        assert_eq!(
            combine(&current, &json!({}), &config(), 11, &records).unwrap(),
            expected
        );
        let mut no_packet = current.clone();
        no_packet.as_object_mut().unwrap().remove("wallet_activity");
        assert_eq!(
            combine(&no_packet, &json!({}), &config(), 11, &records).unwrap(),
            expected
        );
        let original = records[0].clone();
        records[0].review["net_change_lamports"] = json!("-10");
        assert!(combine(&no_packet, &json!({}), &config(), 11, &records).is_err());
        records[0] = original.clone();
        records[0].wallet = Address::new([9; 32]);
        assert!(combine(&no_packet, &json!({}), &config(), 11, &records).is_err());
        records[0] = original.clone();
        records[0].signed_transaction = "changed".into();
        assert!(combine(&no_packet, &json!({}), &config(), 11, &records).is_err());
        records[0] = original.clone();
        let history =
            json!({"recorded_native_cash_flows":[{"signature":original.review["signature"]}]});
        assert!(combine(&no_packet, &history, &config(), 11, &records).is_err());
        no_packet["native_sol"]["slot"] = json!("42");
        assert!(combine(&no_packet, &json!({}), &config(), 11, &records).is_err());
        no_packet["native_sol"]["slot"] = json!("43");
        assert_eq!(
            combine(&no_packet, &json!({}), &config(), 11, &records).unwrap(),
            expected
        );
        // Different raw metadata for the same valid signed transfer must not replace history.
        records[0].evidence["pre_balances"] = json!(["101", "10", "1"]);
        records[0].evidence["post_balances"] = json!(["92", "17", "1"]);
        records[0].review = transaction(&records[0].evidence, config().wallet).unwrap();
        assert!(combine(&current, &json!({}), &config(), 11, &records).is_err());
    }

    #[test]
    fn collected_activity_reuses_signed_transfer_checks_including_failed_fees() {
        for succeeded in [true, false] {
            let rows = review(&activity(succeeded), &json!({}), &config(), 11).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0]["net_change_lamports"],
                if succeeded { "-9" } else { "-2" }
            );
            assert_eq!(
                rows[0]["wallet_transfer_change_lamports"],
                if succeeded { "-7" } else { "0" }
            );
            assert_eq!(rows[0]["signature_verified_locally"], true);
        }
        let mut boundary = activity(true);
        boundary["wallet_activity"]["through_slot_inclusive"] = json!("43");
        boundary["native_sol"]["slot"] = json!("43");
        assert_eq!(
            review(&boundary, &json!({}), &config(), 11).unwrap().len(),
            1
        );
        let mut empty = activity(true);
        empty["wallet_activity"]["signatures"] = json!([]);
        empty["wallet_activity"]["transactions"] = json!([]);
        assert_eq!(
            review(&empty, &json!({}), &config(), 11).unwrap(),
            Vec::<Value>::new()
        );
        empty["wallet_activity"]["after_slot_exclusive"] = json!("44");
        assert!(review(&empty, &json!({}), &config(), 11).is_err());
    }

    #[test]
    fn collected_activity_refuses_incomplete_conflicting_or_unbound_evidence() {
        let valid = activity(true);
        for (path, bad) in [
            ("/wallet_activity/coverage", json!("complete_wallet")),
            ("/wallet_activity/signature_scan_finished", json!(false)),
            ("/wallet_activity/transaction_fetch_finished", Value::Null),
            ("/wallet_activity/after_slot_exclusive", json!("44")),
            ("/wallet_activity/after_slot_exclusive", json!("43")),
            ("/wallet_activity/through_slot_inclusive", json!("42")),
            ("/wallet_activity/through_slot_inclusive", json!("45")),
            ("/wallet_activity/signatures", json!([])),
            ("/wallet_activity/transactions/0/slot", json!("44")),
            (
                "/wallet_activity/signatures/0/signature",
                json!(Signature::new([5; 64])),
            ),
            ("/wallet_activity/signatures/0/outcome", json!("failed")),
            ("/wallet_activity/transactions/0/raw_metadata/fee", json!(3)),
            (
                "/wallet_activity/transactions/0/raw_metadata/err",
                json!(false),
            ),
            (
                "/wallet_activity/transactions/0/raw_metadata/err",
                json!({"error":1}),
            ),
            (
                "/wallet_activity/transactions/0/raw_metadata/preBalances/0",
                json!("100"),
            ),
            (
                "/wallet_activity/transactions/0/raw_metadata/postBalances/0",
                json!(92),
            ),
            (
                "/wallet_activity/transactions/0/raw_metadata/preTokenBalances",
                Value::Null,
            ),
            ("/wallet_activity/wallet", json!(Address::new([8; 32]))),
            ("/wallet_activity/read_completed_at_unix_secs", json!(12)),
        ] {
            let mut bad_packet = valid.clone();
            *bad_packet.pointer_mut(path).unwrap() = bad;
            assert!(
                review(&bad_packet, &json!({}), &config(), 11).is_err(),
                "{path}"
            );
        }
        let mut newer = valid.clone();
        newer["wallet_activity"]["signatures"][0]["slot"] = json!("45");
        newer["wallet_activity"]["transactions"][0]["slot"] = json!("45");
        assert!(review(&newer, &json!({}), &config(), 11).is_err());
        let mut conflict = valid.clone();
        conflict["native_transfers"] = json!({});
        assert!(review(&conflict, &json!({}), &config(), 11).is_err());
        let mut changed_identity = valid.clone();
        let signature = json!(Signature::new([5; 64]));
        changed_identity["wallet_activity"]["signatures"][0]["signature"] = signature.clone();
        changed_identity["wallet_activity"]["transactions"][0]["signature"] = signature;
        assert!(review(&changed_identity, &json!({}), &config(), 11).is_err());
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
        let mut duplicate = edit(
            &value,
            |b| {
                let key = b[69..101].to_vec();
                b[101..133].copy_from_slice(&key);
            },
            true,
        );
        duplicate["post_balances"] = json!(["98", "10", "1"]);
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
        let too_large = packet(&[(1, 0); 43], &pre, &post, true);
        let bytes = b64::decode(too_large["transaction_base64"].as_str().unwrap()).unwrap();
        assert_eq!(bytes.len(), 1249);
        assert!(radar_signer::tx::decode(&bytes).is_ok());
        assert!(transaction(&too_large, payer()).is_err());
        let value = packet(&[(0, 7)], &[100, 1], &[98, 1], true);
        assert_eq!(
            transaction(&value, payer()).unwrap()["net_change_lamports"],
            "-2"
        );
    }

    #[test]
    fn native_transfer_packet_requires_identity_current_reads_and_unique_unrecorded_signatures() {
        let config = config();
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
