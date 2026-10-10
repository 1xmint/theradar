// SPDX-License-Identifier: Apache-2.0
//! Consistency of provider balances with signed intent; no execution provenance.

use radar_signer::tx::Message;
use radar_types::Address;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(PartialEq, Eq)]
struct Balance {
    mint: Address,
    owner: Address,
    program: Address,
    decimals: u8,
    amount: u64,
}

fn balances(
    meta: &Value,
    field: &str,
    count: usize,
) -> Result<BTreeMap<usize, Balance>, &'static str> {
    let mut result = BTreeMap::new();
    for row in meta[field].as_array().ok_or("token_balances_missing")? {
        let index = row["accountIndex"].as_u64().ok_or("token_index_invalid")?;
        let index = usize::try_from(index).map_err(|_| "token_index_invalid")?;
        let address = |field: &str| -> Result<Address, &'static str> {
            row[field]
                .as_str()
                .and_then(|s| s.parse().ok())
                .ok_or("token_identity_missing")
        };
        let balance = Balance {
            mint: address("mint")?,
            owner: address("owner")?,
            program: address("programId")?,
            decimals: row["uiTokenAmount"]["decimals"]
                .as_u64()
                .and_then(|v| u8::try_from(v).ok())
                .ok_or("token_units_invalid")?,
            amount: row["uiTokenAmount"]["amount"]
                .as_str()
                .and_then(|v| v.parse().ok())
                .ok_or("token_amount_invalid")?,
        };
        if index >= count || result.insert(index, balance).is_some() {
            return Err("token_index_duplicate_or_outside_message");
        }
    }
    Ok(result)
}

fn native(meta: &Value, count: usize, fee: u64) -> Result<(), &'static str> {
    let pre = meta["preBalances"]
        .as_array()
        .ok_or("native_balances_missing")?;
    let post = meta["postBalances"]
        .as_array()
        .ok_or("native_balances_missing")?;
    if pre.len() != count || post.len() != count {
        return Err("native_balance_extent");
    }
    for (index, (pre, post)) in pre.iter().zip(post).enumerate() {
        let pre = pre.as_u64().ok_or("native_balance_invalid")?;
        let post = post.as_u64().ok_or("native_balance_invalid")?;
        let expected = if index == 0 {
            pre.checked_sub(fee)
        } else {
            Some(pre)
        };
        if expected != Some(post) {
            return Err("native_effect_unexplained");
        }
    }
    Ok(())
}

fn consistent(message: &Message, row: &Value, intents: &[Value]) -> Result<Value, &'static str> {
    if intents.is_empty()
        || intents
            .iter()
            .any(|intent| intent["kind"] != "spl_token_transfer_checked_intent")
    {
        return Err("unsupported_top_level_activity");
    }
    let meta = &row["raw_metadata"];
    if !meta["innerInstructions"]
        .as_array()
        .is_some_and(Vec::is_empty)
    {
        return Err("inner_activity_unknown_or_present");
    }
    let succeeded = match (row["outcome"].as_str(), meta.get("err")) {
        (Some("succeeded"), Some(Value::Null)) => true,
        (Some("failed"), Some(Value::Object(_) | Value::String(_))) => false,
        _ => return Err("reported_outcome_differs"),
    };
    let fee = meta["fee"].as_u64().ok_or("network_fee_missing")?;
    if row["network_fee_lamports"]
        .as_str()
        .and_then(|v| v.parse::<u64>().ok())
        != Some(fee)
    {
        return Err("network_fee_differs");
    }
    native(meta, message.accounts.len(), fee)?;
    let pre = balances(meta, "preTokenBalances", message.accounts.len())?;
    let post = balances(meta, "postTokenBalances", message.accounts.len())?;
    if pre.len() != post.len() {
        return Err("token_account_creation_or_closure_unknown");
    }
    let mut expected: BTreeMap<_, _> = pre
        .iter()
        .map(|(index, balance)| (*index, balance.amount))
        .collect();
    for intent in intents {
        let account = |field: &str| -> Result<usize, &'static str> {
            let address: Address = serde_json::from_value(intent[field].clone())
                .map_err(|_| "intent_account_invalid")?;
            message
                .accounts
                .iter()
                .position(|key| key == address.as_bytes())
                .ok_or("intent_account_missing")
        };
        let source = account("source_account")?;
        let destination = account("destination_account")?;
        for index in [source, destination] {
            let balance = pre.get(&index).ok_or("transfer_token_balance_missing")?;
            if json!(balance.mint) != intent["mint"]
                || json!(balance.program) != intent["program"]
                || json!(balance.decimals) != intent["requested_decimals"]
            {
                return Err("transfer_token_identity_differs");
            }
        }
        let amount = intent["requested_raw_amount"]
            .as_str()
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or("intent_amount_invalid")?;
        if succeeded {
            let source = expected
                .get_mut(&source)
                .ok_or("transfer_token_balance_missing")?;
            *source = source
                .checked_sub(amount)
                .ok_or("intermediate_token_underflow")?;
            let destination = expected
                .get_mut(&destination)
                .ok_or("transfer_token_balance_missing")?;
            *destination = destination
                .checked_add(amount)
                .ok_or("intermediate_token_overflow")?;
        }
    }
    let mut changes = Vec::new();
    for (index, before) in &pre {
        let after = post.get(index).ok_or("token_account_set_differs")?;
        if before.mint != after.mint
            || before.owner != after.owner
            || before.program != after.program
            || before.decimals != after.decimals
        {
            return Err("token_identity_changed");
        }
        if expected.get(index) != Some(&after.amount) {
            return Err("token_effect_unexplained");
        }
        changes.push(json!({"account":Address::new(message.accounts[*index]),"mint":before.mint,"program":before.program,"reported_owner":before.owner,
            "decimals":before.decimals,"reported_pre_raw":before.amount.to_string(),"reported_post_raw":after.amount.to_string(),
            "reported_change_raw":(i128::from(after.amount)-i128::from(before.amount)).to_string()}));
    }
    Ok(
        json!({"status":"consistent_with_signed_transfer_intents","reported_token_changes":changes,
        "reported_network_fee_lamports":fee.to_string(),"execution_effects_verified":false,"account_ownership_verified":false,
        "metadata_verified_independently":false,"portfolio_state_updated":false}),
    )
}

