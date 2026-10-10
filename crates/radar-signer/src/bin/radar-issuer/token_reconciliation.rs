// SPDX-License-Identifier: Apache-2.0
//! Chain supplied token effects between retained endpoints; never risk authority.

use std::collections::{BTreeMap, BTreeSet};

use radar_journal::OpeningInventoryRecord;
use radar_types::Address;
use serde_json::{Value, json};

fn number(value: &Value) -> Result<u64, &'static str> {
    value
        .as_str()
        .and_then(|text| text.parse().ok())
        .ok_or("token_reconciliation_integer_missing")
}

fn interval(opening: &OpeningInventoryRecord, evidence: &Value) -> Result<(), &'static str> {
    let packet = &evidence["wallet_activity"];
    let after = number(&packet["after_slot_exclusive"])?;
    let through = number(&packet["through_slot_inclusive"])?;
    // Different read contexts do not establish a single endpoint balance.
    if after >= through
        || opening.token_program_slot.get() != after
        || opening.token_2022_slot.get() != after
        || opening
            .raw_token_slot
            .is_some_and(|slot| slot.get() != after)
        || number(&evidence["token_program"]["slot"])? != through
        || number(&evidence["token_2022"]["slot"])? != through
        || evidence["raw_token_verification"]["slot"] != Value::Null
            && number(&evidence["raw_token_verification"]["slot"])? != through
    {
        return Err("token_endpoint_slots_differ");
    }
    if packet["signature_scan_finished"] != true || packet["transaction_fetch_finished"] != true {
        return Err("reported_collection_incomplete");
    }
    Ok(())
}

fn reconcile(
    opening: Option<&OpeningInventoryRecord>,
    evidence: &Value,
    comparison: &Value,
    transactions: &[Value],
) -> Result<Value, &'static str> {
    let opening = opening.ok_or("opening_inventory_missing")?;
    interval(opening, evidence)?;
    if comparison["opening_accounts_retained"] != true {
        return Err("opening_accounts_missing");
    }
    let rows = comparison["accounts"]
        .as_array()
        .ok_or("known_accounts_missing")?;
    let mut expected = BTreeMap::new();
    let mut identities = BTreeMap::new();
    for row in rows {
        if row["opening_present"] != true || row["current_present"] != true {
            return Err("token_account_creation_or_closure_unknown");
        }
        let address: Address =
            serde_json::from_value(row["address"].clone()).map_err(|_| "known_account_invalid")?;
        if expected
            .insert(address, number(&row["opening_raw"])?)
            .is_some()
        {
            return Err("known_account_duplicate");
        }
        identities.insert(address, row);
    }
    let mut ordered = BTreeMap::new();
    for transaction in transactions {
        let slot = number(&transaction["reported_slot"])?;
        if slot <= number(&evidence["wallet_activity"]["after_slot_exclusive"])?
            || slot > number(&evidence["wallet_activity"]["through_slot_inclusive"])?
            || ordered.insert(slot, transaction).is_some()
        {
            // A signature's lexical order cannot supply intra-slot execution order.
            return Err("token_activity_order_unknown");
        }
    }
    for transaction in ordered.values() {
        apply(transaction, opening.wallet, &identities, &mut expected)?;
    }
    for (address, amount) in &expected {
        if *amount != number(&identities[address]["current_raw"])? {
            return Err("current_token_balance_unexplained");
        }
    }
    Ok(
        json!({"status":"consistent_with_supplied_known_account_history",
        "known_token_accounts_compared":expected.len(),"transactions_compared":transactions.len()}),
    )
}

fn apply(
    transaction: &Value,
    wallet: Address,
    identities: &BTreeMap<Address, &Value>,
    expected: &mut BTreeMap<Address, u64>,
) -> Result<(), &'static str> {
    let effects = &transaction["reported_effect_review"];
    if transaction["signature_verified_locally"] != true
        || effects["status"] != "consistent_with_signed_transfer_intents"
    {
        return Err("token_activity_effects_unresolved");
    }
    let mut seen = BTreeSet::new();
    for change in effects["reported_token_changes"]
        .as_array()
        .ok_or("token_changes_missing")?
    {
        let address: Address = serde_json::from_value(change["account"].clone())
            .map_err(|_| "changed_account_invalid")?;
        if !seen.insert(address) {
            return Err("changed_account_duplicate");
        }
        let Some(identity) = identities.get(&address) else {
            if change["reported_owner"] == json!(wallet) {
                return Err("wallet_token_account_outside_known_set");
            }
            continue;
        };
        if change["mint"] != identity["mint"]
            || change["program"] != identity["token_program"]
            || change["decimals"] != identity["decimals"]
            || change["reported_owner"] != json!(wallet)
        {
            return Err("known_token_identity_differs");
        }
        if expected.get(&address) != Some(&number(&change["reported_pre_raw"])?) {
            return Err("token_history_balance_chain_broken");
        }
        expected.insert(address, number(&change["reported_post_raw"])?);
    }
    Ok(())
}

