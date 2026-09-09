// SPDX-License-Identifier: Apache-2.0
//! What an amount is denominated in.
//!
//! Radar has one money type, [`MicroUsd`](crate::MicroUsd), and it is a
//! *valuation* — dollars, for comparing against a dollar limit. It says nothing
//! about what would actually change hands. `Asset` is that second question, and
//! it is a different one: a position sized in dollars can be quoted in SOL on a
//! bonding curve and in USDC on an order book, and those are not interchangeable
//! balances.
//!
//! # Why the distinctions are the ones they are
//!
//! Design 0017 §3 — the private trader, which is not in `docs/design/` at this
//! commit, so this is a quotation and not a link:
//! *"Use mint addresses and pool/program identities, never tickers, as keys.
//! SOL, wrapped SOL and USDC are distinct accounting assets; other SPL and
//! Token-2022 assets can be admitted by supported semantics."*
//!
//! - **SOL and wrapped SOL are not the same balance.** Native SOL is an
//!   account's own lamports; wrapped SOL is an SPL token account that has to be
//!   created, funded and closed. A trade quoted in one cannot be paid from the
//!   other without an instruction that does the wrapping, so a proposal that
//!   confused them would describe a transaction the signer cannot build.
//! - **Token-2022 is kept apart from classic SPL** because its extensions change
//!   transfer and valuation semantics: 0017 §3 names *"transfer fees/hooks,
//!   permanent delegates, freeze or pause behaviour and amount-display
//!   extensions"*. A transfer fee means the amount received is not the amount
//!   sent; a hook means a transfer can be refused outright. Reading one as plain
//!   SPL is how a valuation ends up describing a transfer that cannot happen —
//!   the same class of error `radar_sim::MintStructure` exists to catch, and
//!   pump.fun's own mints are Token-2022.
//!
//! There is no arithmetic here on purpose. Multiplying an asset by a quantity
//! belongs to the portfolio that will hold balances, and a type nothing calls is
//! not a design.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::Address;

/// An accounting asset: what a balance or a quote is denominated in.
///
/// Keyed by mint address rather than by ticker, because tickers are
/// creator-controlled text and two mints may share one. The named variants are
/// the canonical spelling of the three assets Radar accounts for separately.
///
/// # One asset has one value
///
/// `Asset::Spl(Asset::WRAPPED_SOL_MINT)` and [`Asset::WrappedSol`] would be the
/// same asset under two values that do not compare equal and hash apart, and
/// the same for the USDC mint. Two spellings of one trade content-address to
/// two nonces, so an auditor recomputing an authorisation would see a mismatch
/// on a trade that was in fact authorised.
///
/// That is closed by construction rather than by this paragraph:
///
/// - [`Asset::spl`] and [`Asset::token_2022`] are the only way to build the
///   mint-carrying variants, and `spl` folds the two named mints into their
///   named variants. The variants themselves are `#[non_exhaustive]`, so no
///   crate outside this one can write `Asset::Spl(..)` at all.
/// - Deserialisation runs through the same constructors — see `AssetRepr` — so
///   `{"spl":"So111…112"}` on the wire arrives as [`Asset::WrappedSol`].
///
/// A private field would say this more directly, but Rust has no field
/// visibility inside an enum variant; `#[non_exhaustive]` is the nearest thing
/// it offers, and it stops at the crate rather than at the module.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Asset {
    /// Native SOL: an account's own lamport balance. Has no mint, because it is
    /// not a token.
    Sol,
    /// Wrapped SOL, the SPL token `So111…112`. A token account, not a lamport
    /// balance, and it has to exist before it can receive anything.
    WrappedSol,
    /// USDC on Solana, the SPL token `EPjF…Dt1v`. 0017 §3: *"USDC is a token,
    /// not a separate chain or venue."*
    Usdc,
    /// Any other mint on the classic SPL Token program.
    ///
    /// Build it with [`Asset::spl`]. `#[non_exhaustive]` so that no other crate
    /// can name the field and write the wrapped-SOL or USDC mint here, which
    /// would be one of those assets under a value that does not compare equal
    /// to its own canonical spelling.
    #[non_exhaustive]
    Spl(
        /// The mint. Never [`Asset::WRAPPED_SOL_MINT`] or [`Asset::USDC_MINT`].
        Address,
    ),
    /// Any mint on the Token-2022 program.
    ///
    /// Held apart from [`Spl`](Self::Spl) because the extensions this program
    /// allows — transfer fees and hooks, permanent delegates, freeze and pause,
    /// scaled display amounts — can change what a transfer moves and what a
    /// balance is worth. "It is a token" is not enough to price one.
    ///
    /// Build it with [`Asset::token_2022`]. `#[non_exhaustive]` for the reason
    /// [`Spl`](Self::Spl) is, though this one folds nothing: the two named mints
    /// are classic SPL, so the same address under Token-2022 is a different
    /// asset and not a second spelling of the same one.
    #[non_exhaustive]
    Token2022(
        /// The mint.
        Address,
    ),
}

