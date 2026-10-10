// SPDX-License-Identifier: Apache-2.0
//! Known opening/current account union, never exhaustive historical coverage.

use std::collections::{BTreeMap, BTreeSet};

use radar_journal::{OpeningInventoryRecord, OpeningTokenAccount, OpeningTokenHolding};
use radar_types::Address;
use serde_json::{Value, json};

pub(super) fn validate(opening: &OpeningInventoryRecord) -> Result<(), String> {
    let Some(accounts) = &opening.accounts else {
        return Ok(());
    };
    let mut addresses = BTreeSet::new();
    let mut grouped = BTreeMap::<Address, OpeningTokenHolding>::new();
    for account in accounts {
        if !addresses.insert(account.address)
            || !matches!(
                account.state.as_str(),
                "initialized" | "frozen" | "uninitialized"
            )
        {
            return Err("opening account identity or state is invalid".into());
        }
        let holding = &account.holding;
        let total = grouped.entry(holding.mint).or_insert(OpeningTokenHolding {
            raw_amount: 0,
            ..holding.clone()
        });
        if total.token_program != holding.token_program || total.decimals != holding.decimals {
            return Err("opening account units or program conflict".into());
        }
        total.raw_amount = total
            .raw_amount
            .checked_add(holding.raw_amount)
            .ok_or("opening account quantity overflow")?;
    }
    let expected: BTreeMap<_, _> = opening
        .holdings
        .iter()
        .map(|holding| (holding.mint, holding.clone()))
        .collect();
    if expected.len() != opening.holdings.len() || grouped != expected {
        return Err("opening account totals differ from retained holdings".into());
    }
    Ok(())
}

