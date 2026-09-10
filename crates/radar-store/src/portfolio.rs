// SPDX-License-Identifier: Apache-2.0
//! The account, assembled from what the store actually recorded.
//!
//! # What a position row can and cannot say
//!
//! [`Position`] records dollars committed. It does not record **units
//! received**, the **mint's decimals**, or **which token program the mint lives
//! under** — and without the last of those, the same address is two different
//! [`Asset`](radar_types::Asset)s with different transfer semantics.
//!
//! So an open position cannot be turned into a holding. Not "cannot be turned
//! into an accurate holding": there is no asset key to file it under and no
//! quantity to put in it. Anything this function produced for one would be
//! invented.
//!
//! It is written down instead, as
//! [`Portfolio::note_unaccounted`](radar_types::Portfolio::note_unaccounted)
//! with [`Refusal::NotRecorded`]. That is the difference this whole slice is
//! about: **the account knows it is missing something**, and
//! [`Portfolio::incompleteness`](radar_types::Portfolio::incompleteness) hands
//! that fact to a caller that can refuse on it. Dropping the row instead would
//! produce a portfolio indistinguishable from one with no exposure at all, and
//! that reading is the one that authorises more.
//!
//! The same distinction `coverage::ObservedSlots` draws between a window that
//! ran and saw nothing and a window nobody ran.
//!
//! # What it can say
//!
//! A **closed** position with a recorded `realised_micro_usd` is complete: the
//! round trip is over, the number is the number, and it goes straight into the
//! realised total. A closed position *without* one is not — a result nobody
//! computed is not a result of zero — so it is recorded as unaccounted too.
//!
//! Expects rows already folded by [`fold_positions`](crate::fold_positions).
//! Handed raw rows it would count an opened-then-closed position twice.

use radar_types::{Custody, Portfolio, Refusal, SignedMicroUsd, Slot};

use crate::Position;

/// Builds the account from folded position rows.
///
/// `custody` is supplied rather than derived, because no position row names a
/// wallet and inventing one would attribute balances to an address nobody
/// configured. [`Custody::Unattributed`] is the honest value for an instance
/// with no wallet, and it is what makes the resulting portfolio unable to hold
/// or reserve anything (rule 8).
#[must_use]
pub fn portfolio_from(custody: Custody, as_of: Slot, positions: &[Position]) -> Portfolio {
    let mut portfolio = match custody {
        Custody::Wallet(wallet) => Portfolio::at(wallet, as_of),
        Custody::Unattributed => Portfolio::unattributed(as_of),
    };

    for position in positions {
        if position.is_open() {
            // Open exposure the row cannot quantify. See the module docs.
            portfolio.note_unaccounted(position.mint, Refusal::NotRecorded);
            continue;
        }
        match position.realised_micro_usd {
            Some(realised) => portfolio.record_realised(SignedMicroUsd(realised)),
            // Closed, and what it made was never written down. Adding nothing
            // and saying nothing would report the day's realised total as
            // though this round trip had broken even.
            None => portfolio.note_unaccounted(position.mint, Refusal::NotRecorded),
        }
    }

    portfolio
}

#[cfg(test)]
mod tests {
    use super::portfolio_from;
    use crate::Position;
    use radar_types::{Address, Custody, Incomplete, Refusal, SignedMicroUsd, Slot};

    const NOW: Slot = Slot(20_000);

    fn open(mint: u8) -> Position {
        Position {
            mint: Address::new([mint; 32]),
            creator: Address::new([200u8; 32]),
            opened_at: Slot(10_000),
            notional_micro_usd: 5_000_000,
            entry_price: Some(1_000),
            closed_at: None,
            exit_price: None,
            realised_micro_usd: None,
        }
    }

    fn closed(mint: u8, realised: Option<i64>) -> Position {
        let mut position = open(mint);
        position.closed_at = Some(Slot(12_000));
        position.exit_price = Some(870);
        position.realised_micro_usd = realised;
        position
    }

