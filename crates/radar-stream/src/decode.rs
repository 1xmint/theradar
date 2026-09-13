// SPDX-License-Identifier: Apache-2.0
//! From a transaction's balance changes to trades, holder balances and launches.
//!
//! # Why balances and not instructions
//!
//! Every venue has its own instruction layout, and there are a dozen venues.
//! Every venue also leaves the same trace in a transaction's metadata: token
//! balances before and after, and lamports before and after. A swap is a
//! wallet's balance of a coin going one way while a pool's balance of the same
//! coin goes the other, and the pool taking the payment in something else. So
//! this reads balances, and one fold covers pump.fun's bonding curve, PumpSwap,
//! Raydium, Meteora and Orca alike.
//!
//! # Who traded, and which way
//!
//! **The trader is the fee payer**, the transaction's first account. The side is
//! read straight off that wallet's balance of the coin: up is a buy, down is a
//! sell. There is no inference here, which is the improvement over the
//! CryptoHouse fold in `radar_backfill::market::fold`: that one sees transfers
//! without their owners and has to guess the pool from how often an account
//! appears.
//!
//! # The price
//!
//! **The price comes from the pool's side, not the trader's.** The trader's own
//! SOL change also carries rent for any token account the trade opened, and
//! tips; the pool's change is the trade and nothing else. So the pool is the
//! other owner whose balance of the coin moved the opposite way, largest first,
//! and the payment is what that same owner received (on a buy) or paid out (on
//! a sell) in another asset.
//!
//! Checked against pump.fun's own `TradeEvent` log on eleven real mainnet
//! trades fetched 2026-09-13: every side and every token amount matched the
//! event to the base unit, and so did the SOL payment on all four that were
//! paid in SOL (the other seven were quoted in PUMP, which the event does not
//! carry as a SOL amount). `tests/real_transactions.rs` pins a sample.
//!
//! # What is left out, on purpose
//!
//! - **A trade with no pool payment found is not recorded.** Without one there
//!   is no price, and in a transaction touching a DEX program an unpriced
//!   "trade" is as likely to be a plain transfer riding along. A pump.fun launch
//!   is the common case: the curve account is created in the same transaction,
//!   so its lamport change is rent plus payment and cannot be split. The launch
//!   itself is still recorded, from its instruction.
//! - **A route the fee payer is not party to** (an arbitrage bot's inner legs)
//!   yields no trade, because nobody's balance of the coin moved at the payer.

use std::collections::BTreeMap;

use radar_store::MarketSide;
use radar_types::Address;

use crate::tx::Tx;

/// Wrapped SOL. Native lamports and this mint are one asset here.
pub const WSOL: Address = Address::new([
    6, 155, 136, 87, 254, 171, 129, 132, 251, 104, 127, 99, 70, 24, 192, 53, 218, 196, 57, 220, 26,
    235, 59, 85, 152, 160, 240, 0, 0, 0, 0, 1,
]);
/// USDC.
pub const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
/// USDT.
pub const USDT: &str = "Es9vMFrzaCERmJfrF4H2FYD4KConky2wcgqDfLpRsXsn";

/// How a quote asset ranks when a pool moved more than one: SOL, then USDC,
/// then USDT. `None` for anything else.
fn quote_rank(mint: &Address) -> Option<u8> {
    if *mint == WSOL {
        return Some(0);
    }
    let s = mint.to_string();
    if s == USDC {
        Some(1)
    } else if s == USDT {
        Some(2)
    } else {
        None
    }
}

/// Whether a mint is a quote asset, and so never itself the coin being traded.
#[must_use]
pub fn is_quote(mint: &Address) -> bool {
    quote_rank(mint).is_some()
}

/// One trade, before it has a timestamp.
///
/// Block time arrives separately from the transaction on the feed, so the
/// tape stamps it; see [`crate::tape::Tape::apply`].
#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    /// The coin.
    pub mint: Address,
    /// Which way the trader went. Never unknown: it is read off the trader's
    /// balance, which moved or this would not be a fill.
    pub side: MarketSide,
    /// Coin amount, decimals-adjusted, as the trader's balance changed.
    pub token_amount: f64,
    /// What the pool took or paid, decimals-adjusted.
    pub quote_amount: f64,
    /// The asset it was paid in.
    pub quote_mint: Address,
    /// Quote per coin, from the pool's two legs.
    pub price: f64,
    /// The fee payer.
    pub trader: Address,
    /// The owner identified as the pool. Kept so the holders list can mark it.
    pub pool: Address,
}

/// A token account's balance after the transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holding {
    /// The coin.
    pub mint: Address,
    /// The token account.
    pub account: Address,
    /// Its owner, when the node said.
    pub owner: Option<Address>,
    /// Base units. Zero for an account the transaction closed.
    pub amount: u64,
    /// The mint's decimals.
    pub decimals: u8,
}

