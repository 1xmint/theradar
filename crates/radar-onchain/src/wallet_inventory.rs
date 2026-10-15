// SPDX-License-Identifier: Apache-2.0
//! Raw token and mint evidence for the operator wallet reader, not a portfolio.
//! Enumeration and this batch remain separate bank observations.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use radar_pumpfun::token::{AccountState, MintAccount, SPL_TOKEN_PROGRAM, TokenAccount};
use radar_types::{Address, Asset, Slot};
use serde_json::{Value, json};

use crate::rpc::{TokenAccount as ListedAccount, TokenAccountsRead};
use crate::{Budget, MultiAccountRead, RpcClient};

fn verified(
    wallet: Address,
    minimum: Slot,
    listed: &[&ListedAccount],
    addresses: &[Address],
    raw: &MultiAccountRead,
) -> Result<Value, String> {
    let slot = raw.slot.ok_or("raw token context missing")?;
    if slot < minimum {
        return Err("raw token context precedes wallet enumeration".into());
    }
    let by_address: BTreeMap<_, _> = addresses.iter().zip(&raw.accounts).collect();
    let mut totals = BTreeMap::<Address, u64>::new();
    let mut accounts = Vec::new();
    for expected in listed {
        let get = |address| {
            by_address
                .get(&address)
                .and_then(|value| value.as_ref())
                .ok_or("raw token or mint account missing")
        };
        let account = get(expected.address)?;
        let mint = get(expected.mint)?;
        let owner = |value: &crate::OwnedAccount| -> Result<Address, String> {
            value
                .owner
                .as_ref()
                .ok_or("raw account program missing")?
                .parse()
                .map_err(|_| "raw account program invalid".into())
        };
        let account_owner = owner(account)?;
        let mint_owner = owner(mint)?;
        if account_owner != expected.program || mint_owner != expected.program {
            return Err("raw token or mint program differs from enumeration".into());
        }
        let token = TokenAccount::parse(&account.data, &account_owner)
            .map_err(|_| "raw token layout or extension refused")?;
        let parsed_mint = MintAccount::parse(&mint.data, &mint_owner)
            .map_err(|_| "raw mint layout or extension refused")?;
        let state = match token.state {
            AccountState::Initialized => "initialized",
            AccountState::Frozen => "frozen",
            AccountState::Uninitialized => "uninitialized",
        };
        if token.owner != wallet
            || token.mint != expected.mint
            || u128::from(token.amount) != expected.amount
            || state != expected.state
            || parsed_mint.decimals != expected.decimals
        {
            return Err("raw token identity, quantity, state or units differ".into());
        }
        // The parser validates layout and option tags first. Like curve_market,
        // require the canonical initialized byte and retain the authority tag.
        if mint.data[45] != 1 {
            return Err("raw mint is not canonically initialized".into());
        }
        // The captured classic native mint has zero supply despite nonzero
        // wrapped balances. Bind that exception to the mint AND program, never
        // an arbitrary account's is_native tag. Other native mints need captures.
        let wrapped_sol =
            token.mint == Asset::WRAPPED_SOL_MINT && account_owner == SPL_TOKEN_PROGRAM;
        let is_native = account.data[109..113] != [0; 4];
        if is_native != wrapped_sol {
            return Err("unsupported native wrapping identity".into());
        }
        if wrapped_sol
            && (parsed_mint.decimals != 9
                || parsed_mint.supply != 0
                || mint.data[..4] != [0; 4]
                || parsed_mint.freeze_authority.is_some())
        {
            return Err("classic wrapped SOL mint metadata differs".into());
        }
        let total = totals.entry(expected.mint).or_default();
        *total = total
            .checked_add(token.amount)
            .ok_or("raw mint holding total overflow")?;
        if !wrapped_sol && *total > parsed_mint.supply {
            return Err("raw holdings exceed mint supply".into());
        }
        // TokenAccount::parse validates is_native's COption but does not retain
        // its payload. This is wrapping evidence, not additional native cash.
        let native_reserve = is_native.then(|| {
            u64::from_le_bytes(account.data[113..121].try_into().expect("validated layout"))
                .to_string()
        });
        accounts.push(json!({
            "address":expected.address.to_string(), "mint":token.mint.to_string(),
            "owner":token.owner.to_string(), "program":account_owner.to_string(),
            "raw_amount":token.amount.to_string(), "decimals":parsed_mint.decimals,
            "state":state, "mint_supply_raw":parsed_mint.supply.to_string(),
            "mint_authority_active":mint.data[..4] != [0;4],
            "freeze_authority":parsed_mint.freeze_authority.map(|address| address.to_string()),
            "delegate":token.delegate.map(|delegate| json!({"address":delegate.who.to_string(),
                "raw_allowance":delegate.amount.to_string()})),
            "native_reserve_lamports":native_reserve, "spendable":null
        }));
    }
    Ok(json!({"slot":slot.get().to_string(), "accounts":accounts,
        "inventory_complete":false, "authority":"read_only"}))
}