/// The wire shape of an [`Asset`], and the only thing that deserialises one.
///
/// Every deserialised value goes through [`Asset::spl`] and lands canonical.
/// Without this, `#[non_exhaustive]` would close the hole for code and leave it
/// open for anything arriving as JSON — serde constructs a variant whatever the
/// visibility of its fields, and JSON is how a proposal reaches a recomputing
/// auditor.
///
/// It must stay a mirror of `Asset`, variant for variant and name for name. A
/// variant added there and not here fails to deserialise, and the `From` below
/// is where that is caught: it stops compiling.
///
/// `Asset` derives `Serialize` and `JsonSchema` itself rather than delegating
/// here, so the schema and the written form stay descriptions of `Asset`. Only
/// the read is redirected, because only the read can smuggle a value in.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum AssetRepr {
    Sol,
    WrappedSol,
    Usdc,
    Spl(Address),
    Token2022(Address),
}

impl From<AssetRepr> for Asset {
    fn from(repr: AssetRepr) -> Self {
        match repr {
            AssetRepr::Sol => Self::Sol,
            AssetRepr::WrappedSol => Self::WrappedSol,
            AssetRepr::Usdc => Self::Usdc,
            AssetRepr::Spl(mint) => Self::spl(mint),
            AssetRepr::Token2022(mint) => Self::token_2022(mint),
        }
    }
}

impl<'de> Deserialize<'de> for Asset {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        AssetRepr::deserialize(d).map(Self::from)
    }
}

impl Asset {
    /// The wrapped SOL mint, `So11111111111111111111111111111111111111112`.
    pub const WRAPPED_SOL_MINT: Address = Address::new([
        0x06, 0x9b, 0x88, 0x57, 0xfe, 0xab, 0x81, 0x84, 0xfb, 0x68, 0x7f, 0x63, 0x46, 0x18, 0xc0,
        0x35, 0xda, 0xc4, 0x39, 0xdc, 0x1a, 0xeb, 0x3b, 0x55, 0x98, 0xa0, 0xf0, 0x00, 0x00, 0x00,
        0x00, 0x01,
    ]);

    /// The USDC mint, `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`.
    pub const USDC_MINT: Address = Address::new([
        0xc6, 0xfa, 0x7a, 0xf3, 0xbe, 0xdb, 0xad, 0x3a, 0x3d, 0x65, 0xf3, 0x6a, 0xab, 0xc9, 0x74,
        0x31, 0xb1, 0xbb, 0xe4, 0xc2, 0xd2, 0xf6, 0xe0, 0xe4, 0x7c, 0xa6, 0x02, 0x03, 0x45, 0x2f,
        0x5d, 0x61,
    ]);

