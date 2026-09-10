// SPDX-License-Identifier: Apache-2.0
//! How many base units, and what a base unit means.
//!
//! A token balance on Solana is an integer of base units. What that integer is
//! *worth* depends entirely on the mint's `decimals`, which lives on the mint
//! account and nowhere else. 1_000_000 is one USDC and it is a thousandth of a
//! SOL, and the two are the same `u64`.
//!
//! So a bare integer is not a balance. It is a balance and a missing fact, and
//! the missing fact is a factor of a thousand. [`TokenQuantity`] is the pair,
//! and there is no way to build one without the second half.
//!
//! # Why the decimals have to be *verified*
//!
//! [`Decimals`] cannot be built from a number that was assumed. Its two
//! constructors say where the number came from: [`Decimals::NATIVE_SOL`] is a
//! protocol constant, and [`Decimals::from_mint_account`] takes a byte that was
//! read off a mint. Nothing else builds one — not `From<u8>`, not a public
//! field, and not serde, which routes through the same gate.
//!
//! That is deliberate friction. A `decimals` guessed from a ticker is how a
//! position ends up sized a thousand times too large, and the guess is
//! indistinguishable from the read once both are a `u8`.

use core::cmp::Ordering;
use core::fmt;

use serde::{Deserialize, Deserializer, Serialize};

/// The decimal places a mint divides its base unit into.
///
/// Held apart from a plain `u8` so that a quantity cannot be built against a
/// number nobody read. See the module documentation.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize)]
#[serde(transparent)]
pub struct Decimals(u8);

/// The wire shape of [`Decimals`], and the only thing that deserialises one.
///
/// Without it, `{"decimals":200}` would arrive as a `Decimals(200)` that no
/// constructor would have produced — serde ignores field privacy, which is the
/// same hole `AssetRepr` closes for [`Asset`](crate::Asset).
#[derive(Deserialize)]
#[serde(transparent)]
struct DecimalsRepr(u8);

impl<'de> Deserialize<'de> for Decimals {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let repr = DecimalsRepr::deserialize(d)?;
        Self::from_mint_account(repr.0).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "{} decimal places cannot be scaled in 64 bits; the maximum is {}",
                repr.0,
                Self::MAX.get()
            ))
        })
    }
}

impl Decimals {
    /// Native SOL: nine places, one lamport being the base unit.
    ///
    /// A protocol constant rather than a read, which is why it is allowed to
    /// exist as a constant at all. Every other asset's decimals come off its
    /// mint.
    pub const NATIVE_SOL: Self = Self(9);

    /// The most places that can be scaled inside 64 bits.
    ///
    /// `10^19` fits in a `u64` and `10^20` does not, so nineteen is the last
    /// place at which a whole unit is still representable. Eighteen is the cap
    /// here because at nineteen a balance of more than 1.8 whole units
    /// overflows, and a scale factor that only works for dust is not a scale
    /// factor — refusing is honest and truncating is not.
    pub const MAX: Self = Self(18);

    /// The decimals byte read from a mint account.
    ///
    /// `None` when the byte cannot be used for arithmetic here — see
    /// [`Decimals::MAX`]. `None` is *not* "assume zero": zero decimals is a real
    /// and common configuration meaning the token is indivisible, so defaulting
    /// to it would turn an unusable mint into one whose every balance reads as
    /// a whole-unit count.
    #[must_use]
    pub const fn from_mint_account(decimals: u8) -> Option<Self> {
        if decimals > Self::MAX.0 {
            return None;
        }
        Some(Self(decimals))
    }

    /// The number of places.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Base units in one whole unit: `10^decimals`.
    ///
    /// Cannot overflow, because [`Decimals::MAX`] is what bounds it.
    #[must_use]
    pub fn one_whole_unit(self) -> u64 {
        10u64.pow(u32::from(self.0))
    }
}

