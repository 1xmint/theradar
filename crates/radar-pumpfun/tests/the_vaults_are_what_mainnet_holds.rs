// SPDX-License-Identifier: Apache-2.0
//! What a PumpSwap pool holds, asserted against accounts read from mainnet.
//!
//! Every byte below comes from `fixtures/pumpswap_reserves.json`, and every entry
//! in that file is a **single `getMultipleAccounts` call** carrying a pool, both
//! its vaults and both its mints. That is what makes one slot cover all five. A
//! fixture assembled from five `getAccountInfo` calls would look identical and
//! would be five different instants.
//!
//! # What the capture disposed of
//!
//! 1. **Decimals are not a constant, and the quote side is not lamports.** The
//!    four pools captured hold mints with 6, 7 and 9 decimals, and one quotes in
//!    USDC. A quote reserve typed as lamports is wrong by a factor of a thousand
//!    on that pool and by an arbitrary factor on the others.
//! 2. **The two token programs differ within a pool and swap sides between
//!    pools.** Two of the four captures have a Token-2022 base against a classic
//!    SPL quote, and one has it the other way round. Neither program belongs to
//!    a side, and neither is in the `Pool` account.
//! 3. **Token-2022 extensions are real.** Every Token-2022 vault captured carries
//!    `ImmutableOwner`; both Token-2022 mints carry `MetadataPointer` and
//!    `TokenMetadata`. Those three change nothing about a balance and are
//!    accepted. The `specimens` in the fixture carry the ones that do change it,
//!    and each is refused **by name** rather than parsed as though it were plain
//!    SPL.
//!
//! # What is deliberately absent
//!
//! No price, no impact, no capacity, no effective reserve. `Reserve::raw` is the
//! vault balance and nothing here adds `virtual_quote_reserves` to it, because
//! research 0033 read that field without establishing what it means.

use std::str::FromStr as _;

use radar_pumpfun::Pool;
use radar_pumpfun::token::{
    AccountState, Extension, MintAccount, Reserve, TokenAccount, TokenMalformed, TokenProgram,
};
use radar_types::{Address, Asset, b64};

const FIXTURE: &str = include_str!("fixtures/pumpswap_reserves.json");

fn fixture() -> serde_json::Value {
    serde_json::from_str(FIXTURE).expect("the fixture is valid JSON")
}

fn address(text: &str) -> Address {
    Address::from_str(text).expect("the fixture carries base58 addresses")
}

/// One account out of a read, by the role the capture recorded for it.
struct Captured {
    address: Address,
    owner: Address,
    data: Vec<u8>,
}

/// Every account of the read for a pool, keyed by role.
fn read_for(pool: &str) -> Vec<(String, Captured)> {
    fixture()["reads"]
        .as_array()
        .expect("the fixture carries reads")
        .iter()
        .find(|read| read["pool"].as_str() == Some(pool))
        .unwrap_or_else(|| panic!("{pool} is in the fixture"))["accounts"]
        .as_array()
        .expect("a read carries accounts")
        .iter()
        .map(|account| {
            (
                account["role"].as_str().expect("a role").to_owned(),
                Captured {
                    address: address(account["address"].as_str().expect("an address")),
                    owner: address(account["owner"].as_str().expect("an owner")),
                    data: b64::decode(account["data_b64"].as_str().expect("base64"))
                        .expect("the fixture is valid base64"),
                },
            )
        })
        .collect()
}

fn role(pool: &str, role: &str) -> Captured {
    read_for(pool)
        .into_iter()
        .find(|(name, _)| name == role)
        .unwrap_or_else(|| panic!("{pool} has a {role}"))
        .1
}

/// A specimen captured for its extensions rather than for its pool.
fn specimen(pool_mint: &str) -> Captured {
    let found = fixture()["specimens"]
        .as_array()
        .expect("the fixture carries specimens")
        .iter()
        .find(|s| s["address"].as_str() == Some(pool_mint))
        .unwrap_or_else(|| panic!("{pool_mint} is a specimen"))
        .clone();
    Captured {
        address: address(found["address"].as_str().expect("an address")),
        owner: address(found["owner"].as_str().expect("an owner")),
        data: b64::decode(found["data_b64"].as_str().expect("base64")).expect("valid base64"),
    }
}

/// Token-2022 base against a classic SPL wrapped-SOL quote.
const MIXED_PROGRAMS: &str = "C4mLt6fs2dL2W1oovZAT9QpM3tpL6CA7DZ8hqHU9Ldqb";
/// The mirror image, wrapped SOL on the base side.
const MIRROR: &str = "6xsdRpzd53b7LsLHjNppa7fZzJu1xW3s1jAV8X79gPvd";
/// Both sides classic SPL, and the shortest pool length that exists.
const ALL_SPL: &str = "13bkcX5JGKaaj35brGP9ZUJtG1iUAbQVVqYDnUFL1B9";
/// Quotes in USDC, which has six decimals rather than wrapped SOL's nine.
const USDC_QUOTED: &str = "82zcJ16FYLuqbjxbdHKbD3F7YigdhBe6YHTTvsErNHB";

