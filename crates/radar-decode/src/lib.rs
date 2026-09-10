// SPDX-License-Identifier: Apache-2.0
//! Local decoding of Solana program instructions.
//!
//! This crate is the reason Radar can afford to look at every launch. Buying
//! transactions parsed costs $0.05 each; fetching the block they are in costs
//! $0.001 for all of them. Measured over 45 mainnet blocks that is a **4,637×**
//! difference, so decoding is the step worth owning — see
//! [ADR 0001](https://github.com/hey-vera/radar/blob/main/docs/adr/0001-decode-locally-never-buy-parsed-transactions.md).
//!
//! Two rules follow from owning it, and both exist because the alternative
//! produces confident wrong answers rather than visible failures:
//!
//! 1. **Match on discriminator bytes, never on logged instruction names.** Names
//!    get versioned. pump.fun runs `Buy`, `BuyV2`, `BuyExactSolIn` and
//!    `BuyExactQuoteInV2` concurrently, and a matcher written against one
//!    spelling reports the other three as absent.
//! 2. **An unrecognised discriminator is [`Decoded::Unknown`], never a guess.**
//!    A decoder that has silently stopped understanding a program looks exactly
//!    like a program that has gone quiet, so the unknown rate is a signal and
//!    has to be preserved.
//! 3. **Eight bytes do not name a venue, so decoding takes a [`Program`].**
//!    Anchor hashes the instruction *name* and nothing else, so pump.fun's `buy`
//!    and PumpSwap's `buy` are the **same eight bytes** — as are `sell` and five
//!    more. Neither instruction table can be reached without saying which
//!    program the bytes came from, because the alternative is a PumpSwap trade
//!    priced against a bonding curve with nothing to say so.
//!
//! Every discriminator for [`pumpfun`] was captured from live mainnet traffic
//! (`scripts/probe/capture_fixtures.py`) rather than copied from documentation,
//! and `tests/` asserts the table still matches both the Anchor naming
//! convention and the raw bytes those instructions actually carried.
//! [`pumpswap`]'s table is not a capture and its module documentation says so.

#![forbid(unsafe_code)]

pub mod args;
mod discriminator;
pub mod pumpfun;
pub mod pumpswap;

use radar_types::Address;

pub use args::{Amount, ArgError, Launch, Layout, Side, Trade};
pub use discriminator::Discriminator;

/// A program this crate can decode instructions for.
///
/// This is the venue question, asked once and answered before any bytes are
/// looked at. It exists as a type rather than as a caller convention because
/// the convention held only while there was one program in the crate: seven
/// discriminators are shared between the two that are here now, and a caller
/// that forgot to filter would have got a confident wrong venue rather than an
/// error.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Program {
    /// The pump.fun bonding curve, [`pumpfun::PROGRAM_ID`].
    PumpFun,
    /// The PumpSwap AMM, [`pumpswap::PROGRAM_ID`].
    PumpSwap,
}

impl Program {
    /// The program at this address, if this crate decodes it.
    ///
    /// `None` is rule 9: an instruction from a program nobody has decoded is
    /// unknown, and a caller has to say what it does about that rather than
    /// being handed a venue by default.
    #[must_use]
    pub fn at(address: &Address) -> Option<Self> {
        match address.as_bytes() {
            b if b == pumpfun::PROGRAM_ID.as_bytes() => Some(Self::PumpFun),
            b if b == pumpswap::PROGRAM_ID.as_bytes() => Some(Self::PumpSwap),
            _ => None,
        }
    }

    /// This program's on-chain address.
    #[must_use]
    pub const fn address(self) -> Address {
        match self {
            Self::PumpFun => pumpfun::PROGRAM_ID,
            Self::PumpSwap => pumpswap::PROGRAM_ID,
        }
    }
}

/// A recognised instruction, carrying which program it belongs to.
///
/// The program is part of the value and not something a caller remembers
/// separately, because the seven shared discriminators mean an instruction
/// without its program is ambiguous data.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Instruction {
    /// An instruction on the pump.fun bonding curve.
    PumpFun(pumpfun::Instruction),
    /// An instruction on the PumpSwap AMM.
    PumpSwap(pumpswap::Instruction),
}

