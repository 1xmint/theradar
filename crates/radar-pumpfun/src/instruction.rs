// SPDX-License-Identifier: Apache-2.0
//! The instruction data a pump.fun trade carries.
//!
//! # What is observed and what is inferred, kept apart
//!
//! The **encoding** is observed: every byte layout below is reproduced from a
//! real mainnet instruction, and the tests assert the exact bytes rather than a
//! round trip. A round trip would pass just as well against a layout the program
//! has never seen.
//!
//! The **names** are inferred, from the argument values and the accounts the
//! instruction carries — a `track_volume` flag beside a
//! `user_volume_accumulator` account is a reasonable reading, and it is a
//! reading. Where a name is a guess this file says so, because ADR 0009's
//! standard is that what comes from mainnet and what comes from inference are
//! not the same thing.
//!
//! # The discriminator comes from the decoder
//!
//! ADR 0009 precondition 2. A builder that learned its own discriminators could
//! come to disagree with `radar-decode` about the same program, and the
//! disagreement would be invisible until a transaction was rejected. So the
//! bytes are looked up in `radar_decode::pumpfun::KNOWN` by the instruction's
//! own name, and a lookup that fails is a refusal rather than a fallback.

use radar_decode::pumpfun::{Instruction, KNOWN};
use radar_types::Address;

use crate::transaction::AccountMeta;

/// A trade against the bonding curve.
///
/// Three variants because mainnet carries three, and they are not
/// interchangeable: the first fixes what you *receive*, the second what you
/// *spend*, and only the third exits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trade {
    /// Buy an exact number of tokens, spending no more than `max_sol_cost`.
    ///
    /// The ceiling is the slippage bound: the program refuses rather than
    /// filling above it.
    Buy {
        /// Tokens to receive, in base units.
        amount_tokens: u64,
        /// The most lamports that may be spent.
        max_sol_cost: u64,
        /// The trailing flag. **Name inferred**, encoding observed — it sits
        /// beside the volume-accumulator accounts and was `1` on the captured
        /// buy and `0` on the captured `buy_exact_sol_in`.
        track_volume: bool,
    },
    /// Spend an exact number of lamports, receiving whatever that buys.
    ///
    /// The common instruction on this program, and the one that matches how
    /// Radar sizes: it decides a notional, not a token count.
    BuyExactSolIn {
        /// Lamports to spend.
        lamports: u64,
        /// Minimum tokens received, in token base units, not basis points.
        /// The earlier `slippage_bps` name was an incorrect inference from a
        /// capture carrying 500. Use [`Trade::exact_sol_buy`] to apply a
        /// basis-point tolerance to a separately verified token quote.
        min_tokens_out: u64,
        /// See [`Trade::Buy::track_volume`].
        track_volume: bool,
    },
    /// Sell tokens for at least `min_sol_output`.
    ///
    /// The exit, and the half Radar's whole thesis rests on.
    Sell {
        /// Tokens to sell, in base units.
        amount_tokens: u64,
        /// The fewest lamports that may be received.
        ///
        /// Zero on the captured instruction, which is a trader accepting any
        /// price. Radar should not: a floor of zero is a sell with no slippage
        /// bound at all.
        min_sol_output: u64,
    },
}

impl Trade {
    /// Builds an exact-SOL buy with a minimum token output from a quote.
    ///
    /// The quote must already account for venue fees; its provenance and
    /// freshness are the caller's responsibility. This pure calculation does
    /// not obtain or authenticate a quote. The floor rounds down in raw token
    /// units. Zero spend, a zero resulting floor, or tolerance of 100% or more
    /// returns `None`; such a buy would have no meaningful output protection.
    #[must_use]
    pub fn exact_sol_buy(
        lamports: u64,
        quoted_tokens: u64,
        slippage_bps: u16,
        track_volume: bool,
    ) -> Option<Self> {
        let retained_bps = 10_000_u16.checked_sub(slippage_bps)?;
        let minimum = u128::from(quoted_tokens) * u128::from(retained_bps) / 10_000;
        let min_tokens_out = u64::try_from(minimum).ok()?;
        if lamports == 0 || min_tokens_out == 0 {
            return None;
        }
        Some(Self::BuyExactSolIn {
            lamports,
            min_tokens_out,
            track_volume,
        })
    }

    /// Which decoded instruction this builds.
    #[must_use]
    pub const fn instruction(&self) -> Instruction {
        match self {
            Self::Buy { .. } => Instruction::Buy,
            Self::BuyExactSolIn { .. } => Instruction::BuyExactSolIn,
            Self::Sell { .. } => Instruction::Sell,
        }
    }

