// SPDX-License-Identifier: Apache-2.0
//! The PumpSwap `Pool` layout, asserted against ten accounts read from mainnet.
//!
//! # Why a capture and not the README
//!
//! The vendor publishes a field order for `Pool`, and this repository has twice
//! caught the same vendor's references being incomplete about this same program
//! family (LEARNINGS 25). AGENTS §1: let a reference propose and a capture
//! dispose. So every value below is read out of
//! `fixtures/pumpswap_pools.json` -- raw account bytes, their addresses, their
//! owning program and the slot they were read at -- and the reference is used
//! only to name the fields.
//!
//! Two things the capture disposed of, both of which changed the design:
//!
//! 1. **The account has eight lengths on mainnet**, five of which are exactly the
//!    cumulative field boundaries of the documented order. That is what proves
//!    `virtual_quote_reserves` is sixteen bytes and not eight: `245 + 16 = 261`
//!    and no account is 253 bytes long. It also means a shorter account is a real
//!    pool missing later fields, so those fields are `Option` and a length that
//!    stops *inside* a field is a refusal.
//! 2. **The trailing bytes are padding, not an unknown field.** Research 0028
//!    recorded 58 unrecognised bytes after `coin_creator`. Eighteen of them are
//!    the three fields the vendor has since documented; the remaining forty are
//!    zero in every capture. They are still refused when they are *not* zero,
//!    because that is what a program upgrade looks like from the outside.
//!
//! # What is deliberately absent
//!
//! No price, no impact, no capacity. The reserves are in the two token accounts
//! this struct names, and nothing here reads them.

use std::str::FromStr as _;

use radar_pumpfun::curve::Malformed;
use radar_pumpfun::pool::{
    self, BASE_LEN, FULL_LEN, Pool, WITH_CASHBACK_LEN, WITH_COIN_CREATOR_LEN, WITH_MAYHEM_LEN,
};
use radar_types::Address;
use sha2::{Digest as _, Sha256};

const FIXTURE: &str = include_str!("fixtures/pumpswap_pools.json");

/// The two token programs, by address. Neither is a constant in the captures.
const SPL_TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN_2022: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

fn fixture() -> serde_json::Value {
    serde_json::from_str(FIXTURE).expect("the fixture is valid JSON")
}

fn hex_bytes(hex: &str) -> Vec<u8> {
    assert!(hex.len().is_multiple_of(2), "hex is whole bytes");
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("the fixture is valid hex"))
        .collect()
}

/// Every captured pool, as `(address, bytes)`.
fn captures() -> Vec<(String, Vec<u8>)> {
    fixture()["pools"]
        .as_array()
        .expect("the fixture carries pools")
        .iter()
        .map(|pool| {
            (
                pool["address"].as_str().expect("an address").to_owned(),
                hex_bytes(pool["data_hex"].as_str().expect("hex")),
            )
        })
        .collect()
}

/// One captured pool's bytes, by address.
fn capture(address: &str) -> Vec<u8> {
    captures()
        .into_iter()
        .find(|(at, _)| at == address)
        .unwrap_or_else(|| panic!("{address} is in the fixture"))
        .1
}

fn field(address: &str, name: &str) -> String {
    let value = fixture();
    let pools = value["pools"].as_array().expect("pools");
    let pool = pools
        .iter()
        .find(|pool| pool["address"].as_str() == Some(address))
        .unwrap_or_else(|| panic!("{address} is in the fixture"));
    pool[name]
        .as_str()
        .unwrap_or_else(|| panic!("{address} carries {name}"))
        .to_owned()
}

fn address(s: &str) -> Address {
    Address::from_str(s).expect("a base58 address")
}

/// The pool behind the buy research 0028 priced.
const TIED_TO_A_TRANSACTION: &str = "C4mLt6fs2dL2W1oovZAT9QpM3tpL6CA7DZ8hqHU9Ldqb";

#[test]
fn every_capture_is_a_pumpswap_account_carrying_the_pool_discriminator() {
    let value = fixture();
    let pools = value["pools"].as_array().expect("pools");
    assert_eq!(pools.len(), 10, "ten pools were captured");
    for pool in pools {
        let address = pool["address"].as_str().expect("an address");
        let bytes = hex_bytes(pool["data_hex"].as_str().expect("hex"));
        assert_eq!(
            pool["owner"].as_str(),
            Some(radar_decode::pumpswap::PROGRAM_ID.to_string().as_str()),
            "{address} is owned by PumpSwap"
        );
        assert_eq!(
            bytes.len(),
            usize::try_from(pool["len"].as_u64().expect("a length")).expect("fits"),
            "{address}: the hex is as long as the capture says"
        );
        assert_eq!(
            &bytes[..8],
            &pool::DISCRIMINATOR,
            "{address} starts with the Pool discriminator"
        );
    }
}