impl Instruction {
    /// Which program this instruction belongs to.
    #[must_use]
    pub const fn program(self) -> Program {
        match self {
            Self::PumpFun(_) => Program::PumpFun,
            Self::PumpSwap(_) => Program::PumpSwap,
        }
    }

    /// The bonding-curve instruction, if that is what this is.
    ///
    /// Returning `Option` rather than a bare value is the point: a caller that
    /// wants curve semantics has to acknowledge the case where the bytes came
    /// from the AMM instead.
    #[must_use]
    pub const fn pumpfun(self) -> Option<pumpfun::Instruction> {
        match self {
            Self::PumpFun(ix) => Some(ix),
            Self::PumpSwap(_) => None,
        }
    }

    /// The AMM instruction, if that is what this is.
    #[must_use]
    pub const fn pumpswap(self) -> Option<pumpswap::Instruction> {
        match self {
            Self::PumpSwap(ix) => Some(ix),
            Self::PumpFun(_) => None,
        }
    }

    /// The Anchor instruction name.
    #[must_use]
    pub fn anchor_name(self) -> &'static str {
        match self {
            Self::PumpFun(ix) => ix.anchor_name(),
            Self::PumpSwap(ix) => ix.anchor_name(),
        }
    }

    // There is deliberately no venue-agnostic `is_buy`/`is_sell`/`is_trade`
    // here. A caller that wants to know what an instruction *did* wants it in
    // order to price or size something, and those answers are not transferable:
    // a curve buy moves against a bonding curve, an AMM buy moves against two
    // vaults with their own fee ladder. Ask through `pumpfun()` or `pumpswap()`,
    // which makes the caller say which arithmetic it is about to apply.
}

/// The result of decoding an instruction.
///
/// The `Unknown` arm is load-bearing. Radar records unknown discriminators and
/// alarms when their rate rises, because that is what a program upgrade looks
/// like from the outside — and pump.fun ships them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decoded<T> {
    /// Recognised.
    Known(T),
    /// Not in the table. Carried rather than discarded so the rate can be
    /// measured and the bytes chased down.
    Unknown {
        /// The eight bytes that were not recognised.
        discriminator: Discriminator,
        /// Total length of the instruction data, which narrows down what it is.
        data_len: usize,
    },
    /// Fewer than eight bytes, so not an Anchor instruction at all.
    Malformed {
        /// How many bytes were present.
        data_len: usize,
    },
}

impl<T> Decoded<T> {
    /// The instruction if it was recognised.
    pub const fn known(&self) -> Option<&T> {
        match self {
            Self::Known(t) => Some(t),
            _ => None,
        }
    }

    /// Whether this decode failed to recognise the instruction, for either
    /// reason. Both count toward the unknown rate that gates the alarm.
    pub const fn is_unrecognised(&self) -> bool {
        !matches!(self, Self::Known(_))
    }
}

/// Decodes an instruction, given the program it was addressed to.
///
/// The [`Program`] is not a convenience. pump.fun's `buy` and PumpSwap's `buy`
/// carry byte-identical discriminators — as do `sell`, `claim_cashback`,
/// `extend_account` and the three volume-accumulator instructions — so the same
/// eight bytes are two different instructions on two venues with different
/// reserves and different fees. There is no function here that takes bytes
/// alone, because such a function has to guess.
#[must_use]
pub fn decode(program: Program, data: &[u8]) -> Decoded<Instruction> {
    let Some(d) = Discriminator::from_data(data) else {
        return Decoded::Malformed {
            data_len: data.len(),
        };
    };
    let found = match program {
        Program::PumpFun => pumpfun::Instruction::from_discriminator(d).map(Instruction::PumpFun),
        Program::PumpSwap => {
            pumpswap::Instruction::from_discriminator(d).map(Instruction::PumpSwap)
        }
    };
    found.map_or(
        Decoded::Unknown {
            discriminator: d,
            data_len: data.len(),
        },
        Decoded::Known,
    )
}