    /// The eight-byte discriminator, from the decoder's table.
    ///
    /// `None` when the table does not carry it, which cannot happen for these
    /// three and is returned rather than panicked because a builder that
    /// panics on a table change is worse than one that refuses.
    #[must_use]
    pub fn discriminator(&self) -> Option<[u8; 8]> {
        let wanted = self.instruction();
        KNOWN
            .iter()
            .find(|(ix, _, _)| *ix == wanted)
            .map(|(_, bytes, _)| *bytes)
    }

    /// The full instruction data: discriminator followed by arguments.
    ///
    /// Little-endian `u64`s, which is Solana's encoding throughout — and the
    /// tests assert the exact captured bytes, so a big-endian slip fails here
    /// rather than by transferring a wildly different amount.
    #[must_use]
    pub fn data(&self) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(25);
        out.extend_from_slice(&self.discriminator()?);
        match *self {
            Self::Buy {
                amount_tokens,
                max_sol_cost,
                track_volume,
            } => {
                out.extend_from_slice(&amount_tokens.to_le_bytes());
                out.extend_from_slice(&max_sol_cost.to_le_bytes());
                out.push(u8::from(track_volume));
            }
            Self::BuyExactSolIn {
                lamports,
                min_tokens_out,
                track_volume,
            } => {
                out.extend_from_slice(&lamports.to_le_bytes());
                out.extend_from_slice(&min_tokens_out.to_le_bytes());
                out.push(u8::from(track_volume));
            }
            Self::Sell {
                amount_tokens,
                min_sol_output,
            } => {
                out.extend_from_slice(&amount_tokens.to_le_bytes());
                out.extend_from_slice(&min_sol_output.to_le_bytes());
            }
        }
        Some(out)
    }
}

/// The system program: all zeros, `11111111111111111111111111111111`.
pub const SYSTEM_PROGRAM: Address = Address::new([0u8; 32]);

/// `collect_creator_fee`: moves the fees a creator's launches have earned out of
/// the creator vault into the creator's wallet.
///
/// The account order is the program's own, read off its on-chain Anchor IDL
/// (v0.1.0, account `AYgC53tU…`, 2026-09-05): creator, creator vault, system
/// program, event authority, the program itself; no arguments. The IDL marks
/// the creator writable and **not** a signer -- the collection is
/// permissionless and always lands in the creator's wallet -- but the creator
/// signs here because it is the fee payer of a transaction that goes on to
/// transfer what was collected. **An IDL is a reference, not a capture**
/// (LEARNINGS 25; research 0023 found this program's IDL two accounts short
/// on `buy`), so the first mainnet `collect_creator_fee` this crate sends is
/// the capture, and the devnet week design 0007 §6.3 requires is where it is
/// first exercised.
///
/// `None` only when a derivation fails, which for a real creator it cannot.
#[must_use]
pub fn collect_creator_fee(creator: &Address) -> Option<crate::transaction::Instruction> {
    let discriminator = KNOWN
        .iter()
        .find(|(ix, _, _)| *ix == Instruction::CollectCreatorFee)
        .map(|(_, bytes, _)| *bytes)?;
    Some(crate::transaction::Instruction {
        program_id: radar_decode::pumpfun::PROGRAM_ID,
        accounts: vec![
            AccountMeta::signer(*creator),
            AccountMeta::writable(crate::pda::creator_vault(creator)?),
            AccountMeta::readonly(SYSTEM_PROGRAM),
            AccountMeta::readonly(crate::pda::event_authority()?),
            AccountMeta::readonly(radar_decode::pumpfun::PROGRAM_ID),
        ],
        data: discriminator.to_vec(),
    })
}