#[test]
fn the_discriminator_is_anchors_rule_applied_to_the_account_name() {
    // Captured first, derived second. The constant in `pool.rs` is the eight
    // bytes every account in the fixture begins with; this asserts that those
    // bytes *also* happen to be what Anchor's naming rule produces, so the rule
    // is recorded as confirmed rather than assumed.
    let mut hasher = Sha256::new();
    hasher.update(b"account:Pool");
    let derived: [u8; 8] = hasher.finalize()[..8].try_into().expect("eight bytes");
    assert_eq!(
        derived,
        pool::DISCRIMINATOR,
        "sha256(\"account:Pool\")[..8] reproduces the captured discriminator"
    );
}

#[test]
fn every_field_of_the_pool_a_transaction_named_is_the_value_that_transaction_carried() {
    // The five addresses asserted here are not read back out of the fixture's
    // own derived fields, which would be circular. They are the accounts the
    // buy `5HxVtAB7...eTxZ8WG` carried at positions 0, 3, 4, 7 and 8 -- the
    // transaction research 0028 priced, read with `getTransaction` on
    // 2026-09-09. A field order that put any of them at the wrong offset would
    // disagree with a transaction the network accepted.
    let parsed = Pool::parse(&capture(TIED_TO_A_TRANSACTION)).expect("the capture parses");
    assert_eq!(
        parsed.base_mint,
        address("A6Zpvj47pSin4YLdUWKRG92x5HGJjtD35sZmJQ6cpump")
    );
    assert_eq!(
        parsed.quote_mint,
        address("So11111111111111111111111111111111111111112")
    );
    assert_eq!(
        parsed.pool_base_token_account,
        address("7QtzvM1VFVRJMNVeSPNQ8kMzwpDDkPhEHDsV4VUrxiq5")
    );
    assert_eq!(
        parsed.pool_quote_token_account,
        address("7j4nv9cYSye9gbdNtQyMTHNZD54V96BWqYQStaZ8i6xM")
    );

    // The rest of the layout, read from the same capture at slot 445,767,146.
    assert_eq!(parsed.pool_bump, 255);
    assert_eq!(parsed.index, 0);
    assert_eq!(
        parsed.creator,
        address("ATCVspQ27gpALiG8YerojbFxkxTqppQt889AQ5yyUmfk")
    );
    assert_eq!(
        parsed.lp_mint,
        address("EYPNLp1bYj5dquwAe27DZwC9E1V6hhmhyYCHMU5qkKvf")
    );
    assert_eq!(parsed.lp_supply, 4_193_388_282_678);
    assert_eq!(
        parsed.coin_creator,
        Some(address("3J3mJoGcYVQApourtHkneyXM2SWRYbrtrEnKSBQ4sCi6"))
    );
    assert_eq!(parsed.is_mayhem_mode, Some(false));
    assert_eq!(parsed.is_cashback_coin, Some(false));
    assert_eq!(parsed.virtual_quote_reserves, Some(17_584_505_289));
}

#[test]
fn every_length_the_chain_holds_parses_and_carries_exactly_the_fields_it_has() {
    // Rule 9: a field the account is too short to hold is `None`, never a zero.
    for (address, bytes) in captures() {
        let parsed = Pool::parse(&bytes)
            .unwrap_or_else(|e| panic!("{address} ({} bytes): {e:?}", bytes.len()));
        let len = bytes.len();
        assert_eq!(
            parsed.coin_creator.is_some(),
            len >= WITH_COIN_CREATOR_LEN,
            "{address}: coin_creator at {len} bytes"
        );
        assert_eq!(
            parsed.is_mayhem_mode.is_some(),
            len >= WITH_MAYHEM_LEN,
            "{address}: is_mayhem_mode at {len} bytes"
        );
        assert_eq!(
            parsed.is_cashback_coin.is_some(),
            len >= WITH_CASHBACK_LEN,
            "{address}: is_cashback_coin at {len} bytes"
        );
        assert_eq!(
            parsed.virtual_quote_reserves.is_some(),
            len >= FULL_LEN,
            "{address}: virtual_quote_reserves at {len} bytes"
        );
    }
    let lengths: std::collections::BTreeSet<usize> =
        captures().into_iter().map(|(_, b)| b.len()).collect();
    assert_eq!(
        lengths,
        [211, 243, 244, 245, 261, 270, 300, 301]
            .into_iter()
            .collect(),
        "one capture of every length the census found"
    );
}