/// A pump.fun launch seen in the transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchSeen {
    /// The new coin.
    pub mint: Address,
    /// Creator-supplied, untrusted text.
    pub name: String,
    /// Creator-supplied, untrusted text.
    pub symbol: String,
    /// Creator-supplied, untrusted, never fetched here.
    pub uri: String,
    /// The creator named in the instruction.
    pub creator: Address,
}

/// Everything one transaction contributes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decoded {
    /// Priced trades, one per coin the fee payer's balance moved in.
    pub fills: Vec<Fill>,
    /// Coins the fee payer's balance moved in with no pool payment found.
    /// Counted, never recorded; see the module comment.
    pub unpriced: usize,
    /// Every non-quote token balance the transaction left behind.
    pub holdings: Vec<Holding>,
    /// pump.fun launches.
    pub launches: Vec<LaunchSeen>,
}

/// Signed balance changes, per owner and asset, in base units.
struct Deltas {
    /// `(owner, mint) -> change`.
    tokens: BTreeMap<(Address, Address), i128>,
    decimals: BTreeMap<Address, u8>,
}

impl Deltas {
    fn of(tx: &Tx) -> Self {
        let mut tokens: BTreeMap<(Address, Address), i128> = BTreeMap::new();
        let mut decimals = BTreeMap::new();
        for (balances, sign) in [(&tx.pre_tokens, -1i128), (&tx.post_tokens, 1)] {
            for b in balances {
                decimals.insert(b.mint, b.decimals);
                if let Some(owner) = b.owner {
                    *tokens.entry((owner, b.mint)).or_default() += sign * i128::from(b.amount);
                }
            }
        }
        Self { tokens, decimals }
    }

    fn token(&self, owner: &Address, mint: &Address) -> i128 {
        self.tokens.get(&(*owner, *mint)).copied().unwrap_or(0)
    }
}

/// An owner's change in SOL: wrapped, plus native lamports **only for an
/// account that existed before the transaction**.
///
/// An account created in the transaction starts at zero and ends holding its
/// rent deposit, so its lamport change is rent plus whatever it was paid, and
/// the two cannot be told apart. Leaving it out means a trade against a pool
/// born in the same transaction goes unpriced, which is true, rather than
/// priced with rent in it, which is not.
fn sol_delta(tx: &Tx, deltas: &Deltas, owner: &Address) -> i128 {
    let wrapped = deltas.token(owner, &WSOL);
    let native = tx
        .accounts
        .iter()
        .position(|a| a == owner)
        .filter(|&i| tx.pre_lamports.get(i).copied().unwrap_or(0) > 0)
        .map_or(0, |i| {
            i128::from(tx.post_lamports[i]) - i128::from(tx.pre_lamports[i])
        });
    wrapped + native
}

#[expect(clippy::cast_precision_loss, reason = "display precision for a chart")]
fn adjusted(raw: i128, decimals: u8) -> f64 {
    raw.unsigned_abs() as f64 / 10f64.powi(i32::from(decimals))
}

