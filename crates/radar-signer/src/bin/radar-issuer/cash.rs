// SPDX-License-Identifier: Apache-2.0
//! Recorded native effects versus opening cash; external transfers remain unknown.

use std::collections::BTreeSet;

use radar_journal::OpeningInventoryRecord;
use radar_types::Address;
use serde_json::{Value, json};

use super::evidence_integer as integer;

// Called only after acquisition history has replayed the exact retained review.
pub(super) fn flow(value: &Value, operation: &str, wallet: Address) -> Result<Value, String> {
    let effects = value["native_account_effects"]
        .as_array()
        .ok_or("native cash effects missing")?;
    let mut owned = effects
        .iter()
        .filter(|effect| effect["account"] == wallet.to_string());
    let effect = owned.next().ok_or("native cash wallet effect missing")?;
    let pre = integer(effect, "pre_lamports")?;
    let post = integer(effect, "post_lamports")?;
    let delta = i128::from(post) - i128::from(pre);
    if owned.next().is_some()
        || effect["net_change_lamports"] != delta.to_string()
        || value["wallet_net_change_lamports"] != delta.to_string()
    {
        return Err("native cash effect is inconsistent".into());
    }
    Ok(
        json!({"operation":operation,"signature":value["signature"],"source":"recorded_operation","execution_slot":integer(value,"slot")?.to_string(),
        "pre_lamports":pre.to_string(),"post_lamports":post.to_string(),
        "net_change_lamports":delta.to_string()}),
    )
}

