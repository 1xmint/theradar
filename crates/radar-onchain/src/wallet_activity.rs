// SPDX-License-Identifier: Apache-2.0
//! Bounded finalized address history. Collection does not establish wallet coverage.

use std::collections::BTreeSet;

use radar_types::{Address, Signature, b64};
use serde_json::{Value, json};

use crate::{Budget, Exhausted, RpcClient, RpcError};

const PAGE_SIZE: usize = 32;

fn stopped(error: Exhausted) -> &'static str {
    match error {
        Exhausted::Calls => "call_budget",
        Exhausted::Pages => "page_budget",
        Exhausted::Deadline => "deadline",
    }
}

fn outcome(value: &Value) -> Result<&'static str, String> {
    match value.get("err") {
        Some(Value::Null) => Ok("succeeded"),
        Some(Value::Object(_) | Value::String(_)) => Ok("failed"),
        _ => Err("activity execution outcome missing or malformed".into()),
    }
}

fn transaction(raw: &Value, entry: &Value) -> Result<Value, String> {
    if raw["slot"].as_u64().map(|slot| slot.to_string()).as_deref() != entry["slot"].as_str() {
        return Err("activity transaction slot differs from signature entry".into());
    }
    let encoded = raw["transaction"]
        .as_array()
        .filter(|array| array.len() == 2 && array[1] == "base64")
        .and_then(|array| array[0].as_str())
        .ok_or("activity transaction bytes missing")?;
    let bytes = b64::decode(encoded).ok_or("invalid activity transaction base64")?;
    // Bind the first wire signature without claiming to decode or verify the message.
    // Packet size plus the full signature extent exclude multi-byte counts.
    let count = usize::from(*bytes.first().ok_or("empty activity transaction")?);
    if bytes.len() > 1232 || count == 0 || bytes.len() < 1 + 64 * count + 3 {
        return Err("unsupported activity transaction signature extent".into());
    }
    let signature = Signature::new(bytes[1..65].try_into().expect("checked extent"));
    if entry["signature"] != signature.to_string() || outcome(&raw["meta"])? != entry["outcome"] {
        return Err("activity transaction signature or outcome differs from enumeration".into());
    }
    let fee = raw["meta"]["fee"]
        .as_u64()
        .ok_or("activity transaction fee missing")?;
    Ok(
        json!({"signature":signature.to_string(),"slot":entry["slot"],
        "outcome":entry["outcome"],"transaction_base64":b64::encode(&bytes),
        "network_fee_lamports":fee.to_string(),"raw_metadata":raw["meta"],
        "classification":"unresolved","signature_verified_locally":false,
        "message_decoded_locally":false}),
    )
}

