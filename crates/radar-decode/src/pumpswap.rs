// SPDX-License-Identifier: Apache-2.0
//! The PumpSwap AMM (`pump_amm`), where a pump.fun token goes when it graduates.
//!
//! # This table is a proposal, not a capture
//!
//! Every discriminator in [`pumpfun`](crate::pumpfun) was observed on mainnet.
//! **None of these were.** The names come from the vendor's published IDL
//! (`pump-fun/pump-public-docs`, `idl/pump_amm.json`, fetched 2026-09-09) and the
//! bytes are Anchor's `sha256("global:" + name)[..8]` recomputed from those
//! names, which `the_table_follows_anchors_naming_rule` re-derives rather than
//! trusts. That makes the table internally consistent and leaves it exactly as
//! good as the vendor's name list — which this repository has twice caught being
//! incomplete about this exact program (LEARNINGS 25). A live instruction absent
//! from this table is [`Decoded::Unknown`](crate::Decoded::Unknown), so the rate
//! that gates the alarm stays honest until a capture disposes the list.
//!
//! **The `withdraw` instruction is deliberately absent.** The IDL declares it as
//! `[183, 18, 70, 156, 203, 145, 68, 248]`, and `sha256("global:withdraw")[..8]`
//! is `[183, 18, 70, 156, 148, 109, 161, 34]` — the first four bytes agree and
//! the last four do not, which no hash collision produces and no other candidate
//! name reproduced. The reference disagrees with itself there, so the row is not
//! shipped. Real `withdraw` traffic reports as `Unknown`, which is the true
//! answer.
//!
//! # Why this module exists at all
//!
//! Anchor derives a discriminator from the instruction *name* alone. It does not
//! mix in the program id. So `buy`, `sell`, `claim_cashback`, `extend_account`
//! and the three volume-accumulator instructions carry **byte-identical**
//! discriminators on this program and on the bonding curve — seven of them. The
//! bytes cannot tell you which venue you are looking at, and the venues have
//! different reserves, different fees and different pool state. That is why
//! [`crate::decode`] takes a [`Program`](crate::Program) and there is no way to
//! reach either table with bytes alone.

use radar_types::Address;

use crate::discriminator::Discriminator;

/// The PumpSwap program address, `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`.
///
/// Taken from a mainnet capture this repository already holds, not from
/// documentation: `crates/radar-pumpfun/tests/fixtures/pumpswap_fees.json`
/// records account `ADyA8hdefvWN2dbGGWFotbzWxrAvLW83WG6QCVXvJKqw` at slot
/// 444,505,829 with this address as its `owner`, read via `getAccountInfo` on
/// 2026-09-05. `the_program_address_is_the_one_that_owns_the_captured_account`
/// in `crates/radar-pumpfun/tests/the_fee_after_graduation_is_a_ladder.rs` ties
/// the constant to that capture, because the capture is the evidence.
pub const PROGRAM_ID: Address = Address::new([
    0x0c, 0x14, 0xde, 0xfc, 0x82, 0x5e, 0xc6, 0x76, 0x94, 0x25, 0x08, 0x18, 0xbb, 0x65, 0x40, 0x65,
    0xf4, 0x29, 0x8d, 0x31, 0x56, 0xd5, 0x71, 0xb4, 0xd4, 0xf8, 0x09, 0x0c, 0x18, 0xe9, 0xa8, 0x63,
]);