/// A balance, in base units, with the decimals that give them meaning.
///
/// # Arithmetic is checked and refuses across decimals
///
/// [`checked_add`](Self::checked_add) and [`checked_sub`](Self::checked_sub)
/// return `None` when the two quantities were not measured in the same unit.
/// That case is a bug in the caller, and the alternative — adding the raw
/// integers and keeping one side's decimals — produces a number that looks
/// like a balance and is off by whatever the difference in scale was.
///
/// There is deliberately no [`Ord`]: comparing raw units across two decimals is
/// the same error in a cheaper disguise. [`checked_cmp`](Self::checked_cmp) is
/// the comparison, and it can decline.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TokenQuantity {
    /// Base units.
    raw: u64,
    /// What a base unit is.
    decimals: Decimals,
}

impl TokenQuantity {
    /// A quantity of `raw` base units at `decimals` places.
    #[must_use]
    pub const fn new(raw: u64, decimals: Decimals) -> Self {
        Self { raw, decimals }
    }

    /// Nothing, in this unit.
    ///
    /// A *measured* zero: the balance was counted and it was empty. That is a
    /// different fact from a balance nobody could read, which is
    /// [`Balance::Uncounted`](crate::Balance::Uncounted) and not a quantity at
    /// all.
    #[must_use]
    pub const fn zero(decimals: Decimals) -> Self {
        Self::new(0, decimals)
    }

    /// Lamports.
    #[must_use]
    pub const fn lamports(raw: u64) -> Self {
        Self::new(raw, Decimals::NATIVE_SOL)
    }

    /// Base units.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.raw
    }

    /// The unit these base units are in.
    #[must_use]
    pub const fn decimals(self) -> Decimals {
        self.decimals
    }

    /// Whether this counted balance is empty.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.raw == 0
    }

    /// The sum, or `None` on overflow or a unit mismatch.
    #[must_use]
    pub fn checked_add(self, other: Self) -> Option<Self> {
        if self.decimals != other.decimals {
            return None;
        }
        Some(Self::new(self.raw.checked_add(other.raw)?, self.decimals))
    }

    /// The difference, or `None` on underflow or a unit mismatch.
    ///
    /// Underflow returns `None` rather than saturating at zero. A balance that
    /// went negative is an accounting error, and reporting it as an empty
    /// account is exactly the failure this module exists inside.
    #[must_use]
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        if self.decimals != other.decimals {
            return None;
        }
        Some(Self::new(self.raw.checked_sub(other.raw)?, self.decimals))
    }

    /// How these two compare, or `None` when they are not in the same unit.
    #[must_use]
    pub fn checked_cmp(self, other: Self) -> Option<Ordering> {
        (self.decimals == other.decimals).then(|| self.raw.cmp(&other.raw))
    }
}

impl fmt::Display for TokenQuantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scale = self.decimals.one_whole_unit();
        let places = usize::from(self.decimals.get());
        if places == 0 {
            return write!(f, "{}", self.raw);
        }
        write!(f, "{}.{:0places$}", self.raw / scale, self.raw % scale)
    }
}

#[cfg(test)]
mod tests {
    use super::{Decimals, TokenQuantity};
    use core::cmp::Ordering;

    #[test]
    fn decimals_beyond_what_sixty_four_bits_can_scale_are_refused_not_clamped() {
        // Clamping would hand back a `Decimals` nobody read, under a number
        // that is not the mint's. Every balance measured against it would be
        // wrong by a power of ten and would still look like a balance.
        assert_eq!(Decimals::from_mint_account(6).map(Decimals::get), Some(6));
        assert_eq!(Decimals::from_mint_account(18).map(Decimals::get), Some(18));
        assert_eq!(Decimals::from_mint_account(19), None);
        assert_eq!(Decimals::from_mint_account(255), None);
    }

    #[test]
    fn zero_decimals_is_a_real_configuration_and_not_an_absent_one() {
        // An indivisible token. If `from_mint_account` treated 0 as "unset" the
        // NFT-shaped mints would be unrepresentable; if a refusal defaulted to
        // 0 every unusable mint would read as one.
        let indivisible = Decimals::from_mint_account(0).expect("a real configuration");
        assert_eq!(indivisible.one_whole_unit(), 1);
        assert_eq!(TokenQuantity::new(7, indivisible).to_string(), "7");
    }

