// SPDX-License-Identifier: Apache-2.0
//! The PumpSwap `Pool` account, as mainnet holds it.
//!
//! # The account has eight lengths, not one
//!
//! [`crate::curve::BondingCurve`] is a fixed 81 bytes and every curve on the
//! chain is that shape. `Pool` is not. A census on 2026-09-09 over every account
//! owned by PumpSwap carrying the `Pool` discriminator -- 1,374,123 of them,
//! read with `getProgramAccounts` and counted by the `space` the RPC reports --
//! found **eight** lengths:
//!
//! | bytes | accounts | what it holds |
//! |---|---|---|
//! | 211 | 37,625 | through `lp_supply` |
//! | 243 | 74,457 | `+ coin_creator` |
//! | 244 | 40,647 | `+ is_mayhem_mode` |
//! | 245 | 101,699 | `+ is_cashback_coin` |
//! | 261 | 10,813 | `+ virtual_quote_reserves` |
//! | 270 | 268 | the above and nine zero bytes |
//! | 300 | 507,226 | the above and thirty-nine zero bytes |
//! | 301 | 601,388 | the above and forty zero bytes |
//!
//! Five of those are **exactly** the cumulative field boundaries of the vendor's
//! published field order, and they are the strongest evidence in this file. They
//! are what proves the field *widths*: `245 + 16 = 261` and there is no 253, so
//! `virtual_quote_reserves` is sixteen bytes wide. That is why the field is
//! [`i128`](Pool::virtual_quote_reserves) here even though every value observed
//! fits in eight bytes -- the chain drew the boundary, not the documentation.
//!
//! So the layout is a **prefix ladder**, and reading it is not "check the length,
//! then slice". A shorter account is a real pool whose later fields do not exist
//! yet; the program's own `extend_account` instruction is how they come to. Rule
//! 9 governs the difference: a field that is not in the account is
//! [`None`](Option::None), never a zero, because `coin_creator` reading
//! all-zeroes is *also* a real state and the two must not collapse.
//!
//! # What is not here
//!
//! **The reserves.** They live in the two token accounts this struct names, and
//! parsing those is the next piece of work, not this one. Nothing in this module
//! quotes, prices, or estimates impact.
//!
//! **The two token programs.** `base_token_program` and `quote_token_program`
//! are accounts a PumpSwap instruction carries, and they are *not* fields of this
//! account -- the capture settles that, since the field order accounts for every
//! byte through 261 and the rest is zero. They are the owner programs of the two
//! mints and must be read from the mints. They are worth naming here anyway
//! because the capture disposes of two comfortable assumptions: they are not the
//! same program as each other (five of the ten captured pools mix SPL Token and
//! Token-2022 across the two sides), and neither side is a constant (a pool's
//! base can be Token-2022 with an SPL Token quote, and another pool has it the
//! other way round). Token-2022's extensions change what a transfer delivers, so
//! a quote that assumed either would be wrong by a fee.

use radar_types::Address;

use crate::curve::Malformed;

/// A PumpSwap AMM pool.
///
/// The `Option` fields are the ones the account may be too short to hold -- see
/// the module documentation for the eight lengths. `None` means **the account
/// does not have this field**, which is a different fact from the field being
/// zero, and both occur on mainnet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pool {
    /// The bump seed of the pool's own address.
    pub pool_bump: u8,
    /// Which pool this is for its `(creator, base_mint, quote_mint)` triple.
    ///
    /// Not always zero: the 261-byte capture has index 1. The vendor documents
    /// `CANONONICAL_POOL_INDEX == 0` -- its spelling -- as the pool a migrated
    /// coin lands in, which is a statement about migration and not about this
    /// field's range.
    pub index: u16,
    /// Who created the pool. Not necessarily who launched the coin.
    pub creator: Address,
    /// The mint being priced.
    pub base_mint: Address,
    /// The mint it is priced in. **Not always SOL** -- one capture quotes in
    /// USDC and another in a `pump` mint, so nothing may assume lamports.
    pub quote_mint: Address,
    /// The pool's LP mint.
    pub lp_mint: Address,
    /// The token account holding the base reserve. Its balance is not here.
    pub pool_base_token_account: Address,
    /// The token account holding the quote reserve. Its balance is not here.
    pub pool_quote_token_account: Address,
    /// Circulating LP supply, before burns and lock-ups.
    pub lp_supply: u64,
    /// Who launched the coin, and therefore who is paid the creator fee.
    ///
    /// `None` on a 211-byte pool, which predates the field. All-zeroes on
    /// several captures, which is the field present and set to the default
    /// address -- a different thing, and why this is not flattened.
    pub coin_creator: Option<Address>,
    /// The venue's "mayhem mode" flag. `None` below 244 bytes.
    pub is_mayhem_mode: Option<bool>,
    /// Whether the coin is in the cashback programme. `None` below 245 bytes.
    pub is_cashback_coin: Option<bool>,
    /// Virtual quote reserves, in the quote mint's smallest unit.
    ///
    /// `None` below 261 bytes. Signed and sixteen bytes wide because the chain
    /// says so -- see the module documentation. Every value observed on
    /// 2026-09-09 was non-negative and fitted in eight bytes, and the vendor's
    /// IDL comment says it is zero on non-boost pools. Two of the six captures
    /// long enough to hold the field carry a non-zero value, which the vendor's
    /// README -- "currently zero across all pools" -- says cannot happen.
    ///
    /// **This is not a reserve you may price against on its own.** The vendor's
    /// rule is `effective_quote_reserves = quote vault balance + this`, and the
    /// vault balance is not in this account.
    pub virtual_quote_reserves: Option<i128>,
}