/// A PumpSwap instruction, identified by discriminator.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Instruction {
    /// `admin_set_coin_creator`
    AdminSetCoinCreator,
    /// `admin_update_token_incentives`
    AdminUpdateTokenIncentives,
    /// `boost_buy_and_burn`
    BoostBuyAndBurn,
    /// `buy` — **shares its eight bytes with the bonding curve's `buy`.**
    Buy,
    /// `buy_exact_quote_in`
    BuyExactQuoteIn,
    /// `claim_cashback` — shares its bytes with the curve's `claim_cashback`.
    ClaimCashback,
    /// `claim_token_incentives`
    ClaimTokenIncentives,
    /// `close_user_volume_accumulator` — shares its bytes with the curve's.
    CloseUserVolumeAccumulator,
    /// `collect_coin_creator_fee`
    CollectCoinCreatorFee,
    /// `create_config`
    CreateConfig,
    /// `create_pool`
    CreatePool,
    /// `deposit`
    Deposit,
    /// `disable`
    Disable,
    /// `extend_account` — shares its bytes with the curve's `extend_account`.
    ExtendAccount,
    /// `init_boost`
    InitBoost,
    /// `init_user_volume_accumulator` — shares its bytes with the curve's.
    InitUserVolumeAccumulator,
    /// `migrate_pool_coin_creator`
    MigratePoolCoinCreator,
    /// `sell` — **shares its eight bytes with the bonding curve's `sell`.**
    Sell,
    /// `set_boost_authority`
    SetBoostAuthority,
    /// `set_coin_creator`
    SetCoinCreator,
    /// `set_reserved_fee_recipients`
    SetReservedFeeRecipients,
    /// `sync_user_volume_accumulator` — shares its bytes with the curve's.
    SyncUserVolumeAccumulator,
    /// `toggle_boost`
    ToggleBoost,
    /// `toggle_cashback_enabled`
    ToggleCashbackEnabled,
    /// `toggle_mayhem_mode`
    ToggleMayhemMode,
    /// `transfer_creator_fees_to_pump`
    TransferCreatorFeesToPump,
    /// `transfer_creator_fees_to_pump_v2`
    TransferCreatorFeesToPumpV2,
}

