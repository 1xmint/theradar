// SPDX-License-Identifier: Apache-2.0
//! Seven discriminators belong to both programs, and the program decides which
//! instruction they are.
//!
//! Anchor derives a discriminator from `sha256("global:" + name)[..8]`. The
//! program id is not in that hash, so any two Anchor programs that share an
//! instruction *name* share its bytes exactly. pump.fun and PumpSwap share
//! seven: `buy`, `sell`, `claim_cashback`, `extend_account` and the three
//! volume-accumulator instructions.
//!
//! Before [`Program`] existed this was held up by every caller filtering on the
//! program id by hand before calling a `decode_pumpfun` that took bytes alone.
//! That is discipline, not a guarantee, and the cost of forgetting was a
//! PumpSwap trade reported as a bonding-curve trade — a different venue,
//! different reserves, different fee schedule, and nothing anywhere saying so.
//!
//! What this file catches: a lookup that ignores the program it was given, or
//! one that searches both tables and returns the first hit.

use radar_decode::{Decoded, Instruction, Program, decode, pumpfun, pumpswap};
use sha2::{Digest, Sha256};

/// The discriminators that appear in both tables, with the name each carries.
fn shared() -> Vec<(&'static str, [u8; 8])> {
    let mut rows: Vec<(&'static str, [u8; 8])> = pumpfun::KNOWN
        .iter()
        .filter_map(|(_, bytes, name)| {
            pumpswap::KNOWN
                .iter()
                .any(|(_, other, _)| other == bytes)
                .then_some((*name, *bytes))
        })
        .collect();
    rows.sort_unstable();
    rows
}

/// Instruction data: a discriminator plus a plausible two-`u64` trade payload.
fn payload(discriminator: [u8; 8]) -> Vec<u8> {
    let mut data = discriminator.to_vec();
    data.extend_from_slice(&1_000_000u64.to_le_bytes());
    data.extend_from_slice(&2_000_000u64.to_le_bytes());
    data
}

#[test]
fn the_two_programs_share_exactly_these_seven_discriminators() {
    // Pinned as a set rather than a count. A future table edit that widens or
    // narrows the overlap is a fact worth being told about: it changes how much
    // traffic the program argument is load-bearing for.
    let names: Vec<&str> = shared().into_iter().map(|(name, _)| name).collect();
    assert_eq!(
        names,
        vec![
            "buy",
            "claim_cashback",
            "close_user_volume_accumulator",
            "extend_account",
            "init_user_volume_accumulator",
            "sell",
            "sync_user_volume_accumulator",
        ],
    );
}

#[test]
fn every_shared_discriminator_decodes_to_the_program_it_was_given() {
    for (name, bytes) in shared() {
        let data = payload(bytes);

        let Decoded::Known(curve) = decode(Program::PumpFun, &data) else {
            panic!("{name} is in the pump.fun table and must decode under it");
        };
        let Decoded::Known(amm) = decode(Program::PumpSwap, &data) else {
            panic!("{name} is in the PumpSwap table and must decode under it");
        };

        assert_eq!(
            curve.program(),
            Program::PumpFun,
            "{name} decoded under pump.fun but reports {:?}",
            curve.program()
        );
        assert_eq!(
            amm.program(),
            Program::PumpSwap,
            "{name} decoded under PumpSwap but reports {:?}",
            amm.program()
        );
        assert_ne!(
            curve, amm,
            "{name} produced the same instruction on both programs, so the \
             program argument did nothing"
        );
        assert!(
            matches!(curve, Instruction::PumpFun(_)),
            "{name} under pump.fun is {curve:?}"
        );
        assert!(
            matches!(amm, Instruction::PumpSwap(_)),
            "{name} under PumpSwap is {amm:?}"
        );
        // The accessors a caller reaches for are the place the misread would
        // actually happen, so they are asserted in both directions: each answers
        // for its own venue and refuses for the other.
        assert!(
            curve.pumpfun().is_some() && curve.pumpswap().is_none(),
            "{name} under pump.fun answers as {curve:?} but not as a curve \
             instruction"
        );
        assert!(
            amm.pumpswap().is_some() && amm.pumpfun().is_none(),
            "{name} under PumpSwap answers as {amm:?} but not as an AMM \
             instruction"
        );
        assert_eq!(
            curve.anchor_name(),
            name,
            "the curve's instruction lost its name"
        );
        assert_eq!(
            amm.anchor_name(),
            name,
            "the AMM's instruction lost its name"
        );
    }
}

#[test]
fn a_pumpswap_buy_is_never_reported_as_a_bonding_curve_buy() {
    // The specific misread this design exists to prevent, spelled out: these
    // eight bytes are a real PumpSwap buy against a real AMM pool. Read as a
    // curve buy they would be priced against reserves the token no longer has.
    let data = payload(*pumpswap::Instruction::Buy.discriminator().as_bytes());

    let Decoded::Known(ix) = decode(Program::PumpSwap, &data) else {
        panic!("a PumpSwap buy must decode under PumpSwap");
    };
    assert_eq!(ix, Instruction::PumpSwap(pumpswap::Instruction::Buy));
    assert_eq!(ix.pumpfun(), None, "an AMM buy is not a curve instruction");
    assert_eq!(ix.pumpswap(), Some(pumpswap::Instruction::Buy));
    assert!(ix.pumpswap().is_some_and(pumpswap::Instruction::is_buy));
    assert_eq!(ix.anchor_name(), "buy");
    assert_eq!(ix.program(), Program::PumpSwap);

    // The same bytes under the curve are a curve buy, and answer only as one.
    let Decoded::Known(curve) = decode(Program::PumpFun, &data) else {
        panic!("the curve has these bytes too");
    };
    assert_eq!(curve.pumpfun(), Some(pumpfun::Instruction::Buy));
    assert_eq!(curve.pumpswap(), None);
}

#[test]
fn an_instruction_only_one_program_has_is_unknown_on_the_other() {
    // Catches a lookup that searches both tables. `create_v2` launches a token
    // on the curve and has no PumpSwap counterpart; `create_pool` opens an AMM
    // pool and has no curve counterpart.
    let launch = payload(*pumpfun::Instruction::CreateV2.discriminator().as_bytes());
    let pool = payload(*pumpswap::Instruction::CreatePool.discriminator().as_bytes());

    assert!(
        decode(Program::PumpSwap, &launch).is_unrecognised(),
        "the curve's create_v2 is not a PumpSwap instruction"
    );
    assert!(
        decode(Program::PumpFun, &pool).is_unrecognised(),
        "PumpSwap's create_pool is not a curve instruction"
    );
    // And each is still known on its own program, so the assertions above are
    // about the program and not about an unparseable payload. Asserted as *not*
    // unrecognised as well as known, because those are the two halves of the
    // alarm: a recognised instruction must not be counted toward the unknown
    // rate that says a program has been upgraded under us.
    assert!(decode(Program::PumpFun, &launch).known().is_some());
    assert!(!decode(Program::PumpFun, &launch).is_unrecognised());
    assert!(decode(Program::PumpSwap, &pool).known().is_some());
    assert!(!decode(Program::PumpSwap, &pool).is_unrecognised());
}

#[test]
fn the_table_follows_anchors_naming_rule() {
    // Every PumpSwap row must be `sha256("global:" + name)[..8]`. The bytes were
    // derived from the vendor's name list rather than captured from mainnet, so
    // this is what stands between the table and a hand-edited byte. The pump.fun
    // table has the same check in `fixtures_match_mainnet.rs`, against captures.
    for (ix, bytes, name) in pumpswap::KNOWN {
        let mut hasher = Sha256::new();
        hasher.update(format!("global:{name}").as_bytes());
        let digest = hasher.finalize();
        assert_eq!(
            &digest[..8],
            bytes.as_slice(),
            "{ix:?} ({name}) does not follow Anchor's rule"
        );
    }
}