    #[test]
    fn the_largest_admitted_scale_does_not_overflow() {
        // `one_whole_unit` is a `pow`, which panics on overflow in debug and
        // wraps in release. `MAX` is what keeps it in range, so the bound and
        // the arithmetic have to be checked against each other rather than
        // trusted to agree.
        assert_eq!(Decimals::MAX.one_whole_unit(), 1_000_000_000_000_000_000);
        assert_eq!(Decimals::NATIVE_SOL.one_whole_unit(), 1_000_000_000);
    }

    #[test]
    fn a_decimals_that_no_constructor_would_produce_does_not_arrive_over_the_wire() {
        // `#[serde(transparent)]` on a private field still deserialises the
        // field. JSON is how a balance reaches a process that did not read the
        // mint, so the gate has to run on the way in or it holds for code only.
        assert_eq!(
            serde_json::from_str::<Decimals>("6").expect("in range"),
            Decimals::from_mint_account(6).expect("in range")
        );
        assert!(
            serde_json::from_str::<Decimals>("19").is_err(),
            "19 places cannot be scaled in 64 bits and must not be smuggled in"
        );
        assert!(serde_json::from_str::<Decimals>("255").is_err());
    }

    #[test]
    fn quantities_in_different_units_do_not_add() {
        // 1_000_000 is one USDC and one thousandth of a SOL. Adding the raw
        // integers and keeping either side's decimals produces a number that
        // reads as a balance and is off by a factor of a thousand.
        let usdc = TokenQuantity::new(1_000_000, Decimals::from_mint_account(6).expect("six"));
        let sol = TokenQuantity::lamports(1_000_000);

        assert_eq!(usdc.checked_add(sol), None, "different units do not sum");
        assert_eq!(usdc.checked_sub(sol), None);
        assert_eq!(usdc.checked_cmp(sol), None, "nor do they compare");
        assert_ne!(usdc, sol, "nor are they equal on the raw integer alone");
    }

    #[test]
    fn quantities_in_the_same_unit_add_subtract_and_compare() {
        let a = TokenQuantity::lamports(3_000);
        let b = TokenQuantity::lamports(1_250);
        assert_eq!(a.checked_add(b), Some(TokenQuantity::lamports(4_250)));
        assert_eq!(a.checked_sub(b), Some(TokenQuantity::lamports(1_750)));
        assert_eq!(a.checked_cmp(b), Some(Ordering::Greater));
        assert_eq!(b.checked_cmp(a), Some(Ordering::Less));
        assert_eq!(a.checked_cmp(a), Some(Ordering::Equal));
    }

    #[test]
    fn a_balance_that_would_go_negative_is_refused_rather_than_floored() {
        // Saturating here would report an overdrawn account as an empty one,
        // which is the whole failure this type sits inside. Rule 9.
        let held = TokenQuantity::lamports(1_000);
        assert_eq!(held.checked_sub(TokenQuantity::lamports(1_001)), None);
        assert_eq!(
            held.checked_sub(TokenQuantity::lamports(1_000)),
            Some(TokenQuantity::lamports(0)),
            "exactly drained is a counted zero, and that one is fine"
        );
    }

    #[test]
    fn addition_that_would_overflow_is_refused() {
        let huge = TokenQuantity::lamports(u64::MAX);
        assert_eq!(huge.checked_add(TokenQuantity::lamports(1)), None);
    }

    #[test]
    fn a_quantity_renders_at_its_own_scale() {
        // The rendering is what an operator reads off a report, and a missing
        // pad turns 0.000001 USDC into 0.1 USDC.
        let six = Decimals::from_mint_account(6).expect("six");
        assert_eq!(TokenQuantity::new(1_234_567, six).to_string(), "1.234567");
        assert_eq!(TokenQuantity::new(1, six).to_string(), "0.000001");
        assert_eq!(TokenQuantity::lamports(1).to_string(), "0.000000001");
        assert_eq!(
            TokenQuantity::lamports(2_500_000_000).to_string(),
            "2.500000000"
        );
    }

    #[test]
    fn a_quantity_survives_a_round_trip_through_json() {
        let q = TokenQuantity::new(42, Decimals::from_mint_account(6).expect("six"));
        let json = serde_json::to_string(&q).expect("serialises");
        assert_eq!(
            serde_json::from_str::<TokenQuantity>(&json).expect("deserialises"),
            q
        );
    }
}