/// A Token-2022 mint whose only extension is a transfer fee.
const TRANSFER_FEE_MINT: &str = "CKfatsPMUf8SkiURsDXs7eK6GWb4Jsd6UDbs7twMCWxo";
/// Eight extensions, four of which change what a balance is worth.
const MANY_EXTENSIONS_MINT: &str = "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo";

#[test]
fn a_vault_parses_under_both_programs_and_the_program_comes_from_the_owner() {
    // The same field, two programs, and neither is a property of the side. The
    // `Pool` account says nothing about either; the owner in the RPC response is
    // the only source.
    let base = role(MIXED_PROGRAMS, "base_vault");
    let quote = role(MIXED_PROGRAMS, "quote_vault");

    let parsed_base = TokenAccount::parse(&base.data, &base.owner).expect("a Token-2022 vault");
    let parsed_quote = TokenAccount::parse(&quote.data, &quote.owner).expect("a classic SPL vault");

    assert_eq!(parsed_base.program, TokenProgram::Token2022);
    assert_eq!(parsed_quote.program, TokenProgram::Spl);
    assert_eq!(parsed_base.state, AccountState::Initialized);
    assert_eq!(parsed_quote.state, AccountState::Initialized);
    assert_eq!(parsed_base.delegate, None);

    // The vault is owned by the pool it belongs to, and holds the mint the pool
    // names. Both are read out of the pool account captured in the same call.
    let pool = Pool::parse(&role(MIXED_PROGRAMS, "pool").data).expect("a pool");
    assert_eq!(parsed_base.owner, address(MIXED_PROGRAMS));
    assert_eq!(parsed_base.mint, pool.base_mint);
    assert_eq!(parsed_quote.mint, pool.quote_mint);
    assert_eq!(base.address, pool.pool_base_token_account);
    assert_eq!(quote.address, pool.pool_quote_token_account);

    // And the mirror pool has the programs the other way round, so nothing here
    // may key a program off a side.
    let mirror_base = role(MIRROR, "base_vault");
    let mirror_quote = role(MIRROR, "quote_vault");
    assert_eq!(
        TokenAccount::parse(&mirror_base.data, &mirror_base.owner)
            .expect("a vault")
            .program,
        TokenProgram::Spl,
    );
    assert_eq!(
        TokenAccount::parse(&mirror_quote.data, &mirror_quote.owner)
            .expect("a vault")
            .program,
        TokenProgram::Token2022,
    );
}

#[test]
fn decimals_are_read_from_the_mint_and_are_not_a_constant() {
    // Four mints, three different scales. A parser that assumed nine would be
    // wrong by a thousand on USDC and by ten on the 211-byte pool's base.
    let scales = [
        (MIXED_PROGRAMS, "base_mint", 6u8),
        (MIXED_PROGRAMS, "quote_mint", 9),
        (ALL_SPL, "base_mint", 7),
        (USDC_QUOTED, "quote_mint", 6),
    ];
    for (pool, which, decimals) in scales {
        let mint = role(pool, which);
        let parsed = MintAccount::parse(&mint.data, &mint.owner).expect("a mint");
        assert_eq!(
            parsed.decimals, decimals,
            "{pool} {which} has {decimals} decimals",
        );
        assert!(parsed.initialized, "{pool} {which} is initialized");
    }
}

#[test]
fn a_freeze_authority_is_read_and_is_not_a_refusal() {
    // USDC can freeze an account of its mint and wrapped SOL cannot. Both are
    // legitimate quote assets, so the fact is recorded rather than refused --
    // and the four-byte option tag is what distinguishes them, which is why a
    // parser that skipped it would read a freeze authority out of nothing.
    let usdc = role(USDC_QUOTED, "quote_mint");
    let wrapped_sol = role(MIXED_PROGRAMS, "quote_mint");

    let usdc = MintAccount::parse(&usdc.data, &usdc.owner).expect("USDC");
    let wrapped_sol = MintAccount::parse(&wrapped_sol.data, &wrapped_sol.owner).expect("wSOL");

    assert!(
        usdc.freeze_authority.is_some(),
        "USDC has a freeze authority"
    );
    assert_eq!(wrapped_sol.freeze_authority, None);
}