pub(super) fn review(
    opening: Option<&OpeningInventoryRecord>,
    current: &[OpeningTokenAccount],
) -> Result<Value, String> {
    if let Some(opening) = opening {
        validate(opening)?;
    }
    let baseline = opening.and_then(|record| record.accounts.as_ref());
    let old: BTreeMap<_, _> = baseline
        .into_iter()
        .flatten()
        .map(|account| (account.address, account))
        .collect();
    let new: BTreeMap<_, _> = current
        .iter()
        .map(|account| (account.address, account))
        .collect();
    let addresses: BTreeSet<_> = old.keys().chain(new.keys()).copied().collect();
    let mut rows = Vec::new();
    for address in addresses {
        let before = old.get(&address).copied();
        let after = new.get(&address).copied();
        if let (Some(before), Some(after)) = (before, after)
            && (before.holding.mint != after.holding.mint
                || before.holding.token_program != after.holding.token_program
                || before.holding.decimals != after.holding.decimals)
        {
            return Err("opening/current account identity or units conflict".into());
        }
        let identity = after.or(before).ok_or("account union identity missing")?;
        rows.push(json!({"address":address,"mint":identity.holding.mint,"token_program":identity.holding.token_program,"decimals":identity.holding.decimals,
            "opening_present":baseline.map(|_| before.is_some()),"current_present":after.is_some(),
            "opening_raw":before.map(|account| account.holding.raw_amount.to_string()),
            "current_raw":after.map(|account| account.holding.raw_amount.to_string()),
            "opening_state":before.map(|account| &account.state),"current_state":after.map(|account| &account.state)}));
    }
    Ok(
        json!({"coverage":"opening_and_current_accounts_only","opening_accounts_retained":baseline.is_some(),
        "accounts":rows,"historical_account_coverage_complete":false,"portfolio_state_updated":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_types::Slot;

    fn account(seed: u8, raw: u64) -> OpeningTokenAccount {
        OpeningTokenAccount {
            address: Address::new([seed; 32]),
            state: "frozen".into(),
            holding: OpeningTokenHolding {
                mint: Address::new([2; 32]),
                token_program: Address::new([3; 32]),
                decimals: 6,
                raw_amount: raw,
            },
        }
    }
    fn opening() -> OpeningInventoryRecord {
        OpeningInventoryRecord {
            wallet: Address::new([4; 32]),
            native_lamports: 0,
            native_slot: Slot(1),
            token_program_slot: Slot(1),
            token_2022_slot: Slot(1),
            raw_token_slot: Some(Slot(1)),
            read_started_at_unix_secs: 1,
            read_completed_at_unix_secs: 2,
            holdings: vec![account(5, 10).holding],
            accounts: Some(vec![account(5, 7), account(6, 3)]),
        }
    }

    #[test]
    fn account_union_retains_disappeared_accounts_and_exposes_migration_with_equal_mint_total() {
        let baseline = opening();
        let current = vec![account(7, 3), account(5, 7)];
        let report = review(Some(&baseline), &current).unwrap();
        let rows = report["accounts"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["address"], json!(Address::new([5; 32])));
        assert_eq!(rows[0]["opening_raw"], "7");
        assert_eq!(rows[0]["current_raw"], "7");
        assert_eq!(rows[1]["address"], json!(Address::new([6; 32])));
        assert_eq!(rows[1]["opening_present"], true);
        assert_eq!(rows[1]["current_present"], false);
        assert_eq!(rows[1]["opening_raw"], "3");
        assert!(rows[1]["current_raw"].is_null());
        assert_eq!(rows[2]["opening_present"], false);
        assert!(rows[2]["opening_raw"].is_null());
        assert_eq!(rows[2]["current_raw"], "3");
        assert_eq!(report["historical_account_coverage_complete"], false);
        assert_eq!(report["portfolio_state_updated"], false);
        assert_eq!(
            report,
            review(
                Some(&baseline),
                &current.into_iter().rev().collect::<Vec<_>>()
            )
            .unwrap()
        );
        let all_missing = review(Some(&baseline), &[]).unwrap();
        assert_eq!(all_missing["accounts"].as_array().unwrap().len(), 2);
        assert_eq!(all_missing["accounts"][0]["current_present"], false);
    }

    #[test]
    fn old_opening_account_history_stays_unknown_and_conflicting_reuse_refuses() {
        let mut baseline = opening();
        baseline.accounts = None;
        let legacy = review(Some(&baseline), &[account(5, 0)]).unwrap();
        assert_eq!(legacy["opening_accounts_retained"], false);
        assert!(legacy["accounts"][0]["opening_present"].is_null());
        assert!(legacy["accounts"][0]["opening_raw"].is_null());
        assert_eq!(review(None, &[account(5, 0)]).unwrap(), legacy);
        baseline.accounts = Some(vec![]);
        baseline.holdings.clear();
        let empty = review(Some(&baseline), &[account(5, 0)]).unwrap();
        assert_eq!(empty["opening_accounts_retained"], true);
        assert_eq!(empty["accounts"][0]["opening_present"], false);
        assert_eq!(empty["accounts"][0]["current_raw"], "0");
        baseline = opening();
        for change in 0..3 {
            let mut reused = account(5, 7);
            match change {
                0 => reused.holding.mint = Address::new([9; 32]),
                1 => reused.holding.token_program = Address::new([9; 32]),
                _ => reused.holding.decimals = 9,
            }
            assert!(review(Some(&baseline), &[reused]).is_err());
        }
    }

    #[test]
    fn opening_account_duplicates_states_units_totals_and_overflow_refuse() {
        let baseline = opening();
        validate(&baseline).unwrap();
        for change in 0..8 {
            let mut bad = baseline.clone();
            let rows = bad.accounts.as_mut().unwrap();
            match change {
                0 => rows[1].address = rows[0].address,
                1 => rows[0].state = "unknown".into(),
                2 => rows[1].holding.token_program = Address::new([8; 32]),
                3 => rows[1].holding.decimals = 9,
                4 => rows[0].holding.raw_amount += 1,
                5 => rows[0].holding.raw_amount = u64::MAX,
                6 => bad.holdings.push(bad.holdings[0].clone()),
                _ => rows[0].holding.mint = Address::new([8; 32]),
            }
            assert!(validate(&bad).is_err(), "change {change}");
        }
        for state in ["initialized", "frozen", "uninitialized"] {
            let mut valid = baseline.clone();
            valid.accounts.as_mut().unwrap()[0].state = state.into();
            validate(&valid).unwrap();
        }
    }
}