/// The discriminator every `Pool` account starts with.
///
/// Observed on mainnet -- the standard every discriminator in this crate is held
/// to. It also happens to be Anchor's `sha256("account:Pool")[..8]`, which
/// [`the_discriminator_is_the_one_mainnet_uses`] recomputes rather than trusts.
///
/// [`the_discriminator_is_the_one_mainnet_uses`]: https://github.com/hey-vera/radar/blob/main/crates/radar-pumpfun/tests/the_pool_layout_is_what_mainnet_holds.rs
pub const DISCRIMINATOR: [u8; 8] = [0xf1, 0x9a, 0x6d, 0x04, 0x11, 0xb1, 0x6d, 0xbc];

/// Through `lp_supply`. The shortest `Pool` that exists on the chain.
pub const BASE_LEN: usize = 211;
/// Through `coin_creator`.
pub const WITH_COIN_CREATOR_LEN: usize = 243;
/// Through `is_mayhem_mode`.
pub const WITH_MAYHEM_LEN: usize = 244;
/// Through `is_cashback_coin`.
pub const WITH_CASHBACK_LEN: usize = 245;
/// Through `virtual_quote_reserves`. Every field the layout knows about.
pub const FULL_LEN: usize = 261;

/// Reads a `bool` the way Borsh does: one, zero, or a refusal.
fn flag(byte: u8, field: &'static str) -> Result<bool, Malformed> {
    match byte {
        0 => Ok(false),
        1 => Ok(true),
        found => Err(Malformed::NotABool { field, found }),
    }
}

impl Pool {
    /// Reads a pool out of raw account data.
    ///
    /// # Errors
    ///
    /// [`Malformed`], and never a partial struct. Shorter than [`BASE_LEN`] is
    /// [`TooShort`](Malformed::TooShort); a foreign account is
    /// [`WrongDiscriminator`](Malformed::WrongDiscriminator); a length that stops
    /// inside a field is [`PartialField`](Malformed::PartialField); a non-zero
    /// byte past [`FULL_LEN`] is
    /// [`UnknownTrailingData`](Malformed::UnknownTrailingData); and a flag byte
    /// that is not zero or one is [`NotABool`](Malformed::NotABool).
    ///
    /// # Panics
    ///
    /// Cannot. Every slice below is inside a length this function has already
    /// established, and the `expect`s convert a slice of proven length into an
    /// array, which has no error worth propagating.
    pub fn parse(data: &[u8]) -> Result<Self, Malformed> {
        if data.len() < BASE_LEN {
            return Err(Malformed::TooShort {
                len: data.len(),
                needed: BASE_LEN,
            });
        }
        let found: [u8; 8] = data[..8].try_into().expect("checked above");
        if found != DISCRIMINATOR {
            return Err(Malformed::WrongDiscriminator { found });
        }

        let address = |at: usize| -> Address {
            Address::new(data[at..at + 32].try_into().expect("checked above"))
        };

        let base = Self {
            pool_bump: data[8],
            index: u16::from_le_bytes(data[9..11].try_into().expect("checked above")),
            creator: address(11),
            base_mint: address(43),
            quote_mint: address(75),
            lp_mint: address(107),
            pool_base_token_account: address(139),
            pool_quote_token_account: address(171),
            lp_supply: u64::from_le_bytes(data[203..211].try_into().expect("checked above")),
            coin_creator: None,
            is_mayhem_mode: None,
            is_cashback_coin: None,
            virtual_quote_reserves: None,
        };
        if data.len() == BASE_LEN {
            return Ok(base);
        }
        let partial = |field: &'static str, needed: usize| Malformed::PartialField {
            len: data.len(),
            field,
            needed,
        };
        if data.len() < WITH_COIN_CREATOR_LEN {
            return Err(partial("coin_creator", WITH_COIN_CREATOR_LEN));
        }

        let with_creator = Self {
            coin_creator: Some(address(211)),
            ..base
        };
        if data.len() == WITH_COIN_CREATOR_LEN {
            return Ok(with_creator);
        }

        let with_mayhem = Self {
            is_mayhem_mode: Some(flag(data[243], "is_mayhem_mode")?),
            ..with_creator
        };
        if data.len() == WITH_MAYHEM_LEN {
            return Ok(with_mayhem);
        }

        let with_cashback = Self {
            is_cashback_coin: Some(flag(data[244], "is_cashback_coin")?),
            ..with_mayhem
        };
        if data.len() == WITH_CASHBACK_LEN {
            return Ok(with_cashback);
        }
        if data.len() < FULL_LEN {
            return Err(partial("virtual_quote_reserves", FULL_LEN));
        }

        if let Some((offset, byte)) = data[FULL_LEN..]
            .iter()
            .enumerate()
            .find(|(_, byte)| **byte != 0)
        {
            return Err(Malformed::UnknownTrailingData {
                at: FULL_LEN + offset,
                found: *byte,
            });
        }

        Ok(Self {
            virtual_quote_reserves: Some(i128::from_le_bytes(
                data[245..FULL_LEN].try_into().expect("checked above"),
            )),
            ..with_cashback
        })
    }
}