pub(super) fn review(
    opening: Option<&OpeningInventoryRecord>,
    evidence: &Value,
    comparison: &Value,
    transactions: &[Value],
) -> Value {
    let mut result = reconcile(opening, evidence, comparison, transactions)
        .unwrap_or_else(|reason| json!({"status":"unresolved","reason":reason}));
    for field in [
        "wallet_coverage_complete",
        "metadata_verified_independently",
        "economic_reconciliation_complete",
        "portfolio_state_updated",
    ] {
        result[field] = json!(false);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_types::Slot;

    fn fixture() -> (OpeningInventoryRecord, Value, Value, Vec<Value>) {
        let wallet = Address::new([1; 32]);
        let account = Address::new([2; 32]);
        let mint = Address::new([3; 32]);
        let program = Address::new([4; 32]);
        let opening = OpeningInventoryRecord {
            wallet,
            native_lamports: 0,
            native_slot: Slot(40),
            token_program_slot: Slot(40),
            token_2022_slot: Slot(40),
            raw_token_slot: Some(Slot(40)),
            read_started_at_unix_secs: 1,
            read_completed_at_unix_secs: 2,
            holdings: vec![],
            accounts: Some(vec![]),
        };
        let evidence = json!({"token_program":{"slot":"43"},"token_2022":{"slot":"43"},
            "raw_token_verification":{"slot":"43"},"wallet_activity":{"after_slot_exclusive":"40",
            "through_slot_inclusive":"43","signature_scan_finished":true,"transaction_fetch_finished":true}});
        let comparison = json!({"opening_accounts_retained":true,"accounts":[{"address":account,
            "mint":mint,"token_program":program,"decimals":6,"opening_present":true,"current_present":true,
            "opening_raw":"10","current_raw":"12"}]});
        let transaction = |slot, before, after| {
            json!({"reported_slot":slot,"signature_verified_locally":true,
            "reported_effect_review":{"status":"consistent_with_signed_transfer_intents","reported_token_changes":[{
            "account":account,"mint":mint,"program":program,"decimals":6,"reported_owner":wallet,
            "reported_pre_raw":before,"reported_post_raw":after}]}})
        };
        // Input order is deliberately opposite to execution-slot order.
        (
            opening,
            evidence,
            comparison,
            vec![transaction("42", "6", "12"), transaction("41", "10", "6")],
        )
    }

    #[test]
    fn token_history_chains_endpoints_by_slot_without_granting_authority() {
        let (opening, evidence, comparison, transactions) = fixture();
        let report = review(Some(&opening), &evidence, &comparison, &transactions);
        assert_eq!(
            report["status"],
            "consistent_with_supplied_known_account_history"
        );
        assert_eq!(report["known_token_accounts_compared"], 1);
        assert_eq!(report["transactions_compared"], 2);
        for field in [
            "wallet_coverage_complete",
            "metadata_verified_independently",
            "economic_reconciliation_complete",
            "portfolio_state_updated",
        ] {
            assert_eq!(report[field], false);
        }
        let mut unchanged = comparison.clone();
        unchanged["accounts"][0]["current_raw"] = json!("10");
        assert_eq!(
            review(Some(&opening), &evidence, &unchanged, &[])["status"],
            "consistent_with_supplied_known_account_history"
        );
        assert_eq!(
            review(Some(&opening), &evidence, &comparison, &[])["reason"],
            "current_token_balance_unexplained"
        );
        assert_eq!(
            review(None, &evidence, &comparison, &transactions)["reason"],
            "opening_inventory_missing"
        );
    }

    fn bad_transaction_changes() -> Vec<(&'static str, Value)> {
        vec![
            ("/reported_slot", json!("41")),
            ("/reported_slot", json!("40")),
            ("/reported_slot", json!("44")),
            ("/signature_verified_locally", json!(false)),
            ("/reported_effect_review/status", json!("unresolved")),
            (
                "/reported_effect_review/reported_token_changes/0/reported_pre_raw",
                json!("7"),
            ),
            (
                "/reported_effect_review/reported_token_changes/0/reported_post_raw",
                json!("11"),
            ),
            (
                "/reported_effect_review/reported_token_changes/0/mint",
                json!(Address::new([8; 32])),
            ),
            (
                "/reported_effect_review/reported_token_changes/0/program",
                json!(Address::new([8; 32])),
            ),
            (
                "/reported_effect_review/reported_token_changes/0/decimals",
                json!(7),
            ),
            (
                "/reported_effect_review/reported_token_changes/0/reported_owner",
                Value::Null,
            ),
            (
                "/reported_effect_review/reported_token_changes/0/account",
                json!(Address::new([8; 32])),
            ),
        ]
    }

    #[test]
    fn final_slot_is_inclusive_and_duplicate_or_unknown_owned_accounts_refuse() {
        let (opening, evidence, comparison, mut transactions) = fixture();
        transactions[0]["reported_slot"] = json!("43");
        assert_eq!(
            review(Some(&opening), &evidence, &comparison, &transactions)["status"],
            "consistent_with_supplied_known_account_history"
        );
        let mut duplicate = comparison.clone();
        let account = duplicate["accounts"][0].clone();
        duplicate["accounts"].as_array_mut().unwrap().push(account);
        assert_eq!(
            review(Some(&opening), &evidence, &duplicate, &transactions)["reason"],
            "known_account_duplicate"
        );
        let effects = &mut transactions[0]["reported_effect_review"]["reported_token_changes"];
        let mut foreign = effects[0].clone();
        foreign["account"] = json!(Address::new([8; 32]));
        foreign["reported_owner"] = json!(Address::new([9; 32]));
        effects.as_array_mut().unwrap().push(foreign);
        assert_eq!(
            review(Some(&opening), &evidence, &comparison, &transactions)["status"],
            "consistent_with_supplied_known_account_history"
        );
        transactions[0]["reported_effect_review"]["reported_token_changes"][1]["reported_owner"] =
            json!(opening.wallet);
        assert_eq!(
            review(Some(&opening), &evidence, &comparison, &transactions)["reason"],
            "wallet_token_account_outside_known_set"
        );
        let effects = &mut transactions[0]["reported_effect_review"]["reported_token_changes"];
        effects[1] = effects[0].clone();
        assert_eq!(
            review(Some(&opening), &evidence, &comparison, &transactions)["reason"],
            "changed_account_duplicate"
        );
        let mut empty_interval = evidence.clone();
        empty_interval["wallet_activity"]["after_slot_exclusive"] = json!("43");
        assert_eq!(
            review(Some(&opening), &empty_interval, &comparison, &[])["reason"],
            "token_endpoint_slots_differ"
        );
    }

    #[test]
    fn gaps_identity_changes_ambiguous_order_and_incomplete_reads_stay_unresolved() {
        let (opening, evidence, comparison, transactions) = fixture();
        for (pointer, replacement) in bad_transaction_changes() {
            let mut changed = transactions.clone();
            *changed[0].pointer_mut(pointer).unwrap() = replacement;
            assert_eq!(
                review(Some(&opening), &evidence, &comparison, &changed)["status"],
                "unresolved",
                "{pointer}"
            );
        }
        for pointer in [
            "/wallet_activity/signature_scan_finished",
            "/wallet_activity/transaction_fetch_finished",
            "/token_program/slot",
            "/token_2022/slot",
            "/raw_token_verification/slot",
        ] {
            let mut changed = evidence.clone();
            *changed.pointer_mut(pointer).unwrap() = json!(false);
            assert_eq!(
                review(Some(&opening), &changed, &comparison, &transactions)["status"],
                "unresolved",
                "{pointer}"
            );
        }
        for pointer in [
            "/opening_accounts_retained",
            "/accounts/0/opening_present",
            "/accounts/0/current_present",
        ] {
            let mut changed = comparison.clone();
            *changed.pointer_mut(pointer).unwrap() = json!(false);
            assert_eq!(
                review(Some(&opening), &evidence, &changed, &transactions)["status"],
                "unresolved",
                "{pointer}"
            );
        }
        for kind in 0..3 {
            let mut changed = opening.clone();
            match kind {
                0 => changed.token_program_slot = Slot(39),
                1 => changed.token_2022_slot = Slot(39),
                _ => changed.raw_token_slot = Some(Slot(39)),
            }
            assert_eq!(
                review(Some(&changed), &evidence, &comparison, &transactions)["reason"],
                "token_endpoint_slots_differ"
            );
        }
    }
}