    #[test]
    fn no_rows_is_a_complete_account_that_holds_nothing() {
        // The state every deployment is in today. It has to read as a portfolio
        // that was assembled and found empty, not as one that failed -- a
        // refusal on the normal case is a check that fires on everybody.
        let portfolio = portfolio_from(Custody::Unattributed, NOW, &[]);
        assert_eq!(portfolio.incompleteness(), None);
        assert_eq!(portfolio.holdings().count(), 0);
        assert_eq!(portfolio.unaccounted().count(), 0);
        assert_eq!(portfolio.as_of(), NOW);
        assert_eq!(portfolio.results().realised, SignedMicroUsd::ZERO);
    }

    #[test]
    fn an_open_position_is_recorded_as_exposure_the_row_cannot_quantify() {
        // The row says five dollars were committed to a mint. It does not say
        // how many units came back, at what decimals, or under which token
        // program -- so there is no holding to build, and the honest output is
        // an account that knows it is short of the facts.
        //
        // Re-apply the bug by dropping the row instead of noting it, and
        // `incompleteness` returns None: a portfolio with recorded exposure
        // reads as one with none, which is the reading that authorises more.
        let portfolio = portfolio_from(Custody::Unattributed, NOW, &[open(1)]);
        assert_eq!(
            portfolio.incompleteness(),
            Some(Incomplete::Unaccounted {
                mint: Address::new([1u8; 32]),
                why: Refusal::NotRecorded
            })
        );
        assert_eq!(portfolio.unaccounted().count(), 1);
        assert_eq!(
            portfolio.holdings().count(),
            0,
            "and nothing was invented to fill the gap"
        );
    }

    #[test]
    fn a_closed_position_with_a_recorded_result_is_complete() {
        // The one thing the row does say completely. A round trip that is over
        // with its number written down needs nothing further.
        let portfolio = portfolio_from(Custody::Unattributed, NOW, &[closed(1, Some(-650_000))]);
        assert_eq!(portfolio.incompleteness(), None);
        assert_eq!(portfolio.results().realised, SignedMicroUsd(-650_000));
    }

    #[test]
    fn a_closed_position_with_no_recorded_result_is_not_a_break_even_one() {
        // Rule 9 at the realised total. Adding nothing for it and saying nothing
        // reports the round trip as having made exactly zero, which is a real
        // outcome and not this one.
        //
        // Re-apply the bug by treating `None` as `SignedMicroUsd::ZERO` and
        // `incompleteness` goes quiet while the total stays at zero -- the
        // account looks complete and flat.
        let portfolio = portfolio_from(Custody::Unattributed, NOW, &[closed(1, None)]);
        assert_eq!(
            portfolio.incompleteness(),
            Some(Incomplete::Unaccounted {
                mint: Address::new([1u8; 32]),
                why: Refusal::NotRecorded
            })
        );
        assert_eq!(
            portfolio.results().realised,
            SignedMicroUsd::ZERO,
            "the total is unchanged, and the account says why it cannot be trusted"
        );
    }

    #[test]
    fn realised_results_accumulate_across_closed_positions() {
        let portfolio = portfolio_from(
            Custody::Unattributed,
            NOW,
            &[closed(1, Some(-650_000)), closed(2, Some(400_000))],
        );
        assert_eq!(portfolio.results().realised, SignedMicroUsd(-250_000));
        assert_eq!(portfolio.incompleteness(), None);
    }

    #[test]
    fn a_wallet_is_carried_through_and_an_absent_one_is_not_invented() {
        // No position row names a wallet. Deriving one would attribute balances
        // to an address nobody configured; defaulting to the zero address would
        // attribute them to the System Program.
        let wallet = Address::new([9u8; 32]);
        assert_eq!(
            portfolio_from(Custody::Wallet(wallet), NOW, &[]).wallet(),
            Some(wallet)
        );
        assert_eq!(
            portfolio_from(Custody::Unattributed, NOW, &[]).wallet(),
            None
        );
    }
}