    /// The asset for a mint on the classic SPL Token program.
    ///
    /// Folds the two mints that have their own variants into those variants,
    /// which is the whole reason this exists rather than the variant being
    /// public: `Asset::spl(Asset::WRAPPED_SOL_MINT)` is [`Asset::WrappedSol`],
    /// so a caller that reads a mint off a token account cannot produce a
    /// second, unequal spelling of an asset Radar already accounts for.
    #[must_use]
    pub fn spl(mint: Address) -> Self {
        if mint == Self::WRAPPED_SOL_MINT {
            Self::WrappedSol
        } else if mint == Self::USDC_MINT {
            Self::Usdc
        } else {
            Self::Spl(mint)
        }
    }

    /// The asset for a mint on the Token-2022 program.
    ///
    /// Folds nothing, and that is not an oversight. Wrapped SOL and USDC are
    /// classic SPL mints, so the same address under Token-2022 is a *different*
    /// asset — different transfer semantics, different tag — and collapsing it
    /// into [`Asset::WrappedSol`] would be the error this module exists to
    /// refuse, not a canonicalisation. It is a constructor because the variant
    /// is `#[non_exhaustive]` and other crates need some way to build one.
    #[must_use]
    pub const fn token_2022(mint: Address) -> Self {
        Self::Token2022(mint)
    }

    /// The mint that holds this asset, or `None` for native SOL.
    ///
    /// `None` is the honest answer rather than a placeholder: native SOL has no
    /// mint account, and the all-zero address is the System Program.
    #[must_use]
    pub const fn mint(self) -> Option<Address> {
        match self {
            Self::Sol => None,
            Self::WrappedSol => Some(Self::WRAPPED_SOL_MINT),
            Self::Usdc => Some(Self::USDC_MINT),
            Self::Spl(mint) | Self::Token2022(mint) => Some(mint),
        }
    }

    /// A stable byte tag, for content-addressing a value that contains an asset.
    ///
    /// Deliberately not the enum's discriminant: these numbers are hashed into
    /// the risk kernel's nonce, so a recorded authorisation stays recomputable
    /// only while they hold still. **Never renumber one; only append.**
    /// Reordering the variants would silently change every historical nonce.
    ///
    /// Held by `every_asset_carries_a_distinct_tag`, which pins the numbers
    /// themselves rather than their distinctness: a swap leaves them distinct.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::Sol => 1,
            Self::WrappedSol => 2,
            Self::Usdc => 3,
            Self::Spl(_) => 4,
            Self::Token2022(_) => 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Asset;
    use crate::Address;

    #[test]
    fn the_named_mints_are_the_addresses_they_claim_to_be() {
        // Hand-decoded byte arrays. A transposed pair renders as a different
        // base58 string and would silently retarget every quote denominated in
        // one of these -- the kind of constant that is either right or
        // catastrophic, with nothing in between.
        assert_eq!(
            Asset::WRAPPED_SOL_MINT.to_string(),
            "So11111111111111111111111111111111111111112"
        );
        assert_eq!(
            Asset::USDC_MINT.to_string(),
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
        );
    }

    #[test]
    fn native_sol_has_no_mint_and_the_others_do() {
        // Not a formality: the all-zero address is the System Program, so a
        // `mint()` that returned one for native SOL would hand a caller a real
        // account that has nothing to do with the balance being described.
        assert_eq!(Asset::Sol.mint(), None);
        assert_eq!(Asset::WrappedSol.mint(), Some(Asset::WRAPPED_SOL_MINT));
        assert_eq!(Asset::Usdc.mint(), Some(Asset::USDC_MINT));
    }