/// Every known instruction with its discriminator and Anchor name.
///
/// Unlike [`pumpfun::KNOWN`](crate::pumpfun::KNOWN), the bytes here are derived
/// rather than observed — see the module documentation. The name is carried for
/// the same reason: a test recomputes `sha256("global:" + name)[..8]` and fails
/// if a row is ever hand-edited.
pub const KNOWN: &[(Instruction, [u8; 8], &str)] = &[
    (
        Instruction::AdminSetCoinCreator,
        [0xf2, 0x28, 0x75, 0x91, 0x49, 0x60, 0x69, 0x68],
        "admin_set_coin_creator",
    ),
    (
        Instruction::AdminUpdateTokenIncentives,
        [0xd1, 0x0b, 0x73, 0x57, 0xd5, 0x17, 0x7c, 0xcc],
        "admin_update_token_incentives",
    ),
    (
        Instruction::BoostBuyAndBurn,
        [0x69, 0x44, 0x06, 0xaf, 0x00, 0x07, 0x23, 0xa2],
        "boost_buy_and_burn",
    ),
    (
        Instruction::Buy,
        [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea],
        "buy",
    ),
    (
        Instruction::BuyExactQuoteIn,
        [0xc6, 0x2e, 0x15, 0x52, 0xb4, 0xd9, 0xe8, 0x70],
        "buy_exact_quote_in",
    ),
    (
        Instruction::ClaimCashback,
        [0x25, 0x3a, 0x23, 0x7e, 0xbe, 0x35, 0xe4, 0xc5],
        "claim_cashback",
    ),
    (
        Instruction::ClaimTokenIncentives,
        [0x10, 0x04, 0x47, 0x1c, 0xcc, 0x01, 0x28, 0x1b],
        "claim_token_incentives",
    ),
    (
        Instruction::CloseUserVolumeAccumulator,
        [0xf9, 0x45, 0xa4, 0xda, 0x96, 0x67, 0x54, 0x8a],
        "close_user_volume_accumulator",
    ),
    (
        Instruction::CollectCoinCreatorFee,
        [0xa0, 0x39, 0x59, 0x2a, 0xb5, 0x8b, 0x2b, 0x42],
        "collect_coin_creator_fee",
    ),
    (
        Instruction::CreateConfig,
        [0xc9, 0xcf, 0xf3, 0x72, 0x4b, 0x6f, 0x2f, 0xbd],
        "create_config",
    ),
    (
        Instruction::CreatePool,
        [0xe9, 0x92, 0xd1, 0x8e, 0xcf, 0x68, 0x40, 0xbc],
        "create_pool",
    ),
    (
        Instruction::Deposit,
        [0xf2, 0x23, 0xc6, 0x89, 0x52, 0xe1, 0xf2, 0xb6],
        "deposit",
    ),
    (
        Instruction::Disable,
        [0xb9, 0xad, 0xbb, 0x5a, 0xd8, 0x0f, 0xee, 0xe9],
        "disable",
    ),
    (
        Instruction::ExtendAccount,
        [0xea, 0x66, 0xc2, 0xcb, 0x96, 0x48, 0x3e, 0xe5],
        "extend_account",
    ),
    (
        Instruction::InitBoost,
        [0x8c, 0xe9, 0x21, 0x5e, 0x84, 0x5a, 0xc2, 0x8f],
        "init_boost",
    ),
    (
        Instruction::InitUserVolumeAccumulator,
        [0x5e, 0x06, 0xca, 0x73, 0xff, 0x60, 0xe8, 0xb7],
        "init_user_volume_accumulator",
    ),
    (
        Instruction::MigratePoolCoinCreator,
        [0xd0, 0x08, 0x9f, 0x04, 0x4a, 0xaf, 0x10, 0x3a],
        "migrate_pool_coin_creator",
    ),
    (
        Instruction::Sell,
        [0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad],
        "sell",
    ),
    (
        Instruction::SetBoostAuthority,
        [0xe3, 0x95, 0x4c, 0x2a, 0x82, 0x27, 0xea, 0xcd],
        "set_boost_authority",
    ),
    (
        Instruction::SetCoinCreator,
        [0xd2, 0x95, 0x80, 0x2d, 0xbc, 0x3a, 0x4e, 0xaf],
        "set_coin_creator",
    ),
    (
        Instruction::SetReservedFeeRecipients,
        [0x6f, 0xac, 0xa2, 0xe8, 0x72, 0x59, 0xd5, 0x8e],
        "set_reserved_fee_recipients",
    ),
    (
        Instruction::SyncUserVolumeAccumulator,
        [0x56, 0x1f, 0xc0, 0x57, 0xa3, 0x57, 0x4f, 0xee],
        "sync_user_volume_accumulator",
    ),
    (
        Instruction::ToggleBoost,
        [0x75, 0xa1, 0xa0, 0x4a, 0xdf, 0x89, 0x76, 0x63],
        "toggle_boost",
    ),
    (
        Instruction::ToggleCashbackEnabled,
        [0x73, 0x67, 0xe0, 0xff, 0xbd, 0x59, 0x56, 0xc3],
        "toggle_cashback_enabled",
    ),
    (
        Instruction::ToggleMayhemMode,
        [0x01, 0x09, 0x6f, 0xd0, 0x64, 0x1f, 0xff, 0xa3],
        "toggle_mayhem_mode",
    ),
    (
        Instruction::TransferCreatorFeesToPump,
        [0x8b, 0x34, 0x86, 0x55, 0xe4, 0xe5, 0x6c, 0xf1],
        "transfer_creator_fees_to_pump",
    ),
    (
        Instruction::TransferCreatorFeesToPumpV2,
        [0x01, 0x21, 0x4e, 0xb9, 0x21, 0x43, 0x2c, 0x5c],
        "transfer_creator_fees_to_pump_v2",
    ),
];

impl Instruction {
    /// Looks up an instruction by discriminator.
    ///
    /// Private on purpose: reaching this table means having already said the
    /// program is PumpSwap, and [`crate::decode`] is where that is said. The
    /// seven discriminators shared with the bonding curve are the reason.
    pub(crate) fn from_discriminator(d: Discriminator) -> Option<Self> {
        KNOWN
            .iter()
            .find(|(_, bytes, _)| bytes == d.as_bytes())
            .map(|(ix, _, _)| *ix)
    }

    /// This instruction's discriminator.
    ///
    /// # Panics
    ///
    /// If `KNOWN` has no row for this variant. That is a table-integrity bug
    /// rather than a runtime condition, and `lookup_round_trips` fails first.
    #[must_use]
    pub fn discriminator(self) -> Discriminator {
        let (_, bytes, _) = KNOWN
            .iter()
            .find(|(ix, _, _)| *ix == self)
            .expect("KNOWN is exhaustive");
        Discriminator::new(*bytes)
    }