/// Collects provider-reported signatures and raw transactions in `(after, through]`.
///
/// # Errors
/// Invalid bounds or inconsistent/malformed entries. Provider failures and budget
/// exhaustion produce explicitly incomplete evidence. No signing or journal write occurs.
pub fn read(
    rpc: &RpcClient,
    wallet: Address,
    after: u64,
    through: u64,
    budget: &mut Budget,
) -> Result<Value, String> {
    if after >= through {
        return Err("activity needs after-slot below through-slot".into());
    }
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    let mut previous_slot = u64::MAX;
    let mut before: Option<String> = None;
    let (scan_finished, scan_stop) = loop {
        if let Err(error) = budget.take_page() {
            break (false, stopped(error));
        }
        let mut options =
            json!({"limit":PAGE_SIZE,"commitment":"finalized","minContextSlot":through});
        if let Some(cursor) = &before {
            options["before"] = json!(cursor);
        }
        let page: Value = match rpc.call(
            budget,
            "getSignaturesForAddress",
            &json!([wallet.to_string(), options]),
        ) {
            Ok(page) => page,
            Err(RpcError::Stopped(error)) => break (false, stopped(error)),
            Err(_) => break (false, "provider_error"),
        };
        let rows = page
            .as_array()
            .filter(|rows| rows.len() <= PAGE_SIZE)
            .ok_or("invalid activity signature page")?;
        let mut reached = false;
        for row in rows {
            let signature: Signature = row["signature"]
                .as_str()
                .and_then(|value| value.parse().ok())
                .ok_or("invalid activity signature")?;
            let signature = signature.to_string();
            let slot = row["slot"]
                .as_u64()
                .ok_or("invalid activity signature slot")?;
            if row["confirmationStatus"] != "finalized"
                || slot > previous_slot
                || !seen.insert(signature.clone())
            {
                return Err("activity page is not finalized, ordered and unique".into());
            }
            let result = outcome(row)?;
            previous_slot = slot;
            before = Some(signature.clone());
            if slot <= after {
                reached = true;
            } else if slot <= through {
                entries
                    .push(json!({"signature":signature,"slot":slot.to_string(),"outcome":result}));
            }
        }
        if reached {
            break (true, "lower_boundary_reached");
        }
        if rows.len() < PAGE_SIZE {
            break (true, "provider_history_exhausted");
        }
    };
    let mut transactions = Vec::new();
    let mut fetch_stop = "all_enumerated_transactions_fetched";
    for entry in &entries {
        let raw: Value = match rpc.call(
            budget,
            "getTransaction",
            &json!([entry["signature"],
            {"encoding":"base64","commitment":"finalized","maxSupportedTransactionVersion":0}]),
        ) {
            Ok(raw) => raw,
            Err(RpcError::Stopped(error)) => {
                fetch_stop = stopped(error);
                break;
            }
            Err(_) => {
                fetch_stop = "transaction_unavailable";
                break;
            }
        };
        transactions.push(transaction(&raw, entry)?);
    }
    Ok(
        json!({"version":1,"authority":"read_only","commitment":"finalized","wallet":wallet.to_string(),
        "after_slot_exclusive":after.to_string(),"through_slot_inclusive":through.to_string(),
        "signature_scan_finished":scan_finished,"signature_scan_stop":scan_stop,
        "transaction_fetch_finished":transactions.len()==entries.len(),"transaction_fetch_stop":fetch_stop,
        "signatures":entries,"transactions":transactions,"coverage":"provider_reported_address_history",
        "wallet_coverage_complete":false,"economic_reconciliation_complete":false,
        "portfolio_state_updated":false,"reservation_released":false,
        "address_membership_verified_locally":false}),
    )
}

/// Collect the wallet and at most fifteen other known addresses with one shared
/// budget. Targets are caller-supplied, not an ownership or completeness proof.
///
/// # Errors
/// Invalid bounds/target count or inconsistent duplicate evidence across scans.
pub fn read_known_accounts(
    rpc: &RpcClient,
    wallet: Address,
    accounts: &[Address],
    after: u64,
    through: u64,
    budget: &mut Budget,
) -> Result<Value, String> {
    let addresses: BTreeSet<_> = accounts.iter().copied().chain([wallet]).collect();
    if addresses.len() > 16 {
        return Err("activity supports at most sixteen known addresses including wallet".into());
    }
    let mut scans = Vec::new();
    let mut signatures = std::collections::BTreeMap::<String, Value>::new();
    let mut transactions = std::collections::BTreeMap::<String, Value>::new();
    let mut reported_for = std::collections::BTreeMap::<String, BTreeSet<Address>>::new();
    for address in &addresses {
        let mut scan = read(rpc, *address, after, through, budget)?;
        for entry in scan["signatures"]
            .as_array()
            .ok_or("activity signatures missing")?
        {
            merge_known(&mut signatures, entry)?;
            reported_for
                .entry(
                    entry["signature"]
                        .as_str()
                        .ok_or("activity signature missing")?
                        .to_owned(),
                )
                .or_default()
                .insert(*address);
        }
        for entry in scan["transactions"]
            .as_array()
            .ok_or("activity transactions missing")?
        {
            merge_known(&mut transactions, entry)?;
        }
        // The single-address reader's field names its query target. Preserve the
        // configured wallet only at the outer boundary, not on a token-account scan.
        scan.as_object_mut()
            .ok_or("activity scan missing")?
            .remove("wallet");
        scan["queried_address"] = json!(address);
        scans.push(scan);
    }
    let entries: Vec<_> = signatures
        .into_iter()
        .map(|(signature, mut entry)| {
            entry["reported_for_addresses"] = json!(reported_for.get(&signature));
            entry
        })
        .collect();
    Ok(
        json!({"version":1,"authority":"read_only","commitment":"finalized","wallet":wallet,
        "after_slot_exclusive":after.to_string(),"through_slot_inclusive":through.to_string(),
        "queried_addresses":addresses,"address_scans":scans,
        "signature_scan_finished":scans.iter().all(|scan| scan["signature_scan_finished"] == true),
        "transaction_fetch_finished":scans.iter().all(|scan| scan["transaction_fetch_finished"] == true),
        "signatures":entries,"transactions":transactions.into_values().collect::<Vec<_>>(),
        "coverage":"provider_reported_known_address_history","targets_owned_by_wallet_verified":false,
        "wallet_coverage_complete":false,"economic_reconciliation_complete":false,
        "portfolio_state_updated":false,"reservation_released":false}),
    )
}