#[test]
fn a_reserve_carries_its_asset_and_never_a_bare_integer() {
    // The USDC-quoted pool, because it is the one where every wrong assumption
    // is visible: the quote is not SOL, it is not lamports, and its scale is not
    // the base's.
    let pool = Pool::parse(&role(USDC_QUOTED, "pool").data).expect("a pool");
    let vault = role(USDC_QUOTED, "quote_vault");
    let mint = role(USDC_QUOTED, "quote_mint");

    let account = TokenAccount::parse(&vault.data, &vault.owner).expect("a vault");
    let parsed_mint = MintAccount::parse(&mint.data, &mint.owner).expect("a mint");
    let reserve =
        Reserve::new(vault.address, &account, pool.quote_mint, &parsed_mint).expect("a reserve");

    assert_eq!(reserve.asset, Asset::Usdc);
    assert_eq!(reserve.asset.mint(), Some(Asset::USDC_MINT));
    assert_eq!(reserve.decimals, 6);
    assert_eq!(reserve.raw, account.amount);
    assert_eq!(reserve.vault, vault.address);

    // And a Token-2022 mint is a different asset from the same address under the
    // classic program, which is why the reserve keys off the owner and not the
    // mint alone.
    let base_vault = role(MIXED_PROGRAMS, "base_vault");
    let base_mint = role(MIXED_PROGRAMS, "base_mint");
    let pool = Pool::parse(&role(MIXED_PROGRAMS, "pool").data).expect("a pool");
    let base = Reserve::new(
        base_vault.address,
        &TokenAccount::parse(&base_vault.data, &base_vault.owner).expect("a vault"),
        pool.base_mint,
        &MintAccount::parse(&base_mint.data, &base_mint.owner).expect("a mint"),
    )
    .expect("a reserve");
    assert_eq!(base.asset, Asset::token_2022(pool.base_mint));
    assert_ne!(base.asset, Asset::spl(pool.base_mint));
}

#[test]
fn the_decimals_cannot_come_from_a_different_mint_than_the_vault_holds() {
    // The failure this closes is silent: the base vault of one pool with the
    // quote mint of the same pool produces a number scaled by the wrong power of
    // ten, and every field of it looks ordinary.
    let vault = role(USDC_QUOTED, "base_vault");
    let wrong = role(USDC_QUOTED, "quote_mint");
    let pool = Pool::parse(&role(USDC_QUOTED, "pool").data).expect("a pool");

    let refused = Reserve::new(
        vault.address,
        &TokenAccount::parse(&vault.data, &vault.owner).expect("a vault"),
        pool.quote_mint,
        &MintAccount::parse(&wrong.data, &wrong.owner).expect("a mint"),
    )
    .expect_err("the vault does not hold that mint");

    assert!(
        matches!(refused, TokenMalformed::MintMismatch { held, read_from }
            if held == pool.base_mint && read_from == pool.quote_mint),
        "{refused:?}",
    );
}

#[test]
fn the_extensions_the_pools_carry_are_the_three_that_change_nothing() {
    // Every Token-2022 vault captured carries ImmutableOwner and nothing else;
    // both Token-2022 mints carry MetadataPointer and TokenMetadata. This is the
    // evidence behind the accepted list being exactly three long.
    assert_eq!(
        Extension::ACCEPTED,
        [
            Extension::IMMUTABLE_OWNER,
            Extension::METADATA_POINTER,
            Extension::TOKEN_METADATA,
        ],
    );
    for extension in Extension::ACCEPTED {
        assert!(extension.is_accepted(), "{extension}");
        assert!(extension.name().is_some(), "{extension} has a name");
    }

    // Which is only meaningful because the captured accounts do carry them: a
    // parser that refused every extension would pass an emptier version of this
    // test and refuse every Token-2022 pool on the chain.
    for (pool, which) in [
        (MIXED_PROGRAMS, "base_vault"),
        (MIRROR, "quote_vault"),
        (MIXED_PROGRAMS, "base_mint"),
        (MIRROR, "quote_mint"),
    ] {
        let account = role(pool, which);
        assert!(
            account.data.len() > 165,
            "{pool} {which} carries extensions",
        );
    }
}

#[test]
fn a_transfer_fee_is_refused_and_the_refusal_names_it() {
    // The cleanest case there is: one extension, nothing else unusual about the
    // mint, and a balance in it that still cannot be moved without losing a fee
    // this crate does not model. Parsed as plain SPL it would read as an
    // ordinary five-decimal mint.
    let mint = specimen(TRANSFER_FEE_MINT);
    let refused = MintAccount::parse(&mint.data, &mint.owner).expect_err("a transfer fee");

    let TokenMalformed::UnmodelledExtension { extension } = refused else {
        panic!("{refused:?} should name the extension");
    };
    assert_eq!(extension.name(), Some("TransferFeeConfig"));
    assert_eq!(extension.to_string(), "TransferFeeConfig (1)");

    // The base 82 bytes are perfectly readable. That is exactly the danger: the
    // refusal is not because the bytes are unreadable, it is because reading
    // them would answer a question the extension has changed.
    assert_eq!(mint.data[44], 5, "five decimals, plainly there");
}