/// A system-program transfer of `lamports` from `from` to `to`.
///
/// Instruction index 2 as a little-endian `u32`, then the amount as a `u64`:
/// the wire format of `SystemInstruction::Transfer`, written here rather than
/// pulled from an SDK for the reason the rest of this crate is.
#[must_use]
pub fn system_transfer(
    from: &Address,
    to: &Address,
    lamports: u64,
) -> crate::transaction::Instruction {
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&2u32.to_le_bytes());
    data.extend_from_slice(&lamports.to_le_bytes());
    crate::transaction::Instruction {
        program_id: SYSTEM_PROGRAM,
        accounts: vec![AccountMeta::signer(*from), AccountMeta::writable(*to)],
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_creator_fee_names_the_idls_five_accounts_in_order_and_carries_only_its_discriminator()
     {
        // The on-chain IDL's list, 2026-09-05. A reference until a capture
        // replaces it; the test pins that the builder says what the reference
        // says, so a drift between them is a test and not a failed transaction.
        let creator = Address::new([9u8; 32]);
        let ix = collect_creator_fee(&creator).expect("derivations succeed");
        assert_eq!(ix.program_id, radar_decode::pumpfun::PROGRAM_ID);
        assert_eq!(
            ix.data,
            vec![0x14, 0x16, 0x56, 0x7b, 0xc6, 0x1c, 0xdb, 0x84]
        );
        let names: Vec<(Address, bool, bool)> = ix
            .accounts
            .iter()
            .map(|a| (a.pubkey, a.signer, a.writable))
            .collect();
        assert_eq!(
            names,
            vec![
                (creator, true, true),
                (
                    crate::pda::creator_vault(&creator).expect("vault"),
                    false,
                    true
                ),
                (SYSTEM_PROGRAM, false, false),
                (
                    crate::pda::event_authority().expect("authority"),
                    false,
                    false
                ),
                (radar_decode::pumpfun::PROGRAM_ID, false, false),
            ]
        );
        assert_eq!(
            SYSTEM_PROGRAM.to_string(),
            "11111111111111111111111111111111"
        );
    }

    #[test]
    fn a_system_transfer_is_index_two_then_the_amount_from_a_signer_to_a_writable() {
        let (from, to) = (Address::new([1u8; 32]), Address::new([2u8; 32]));
        let ix = system_transfer(&from, &to, 1_234_567);
        assert_eq!(ix.program_id, SYSTEM_PROGRAM);
        assert_eq!(&ix.data[..4], &[2, 0, 0, 0]);
        assert_eq!(&ix.data[4..], &1_234_567u64.to_le_bytes());
        assert_eq!(ix.data.len(), 12);
        assert!(ix.accounts[0].signer && ix.accounts[0].pubkey == from);
        assert!(ix.accounts[1].writable && !ix.accounts[1].signer && ix.accounts[1].pubkey == to);
    }

    /// Instruction data lifted verbatim from mainnet transactions on
    /// 2026-09-01, alongside the account layouts in
    /// `radar-decode/tests/fixtures/pumpfun_accounts.json`.
    const OBSERVED_BUY: &str = "66063d1201daebea68461a7418000000407c43000000000001";
    const OBSERVED_BUY_EXACT: &str = "38fc74089edfcd5f00c39dd000000000f40100000000000000";
    const OBSERVED_SELL: &str = "33e685a4017f83ad7bd4d7c31e0000000000000000000000";

    fn hex(bytes: &[u8]) -> String {
        use core::fmt::Write as _;
        bytes.iter().fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
    }

    #[test]
    fn a_buy_encodes_to_the_bytes_mainnet_carried() {
        // Byte-for-byte against a real instruction, not a round trip. A round
        // trip passes just as well against a layout the program has never seen.
        let built = Trade::Buy {
            amount_tokens: 105_027_094_120,
            max_sol_cost: 4_422_720,
            track_volume: true,
        }
        .data()
        .expect("a discriminator");
        assert_eq!(hex(&built), OBSERVED_BUY);
    }

    #[test]
    fn a_buy_of_an_exact_sol_amount_encodes_to_the_bytes_mainnet_carried() {
        // This older packet was in a failed transaction before the trade ran.
        // It pins observed bytes, not acceptance or the argument's meaning.
        let built = Trade::BuyExactSolIn {
            lamports: 3_500_000_000,
            min_tokens_out: 500,
            track_volume: false,
        }
        .data()
        .expect("a discriminator");
        assert_eq!(hex(&built), OBSERVED_BUY_EXACT);
    }

    #[test]
    fn captured_exact_sol_buy_arguments_are_spend_and_token_floor() {
        let captures: serde_json::Value = serde_json::from_str(include_str!(
            "../../radar-signer/tests/fixtures/pumpfun_trade_roles.json"
        ))
        .unwrap();
        let row = captures["trades"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == "buy_exact_sol_in")
            .unwrap();
        assert!(row["execution_error"].is_null());
        let captured = radar_types::b64::decode(row["data_base64"].as_str().unwrap()).unwrap();
        let built = Trade::BuyExactSolIn {
            lamports: 654_301_622,
            min_tokens_out: 2_202_113_837_114,
            track_volume: false,
        }
        .data()
        .unwrap();
        // This accepted capture omits trailing flags. Only its discriminator
        // and two quantity arguments are an execution-backed layout anchor.
        assert_eq!(captured.len(), 24);
        assert_eq!(&built[..24], captured);
    }

    #[test]
    fn exact_sol_buy_converts_tolerance_to_raw_tokens_and_refuses_unbounded_output() {
        for (spend, quote, bps, expected) in [
            (10, 1_000_000, 500, Some(950_000)),
            (10, 101, 1, Some(100)),
            (10, u64::MAX, 0, Some(u64::MAX)),
            (10, 10_000, 9_999, Some(1)),
            (0, 1_000_000, 500, None),
            (10, 0, 500, None),
            (10, 1, 1, None),
            (10, 10_000, 10_000, None),
            (10, 10_000, u16::MAX, None),
        ] {
            let actual = Trade::exact_sol_buy(spend, quote, bps, true);
            assert_eq!(
                actual,
                expected.map(|min_tokens_out| Trade::BuyExactSolIn {
                    lamports: spend,
                    min_tokens_out,
                    track_volume: true,
                })
            );
        }
        let trade = Trade::exact_sol_buy(10, 1_000, 500, false).unwrap();
        assert_eq!(
            trade,
            Trade::BuyExactSolIn {
                lamports: 10,
                min_tokens_out: 950,
                track_volume: false,
            }
        );
        assert_eq!(&trade.data().unwrap()[16..24], &950_u64.to_le_bytes());
    }

    #[test]
    fn a_sell_encodes_to_the_bytes_mainnet_carried() {
        // The exit, and one argument shorter than a buy -- no trailing flag.
        // Assuming symmetry would append a byte the program does not expect.
        let built = Trade::Sell {
            amount_tokens: 132_134_720_635,
            min_sol_output: 0,
        }
        .data()
        .expect("a discriminator");
        assert_eq!(hex(&built), OBSERVED_SELL);
        assert_eq!(built.len(), 24, "sell is 8 + 8 + 8, with no trailing flag");
    }

    #[test]
    fn the_discriminators_come_from_the_decoders_table() {
        // ADR 0009 precondition 2. If this crate held its own copy, the two
        // could drift and nothing would notice until a transaction was
        // rejected.
        for (trade, name) in [
            (
                Trade::Buy {
                    amount_tokens: 1,
                    max_sol_cost: 1,
                    track_volume: false,
                },
                "buy",
            ),
            (
                Trade::BuyExactSolIn {
                    lamports: 1,
                    min_tokens_out: 1,
                    track_volume: false,
                },
                "buy_exact_sol_in",
            ),
            (
                Trade::Sell {
                    amount_tokens: 1,
                    min_sol_output: 1,
                },
                "sell",
            ),
        ] {
            let disc = trade.discriminator().expect("in the table");
            let (_, table_bytes, table_name) = KNOWN
                .iter()
                .find(|(_, d, _)| *d == disc)
                .expect("the table knows it");
            assert_eq!(*table_name, name);
            assert_eq!(*table_bytes, disc);
        }
    }

    #[test]
    fn the_numbers_are_little_endian() {
        // Solana's encoding throughout. A big-endian slip would still produce
        // 24 well-formed bytes and would transfer a wildly different amount.
        let built = Trade::Sell {
            amount_tokens: 1,
            min_sol_output: 0,
        }
        .data()
        .expect("data");
        assert_eq!(&built[8..16], &[1, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn the_two_buy_variants_are_not_interchangeable() {
        // One fixes what you receive and the other what you spend. They carry
        // the same argument *shape*, so a mixed-up call site produces valid
        // bytes for the wrong instruction -- which is why the discriminators
        // are asserted to differ.
        let a = Trade::Buy {
            amount_tokens: 100,
            max_sol_cost: 200,
            track_volume: true,
        };
        let b = Trade::BuyExactSolIn {
            lamports: 100,
            min_tokens_out: 200,
            track_volume: true,
        };
        assert_ne!(a.discriminator(), b.discriminator());
        assert_eq!(
            a.data().expect("a")[8..],
            b.data().expect("b")[8..],
            "the arguments really are the same shape, which is the hazard"
        );
    }

    #[test]
    fn a_zero_floor_on_a_sell_is_representable_and_should_not_be_used() {
        // The captured sell had `min_sol_output: 0` -- a trader accepting any
        // price. This type can express that because mainnet does, and the field
        // documentation says why Radar must not: a floor of zero is a sell with
        // no slippage bound at all.
        let data = Trade::Sell {
            amount_tokens: 1,
            min_sol_output: 0,
        }
        .data()
        .expect("data");
        assert_eq!(&data[16..24], &[0u8; 8]);
    }
}
