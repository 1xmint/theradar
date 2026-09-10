// SPDX-License-Identifier: Apache-2.0
//! Integer money for cost accounting.

use core::fmt;
use core::iter::Sum;
use core::ops::{Add, AddAssign, Mul};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// US dollars in millionths.
///
/// Radar sums call costs across millions of invocations and compares the total
/// against a hard budget cap that is allowed to stop trading. Floating point
/// accumulates error over exactly that kind of summation, so the unit is an
/// integer and the smallest representable amount is one micro-dollar — a
/// thousandth of the cheapest call in the catalogue.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, Default,
)]
#[serde(transparent)]
pub struct MicroUsd(pub u64);

impl MicroUsd {
    /// Free.
    pub const ZERO: Self = Self(0);

    /// One US dollar.
    pub const DOLLAR: Self = Self(1_000_000);

    /// Whole micro-dollars.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Builds an amount from a decimal dollar figure, rounding to the nearest
    /// micro-dollar. For reading config and vendor price lists, which are written
    /// in dollars; never for arithmetic on an amount already in hand.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "non-finite and non-positive inputs return early on the line above the cast"
    )]
    pub fn from_dollars(usd: f64) -> Self {
        if !usd.is_finite() || usd <= 0.0 {
            return Self::ZERO;
        }
        Self((usd * 1_000_000.0).round() as u64)
    }

    /// Saturating addition. A budget meter that panics mid-trade is worse than
    /// one that pins at the maximum and trips every cap it is checked against.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    /// This amount repeated `n` times, saturating.
    #[must_use]
    pub const fn saturating_mul(self, n: u64) -> Self {
        Self(self.0.saturating_mul(n))
    }
}

/// A signed dollar amount, in millionths.
///
/// [`MicroUsd`] is unsigned because a *cost* cannot be negative. A realised
/// result can, and the difference is not cosmetic: a profit-and-loss figure
/// stored unsigned has to carry its sign somewhere else, and the place it ends
/// up is a separate flag a caller forgets to read.
///
/// Deliberately not a replacement for [`MicroUsd`]. Every crate that depends on
/// that type is talking about a cost or a limit, where unsigned is the right
/// shape and a negative value would be a bug the type already makes
/// unrepresentable.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, Default,
)]
#[serde(transparent)]
pub struct SignedMicroUsd(pub i64);

impl SignedMicroUsd {
    /// Broke exactly even.
    ///
    /// A *measured* zero. A result nobody computed is `None` at the call site
    /// that holds one; this type has no room for that state, on purpose.
    pub const ZERO: Self = Self(0);

    /// Whole micro-dollars, signed.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }

    /// Saturating addition.
    ///
    /// A running total that panicked mid-accounting is worse than one pinned at
    /// the extreme, and the extreme trips every limit it is checked against —
    /// the direction that refuses rather than authorises.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    /// What this lost, in micro-dollars, or zero if it did not lose.
    ///
    /// `i64::MIN` has no positive counterpart, so it is dropped rather than
    /// negated: wrapping would turn the largest representable loss into a
    /// number that reads as a gain.
    #[must_use]
    pub const fn loss(self) -> u64 {
        match self.0.checked_neg() {
            Some(positive) if positive > 0 => positive.unsigned_abs(),
            _ => 0,
        }
    }
}

impl Add for SignedMicroUsd {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        self.saturating_add(other)
    }
}

impl Sum for SignedMicroUsd {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Self::saturating_add)
    }
}

impl fmt::Display for SignedMicroUsd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let magnitude = self.0.unsigned_abs();
        write!(
            f,
            "{sign}${}.{:06}",
            magnitude / 1_000_000,
            magnitude % 1_000_000
        )
    }
}

impl fmt::Debug for SignedMicroUsd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SignedMicroUsd({self})")
    }
}

impl Add for MicroUsd {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        self.saturating_add(other)
    }
}

impl AddAssign for MicroUsd {
    fn add_assign(&mut self, other: Self) {
        *self = self.saturating_add(other);
    }
}

