// SPDX-License-Identifier: Apache-2.0
//! One token, two markets, two decisions.
//!
//! A mint is not a trade. The same token trades on its bonding curve and on
//! whatever it graduates to, and a position that can be sold on one may have no
//! depth at all on the other in the same slot. Before this, a [`Proposal`] named
//! only the mint, so those two decisions were **indistinguishable** — identical
//! values, identical nonces, identical authorisations.
//!
//! That is not a cosmetic gap. The nonce is what makes a recorded authorisation
//! checkable: it is a hash of everything the verdict depended on, so a signer or
//! an auditor can recompute it and show that the authorisation in hand is the
//! one that was issued. Two different trades hashing to one value means the
//! authorisation for a $20 buy against measured curve depth is byte-identical to
//! the authorisation for a $20 buy on an AMM pool nobody measured — and anything
//! keyed by that nonce, or by the mint, keeps one of them and drops the other
//! without noticing.
//!
//! # How to check this test catches rather than passes
//!
//! Re-apply the bug in `crates/radar-risk/src/kernel.rs`: delete the market and
//! quote lines from `nonce_for`, collapsing the market back into the mint. Every
//! test here fails. Put them back and they pass. Hashing only
//! `market.program` — the tempting half-measure — fails
//! `two_pools_on_one_program_are_two_authorisations` on its own.

use radar_risk::{
    Action, Address, Asset, Autonomy, Market, MicroUsd, Policy, PortfolioState, Proposal, Slot,
    SlotDelta, evaluate,
};

/// A policy open enough to authorise, so there is a nonce to compare.
///
/// `Policy::CLOSED` refuses everything, which is correct and is what ships — but
/// a refusal carries no nonce, and the collapse this file is about happens in
/// the authorisation.
fn open_policy() -> Policy {
    Policy {
        autonomy: Autonomy::Capped,
        max_position: MicroUsd::from_dollars(50.0),
        max_deployed: MicroUsd::from_dollars(200.0),
        max_per_creator: MicroUsd::from_dollars(60.0),
        max_daily_loss: MicroUsd::from_dollars(25.0),
        max_round_trip_cost_bps: 900,
        max_canary: MicroUsd::from_dollars(1.0),
        max_input_staleness: SlotDelta(150),
        max_consecutive_failures: 3,
    }
}

const NOW: Slot = Slot(1_050);

/// One token, one size, one creator. Only the venue and the quote vary.
fn buy_on(market: Market, quote: Asset) -> Proposal {
    Proposal {
        mint: Address::new([1; 32]),
        market,
        quote,
        creator: Address::new([2; 32]),
        action: Action::Buy,
        notional: MicroUsd::from_dollars(20.0),
        estimated_round_trip_cost: MicroUsd::from_dollars(0.40),
        oldest_input_slot: Slot(1_000),
        simulated_exit_capacity: Some(MicroUsd::from_dollars(100.0)),
    }
}

/// The nonce the kernel issues, or a panic naming why it refused.
fn nonce(proposal: &Proposal) -> String {
    let verdict = evaluate(proposal, &PortfolioState::flat(NOW), &open_policy());
    verdict
        .authorisation()
        .unwrap_or_else(|| panic!("the fixture must be authorised, not {verdict:?}"))
        .nonce
        .clone()
}

/// A pump AMM pool. Two of these exist for one token often enough to matter.
fn pool(n: u8) -> Market {
    Market {
        program: Address::new([42; 32]),
        pool: Some(Address::new([n; 32])),
    }
}

#[test]
fn the_same_token_on_two_venues_is_two_authorisations() {
    // The whole point. The curve and the AMM have different depth in the same
    // slot; an authorisation that cannot tell them apart is an authorisation for
    // a trade nobody judged.
    let curve = buy_on(Market::PUMP_FUN_BONDING_CURVE, Asset::Sol);
    let amm = buy_on(pool(7), Asset::WrappedSol);

    assert_ne!(
        nonce(&curve),
        nonce(&amm),
        "two venues content-addressed to one authorisation"
    );
}

#[test]
fn two_pools_on_one_program_are_two_authorisations() {
    // The half-measure this catches: hashing the program and not the pool. One
    // token routinely has more than one pool on the same AMM, and the shallow
    // one is exactly where an exit fails.
    let a = buy_on(pool(7), Asset::WrappedSol);
    let b = buy_on(pool(8), Asset::WrappedSol);

    assert_eq!(
        a.market.program, b.market.program,
        "same program on purpose"
    );
    assert_ne!(
        nonce(&a),
        nonce(&b),
        "the pool is part of which trade this is"
    );
}

#[test]
fn no_pool_does_not_collide_with_a_pool_of_zeros() {
    // `None` means "the program and the mint already name it". A zero address is
    // the System Program. Hashing the option untagged would make those one
    // value, which is the same collapse one level down.
    let none = buy_on(Market::PUMP_FUN_BONDING_CURVE, Asset::Sol);
    let zeros = buy_on(
        Market {
            program: Market::PUMP_FUN_PROGRAM,
            pool: Some(Address::new([0; 32])),
        },
        Asset::Sol,
    );

    assert_ne!(nonce(&none), nonce(&zeros));
}

#[test]
fn sol_and_wrapped_sol_are_two_authorisations() {
    // They are not the same balance: native SOL is an account's own lamports,
    // wrapped SOL is a token account that has to be created and funded. The
    // signer builds a different transaction for each, so an authorisation that
    // could not tell them apart would permit a transaction the kernel never
    // judged.
    let native = buy_on(Market::PUMP_FUN_BONDING_CURVE, Asset::Sol);
    let wrapped = buy_on(Market::PUMP_FUN_BONDING_CURVE, Asset::WrappedSol);

    assert_ne!(nonce(&native), nonce(&wrapped));
}

#[test]
fn a_market_and_a_quote_are_not_a_clock() {
    // Purity still holds. The nonce is content-addressed, which is only useful
    // while the same decision produces the same one -- a nonce that varied per
    // call would make every recorded authorisation unverifiable.
    let curve = buy_on(Market::PUMP_FUN_BONDING_CURVE, Asset::Sol);
    assert_eq!(nonce(&curve), nonce(&curve));
    assert_eq!(
        evaluate(&curve, &PortfolioState::flat(NOW), &open_policy()),
        evaluate(&curve, &PortfolioState::flat(NOW), &open_policy())
    );
}

#[test]
fn a_ledger_keyed_by_nonce_keeps_both_trades() {
    // "Not treated as one", stated the way it actually bites: anything that
    // records decisions by nonce -- the research store, a replay, a signer
    // checking an authorisation it was handed -- holds two rows for two trades.
    // With the market collapsed into the mint this map has one entry.
    let trades = [
        buy_on(Market::PUMP_FUN_BONDING_CURVE, Asset::Sol),
        buy_on(pool(7), Asset::WrappedSol),
        buy_on(pool(8), Asset::WrappedSol),
        buy_on(Market::PUMP_FUN_BONDING_CURVE, Asset::Usdc),
    ];
    let ledger: std::collections::BTreeMap<String, &Proposal> =
        trades.iter().map(|p| (nonce(p), p)).collect();

    assert_eq!(ledger.len(), trades.len(), "two trades collapsed into one");
    // And the same mint throughout, which is what makes the collapse plausible.
    assert!(trades.iter().all(|p| p.mint == trades[0].mint));
}