/// Verifies listed token accounts and mints together under one reported context.
/// At most 100 distinct account/mint addresses, following Solana's RPC bound.
/// Empty enumeration remains incomplete inventory and needs no extra RPC call.
///
/// # Errors
/// Missing/old contexts, duplicate identities, over-limit batches, unavailable
/// raw accounts, changed metadata or unsupported layouts/extensions refuse.
/// Provider details never enter a refusal; no key or trading authority is used.
pub fn read(
    client: &RpcClient,
    budget: &mut Budget,
    wallet: Address,
    native_slot: Slot,
    legacy: &TokenAccountsRead,
    extended: &TokenAccountsRead,
) -> Result<Value, String> {
    let minimum = native_slot
        .max(legacy.slot.ok_or("token context missing")?)
        .max(extended.slot.ok_or("Token-2022 context missing")?);
    let listed: Vec<_> = legacy.accounts.iter().chain(&extended.accounts).collect();
    let mut seen = HashSet::new();
    let mut addresses = BTreeSet::new();
    for account in &listed {
        if !seen.insert(account.address) {
            return Err("duplicate raw token account identity".into());
        }
        addresses.insert(account.address);
        addresses.insert(account.mint);
    }
    if addresses.len() > 100 {
        return Err("raw wallet token batch exceeds 100 addresses".into());
    }
    if addresses.is_empty() {
        return Ok(
            json!({"slot":null, "accounts":[], "inventory_complete":false,
            "authority":"read_only"}),
        );
    }
    let addresses: Vec<_> = addresses.into_iter().collect();
    let raw = client
        .accounts(budget, &addresses)
        .map_err(|_| "raw wallet token read failed")?;
    verified(wallet, minimum, &listed, &addresses, &raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OwnedAccount;
    use crate::rpc::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID, Transport};
    use std::time::Duration;

    fn wallet() -> Address {
        Address::new([0x55; 32])
    }
    fn listed() -> ListedAccount {
        ListedAccount {
            address: Address::new([2; 32]),
            mint: Address::new([1; 32]),
            program: TOKEN_PROGRAM_ID.parse().expect("program"),
            state: "initialized".into(),
            amount: 7,
            decimals: 6,
        }
    }
    fn mint(amount: u64) -> Vec<u8> {
        let mut data = vec![0; 82];
        data[36..44].copy_from_slice(&amount.to_le_bytes());
        data[44] = 6;
        data[45] = 1;
        data
    }
    fn token(mint: Address, amount: u64) -> Vec<u8> {
        let mut data = vec![0; 165];
        data[..32].copy_from_slice(mint.as_bytes());
        data[32..64].copy_from_slice(wallet().as_bytes());
        data[64..72].copy_from_slice(&amount.to_le_bytes());
        data[108] = 1;
        data
    }
    fn owned(data: Vec<u8>) -> OwnedAccount {
        OwnedAccount {
            data,
            owner: Some(TOKEN_PROGRAM_ID.into()),
        }
    }
    fn raw() -> MultiAccountRead {
        MultiAccountRead {
            slot: Some(Slot(40)),
            accounts: vec![Some(owned(mint(10))), Some(owned(token(listed().mint, 7)))],
        }
    }
    fn check(expected: &ListedAccount, raw: &MultiAccountRead) -> Result<Value, String> {
        verified(
            wallet(),
            Slot(40),
            &[expected],
            &[expected.mint, expected.address],
            raw,
        )
    }
    fn bytes(raw: &mut MultiAccountRead, index: usize) -> &mut Vec<u8> {
        &mut raw.accounts[index].as_mut().expect("fixture").data
    }

    #[test]
    fn restrictions_survive_raw_verification_without_spendability() {
        let expected = listed();
        let mut raw = raw();
        bytes(&mut raw, 0)[..4].copy_from_slice(&1u32.to_le_bytes());
        bytes(&mut raw, 0)[46..50].copy_from_slice(&1u32.to_le_bytes());
        bytes(&mut raw, 0)[50..82].copy_from_slice(&[9; 32]);
        bytes(&mut raw, 1)[72..76].copy_from_slice(&1u32.to_le_bytes());
        bytes(&mut raw, 1)[76..108].copy_from_slice(&[8; 32]);
        bytes(&mut raw, 1)[121..129].copy_from_slice(&3u64.to_le_bytes());
        for (byte, state) in [(1, "initialized"), (2, "frozen"), (0, "uninitialized")] {
            bytes(&mut raw, 1)[108] = byte;
            let mut expected = expected.clone();
            expected.state = state.into();
            let output = check(&expected, &raw).expect("known raw state");
            let account = &output["accounts"][0];
            assert_eq!(account["state"], state);
            assert_eq!(account["raw_amount"], "7");
            assert_eq!(account["mint_supply_raw"], "10");
            assert_eq!(account["mint_authority_active"], true);
            assert_eq!(
                account["freeze_authority"],
                Address::new([9; 32]).to_string()
            );
            assert_eq!(
                account["delegate"]["address"],
                Address::new([8; 32]).to_string()
            );
            assert_eq!(account["delegate"]["raw_allowance"], "3");
            assert!(account["native_reserve_lamports"].is_null());
            assert!(account["spendable"].is_null());
            assert_eq!(output["inventory_complete"], false);
            assert_eq!(output["authority"], "read_only");
        }
        let output = check(&expected, &self::raw()).expect("ordinary");
        for field in ["freeze_authority", "delegate", "native_reserve_lamports"] {
            assert!(output["accounts"][0][field].is_null());
        }
        assert_eq!(output["accounts"][0]["mint_authority_active"], false);
    }

    #[test]
    fn captured_classic_native_mint_has_zero_supply_without_zero_wrapped_balance() {
        let capture: Value = serde_json::from_str(include_str!(
            "../../radar-pumpfun/tests/fixtures/pumpswap_reserves.json"
        ))
        .expect("capture");
        let mint = &capture["reads"][0]["accounts"][2];
        assert_eq!(mint["address"], Asset::WRAPPED_SOL_MINT.to_string());
        let mut expected = listed();
        expected.mint = Asset::WRAPPED_SOL_MINT;
        expected.decimals = 9;
        let mut raw = self::raw();
        raw.accounts[0] = Some(OwnedAccount {
            owner: Some(mint["owner"].as_str().expect("owner").into()),
            data: radar_types::b64::decode(mint["data_b64"].as_str().expect("data"))
                .expect("bytes"),
        });
        *bytes(&mut raw, 1) = token(expected.mint, 7);
        bytes(&mut raw, 1)[109..113].copy_from_slice(&1u32.to_le_bytes());
        bytes(&mut raw, 1)[113..121].copy_from_slice(&99u64.to_le_bytes());
        let output = check(&expected, &raw).expect("nonzero captured-native holding");
        assert_eq!(output["accounts"][0]["mint_supply_raw"], "0");
        assert_eq!(output["accounts"][0]["raw_amount"], "7");
        assert_eq!(output["accounts"][0]["native_reserve_lamports"], "99");
        assert!(output["accounts"][0]["spendable"].is_null());
        for at in [0, 36, 44, 46] {
            let mut bad = raw.clone();
            bytes(&mut bad, 0)[at] = 1;
            assert!(check(&expected, &bad).is_err(), "native mint metadata {at}");
        }
        let mut wrong_units = raw.clone();
        let mut matching_listing = expected.clone();
        bytes(&mut wrong_units, 0)[44] = 6;
        matching_listing.decimals = 6;
        assert!(
            check(&matching_listing, &wrong_units).is_err(),
            "native units cannot change with listing"
        );
        let mut absent = raw.clone();
        bytes(&mut absent, 1)[109..113].fill(0);
        assert!(
            check(&expected, &absent).is_err(),
            "native mint needs wrapping flag"
        );
        let mut other = listed();
        other.decimals = 9;
        let mut wrong_mint = raw.clone();
        bytes(&mut wrong_mint, 1)[..32].copy_from_slice(other.mint.as_bytes());
        assert!(
            check(&other, &wrong_mint).is_err(),
            "wrapping flag cannot exempt arbitrary mint"
        );
        let mut other_program = expected.clone();
        other_program.program = TOKEN_2022_PROGRAM_ID.parse().expect("program");
        let mut wrong_program = raw.clone();
        for account in wrong_program.accounts.iter_mut().flatten() {
            account.owner = Some(TOKEN_2022_PROGRAM_ID.into());
        }
        assert!(
            check(&other_program, &wrong_program).is_err(),
            "classic mint under different program"
        );
    }

    #[test]
    fn missing_changed_or_unsupported_raw_evidence_refuses() {
        let expected = listed();
        for index in 0..2 {
            for owner in [
                None,
                Some("invalid".into()),
                Some(wallet().to_string()),
                Some(TOKEN_2022_PROGRAM_ID.into()),
            ] {
                let mut bad = raw();
                bad.accounts[index].as_mut().expect("account").owner = owner;
                assert!(check(&expected, &bad).is_err());
            }
            let mut missing = raw();
            missing.accounts[index] = None;
            assert!(check(&expected, &missing).is_err());
            let mut short = raw();
            bytes(&mut short, index).truncate(3);
            assert!(check(&expected, &short).is_err());
        }
        for (index, at, value) in [
            (0, 44, 7),
            (0, 45, 0),
            (0, 45, 2),
            (0, 36, 6),
            (1, 0, 9),
            (1, 32, 9),
            (1, 64, 8),
            (1, 108, 2),
            (1, 108, 3),
            (1, 72, 2),
            (1, 109, 2),
        ] {
            let mut bad = raw();
            bytes(&mut bad, index)[at] = value;
            assert!(check(&expected, &bad).is_err(), "{index}:{at}:{value}");
        }
        let mut oversized = expected.clone();
        oversized.amount = u128::from(u64::MAX) + 1;
        assert!(check(&oversized, &raw()).is_err());
        let mut old = raw();
        old.slot = Some(Slot(39));
        assert!(check(&expected, &old).is_err());
        old.slot = None;
        assert!(check(&expected, &old).is_err());
        for index in 0..2 {
            let mut extended = raw();
            let mut expected = expected.clone();
            expected.program = TOKEN_2022_PROGRAM_ID.parse().expect("program");
            for account in extended.accounts.iter_mut().flatten() {
                account.owner = Some(TOKEN_2022_PROGRAM_ID.into());
            }
            let data = bytes(&mut extended, index);
            data.resize(165, 0);
            data.extend_from_slice(&[if index == 0 { 1 } else { 2 }, 1, 0, 0, 0]);
            assert!(
                check(&expected, &extended).is_err(),
                "unsupported extension {index}"
            );
        }
        let mut extended = raw();
        let mut expected = expected.clone();
        expected.program = TOKEN_2022_PROGRAM_ID.parse().expect("program");
        for account in extended.accounts.iter_mut().flatten() {
            account.owner = Some(TOKEN_2022_PROGRAM_ID.into());
        }
        bytes(&mut extended, 1).extend_from_slice(&[2, 7, 0, 0, 0]);
        assert!(
            check(&expected, &extended).is_ok(),
            "supported immutable owner"
        );
    }

    #[test]
    fn same_mint_aggregate_cannot_exceed_supply_or_overflow() {
        for (supply, first, second) in [(10, 7, 4), (u64::MAX, u64::MAX, 1), (10, 7, 3)] {
            let mut one = listed();
            one.amount = u128::from(first);
            let mut two = one.clone();
            two.address = Address::new([3; 32]);
            two.amount = u128::from(second);
            let raw = MultiAccountRead {
                slot: Some(Slot(40)),
                accounts: vec![
                    Some(owned(mint(supply))),
                    Some(owned(token(one.mint, first))),
                    Some(owned(token(one.mint, second))),
                ],
            };
            let result = verified(
                wallet(),
                Slot(40),
                &[&one, &two],
                &[one.mint, one.address, two.address],
                &raw,
            );
            assert_eq!(result.is_ok(), supply == 10 && second == 3);
        }
    }

    struct Answer(String);
    impl Transport for Answer {
        fn post(&self, _: &str, _: String) -> Result<String, String> {
            Ok(self.0.clone())
        }
    }
    fn empty() -> TokenAccountsRead {
        TokenAccountsRead {
            slot: Some(Slot(40)),
            accounts: vec![],
        }
    }
    fn budget() -> Budget {
        Budget::new(1, 0, Duration::from_secs(20))
    }

    fn hundred_addresses() -> (TokenAccountsRead, Vec<Value>) {
        let mut accounts = Vec::new();
        let mut values = Vec::new();
        for index in 0..50u8 {
            let mut expected = listed();
            expected.mint = Address::new([index * 2; 32]);
            expected.address = Address::new([index * 2 + 1; 32]);
            expected.amount = 1;
            values.push(json!({"owner":TOKEN_PROGRAM_ID,"data":[radar_types::b64::encode(&mint(10)),"base64"]}));
            values.push(json!({"owner":TOKEN_PROGRAM_ID,"data":[radar_types::b64::encode(&token(expected.mint,1)),"base64"]}));
            accounts.push(expected);
        }
        (
            TokenAccountsRead {
                slot: Some(Slot(40)),
                accounts,
            },
            values,
        )
    }

    #[test]
    fn bounded_batch_requires_contexts_and_empty_enumeration_is_not_complete_inventory() {
        let client =
            RpcClient::with_transport("http://fixture", Box::new(Answer("unknown".into())));
        let output = read(
            &client,
            &mut budget(),
            wallet(),
            Slot(40),
            &empty(),
            &empty(),
        )
        .expect("no token accounts");
        assert!(output["slot"].is_null());
        assert_eq!(output["inventory_complete"], false);
        let mut missing = empty();
        missing.slot = None;
        assert!(
            read(
                &client,
                &mut budget(),
                wallet(),
                Slot(40),
                &missing,
                &empty()
            )
            .is_err()
        );
        assert!(
            read(
                &client,
                &mut budget(),
                wallet(),
                Slot(40),
                &empty(),
                &missing
            )
            .is_err()
        );
        let (mut legacy, values) = hundred_addresses();
        let client = RpcClient::with_transport(
            "http://fixture",
            Box::new(Answer(
                json!({"result":{"context":{"slot":40},"value":values}}).to_string(),
            )),
        );
        assert!(
            read(
                &client,
                &mut budget(),
                wallet(),
                Slot(40),
                &legacy,
                &empty()
            )
            .is_ok(),
            "exactly 100 addresses"
        );
        legacy.accounts.push(legacy.accounts[0].clone());
        assert_eq!(
            read(
                &client,
                &mut budget(),
                wallet(),
                Slot(40),
                &legacy,
                &empty()
            ),
            Err("duplicate raw token account identity".into())
        );
        legacy.accounts.last_mut().expect("last").address = Address::new([100; 32]);
        assert_eq!(
            read(
                &client,
                &mut budget(),
                wallet(),
                Slot(40),
                &legacy,
                &empty()
            ),
            Err("raw wallet token batch exceeds 100 addresses".into())
        );
        legacy.accounts.pop();
        for (native, legacy_slot, extended_slot) in [(41, 40, 40), (40, 41, 40), (40, 40, 41)] {
            legacy.slot = Some(Slot(legacy_slot));
            let mut extended = empty();
            extended.slot = Some(Slot(extended_slot));
            assert!(
                read(
                    &client,
                    &mut budget(),
                    wallet(),
                    Slot(native),
                    &legacy,
                    &extended
                )
                .is_err()
            );
        }
    }
}