    /// The Anchor instruction name.
    ///
    /// # Panics
    ///
    /// If `KNOWN` has no row for this variant; see [`discriminator`](Self::discriminator).
    #[must_use]
    pub fn anchor_name(self) -> &'static str {
        let (_, _, name) = KNOWN
            .iter()
            .find(|(ix, _, _)| *ix == self)
            .expect("KNOWN is exhaustive");
        name
    }

    /// Whether this instruction acquires the pool's base token, across every
    /// buy variant.
    ///
    /// Ask this rather than comparing against a variant — the same mistake that
    /// made a curve detector blind to three of four buys (LEARNINGS 3).
    #[must_use]
    pub const fn is_buy(self) -> bool {
        matches!(self, Self::Buy | Self::BuyExactQuoteIn)
    }

    /// Whether this instruction disposes of the pool's base token.
    #[must_use]
    pub const fn is_sell(self) -> bool {
        matches!(self, Self::Sell)
    }

    /// Whether this instruction moves a position.
    ///
    /// Liquidity movement (`deposit`, `create_pool`) is deliberately not a
    /// trade: it changes the reserves without anyone taking a side.
    #[must_use]
    pub const fn is_trade(self) -> bool {
        self.is_buy() || self.is_sell()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_way_of_acquiring_the_base_token_counts_as_a_buy() {
        // LEARNINGS 3 in its PumpSwap form. A detector that compares against
        // one variant goes blind to the others, and the others are not rare --
        // `BuyExactQuoteIn` is how a buyer who names a SOL amount arrives,
        // which is most of them.
        assert!(Instruction::Buy.is_buy(), "the plain buy");
        assert!(
            Instruction::BuyExactQuoteIn.is_buy(),
            "quote-in is a buy and skipping it loses most of them"
        );
        assert!(Instruction::Sell.is_sell(), "the plain sell");

        // And the ones that are not trades. Liquidity movement changes the
        // reserves without anyone taking a side, so counting it as a trade
        // would inflate volume with money that never chose a direction.
        assert!(!Instruction::Deposit.is_buy(), "deposit acquires nothing");
        assert!(!Instruction::Deposit.is_sell());
        assert!(!Instruction::Deposit.is_trade(), "deposit is not a trade");
        assert!(!Instruction::CreatePool.is_trade());
        // `Withdraw` is absent from this table on purpose and cannot be named
        // here: the vendor IDL declares bytes for it that Anchor's own naming
        // rule does not produce, so no row was shipped rather than guessing
        // which half of the reference is wrong. It reads as `Unknown` instead.

        // `is_trade` is the union and nothing else. Both halves have to reach
        // it: an `and` here would call nothing a trade at all, and a version
        // fixed at true would call a deposit one.
        assert!(Instruction::Buy.is_trade());
        assert!(Instruction::BuyExactQuoteIn.is_trade());
        assert!(Instruction::Sell.is_trade());
    }

    #[test]
    fn the_program_address_renders_as_the_captured_owner() {
        // Tied to the capture itself by
        // `the_program_address_is_the_one_that_owns_the_captured_account` in
        // `crates/radar-pumpfun/tests/the_fee_after_graduation_is_a_ladder.rs`,
        // which is the crate that holds the fixture. This one only catches a
        // typo in the byte literal above.
        assert_eq!(
            PROGRAM_ID.to_string(),
            "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA"
        );
    }

    #[test]
    fn every_discriminator_is_distinct() {
        let mut seen: Vec<[u8; 8]> = KNOWN.iter().map(|(_, b, _)| *b).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), before, "two instructions share a discriminator");
    }

    #[test]
    fn lookup_round_trips() {
        for (ix, _, _) in KNOWN {
            assert_eq!(
                Instruction::from_discriminator(ix.discriminator()),
                Some(*ix)
            );
        }
    }

    #[test]
    fn an_unrecognised_discriminator_is_not_forced_into_a_variant() {
        assert_eq!(
            Instruction::from_discriminator(Discriminator::new([0; 8])),
            None
        );
    }

    #[test]
    fn the_withdraw_row_the_idl_declares_is_not_in_the_table() {
        // The vendor's IDL declares `withdraw` as these bytes, and Anchor's own
        // rule produces different ones for that name. Shipping the row would be
        // shipping a guess; `Unknown` is the true answer until a capture says
        // otherwise. See the module documentation.
        let declared = Discriminator::new([183, 18, 70, 156, 203, 145, 68, 248]);
        assert_eq!(Instruction::from_discriminator(declared), None);
    }
}