fn merge_known(
    records: &mut std::collections::BTreeMap<String, Value>,
    entry: &Value,
) -> Result<(), String> {
    let key = entry["signature"]
        .as_str()
        .ok_or("activity signature missing")?
        .to_owned();
    if records.get(&key).is_some_and(|prior| prior != entry) {
        return Err("activity evidence conflicts across known address scans".into());
    }
    records.insert(key, entry.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::Transport;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct Fixture {
        answers: Mutex<VecDeque<Value>>,
        calls: Arc<Mutex<Vec<Value>>>,
    }
    impl Transport for Fixture {
        fn post(&self, _: &str, body: String) -> Result<String, String> {
            self.calls
                .lock()
                .unwrap()
                .push(serde_json::from_str(&body).unwrap());
            self.answers
                .lock()
                .unwrap()
                .pop_front()
                .map(|result| json!({"result":result}).to_string())
                .ok_or("fixture unavailable".into())
        }
    }
    fn client(answers: Vec<Value>) -> (RpcClient, Arc<Mutex<Vec<Value>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            RpcClient::with_transport(
                "http://fixture",
                Box::new(Fixture {
                    answers: Mutex::new(answers.into()),
                    calls: Arc::clone(&calls),
                }),
            ),
            calls,
        )
    }
    fn row(id: u8, slot: u64, failed: bool) -> Value {
        json!({"signature":Signature::new([id;64]).to_string(),"slot":slot,
            "err":if failed {json!({"InstructionError":[0,1]})} else {Value::Null},"confirmationStatus":"finalized"})
    }
    fn raw(id: u8, slot: u64, failed: bool) -> Value {
        let mut bytes = vec![1];
        bytes.extend([id; 64]);
        bytes.extend([1, 0, 0, 1]);
        bytes.extend([2; 32]); // Different from the queried owner: collection is not membership proof.
        bytes.extend([3; 32]);
        bytes.push(0);
        json!({"slot":slot,"version":"legacy","transaction":[b64::encode(&bytes),"base64"],
            "meta":{"fee":u64::MAX,"err":if failed {json!({"InstructionError":[0,1]})} else {Value::Null},
            "preBalances":[u64::MAX],"postBalances":[0],"preTokenBalances":[],"postTokenBalances":[]}})
    }
    fn budget(calls: u32, pages: u32) -> Budget {
        Budget::new(calls, pages, Duration::from_secs(20))
    }
    fn run(rpc: &RpcClient, budget: &mut Budget) -> Result<Value, String> {
        read(rpc, Address::new([0x55; 32]), 40, 50, budget)
    }

    #[test]
    fn finalized_interval_preserves_failures_raw_bytes_and_unknown_authority() {
        let transaction = raw(2, 41, true);
        let (rpc, calls) = client(vec![
            json!([row(1, 51, false), row(2, 41, true), row(3, 40, false)]),
            transaction.clone(),
        ]);
        let result = run(&rpc, &mut budget(3, 1)).unwrap();
        assert_eq!(result["signature_scan_finished"], true);
        assert_eq!(result["signature_scan_stop"], "lower_boundary_reached");
        assert_eq!(result["transaction_fetch_finished"], true);
        assert_eq!(result["signatures"].as_array().unwrap().len(), 1);
        assert_eq!(result["transactions"][0]["slot"], "41");
        assert_eq!(result["transactions"][0]["outcome"], "failed");
        assert_eq!(
            result["transactions"][0]["network_fee_lamports"],
            u64::MAX.to_string()
        );
        assert_eq!(
            result["transactions"][0]["transaction_base64"],
            transaction["transaction"][0]
        );
        assert_eq!(
            result["transactions"][0]["raw_metadata"],
            transaction["meta"]
        );
        assert_eq!(result["transactions"][0]["classification"], "unresolved");
        for flag in [
            "wallet_coverage_complete",
            "economic_reconciliation_complete",
            "portfolio_state_updated",
            "reservation_released",
            "address_membership_verified_locally",
        ] {
            assert_eq!(result[flag], false);
        }
        assert_eq!(
            result["transactions"][0]["signature_verified_locally"],
            false
        );
        assert_eq!(result["transactions"][0]["message_decoded_locally"], false);
        let calls = calls.lock().unwrap();
        assert_eq!(
            calls[0]["params"],
            json!([Address::new([0x55;32]).to_string(),{"limit":32,"commitment":"finalized","minContextSlot":50}])
        );
        assert_eq!(
            calls[1]["params"],
            json!([Signature::new([2;64]).to_string(),{"encoding":"base64","commitment":"finalized","maxSupportedTransactionVersion":0}])
        );
        let (rpc, _) = client(vec![json!([row(2, 50, false)]), raw(2, 50, false)]);
        let result = run(&rpc, &mut budget(2, 1)).unwrap();
        assert_eq!(result["transactions"][0]["slot"], "50");
        assert_eq!(result["transactions"][0]["outcome"], "succeeded");
        assert_eq!(result["signature_scan_stop"], "provider_history_exhausted");
    }

    #[test]
    fn full_pages_advance_cursor_and_filter_newer_slots_across_pages() {
        let page: Vec<_> = (1..=32).map(|id| row(id, 51, false)).collect();
        let (rpc, calls) = client(vec![
            json!(page),
            json!([row(33, 50, false), row(34, 40, false)]),
            raw(33, 50, false),
        ]);
        let result = run(&rpc, &mut budget(3, 2)).unwrap();
        assert_eq!(result["signatures"].as_array().unwrap().len(), 1);
        assert_eq!(result["transactions"].as_array().unwrap().len(), 1);
        assert_eq!(
            result["transactions"][0]["signature"],
            Signature::new([33; 64]).to_string()
        );
        assert_eq!(
            calls.lock().unwrap()[1]["params"][1]["before"],
            Signature::new([32; 64]).to_string()
        );
        let (rpc, _) = client(vec![json!([])]);
        let result = run(&rpc, &mut budget(1, 1)).unwrap();
        assert_eq!(result["signature_scan_finished"], true);
        assert_eq!(result["transaction_fetch_finished"], true);
        assert_eq!(result["wallet_coverage_complete"], false);
    }

    #[test]
    fn malformed_status_outcome_order_duplicates_and_bounds_refuse() {
        for (field, bad) in [
            ("signature", json!("bad")),
            ("slot", json!(-1)),
            ("confirmationStatus", Value::Null),
            ("confirmationStatus", json!("confirmed")),
            ("err", json!(true)),
        ] {
            let mut value = row(1, 45, false);
            value[field] = bad;
            let (rpc, _) = client(vec![json!([value])]);
            assert!(run(&rpc, &mut budget(2, 1)).is_err(), "{field}");
        }
        let mut missing = row(1, 45, false);
        missing.as_object_mut().unwrap().remove("err");
        for page in [
            json!([missing]),
            json!([row(1, 45, false), row(2, 46, false)]),
            json!([row(1, 45, false), row(1, 44, false)]),
            json!({}),
            json!((1..=33).map(|id| row(id, 45, false)).collect::<Vec<_>>()),
        ] {
            let (rpc, _) = client(vec![page]);
            assert!(run(&rpc, &mut budget(2, 1)).is_err());
        }
        let page: Vec<_> = (1..=32).map(|id| row(id, 51, false)).collect();
        for second in [json!([row(32, 51, false)]), json!([row(33, 52, false)])] {
            let (rpc, _) = client(vec![json!(page), second]);
            assert!(run(&rpc, &mut budget(3, 2)).is_err());
        }
        for (after, through) in [(50, 50), (51, 50)] {
            let (rpc, calls) = client(vec![]);
            assert!(
                read(
                    &rpc,
                    Address::new([1; 32]),
                    after,
                    through,
                    &mut budget(2, 1)
                )
                .is_err()
            );
            assert!(calls.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn exhausted_or_unavailable_reads_remain_explicitly_incomplete() {
        let page: Vec<_> = (1..=32).map(|id| row(id, 51, false)).collect();
        for (answers, mut budget, scan_stop, fetch_stop) in [
            (
                vec![],
                budget(1, 0),
                "page_budget",
                "all_enumerated_transactions_fetched",
            ),
            (
                vec![],
                budget(0, 1),
                "call_budget",
                "all_enumerated_transactions_fetched",
            ),
            (
                vec![],
                Budget::new(1, 1, Duration::ZERO),
                "deadline",
                "all_enumerated_transactions_fetched",
            ),
            (
                vec![],
                budget(1, 1),
                "provider_error",
                "all_enumerated_transactions_fetched",
            ),
            (
                vec![json!(page)],
                budget(2, 1),
                "page_budget",
                "all_enumerated_transactions_fetched",
            ),
            (
                vec![json!([row(1, 45, false)])],
                budget(1, 1),
                "provider_history_exhausted",
                "call_budget",
            ),
            (
                vec![json!([row(1, 45, false)]), Value::Null],
                budget(2, 1),
                "provider_history_exhausted",
                "transaction_unavailable",
            ),
        ] {
            let (rpc, _) = client(answers);
            let result = run(&rpc, &mut budget).unwrap();
            assert_eq!(result["signature_scan_stop"], scan_stop);
            assert_eq!(
                result["signature_scan_finished"],
                scan_stop == "provider_history_exhausted"
            );
            assert_eq!(result["transaction_fetch_stop"], fetch_stop);
            assert_eq!(
                result["transaction_fetch_finished"],
                fetch_stop == "all_enumerated_transactions_fetched"
            );
            assert_eq!(result["wallet_coverage_complete"], false);
        }
        let (rpc, _) = client(vec![
            json!([row(1, 45, false), row(2, 44, false)]),
            raw(1, 45, false),
        ]);
        let result = run(&rpc, &mut budget(2, 1)).unwrap();
        assert_eq!(result["transactions"].as_array().unwrap().len(), 1);
        assert_eq!(result["signatures"].as_array().unwrap().len(), 2);
        assert_eq!(result["transaction_fetch_finished"], false);
    }

    #[test]
    fn mismatched_transaction_identity_slot_outcome_encoding_and_fee_refuse() {
        for (pointer, bad) in [
            ("/slot", json!(46)),
            ("/transaction/1", json!("json")),
            ("/transaction/0", json!("bad")),
            ("/meta/err", json!({"failed":true})),
            ("/meta/fee", Value::Null),
        ] {
            let mut value = raw(1, 45, false);
            *value.pointer_mut(pointer).unwrap() = bad;
            let (rpc, _) = client(vec![json!([row(1, 45, false)]), value]);
            assert!(run(&rpc, &mut budget(2, 1)).is_err(), "{pointer}");
        }
        let (rpc, _) = client(vec![json!([row(1, 45, false)]), raw(2, 45, false)]);
        assert!(run(&rpc, &mut budget(2, 1)).is_err());
        for bytes in [
            vec![],
            vec![1; 67],
            vec![0; 68],
            vec![19; 1220],
            vec![1; 1233],
        ] {
            let mut value = raw(1, 45, false);
            value["transaction"][0] = json!(b64::encode(&bytes));
            let (rpc, _) = client(vec![json!([row(1, 45, false)]), value]);
            assert!(run(&rpc, &mut budget(2, 1)).is_err());
        }
    }

    #[test]
    fn signature_extent_and_packet_boundaries_are_distinct_from_message_validation() {
        let entry = json!({"signature":Signature::new([1;64]).to_string(),"slot":"45","outcome":"succeeded"});
        for (count, length, accepted) in [
            (1, 68, true),
            (1, 67, false),
            (18, 1156, true),
            (18, 1155, false),
            (19, 1220, true),
            (0, 68, false),
            (1, 1232, true),
            (1, 1233, false),
        ] {
            let mut bytes = vec![2; length];
            bytes[0] = count;
            bytes[1..65].fill(1);
            let mut value = raw(1, 45, false);
            value["transaction"][0] = json!(b64::encode(&bytes));
            assert_eq!(
                transaction(&value, &entry).is_ok(),
                accepted,
                "count {count}, bytes {length}"
            );
        }
        let mut value = raw(1, 45, false);
        value["transaction"]
            .as_array_mut()
            .unwrap()
            .push(json!("extra"));
        assert!(transaction(&value, &entry).is_err());
        value = raw(1, 45, false);
        value["meta"].as_object_mut().unwrap().remove("err");
        assert!(transaction(&value, &entry).is_err());
    }

    #[test]
    fn known_account_scans_merge_duplicates_and_preserve_query_identity() {
        let wallet = Address::new([0x55; 32]);
        let token = Address::new([0x56; 32]);
        let (rpc, calls) = client(vec![
            json!([row(5, 45, false)]),
            raw(5, 45, false),
            json!([row(6, 46, true), row(5, 45, false)]),
            raw(6, 46, true),
            raw(5, 45, false),
        ]);
        let report = read_known_accounts(
            &rpc,
            wallet,
            &[token, token, wallet],
            40,
            50,
            &mut budget(10, 3),
        )
        .unwrap();
        assert_eq!(report["wallet"], wallet.to_string());
        assert_eq!(report["queried_addresses"], json!([wallet, token]));
        assert_eq!(report["signatures"].as_array().unwrap().len(), 2);
        assert_eq!(report["transactions"].as_array().unwrap().len(), 2);
        let shared = report["signatures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["signature"] == Signature::new([5; 64]).to_string())
            .unwrap();
        assert_eq!(shared["reported_for_addresses"], json!([wallet, token]));
        assert_eq!(report["signature_scan_finished"], true);
        assert_eq!(report["transaction_fetch_finished"], true);
        assert_eq!(
            report["address_scans"][1]["queried_address"],
            token.to_string()
        );
        assert!(report["address_scans"][1].get("wallet").is_none());
        for flag in [
            "wallet_coverage_complete",
            "economic_reconciliation_complete",
            "portfolio_state_updated",
            "reservation_released",
            "targets_owned_by_wallet_verified",
        ] {
            assert_eq!(report[flag], false);
        }
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 5);
        assert_eq!(calls[0]["params"][0], wallet.to_string());
        assert_eq!(calls[2]["params"][0], token.to_string());
    }

    #[test]
    fn known_account_conflicting_signature_outcome_or_raw_metadata_refuses() {
        let wallet = Address::new([0x55; 32]);
        let token = Address::new([0x56; 32]);
        for change in 0..3 {
            let (slot, failed) = match change {
                0 => (46, false),
                1 => (45, true),
                _ => (45, false),
            };
            let mut second = raw(5, slot, failed);
            if change == 2 {
                second["meta"]["fee"] = json!(1);
            }
            let (rpc, _) = client(vec![
                json!([row(5, 45, false)]),
                raw(5, 45, false),
                json!([row(5, slot, failed)]),
                second,
            ]);
            assert_eq!(
                read_known_accounts(&rpc, wallet, &[token], 40, 50, &mut budget(10, 3))
                    .unwrap_err(),
                "activity evidence conflicts across known address scans"
            );
        }
    }

    #[test]
    fn known_account_scans_share_budgets_and_never_hide_unread_targets() {
        let wallet = Address::new([0x55; 32]);
        let token = Address::new([0x56; 32]);
        let (rpc, calls) = client(vec![json!([row(5, 45, false)]), raw(5, 45, false)]);
        let report =
            read_known_accounts(&rpc, wallet, &[token], 40, 50, &mut budget(2, 2)).unwrap();
        assert_eq!(calls.lock().unwrap().len(), 2);
        assert_eq!(report["signature_scan_finished"], false);
        assert_eq!(report["address_scans"].as_array().unwrap().len(), 2);
        assert_eq!(
            report["address_scans"][1]["signature_scan_stop"],
            "call_budget"
        );
        let (rpc, calls) = client(vec![]);
        let report =
            read_known_accounts(&rpc, wallet, &[token], 40, 50, &mut budget(10, 0)).unwrap();
        assert!(calls.lock().unwrap().is_empty());
        assert_eq!(report["signature_scan_finished"], false);
        assert_eq!(
            report["address_scans"][0]["signature_scan_stop"],
            "page_budget"
        );
        let (rpc, _) = client(vec![json!([row(5, 45, false)]), Value::Null, json!([])]);
        let report =
            read_known_accounts(&rpc, wallet, &[token], 40, 50, &mut budget(10, 3)).unwrap();
        assert_eq!(report["signature_scan_finished"], true);
        assert_eq!(report["transaction_fetch_finished"], false);
        assert_eq!(
            report["address_scans"][0]["transaction_fetch_stop"],
            "transaction_unavailable"
        );
    }

    #[test]
    fn known_account_target_count_and_bounds_refuse_before_rpc() {
        let wallet = Address::new([0x55; 32]);
        let accounts: Vec<_> = (1..17).map(|seed| Address::new([seed; 32])).collect();
        let (rpc, calls) = client(vec![]);
        assert!(read_known_accounts(&rpc, wallet, &accounts, 40, 50, &mut budget(30, 20)).is_err());
        assert!(read_known_accounts(&rpc, wallet, &[], 50, 50, &mut budget(30, 20)).is_err());
        assert!(calls.lock().unwrap().is_empty());
        let (rpc, _) = client(vec![json!([]); 16]);
        assert_eq!(
            read_known_accounts(&rpc, wallet, &accounts[..15], 40, 50, &mut budget(16, 16))
                .unwrap()["queried_addresses"]
                .as_array()
                .unwrap()
                .len(),
            16
        );
    }
}