#[test]
fn the_shortest_pool_has_no_coin_creator_rather_than_a_zero_one() {
    // 37,625 accounts on the chain are this shape. The 243-byte capture beside
    // it *does* carry the field and it is the all-zero address, so "absent" and
    // "present and default" are both real and must not collapse into each other.
    let short = Pool::parse(&capture("13bkcX5JGKaaj35brGP9ZUJtG1iUAbQVVqYDnUFL1B9"))
        .expect("a 211-byte pool parses");
    assert_eq!(short.coin_creator, None);
    let default = Pool::parse(&capture("114hoiDuak8RVc8JCQgX7hPrEuqaoUwKLeQrKE7ESFa"))
        .expect("a 243-byte pool parses");
    assert_eq!(default.coin_creator, Some(Address::SYSTEM_PROGRAM));
    assert_ne!(short.coin_creator, default.coin_creator);
}

#[test]
fn no_field_that_reads_zero_in_one_pool_is_taken_for_a_constant() {
    // AGENTS §1: zero is a measurement about the instrument until proved
    // otherwise. Three captures carry three different `virtual_quote_reserves`,
    // one of them zero, so the field is read rather than assumed -- and the
    // vendor's README, which says the value is currently zero across all pools,
    // is wrong about mainnet on 2026-09-09.
    let a = Pool::parse(&capture(TIED_TO_A_TRANSACTION)).expect("parses");
    let b = Pool::parse(&capture("GNUcWTRcY94cmP9HgxH8gMNRPg1PaDcukmWp15hA3Wfk")).expect("parses");
    let zero =
        Pool::parse(&capture("6xsdRpzd53b7LsLHjNppa7fZzJu1xW3s1jAV8X79gPvd")).expect("parses");
    assert_eq!(a.virtual_quote_reserves, Some(17_584_505_289));
    assert_eq!(b.virtual_quote_reserves, Some(17_584_505_417));
    assert_eq!(zero.virtual_quote_reserves, Some(0));
    assert_ne!(a.virtual_quote_reserves, b.virtual_quote_reserves);

    // The same for `index`, which the vendor documents only through its
    // `CANONONICAL_POOL_INDEX == 0` -- its spelling -- and which is 1 here.
    let indexed =
        Pool::parse(&capture("1ED6iUwkgnqLBcASfPUcSKitowQdGqq52M6TLyohTpF")).expect("parses");
    assert_eq!(indexed.index, 1);
}

#[test]
fn the_quote_mint_is_not_always_sol() {
    // One capture quotes in USDC and one in a `pump` mint. Any later pricing
    // that reaches for lamports has to consult this field first.
    let usdc =
        Pool::parse(&capture("82zcJ16FYLuqbjxbdHKbD3F7YigdhBe6YHTTvsErNHB")).expect("parses");
    assert_eq!(
        usdc.quote_mint,
        address("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")
    );
    let wsol = address("So11111111111111111111111111111111111111112");
    let quotes: std::collections::BTreeSet<String> = captures()
        .into_iter()
        .map(|(_, b)| Pool::parse(&b).expect("parses").quote_mint.to_string())
        .collect();
    assert!(quotes.len() > 1, "the quote mint varies across pools");
    assert!(quotes.contains(&wsol.to_string()));
}

#[test]
fn the_two_token_programs_are_separate_facts_and_neither_is_a_constant() {
    // They are NOT fields of this account -- the documented field order accounts
    // for every byte through 261 and the rest is zero -- so they are captured
    // from the mints and asserted here. What matters for a later quote is that
    // a pool can be Token-2022 on one side and SPL Token on the other, and that
    // which side is which varies by pool. Token-2022's transfer-fee extension
    // means the amount that reaches a vault is not the amount in the
    // instruction, so an assumption either way is a wrong price.
    let mut mixed = 0_usize;
    let mut base_programs = std::collections::BTreeSet::new();
    let mut quote_programs = std::collections::BTreeSet::new();
    for (address, _) in captures() {
        let base = field(&address, "base_token_program");
        let quote = field(&address, "quote_token_program");
        assert!(base == SPL_TOKEN || base == TOKEN_2022, "{address}: {base}");
        assert!(
            quote == SPL_TOKEN || quote == TOKEN_2022,
            "{address}: {quote}"
        );
        if base != quote {
            mixed += 1;
        }
        base_programs.insert(base);
        quote_programs.insert(quote);
    }
    assert!(mixed >= 1, "at least one pool mixes the two token programs");
    assert_eq!(base_programs.len(), 2, "the base side is not one program");
    assert_eq!(quote_programs.len(), 2, "the quote side is not one program");
}