/// The payment leg a pool moved against `mint`, in the direction given.
///
/// A known quote asset wins, ranked SOL, USDC, USDT, so a pool that also
/// shuffled fees in a second token is still priced in its quote. With no known
/// quote asset, exactly one other asset must have moved: pump.fun curves quoted
/// in PUMP are real (seen 2026-09-13), and two unknown assets moving at once is
/// a route, not a price.
fn payment(
    tx: &Tx,
    deltas: &Deltas,
    pool: &Address,
    mint: &Address,
    received: bool,
) -> Option<(Address, i128)> {
    let direction_ok = |d: i128| d != 0 && (d > 0) == received;
    let mut legs: Vec<(Address, i128)> = Vec::new();
    let sol = sol_delta(tx, deltas, pool);
    if direction_ok(sol) {
        legs.push((WSOL, sol));
    }
    for (&(owner, other), &d) in &deltas.tokens {
        if owner == *pool && other != *mint && other != WSOL && direction_ok(d) {
            legs.push((other, d));
        }
    }
    if let Some(best) = legs
        .iter()
        .filter_map(|leg| quote_rank(&leg.0).map(|r| (r, *leg)))
        .min_by_key(|(r, _)| *r)
    {
        return Some(best.1);
    }
    match legs.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// Folds one transaction.
#[must_use]
pub fn decode(tx: &Tx) -> Decoded {
    let mut out = Decoded {
        holdings: holdings(tx),
        launches: launches(tx),
        ..Decoded::default()
    };
    let Some(&trader) = tx.accounts.first() else {
        return out;
    };
    let deltas = Deltas::of(tx);

    let coins: Vec<Address> = deltas
        .tokens
        .keys()
        .filter(|(owner, mint)| *owner == trader && !is_quote(mint))
        .map(|(_, mint)| *mint)
        .collect();

    for mint in coins {
        let moved = deltas.token(&trader, &mint);
        if moved == 0 {
            continue;
        }
        let bought = moved > 0;
        let decimals = deltas.decimals.get(&mint).copied().unwrap_or(0);

        // Owners whose balance of the coin went the other way, largest first.
        // Ties broken by address so the same transaction always folds the same.
        let mut pools: Vec<(i128, Address)> = deltas
            .tokens
            .iter()
            .filter(|((owner, m), d)| {
                *m == mint && *owner != trader && **d != 0 && (**d > 0) != bought
            })
            .map(|((owner, _), d)| (d.abs(), *owner))
            .collect();
        pools.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

        let priced = pools.iter().find_map(|&(pool_moved, pool)| {
            // On a buy the pool received payment; on a sell it paid out.
            payment(tx, &deltas, &pool, &mint, bought).map(|leg| (pool, pool_moved, leg))
        });
        let Some((pool, pool_moved, (quote_mint, paid))) = priced else {
            out.unpriced += 1;
            continue;
        };
        let quote_decimals = if quote_mint == WSOL {
            9
        } else {
            deltas.decimals.get(&quote_mint).copied().unwrap_or(0)
        };
        let quote_amount = adjusted(paid, quote_decimals);
        let price = quote_amount / adjusted(pool_moved, decimals);
        out.fills.push(Fill {
            mint,
            side: if bought {
                MarketSide::Buy
            } else {
                MarketSide::Sell
            },
            token_amount: adjusted(moved, decimals),
            quote_amount,
            quote_mint,
            price,
            trader,
            pool,
        });
    }
    out
}

fn holdings(tx: &Tx) -> Vec<Holding> {
    let mut out: Vec<Holding> = Vec::new();
    for b in &tx.post_tokens {
        if is_quote(&b.mint) {
            continue;
        }
        let Some(account) = tx.account(b.account_index) else {
            continue;
        };
        out.push(Holding {
            mint: b.mint,
            account: *account,
            owner: b.owner,
            amount: b.amount,
            decimals: b.decimals,
        });
    }
    // A balance present before and absent after is an account the transaction
    // closed. Its holder now holds nothing, and leaving the old balance in
    // place would keep a seller ranked as a holder forever.
    for b in &tx.pre_tokens {
        if is_quote(&b.mint)
            || tx
                .post_tokens
                .iter()
                .any(|p| p.account_index == b.account_index && p.mint == b.mint)
        {
            continue;
        }
        let Some(account) = tx.account(b.account_index) else {
            continue;
        };
        out.push(Holding {
            mint: b.mint,
            account: *account,
            owner: b.owner,
            amount: 0,
            decimals: b.decimals,
        });
    }
    out
}

fn launches(tx: &Tx) -> Vec<LaunchSeen> {
    let pumpfun = radar_decode::Program::PumpFun.address();
    let mut out = Vec::new();
    for ix in &tx.instructions {
        if tx.account(ix.program) != Some(&pumpfun) {
            continue;
        }
        // Handed the program explicitly: pump.fun and PumpSwap share seven
        // discriminators, so bytes alone do not say which venue this is.
        let Some(instruction) = radar_decode::decode(radar_decode::Program::PumpFun, &ix.data)
            .known()
            .copied()
            .and_then(radar_decode::Instruction::pumpfun)
        else {
            continue;
        };
        let Some(Ok(launch)) = radar_decode::pumpfun::launch_args(instruction, &ix.data) else {
            continue;
        };
        // The mint is the instruction's first account for both `create` and
        // `create_v2`, confirmed on a real `create_v2` of 2026-09-13.
        let Some(mint) = ix.accounts.first().and_then(|&i| tx.account(i)) else {
            continue;
        };
        out.push(LaunchSeen {
            mint: *mint,
            name: launch.name.to_owned(),
            symbol: launch.symbol.to_owned(),
            uri: launch.uri.to_owned(),
            creator: launch.creator,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wrapped_sol_constant_is_the_wrapped_sol_mint() {
        assert_eq!(
            WSOL.to_string(),
            "So11111111111111111111111111111111111111112"
        );
    }

    #[test]
    fn only_the_three_quote_assets_rank() {
        assert_eq!(quote_rank(&WSOL), Some(0));
        assert_eq!(quote_rank(&USDC.parse().unwrap()), Some(1));
        assert_eq!(quote_rank(&USDT.parse().unwrap()), Some(2));
        assert_eq!(
            quote_rank(
                &"pumpCmXqMfrsAkQ5r49WcJnRayYRqmXz6ae8H7H9Dfn"
                    .parse()
                    .unwrap()
            ),
            None
        );
    }
}
