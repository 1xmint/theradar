// SPDX-License-Identifier: Apache-2.0
//! Where a trade would happen.
//!
//! A mint is not a market. The same token trades on its bonding curve, on the
//! AMM it graduates to, and on any pool anyone cares to open, and those venues
//! have different prices, different depth and different exit risk at the same
//! instant. A decision that names only the mint is a decision that cannot say
//! which of them it measured — and two such decisions look identical, which is
//! how one silently overwrites the other in anything keyed by token.
//!
//! Design 0017 §3 — the private trader, which is not in `docs/design/` at this
//! commit, so this is a quotation and not a link: *"Use mint addresses and
//! pool/program identities, never tickers, as keys."*

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Address;

/// The venue a proposal is about: the program that runs it, and the pool where
/// the venue has one that the program and the mint do not already determine.
#[derive(
    Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize, JsonSchema,
)]
pub struct Market {
    /// The program that executes the trade.
    ///
    /// The coarsest thing that distinguishes two markets for one token, and the
    /// one that always exists: every venue is a program, whatever it calls its
    /// liquidity.
    pub program: Address,
    /// The account holding this market's own state, where naming it adds
    /// identity.
    ///
    /// `None` means **this market needs no pool to be identified** — not that
    /// one could not be found. A pump.fun bonding curve is the program-derived
    /// address of `["bonding-curve", mint]`, so the program and the mint already
    /// name it exactly and a copy here would be a second spelling of the same
    /// fact. An AMM is the other case: one token can have several pools on one
    /// program, and there `Some` is the only thing that says which.
    ///
    /// A site that does not know which pool it means must not write `None`. It
    /// is naming a different market from the one it measured, and the point of
    /// this type is that such a site cannot compile without saying so.
    pub pool: Option<Address>,
}

impl Market {
    /// The pump.fun program, `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`.
    ///
    /// The same address as `radar_decode::pumpfun::PROGRAM_ID`, which cannot be
    /// used here: the decoder depends on this crate, so this crate cannot depend
    /// on the decoder. The two are checked against each other from a crate that
    /// sees both — see `the_program_this_lane_trades_is_the_one_it_decodes` in
    /// `crates/radar-cli/src/consider.rs`.
    pub const PUMP_FUN_PROGRAM: Address = Address::new([
        0x01, 0x56, 0xe0, 0xf6, 0x93, 0x66, 0x5a, 0xcf, 0x44, 0xdb, 0x15, 0x68, 0xbf, 0x17, 0x5b,
        0xaa, 0x51, 0x89, 0xcb, 0x97, 0xf5, 0xd2, 0xff, 0x3b, 0x65, 0x5d, 0x2b, 0xb6, 0xfd, 0x6d,
        0x18, 0xb0,
    ]);

    /// A pump.fun bonding curve.
    ///
    /// `pool` is `None` for the reason the field documents: the curve account is
    /// `["bonding-curve", mint]` under this program, so the mint on the proposal
    /// and this program name it already. This is the venue Radar trades today,
    /// and the only one: design 0017 admits others by measurement, not by
    /// assumption.
    pub const PUMP_FUN_BONDING_CURVE: Self = Self {
        program: Self::PUMP_FUN_PROGRAM,
        pool: None,
    };
}

#[cfg(test)]
mod tests {
    use super::Market;
    use crate::Address;

    #[test]
    fn the_pump_fun_program_is_the_address_it_claims_to_be() {
        // Hand-decoded from base58. Wrong here means every proposal names a
        // venue that does not exist, and the byte array itself is unreadable --
        // so the string it renders as is what a reviewer can actually check.
        assert_eq!(
            Market::PUMP_FUN_PROGRAM.to_string(),
            "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
        );
    }

    #[test]
    fn two_pools_on_one_program_are_two_markets() {
        // The case `pool` exists for. If a market compared equal on the program
        // alone, a proposal measured against one AMM pool would be authorised
        // against another with different depth -- and nothing downstream could
        // tell the two apart.
        let program = Market::PUMP_FUN_PROGRAM;
        let a = Market {
            program,
            pool: Some(Address::new([7; 32])),
        };
        let b = Market {
            program,
            pool: Some(Address::new([8; 32])),
        };
        assert_ne!(a, b);
        assert_ne!(a, Market::PUMP_FUN_BONDING_CURVE);
        assert_eq!(Market::PUMP_FUN_BONDING_CURVE.pool, None);
    }
}