#[test]
fn extensions_arrive_in_bundles_and_one_harmless_one_proves_nothing() {
    // Eight extensions on one mint, and the first two the walk meets are a mint
    // close authority and a permanent delegate. A parser that stopped at the
    // first recognised extension, or that accepted a mint because its metadata
    // pointer was familiar, would price a mint someone else can empty.
    let mint = specimen(MANY_EXTENSIONS_MINT);
    let refused = MintAccount::parse(&mint.data, &mint.owner).expect_err("eight extensions");

    let TokenMalformed::UnmodelledExtension { extension } = refused else {
        panic!("{refused:?} should name the extension");
    };
    assert_eq!(extension.name(), Some("MintCloseAuthority"));
}

#[test]
fn an_account_owned_by_neither_token_program_is_refused_rather_than_read() {
    // The pool account itself, offered as a vault. It is 301 bytes of plausible
    // data and its first 165 bytes would parse into a mint, an owner and a
    // balance without complaint if the owner were not checked first.
    let pool = role(MIXED_PROGRAMS, "pool");
    let refused = TokenAccount::parse(&pool.data, &pool.owner).expect_err("not a token account");
    assert!(
        matches!(refused, TokenMalformed::NotATokenProgram { owner } if owner == pool.owner),
        "{refused:?}",
    );
}

#[test]
fn a_token_account_is_refused_as_a_mint_and_a_mint_as_a_token_account() {
    // Both directions, because both are silent. A mint read as a token account
    // puts the supply where the balance belongs; a token account read as a mint
    // takes one byte of the owner's key as the decimals.
    let vault = role(MIXED_PROGRAMS, "base_vault");
    let mint = role(MIXED_PROGRAMS, "base_mint");

    let as_mint = MintAccount::parse(&vault.data, &vault.owner).expect_err("a vault is not a mint");
    assert!(
        matches!(
            as_mint,
            TokenMalformed::WrongAccountType {
                found: 2,
                expected: 1
            }
        ),
        "{as_mint:?}",
    );
    let as_account =
        TokenAccount::parse(&mint.data, &mint.owner).expect_err("a mint is not a vault");
    assert!(
        matches!(
            as_account,
            TokenMalformed::WrongAccountType {
                found: 1,
                expected: 2
            }
        ),
        "{as_account:?}",
    );

    // The unextended case has no account-type byte to check, so the length rule
    // is what has to hold it: a classic SPL vault is 165 bytes and a mint is 82,
    // and neither is a valid length for the other.
    let plain_vault = role(ALL_SPL, "base_vault");
    let plain_mint = role(ALL_SPL, "base_mint");
    assert!(matches!(
        MintAccount::parse(&plain_vault.data, &plain_vault.owner).expect_err("165 is not a mint"),
        TokenMalformed::WrongLength {
            len: 165,
            expected: 82
        },
    ),);
    assert!(matches!(
        TokenAccount::parse(&plain_mint.data, &plain_mint.owner).expect_err("82 is not a vault"),
        TokenMalformed::WrongLength {
            len: 82,
            expected: 165
        },
    ),);
}

#[test]
fn a_token_2022_length_between_a_mint_and_an_account_type_byte_is_refused() {
    // A 165-byte Token-2022 token account is longer than a mint's 82 bytes and
    // has no byte at 165 to say what it is. Accepting the band would let one
    // parse as a mint whose decimals are a byte of somebody's public key.
    let vault = role(MIXED_PROGRAMS, "quote_vault");
    assert_eq!(vault.data.len(), 165);
    let refused = MintAccount::parse(&vault.data, &TokenProgram::Token2022.id())
        .expect_err("165 bytes is neither shape");
    assert!(
        matches!(
            refused,
            TokenMalformed::TooShort {
                len: 165,
                needed: 166
            }
        ),
        "{refused:?}",
    );
}

#[test]
fn the_two_program_addresses_are_the_ones_mainnet_used() {
    // Hand-decoded byte arrays, checked against the base58 the capture recorded.
    // A transposed pair renders as a different address and would make every
    // account in the workspace refuse.
    let seen: Vec<String> = fixture()["reads"]
        .as_array()
        .expect("reads")
        .iter()
        .flat_map(|read| read["accounts"].as_array().expect("accounts"))
        .filter(|account| account["role"].as_str() != Some("pool"))
        .map(|account| account["owner"].as_str().expect("an owner").to_owned())
        .collect();

    assert!(seen.contains(&TokenProgram::Spl.id().to_string()));
    assert!(seen.contains(&TokenProgram::Token2022.id().to_string()));
    for owner in &seen {
        assert!(
            TokenProgram::of(&address(owner)).is_some(),
            "{owner} is a token program",
        );
    }
    assert_eq!(TokenProgram::of(&address(MIXED_PROGRAMS)), None);
}