impl Mul<u64> for MicroUsd {
    type Output = Self;
    fn mul(self, n: u64) -> Self {
        self.saturating_mul(n)
    }
}

impl Sum for MicroUsd {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Self::saturating_add)
    }
}

impl fmt::Display for MicroUsd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${}.{:06}", self.0 / 1_000_000, self.0 % 1_000_000)
    }
}

impl fmt::Debug for MicroUsd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MicroUsd({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_prices_parse_exactly() {
        // The four prices the whole cost model rests on.
        assert_eq!(MicroUsd::from_dollars(0.001), MicroUsd(1_000));
        assert_eq!(MicroUsd::from_dollars(0.005), MicroUsd(5_000));
        assert_eq!(MicroUsd::from_dollars(0.01), MicroUsd(10_000));
        assert_eq!(MicroUsd::from_dollars(0.05), MicroUsd(50_000));
    }

    #[test]
    fn a_million_cheap_calls_sum_without_drift() {
        // The same sum in f64 lands just off 1000.0. This is the entire reason
        // the type is an integer.
        let total: MicroUsd = std::iter::repeat_n(MicroUsd::from_dollars(0.001), 1_000_000).sum();
        assert_eq!(total, MicroUsd(1_000_000_000));
        assert_eq!(total.to_string(), "$1000.000000");
    }

    #[test]
    fn nonsense_dollar_figures_become_zero_rather_than_garbage() {
        assert_eq!(MicroUsd::from_dollars(f64::NAN), MicroUsd::ZERO);
        assert_eq!(MicroUsd::from_dollars(f64::INFINITY), MicroUsd::ZERO);
        assert_eq!(MicroUsd::from_dollars(-1.0), MicroUsd::ZERO);
    }

    #[test]
    fn addition_saturates_rather_than_overflowing() {
        assert_eq!(MicroUsd(u64::MAX) + MicroUsd(1), MicroUsd(u64::MAX));
    }

    #[test]
    fn a_signed_result_renders_its_sign_and_does_not_wrap_at_the_extreme() {
        // The magnitude is taken with `unsigned_abs`, because negating
        // `i64::MIN` overflows -- and a loss rendered as a gain is the one
        // direction that reads as permission.
        assert_eq!(SignedMicroUsd(-1_500_000).to_string(), "-$1.500000");
        assert_eq!(SignedMicroUsd(1_500_000).to_string(), "$1.500000");
        assert_eq!(SignedMicroUsd::ZERO.to_string(), "$0.000000");
        assert!(SignedMicroUsd(i64::MIN).to_string().starts_with("-$"));
    }

    #[test]
    fn the_most_negative_result_does_not_wrap_into_a_gain_when_read_as_a_loss() {
        // `checked_neg` on `i64::MIN` is `None`. Casting instead would report
        // the largest representable loss as zero loss in one direction and as a
        // wrapped positive in the other; both let a daily-loss limit through.
        assert_eq!(SignedMicroUsd(i64::MIN).loss(), 0, "dropped, never wrapped");
        assert_eq!(SignedMicroUsd(i64::MIN + 1).loss(), i64::MAX.unsigned_abs());
        assert_eq!(SignedMicroUsd(-650_000).loss(), 650_000);
        assert_eq!(SignedMicroUsd::ZERO.loss(), 0);
        assert_eq!(SignedMicroUsd(4_000_000).loss(), 0, "a gain is not a loss");
    }

    #[test]
    fn signed_addition_saturates_at_both_ends() {
        assert_eq!(
            SignedMicroUsd(i64::MAX) + SignedMicroUsd(1),
            SignedMicroUsd(i64::MAX)
        );
        assert_eq!(
            SignedMicroUsd(i64::MIN) + SignedMicroUsd(-1),
            SignedMicroUsd(i64::MIN)
        );
        let mixed: SignedMicroUsd = [SignedMicroUsd(5), SignedMicroUsd(-8)].into_iter().sum();
        assert_eq!(mixed, SignedMicroUsd(-3));
    }
}