pub(super) fn review(
    opening: Option<&OpeningInventoryRecord>,
    wallet: Address,
    history: &Value,
    current: &Value,
    external: &[Value],
) -> Result<Option<Value>, String> {
    let Some(opening) = opening else {
        return Ok(None);
    };
    let floor = super::opening::floor(opening, wallet)?.get();
    let native_slot = integer(&current["native_sol"], "slot")?;
    if native_slot < opening.native_slot.get() {
        return Err("native cash observation precedes opening read".into());
    }
    let mut flows = history["recorded_native_cash_flows"]
        .as_array()
        .ok_or("recorded native cash flows missing")?
        .iter()
        .chain(external)
        .map(|row| Ok((integer(row, "execution_slot")?, row)))
        .collect::<Result<Vec<_>, String>>()?;
    flows.sort_by_key(|(slot, _)| *slot);
    let mut slots = BTreeSet::new();
    let mut expected = opening.native_lamports;
    let mut anchors = Vec::new();
    for (slot, row) in flows {
        if slot <= floor || slot > native_slot || !slots.insert(slot) {
            return Err("native cash executions lack unambiguous opening/current order".into());
        }
        let pre = integer(row, "pre_lamports")?;
        let post = integer(row, "post_lamports")?;
        let difference = i128::from(pre) - i128::from(expected);
        anchors.push(json!({"operation":row["operation"],"signature":row["signature"],"source":row["source"],"execution_slot":slot.to_string(),
            "expected_pre_lamports":expected.to_string(),"recorded_pre_lamports":pre.to_string(),
            "unexplained_change_lamports":difference.to_string(),"balance_matches":pre == expected}));
        // Do not reset to recorded pre: that would absorb an unexplained transfer.
        expected = u64::try_from(i128::from(expected) + i128::from(post) - i128::from(pre))
            .map_err(|_| "recorded native cash projection exceeds balance range")?;
    }
    let observed = integer(&current["native_sol"], "raw_amount")?;
    let anchors_match = anchors.iter().all(|row| row["balance_matches"] == true);
    Ok(Some(
        json!({"coverage":if external.is_empty() {"recorded_operations_only"} else {"recorded_operations_and_supplied_native_transfers"},
        "opening_lamports":opening.native_lamports.to_string(),
        "expected_lamports":expected.to_string(),"observed_lamports":observed.to_string(),
        "unexplained_change_lamports":(i128::from(observed)-i128::from(expected)).to_string(),
        "balance_matches":observed == expected,"transaction_anchors_match":anchors_match,
        "transaction_anchors":anchors,"external_cash_flows_complete":false,
        "economic_reconciliation_complete":false,"portfolio_state_updated":false,
        "reservation_released":false}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_types::Slot;

    fn opening(amount: u64) -> OpeningInventoryRecord {
        OpeningInventoryRecord {
            wallet: Address::new([4; 32]),
            native_lamports: amount,
            native_slot: Slot(40),
            token_program_slot: Slot(41),
            token_2022_slot: Slot(42),
            raw_token_slot: None,
            read_started_at_unix_secs: 1,
            read_completed_at_unix_secs: 2,
            accounts: None,
            holdings: vec![],
        }
    }
    fn row(slot: u64, pre: u64, post: u64) -> Value {
        json!({"operation":slot.to_string(),"execution_slot":slot.to_string(),
            "pre_lamports":pre.to_string(),"post_lamports":post.to_string()})
    }
    fn run(amount: u64, flows: &[Value], observed: u64, slot: u64) -> Result<Value, String> {
        let opening = opening(amount);
        review(
            Some(&opening),
            opening.wallet,
            &json!({"recorded_native_cash_flows":flows}),
            &json!({"native_sol":{"slot":slot.to_string(),"raw_amount":observed.to_string()}}),
            &[],
        )?
        .ok_or("comparison missing".into())
    }

    #[test]
    fn native_cash_replays_debits_credits_zero_and_fees_in_slot_order() {
        let flows = vec![
            row(45, 105, 103),
            row(43, 100, 80),
            row(44, 80, 105),
            row(46, 103, 103),
        ];
        let report = run(100, &flows, 103, 46).expect("exact-slot read");
        assert_eq!(report["expected_lamports"], "103");
        assert_eq!(report["opening_lamports"], "100");
        assert_eq!(report["observed_lamports"], "103");
        assert_eq!(report["unexplained_change_lamports"], "0");
        assert_eq!(report["balance_matches"], true);
        assert_eq!(report["transaction_anchors_match"], true);
        assert_eq!(report["transaction_anchors"][0]["operation"], "43");
        assert_eq!(
            report["transaction_anchors"][3]["expected_pre_lamports"],
            "103"
        );
        assert_eq!(
            run(100, &flows.into_iter().rev().collect::<Vec<_>>(), 103, 47).unwrap(),
            report
        );
        assert_eq!(report["coverage"], "recorded_operations_only");
        for flag in [
            "external_cash_flows_complete",
            "economic_reconciliation_complete",
            "portfolio_state_updated",
            "reservation_released",
        ] {
            assert_eq!(report[flag], false);
        }
        assert_eq!(run(0, &[], 0, 40).unwrap()["balance_matches"], true);
        assert_eq!(
            run(
                u64::MAX,
                &[row(43, u64::MAX, 0), row(44, 0, u64::MAX)],
                u64::MAX,
                44
            )
            .unwrap()["expected_lamports"],
            u64::MAX.to_string()
        );
    }

    #[test]
    fn native_cash_keeps_offsetting_gaps_visible_even_when_final_cash_matches() {
        let report = run(100, &[row(43, 110, 90), row(44, 80, 100)], 100, 44).unwrap();
        assert_eq!(report["balance_matches"], true);
        assert_eq!(report["transaction_anchors_match"], false);
        assert_eq!(
            report["transaction_anchors"][0]["unexplained_change_lamports"],
            "10"
        );
        assert_eq!(
            report["transaction_anchors"][1]["unexplained_change_lamports"],
            "0"
        );
        assert_eq!(
            report["transaction_anchors"][0]["recorded_pre_lamports"],
            "110"
        );
        for (observed, difference) in [(99, "-1"), (101, "1")] {
            let report = run(100, &[], observed, 42).unwrap();
            assert_eq!(report["balance_matches"], false);
            assert_eq!(report["unexplained_change_lamports"], difference);
        }
        let report = run(100, &[row(43, 90, 100)], 110, 43).unwrap();
        assert_eq!(
            report["transaction_anchors"][0]["unexplained_change_lamports"],
            "-10"
        );
    }

    #[test]
    fn native_cash_refuses_ambiguous_or_impossible_projection_and_keeps_missing_opening_unknown() {
        for (amount, flows, slot) in [
            (100, vec![row(42, 100, 99)], 43),
            (100, vec![row(41, 100, 99)], 43),
            (100, vec![row(44, 100, 99)], 43),
            (100, vec![row(43, 100, 99), row(43, 99, 98)], 44),
            (0, vec![row(43, 1, 0), row(44, 0, 1)], 44),
            (u64::MAX, vec![row(43, 0, 1), row(44, 1, 0)], 44),
            (100, vec![], 39),
        ] {
            assert!(run(amount, &flows, 100, slot).is_err());
        }
        let o = opening(100);
        assert!(
            review(None, o.wallet, &Value::Null, &Value::Null, &[])
                .unwrap()
                .is_none()
        );
        assert!(
            review(
                Some(&o),
                Address::new([5; 32]),
                &Value::Null,
                &Value::Null,
                &[]
            )
            .is_err()
        );
        assert!(
            review(
                Some(&o),
                o.wallet,
                &json!({}),
                &json!({"native_sol":{"slot":"42"}}),
                &[],
            )
            .is_err()
        );
    }

    #[test]
    fn native_cash_extracts_only_one_consistent_wallet_effect() {
        let wallet = opening(100).wallet;
        let effect = json!({"account":wallet,"pre_lamports":"100","post_lamports":"80",
            "net_change_lamports":"-20"});
        let packet = json!({"slot":"43","wallet_net_change_lamports":"-20",
            "native_account_effects":[effect]});
        let report = flow(&packet, "op", wallet).unwrap();
        assert_eq!(
            report,
            json!({"operation":"op","signature":null,"source":"recorded_operation","execution_slot":"43","pre_lamports":"100",
            "post_lamports":"80","net_change_lamports":"-20"})
        );
        for bad in [
            json!([]),
            json!([effect.clone(), effect.clone()]),
            json!([{"account":Address::new([5;32]),"pre_lamports":"100","post_lamports":"80"}]),
        ] {
            let mut p = packet.clone();
            p["native_account_effects"] = bad;
            assert!(flow(&p, "op", wallet).is_err());
        }
        for path in [
            "/wallet_net_change_lamports",
            "/native_account_effects/0/net_change_lamports",
            "/native_account_effects/0/pre_lamports",
            "/native_account_effects/0/post_lamports",
            "/slot",
        ] {
            let mut p = packet.clone();
            *p.pointer_mut(path).unwrap() = json!("wrong");
            assert!(flow(&p, "op", wallet).is_err());
        }
        for (pre, post, delta) in [
            (0, u64::MAX, u64::MAX.to_string()),
            (u64::MAX, 0, format!("-{}", u64::MAX)),
            (0, 0, "0".into()),
        ] {
            let mut p = packet.clone();
            p["native_account_effects"][0]["pre_lamports"] = json!(pre.to_string());
            p["native_account_effects"][0]["post_lamports"] = json!(post.to_string());
            p["native_account_effects"][0]["net_change_lamports"] = json!(delta);
            p["wallet_net_change_lamports"] = json!(delta);
            assert_eq!(
                flow(&p, "op", wallet).unwrap()["net_change_lamports"],
                delta
            );
        }
    }
}