/// Decodes a pump.fun instruction from its data.
///
/// **This is the one entry point left that assumes a venue, and it is a hole.**
/// `radar-backfill`'s CryptoHouse query filters by program in SQL, so its rows
/// carry no program column and its call site has nothing to pass; converting it
/// to [`decode`] is a one-line change (`decode(Program::PumpFun, &data)`) that
/// was outside the file list of the change that added [`Program`]. Every other
/// caller has been converted. **Do not reach for this from new code** — bytes
/// from PumpSwap will decode here as confident bonding-curve instructions.
#[must_use]
pub fn decode_pumpfun(data: &[u8]) -> Decoded<pumpfun::Instruction> {
    match decode(Program::PumpFun, data) {
        Decoded::Known(Instruction::PumpFun(ix)) => Decoded::Known(ix),
        // Unreachable: `decode` returns the arm matching the program it was
        // given. Folded rather than asserted so this shim cannot panic.
        Decoded::Known(_) => Decoded::Malformed {
            data_len: data.len(),
        },
        Decoded::Unknown {
            discriminator,
            data_len,
        } => Decoded::Unknown {
            discriminator,
            data_len,
        },
        Decoded::Malformed { data_len } => Decoded::Malformed { data_len },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_trade_instruction_yields_no_trade() {
        // Bookkeeping instructions carry no amounts. Returning a zeroed Trade
        // would silently add phantom volume to every token that claims cashback.
        let ix = pumpfun::Instruction::ClaimCashback;
        let data = ix.discriminator().as_bytes().to_vec();
        assert!(pumpfun::trade_args(ix, &data).is_none());
        assert!(pumpfun::launch_args(ix, &data).is_none());
    }

    #[test]
    fn a_known_instruction_decodes() {
        let mut data = pumpfun::Instruction::Buy
            .discriminator()
            .as_bytes()
            .to_vec();
        data.extend_from_slice(&[0u8; 17]);
        assert_eq!(
            decode(Program::PumpFun, &data),
            Decoded::Known(Instruction::PumpFun(pumpfun::Instruction::Buy))
        );
    }

    #[test]
    fn an_unknown_discriminator_is_carried_not_guessed() {
        let data = [0xAAu8; 24];
        let d = decode(Program::PumpFun, &data);
        assert!(d.is_unrecognised());
        let Decoded::Unknown {
            discriminator,
            data_len,
        } = d
        else {
            panic!("expected Unknown, got {d:?}")
        };
        assert_eq!(discriminator.to_string(), "aaaaaaaaaaaaaaaa");
        // The length is kept because it is often the fastest way to work out
        // which new instruction a program upgrade added.
        assert_eq!(data_len, 24);
    }

    #[test]
    fn short_data_is_malformed_rather_than_unknown() {
        // Distinct from Unknown: this is not an Anchor instruction at all, and
        // conflating the two would pollute the unknown rate that gates the alarm.
        assert_eq!(
            decode(Program::PumpFun, &[1, 2, 3]),
            Decoded::Malformed { data_len: 3 }
        );
    }

    #[test]
    fn an_address_no_program_claims_is_not_resolved_to_one() {
        // Rule 9 at the venue question: a caller holding an instruction from an
        // unrecognised program has to decide what that means, and cannot be
        // handed a default venue to price it against.
        assert_eq!(Program::at(&Address::new([7u8; 32])), None);
        assert_eq!(
            Program::at(&pumpfun::PROGRAM_ID),
            Some(Program::PumpFun),
            "the curve's own address must resolve to the curve"
        );
        assert_eq!(
            Program::at(&pumpswap::PROGRAM_ID),
            Some(Program::PumpSwap),
            "the AMM's own address must resolve to the AMM"
        );
    }

    #[test]
    fn a_program_round_trips_through_its_address() {
        for program in [Program::PumpFun, Program::PumpSwap] {
            assert_eq!(Program::at(&program.address()), Some(program));
        }
    }

    #[test]
    fn the_shim_left_for_the_backfill_still_reads_the_curve() {
        // `decode_pumpfun` is documented as the one venue-assuming entry point
        // left. While it exists it must agree with `decode`, or the two decoders
        // in this crate would disagree about the same bytes.
        let mut data = pumpfun::Instruction::Sell
            .discriminator()
            .as_bytes()
            .to_vec();
        data.extend_from_slice(&[0u8; 16]);
        assert_eq!(
            decode_pumpfun(&data),
            Decoded::Known(pumpfun::Instruction::Sell)
        );
        assert_eq!(
            decode_pumpfun(&[1, 2, 3]),
            Decoded::Malformed { data_len: 3 }
        );
        assert!(decode_pumpfun(&[0xAAu8; 24]).is_unrecognised());
    }
}