#[test]
fn an_account_shorter_than_the_base_layout_is_refused() {
    let bytes = capture(TIED_TO_A_TRANSACTION);
    assert_eq!(
        Pool::parse(&bytes[..BASE_LEN - 1]),
        Err(Malformed::TooShort {
            len: BASE_LEN - 1,
            needed: BASE_LEN
        })
    );
    assert_eq!(
        Pool::parse(&[]),
        Err(Malformed::TooShort {
            len: 0,
            needed: BASE_LEN
        })
    );
}

#[test]
fn a_foreign_account_of_ample_length_is_refused_rather_than_read() {
    // PumpSwap's own global config: same owner, 940 bytes, different account.
    // Nothing about its length would stop a parser that only checked size, and
    // every field it produced would look like a plausible address.
    let global: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/pumpswap_fees.json")).expect("valid JSON");
    let bytes = hex_bytes(
        global["accounts"]["global_config"]["data_hex"]
            .as_str()
            .expect("hex"),
    );
    assert!(bytes.len() > FULL_LEN);
    let found: [u8; 8] = bytes[..8].try_into().expect("eight bytes");
    assert_ne!(found, pool::DISCRIMINATOR);
    assert_eq!(
        Pool::parse(&bytes),
        Err(Malformed::WrongDiscriminator { found })
    );
}

#[test]
fn an_account_that_stops_inside_a_field_is_refused_rather_than_half_read() {
    let bytes = capture(TIED_TO_A_TRANSACTION);
    // One byte into `coin_creator`: neither present nor absent.
    let one_into_coin_creator = BASE_LEN + 1;
    assert_eq!(
        Pool::parse(&bytes[..one_into_coin_creator]),
        Err(Malformed::PartialField {
            len: one_into_coin_creator,
            field: "coin_creator",
            needed: WITH_COIN_CREATOR_LEN
        })
    );
    // One byte into `virtual_quote_reserves`, which is sixteen bytes wide.
    let one_into_reserves = WITH_CASHBACK_LEN + 1;
    assert_eq!(
        Pool::parse(&bytes[..one_into_reserves]),
        Err(Malformed::PartialField {
            len: one_into_reserves,
            field: "virtual_quote_reserves",
            needed: FULL_LEN
        })
    );
    // Eight bytes in is where a `u64` reading of the field would have stopped.
    // The chain says sixteen, so this is still a refusal and not a pool.
    assert_eq!(
        Pool::parse(&bytes[..253]),
        Err(Malformed::PartialField {
            len: 253,
            field: "virtual_quote_reserves",
            needed: FULL_LEN
        })
    );
}

#[test]
fn a_non_zero_byte_past_the_known_fields_is_refused() {
    // Every one of the 601,388 accounts of this length holds zero here today.
    // A byte that is not zero means a field this layout does not know about is
    // set, and answering anyway is how a decoder survives a program upgrade
    // while quietly becoming wrong.
    let mut bytes = capture(TIED_TO_A_TRANSACTION);
    assert_eq!(bytes.len(), 301);
    bytes[FULL_LEN] = 1;
    assert_eq!(
        Pool::parse(&bytes),
        Err(Malformed::UnknownTrailingData {
            at: FULL_LEN,
            found: 1
        })
    );
    let mut last = capture(TIED_TO_A_TRANSACTION);
    let end = last.len() - 1;
    last[end] = 0xff;
    assert_eq!(
        Pool::parse(&last),
        Err(Malformed::UnknownTrailingData {
            at: end,
            found: 0xff
        })
    );
}

#[test]
fn a_flag_byte_that_is_not_a_boolean_is_refused() {
    let mut bytes = capture(TIED_TO_A_TRANSACTION);
    bytes[243] = 2;
    assert_eq!(
        Pool::parse(&bytes),
        Err(Malformed::NotABool {
            field: "is_mayhem_mode",
            found: 2
        })
    );
    let mut cashback = capture(TIED_TO_A_TRANSACTION);
    cashback[244] = 9;
    assert_eq!(
        Pool::parse(&cashback),
        Err(Malformed::NotABool {
            field: "is_cashback_coin",
            found: 9
        })
    );
    // One is still true, and the refusal is about the byte, not about the flag.
    let mut set = capture(TIED_TO_A_TRANSACTION);
    set[243] = 1;
    assert_eq!(
        Pool::parse(&set).expect("parses").is_mayhem_mode,
        Some(true)
    );
}