pub(super) fn review(message: &Message, row: &Value, intents: &[Value]) -> Value {
    consistent(message, row, intents).unwrap_or_else(|reason| json!({"status":"unresolved","reason":reason,
        "execution_effects_verified":false,"account_ownership_verified":false,"metadata_verified_independently":false,
        "portfolio_state_updated":false}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_signer::tx::Instruction;

    fn fixture() -> (Message, Value) {
        let program: Address = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        let mut data = vec![12];
        data.extend(4_u64.to_le_bytes());
        data.push(6);
        let message = Message {
            required_signatures: 1,
            accounts: vec![
                [1; 32],
                [2; 32],
                [3; 32],
                [4; 32],
                program.to_bytes(),
                [6; 32],
            ],
            recent_blockhash: [0; 32],
            message_offset: 65,
            versioned: false,
            instructions: vec![Instruction {
                program_id: program.to_bytes(),
                accounts: vec![[2; 32], [4; 32], [3; 32], [1; 32]],
                data,
            }],
        };
        let balance = |index, amount: &str| json!({"accountIndex":index,"mint":Address::new([4;32]),"owner":Address::new([1;32]),"programId":program,"uiTokenAmount":{"amount":amount,"decimals":6}});
        let row = json!({"outcome":"succeeded","network_fee_lamports":"2","raw_metadata":{"err":null,"fee":2,"innerInstructions":[],
            "preBalances":[100,0,0,0,0,0],"postBalances":[98,0,0,0,0,0],
            "preTokenBalances":[balance(1,"10"),balance(2,"3"),balance(5,"9")],
            "postTokenBalances":[balance(1,"6"),balance(2,"7"),balance(5,"9")]}});
        (message, row)
    }
    fn check(message: &Message, row: &Value) -> Value {
        review(message, row, &super::super::token_intents::review(message))
    }
    #[test]
    fn exact_transfers_failed_execution_and_intermediate_self_debits_are_distinct() {
        let (mut message, mut row) = fixture();
        let report = check(&message, &row);
        assert_eq!(report["status"], "consistent_with_signed_transfer_intents");
        assert_eq!(
            report["reported_token_changes"][0]["reported_change_raw"],
            "-4"
        );
        assert_eq!(
            report["reported_token_changes"][1]["reported_change_raw"],
            "4"
        );
        assert_eq!(
            report["reported_token_changes"][2]["reported_change_raw"],
            "0"
        );
        assert_eq!(report["reported_network_fee_lamports"], "2");
        for flag in [
            "execution_effects_verified",
            "metadata_verified_independently",
            "account_ownership_verified",
            "portfolio_state_updated",
        ] {
            assert_eq!(report[flag], false);
        }
        message.instructions.push(message.instructions[0].clone());
        row["raw_metadata"]["postTokenBalances"][0]["uiTokenAmount"]["amount"] = json!("2");
        row["raw_metadata"]["postTokenBalances"][1]["uiTokenAmount"]["amount"] = json!("11");
        assert_eq!(
            check(&message, &row)["status"],
            "consistent_with_signed_transfer_intents"
        );
        message.instructions.push(message.instructions[0].clone());
        assert_eq!(
            check(&message, &row)["reason"],
            "intermediate_token_underflow"
        );
        row["outcome"] = json!("failed");
        row["raw_metadata"]["err"] = json!({"InstructionError":[2,1]});
        row["raw_metadata"]["postTokenBalances"] = row["raw_metadata"]["preTokenBalances"].clone();
        assert_eq!(
            check(&message, &row)["status"],
            "consistent_with_signed_transfer_intents"
        );
        message.instructions.truncate(1);
        message.instructions[0].accounts[2] = message.instructions[0].accounts[0];
        row["outcome"] = json!("succeeded");
        row["raw_metadata"]["err"] = Value::Null;
        message.instructions[0].data[1..9].copy_from_slice(&10_u64.to_le_bytes());
        assert_eq!(
            check(&message, &row)["status"],
            "consistent_with_signed_transfer_intents"
        );
        message.instructions[0].data[1..9].copy_from_slice(&11_u64.to_le_bytes());
        assert_eq!(
            check(&message, &row)["reason"],
            "intermediate_token_underflow"
        );
        let (message, mut row) = fixture();
        row["raw_metadata"]["preTokenBalances"][1]["uiTokenAmount"]["amount"] =
            json!(u64::MAX.to_string());
        assert_eq!(
            check(&message, &row)["reason"],
            "intermediate_token_overflow"
        );
    }
    #[test]
    fn incomplete_conflicting_and_unexplained_metadata_remains_unresolved() {
        let (message, row) = fixture();
        for (pointer, value) in [
            ("/raw_metadata/innerInstructions", Value::Null),
            (
                "/raw_metadata/innerInstructions",
                json!([{"index":0,"instructions":[]}]),
            ),
            ("/raw_metadata/err", json!({})),
            ("/network_fee_lamports", json!("3")),
            ("/raw_metadata/fee", Value::Null),
            ("/raw_metadata/preBalances", json!([100])),
            ("/raw_metadata/postBalances", json!([98])),
            ("/raw_metadata/postBalances/1", json!(1)),
            ("/raw_metadata/postBalances/0", json!(99)),
            ("/raw_metadata/preBalances/0", json!(1)),
            ("/raw_metadata/preTokenBalances", Value::Null),
            ("/raw_metadata/postTokenBalances", json!([])),
            ("/raw_metadata/postTokenBalances/0/accountIndex", json!(5)),
            ("/raw_metadata/postTokenBalances/0/accountIndex", json!(6)),
            ("/raw_metadata/preTokenBalances/0/accountIndex", json!(0)),
            (
                "/raw_metadata/preTokenBalances/0/uiTokenAmount/amount",
                json!("18446744073709551616"),
            ),
            (
                "/raw_metadata/preTokenBalances/0/uiTokenAmount/decimals",
                json!(256),
            ),
            (
                "/raw_metadata/postTokenBalances/0/owner",
                json!(Address::new([7; 32])),
            ),
            (
                "/raw_metadata/postTokenBalances/0/mint",
                json!(Address::new([7; 32])),
            ),
            (
                "/raw_metadata/postTokenBalances/0/programId",
                json!(Address::new([7; 32])),
            ),
            (
                "/raw_metadata/postTokenBalances/0/uiTokenAmount/decimals",
                json!(7),
            ),
            (
                "/raw_metadata/preTokenBalances/0/mint",
                json!(Address::new([7; 32])),
            ),
            (
                "/raw_metadata/preTokenBalances/0/programId",
                json!(Address::new([7; 32])),
            ),
            (
                "/raw_metadata/preTokenBalances/0/uiTokenAmount/decimals",
                json!(7),
            ),
            (
                "/raw_metadata/postTokenBalances/2/uiTokenAmount/amount",
                json!("10"),
            ),
        ] {
            let mut bad = row.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            let report = check(&message, &bad);
            assert_eq!(report["status"], "unresolved", "{pointer}");
            for flag in [
                "execution_effects_verified",
                "metadata_verified_independently",
                "account_ownership_verified",
                "portfolio_state_updated",
            ] {
                assert_eq!(report[flag], false);
            }
            assert!(report.get("reported_token_changes").is_none());
        }
        let mut unsupported = message.clone();
        unsupported.instructions[0].program_id = [8; 32];
        assert_eq!(
            check(&unsupported, &row)["reason"],
            "unsupported_top_level_activity"
        );
        unsupported.instructions.clear();
        assert_eq!(
            check(&unsupported, &row)["reason"],
            "unsupported_top_level_activity"
        );
        paired_identity_and_extra_index_cases(&message, &row);
    }

    fn paired_identity_and_extra_index_cases(message: &Message, row: &Value) {
        for field in ["mint", "programId", "uiTokenAmount/decimals"] {
            let mut bad = row.clone();
            for side in ["preTokenBalances", "postTokenBalances"] {
                *bad.pointer_mut(&format!("/raw_metadata/{side}/0/{field}"))
                    .unwrap() = if field.ends_with("decimals") {
                    json!(7)
                } else {
                    json!(Address::new([7; 32]))
                };
            }
            assert_eq!(
                check(message, &bad)["reason"],
                "transfer_token_identity_differs"
            );
        }
        for index in [1, 6] {
            let mut bad = row.clone();
            for side in ["preTokenBalances", "postTokenBalances"] {
                bad["raw_metadata"][side][2]["accountIndex"] = json!(index);
            }
            assert_eq!(
                check(message, &bad)["reason"],
                "token_index_duplicate_or_outside_message"
            );
        }
        let mut extra = row.clone();
        let mut balance = extra["raw_metadata"]["postTokenBalances"][2].clone();
        balance["accountIndex"] = json!(0);
        extra["raw_metadata"]["postTokenBalances"]
            .as_array_mut()
            .unwrap()
            .push(balance);
        assert_eq!(
            check(message, &extra)["reason"],
            "token_account_creation_or_closure_unknown"
        );
    }
}