    #[test]
    fn every_asset_carries_a_distinct_tag() {
        // The values, not merely their distinctness. These numbers are hashed
        // into a nonce, so swapping two of them keeps every asset distinct and
        // changes every authorisation ever recorded -- a rewrite that no
        // dedup check can see. `tag()`'s doc says "never renumber one; only
        // append"; this is what holds it. 0 is absent from the list because it
        // is reserved: it reads as an absent asset.
        let mint = Asset::WRAPPED_SOL_MINT;
        assert_eq!(
            [
                Asset::Sol.tag(),
                Asset::WrappedSol.tag(),
                Asset::Usdc.tag(),
                Asset::Spl(mint).tag(),
                Asset::Token2022(mint).tag(),
            ],
            [1, 2, 3, 4, 5],
            "tags are hashed into nonces; append only"
        );
    }

    #[test]
    fn the_same_mint_under_two_token_programs_is_two_assets() {
        // Token-2022's extensions can tax or refuse a transfer, so a valuation
        // that read one as classic SPL would be describing a transfer that may
        // not happen. Collapsing the two variants makes this fail.
        let mint = Asset::WRAPPED_SOL_MINT;
        assert_ne!(Asset::Spl(mint).tag(), Asset::Token2022(mint).tag());
    }

    #[test]
    fn a_named_mint_written_as_plain_spl_is_the_named_asset() {
        // The two spellings hash apart and do not compare equal, so a trade
        // written one way and recomputed the other way content-addresses to a
        // different nonce -- an auditor sees a mismatch on a trade that was in
        // fact authorised. `spl` is the only constructor, so there is nowhere
        // for the second spelling to come from.
        assert_eq!(Asset::spl(Asset::WRAPPED_SOL_MINT), Asset::WrappedSol);
        assert_eq!(Asset::spl(Asset::USDC_MINT), Asset::Usdc);
        assert_eq!(Asset::spl(Asset::WRAPPED_SOL_MINT).tag(), 2);

        // Any other mint is left alone: this folds two addresses, it does not
        // rewrite the variant.
        let other = Address::new([9u8; 32]);
        assert_eq!(Asset::spl(other).mint(), Some(other));
        assert_eq!(Asset::spl(other).tag(), 4);

        // Token-2022 folds nothing. Wrapped SOL is a classic SPL mint, so the
        // same address under the other program is a different asset and not a
        // second spelling of this one.
        assert_eq!(
            Asset::token_2022(Asset::WRAPPED_SOL_MINT).tag(),
            Asset::Token2022(Asset::WRAPPED_SOL_MINT).tag()
        );
        assert_ne!(
            Asset::token_2022(Asset::WRAPPED_SOL_MINT),
            Asset::WrappedSol
        );
    }

    #[test]
    fn the_second_spelling_does_not_survive_the_wire_either() {
        // `#[non_exhaustive]` stops a *crate* writing the non-canonical value.
        // It does not stop serde constructing one, and JSON is how a proposal
        // reaches a recomputing auditor -- so the same fold has to run on the
        // way in or the guarantee holds only for code.
        let smuggled = format!("{{\"spl\":\"{}\"}}", Asset::WRAPPED_SOL_MINT);
        let got: Asset = serde_json::from_str(&smuggled).expect("a valid asset");
        assert_eq!(got, Asset::WrappedSol, "the wire kept the second spelling");
        assert_eq!(got.tag(), Asset::WrappedSol.tag());

        let smuggled_usdc = format!("{{\"spl\":\"{}\"}}", Asset::USDC_MINT);
        assert_eq!(
            serde_json::from_str::<Asset>(&smuggled_usdc).expect("a valid asset"),
            Asset::Usdc
        );

        // Everything else still round-trips unchanged, including the
        // Token-2022 value that must *not* be folded.
        for asset in [
            Asset::Sol,
            Asset::WrappedSol,
            Asset::Usdc,
            Asset::spl(Address::new([9u8; 32])),
            Asset::token_2022(Asset::WRAPPED_SOL_MINT),
        ] {
            let json = serde_json::to_string(&asset).expect("serialisable");
            assert_eq!(
                serde_json::from_str::<Asset>(&json).expect("a valid asset"),
                asset,
                "{json} did not survive its own round trip"
            );
        }
    }
}
