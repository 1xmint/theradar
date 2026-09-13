// SPDX-License-Identifier: Apache-2.0
//! The recent market, held in memory, inside a fixed budget.
//!
//! # Why memory and not the store
//!
//! `radar_store` writes a Parquet file per flush and its reader lists and opens
//! every file in a table on each read. That suits the free collector's ten
//! coins every five minutes. A live feed of every trade on Solana either
//! flushes rarely, and the screen is minutes behind, or flushes often, and the
//! reader drowns in files within the day. So the screen reads this instead, and
//! the store stays what it is good at: history.
//!
//! # The budget
//!
//! **One number bounds all of it**, [`Tape::new`]'s `budget_bytes`. Every
//! trade, minute candle and holder balance is charged against it at a fixed
//! estimate, and when a new one would exceed it, the coin that traded longest
//! ago is dropped whole. The production box has 3.8 GB shared with everything
//! else, and a feed that grows until the kernel kills the web server is the
//! failure this exists to rule out. The estimates are deliberately generous:
//! over-charging drops coins early, which is visible and harmless;
//! under-charging is the outage.
//!
//! Inside a coin there are three more caps, each for its own reason: the tape
//! keeps the newest [`TRADES_PER_MINT`] trades (the most the endpoint returns),
//! candles keep [`CANDLE_MINUTES`] (a day, the widest chart), and holders keep
//! [`HOLDER_ACCOUNTS`] accounts, dropping the smallest.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use radar_store::MarketTrade;
use radar_types::{Address, Signature, Slot};

use crate::decode::{Decoded, Fill, Holding, LaunchSeen};

/// The newest trades kept per coin: the most `/v1/market/trades` will return.
pub const TRADES_PER_MINT: usize = 500;
/// Minute candles kept per coin: one day, the widest chart the API draws.
pub const CANDLE_MINUTES: usize = 24 * 60;
/// Token accounts tracked per coin before the smallest are dropped.
pub const HOLDER_ACCOUNTS: usize = 5_000;
/// Pool owners remembered per coin, for marking them in the holder list.
const POOLS_PER_MINT: usize = 8;

/// What each kept thing is charged against the budget, in bytes.
///
/// Sizes of the in-memory value plus a flat allowance for the collection
/// holding it and, for a trade, the heap string of its timestamp.
const TRADE_BYTES: usize = std::mem::size_of::<MarketTrade>() + 64;
const MINUTE_BYTES: usize = std::mem::size_of::<Minute>() + 16;
const HOLDER_BYTES: usize = 32 + std::mem::size_of::<HeldBalance>() + 48;
const MINT_BYTES: usize = 1_024;

/// One minute of one coin's priced trades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Minute {
    /// The minute's start, unix seconds.
    pub start: i64,
    /// First price.
    pub open: f64,
    /// Highest price.
    pub high: f64,
    /// Lowest price.
    pub low: f64,
    /// Last price.
    pub close: f64,
    /// Quote asset traded.
    pub quote_volume: f64,
    /// Coin traded.
    pub token_volume: f64,
    /// Trades.
    pub trades: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HeldBalance {
    owner: Option<Address>,
    amount: u64,
}

/// A pump.fun launch, as the feed saw it happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    /// Creator-supplied, untrusted.
    pub name: String,
    /// Creator-supplied, untrusted.
    pub symbol: String,
    /// Creator-supplied, untrusted, never fetched.
    pub uri: String,
    /// The creator named in the instruction.
    pub creator: Address,
    /// When it landed, unix seconds.
    pub at: i64,
}

#[derive(Debug)]
struct Coin {
    trades: VecDeque<MarketTrade>,
    minutes: VecDeque<Minute>,
    /// The asset candles are priced in: the first one this coin traded
    /// against while watched. A trade in any other asset still goes on the
    /// tape, and never into a candle, because a candle mixing SOL and USDC
    /// prices is a line with no meaning.
    quote: Option<Address>,
    holders: HashMap<Address, HeldBalance>,
    /// Whether [`HOLDER_ACCOUNTS`] has ever been hit, so the list can say it
    /// is a top slice rather than everyone.
    holders_truncated: bool,
    decimals: Option<u8>,
    pools: Vec<Address>,
    launch: Option<Launch>,
    /// When this coin was first seen, unix seconds. Nothing before it is known.
    first_seen: i64,
    /// The newest trade or balance change, unix seconds; the eviction key.
    last_active: i64,
    bytes: usize,
}

impl Coin {
    fn new(at: i64) -> Self {
        Self {
            trades: VecDeque::new(),
            minutes: VecDeque::new(),
            quote: None,
            holders: HashMap::new(),
            holders_truncated: false,
            decimals: None,
            pools: Vec::new(),
            launch: None,
            first_seen: at,
            last_active: at,
            bytes: MINT_BYTES,
        }
    }
}

/// Counts of what the feed has done, for the probe and for `/health`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Transactions folded.
    pub transactions: u64,
    /// Trades recorded.
    pub fills: u64,
    /// Coin movements with no pool payment found, not recorded.
    pub unpriced: u64,
    /// Transactions that failed on chain or arrived malformed.
    pub unreadable: u64,
    /// Transactions dropped because their block's time never arrived.
    pub untimed: u64,
    /// Coins dropped to stay inside the budget.
    pub evicted: u64,
    /// pump.fun launches seen.
    pub launches: u64,
}

/// A coin's activity over a window, for the coin list.
#[derive(Clone, Debug, PartialEq)]
pub struct Activity {
    /// The coin.
    pub mint: Address,
    /// Trades in the window.
    pub trades: u64,
    /// Coin traded.
    pub token_volume: f64,
    /// The candle quote asset.
    pub quote: Option<Address>,
    /// Quote traded.
    pub quote_volume: f64,
    /// The window's first price.
    pub open: Option<f64>,
    /// The window's last price.
    pub close: Option<f64>,
    /// The launch, if the feed saw it.
    pub launch: Option<Launch>,
}

/// One ranked holder.
#[derive(Clone, Debug, PartialEq)]
pub struct HolderRow {
    /// The owning wallet, or the token account when the owner is unknown.
    pub owner: Address,
    /// Balance, decimals-adjusted, summed over the owner's accounts.
    pub balance: f64,
    /// Whether this owner acted as a pool in a trade the feed saw.
    pub pool: bool,
}

/// A coin's holders, and how much of the truth they are.
#[derive(Clone, Debug, PartialEq)]
pub struct Holders {
    /// Richest first.
    pub rows: Vec<HolderRow>,
    /// Whether the feed saw this coin launch. If it did, every balance since
    /// the first has been seen and the list is complete; if not, only
    /// accounts that moved while watched are here.
    pub since_launch: bool,
    /// When watching this coin began, unix seconds.
    pub since: i64,
    /// Whether accounts were dropped for [`HOLDER_ACCOUNTS`].
    pub truncated: bool,
}

/// The live tape.
#[derive(Debug)]
pub struct Tape {
    coins: HashMap<Address, Coin>,
    /// `(last_active, mint)`, oldest first: the eviction order.
    by_activity: BTreeSet<(i64, Address)>,
    budget_bytes: usize,
    used_bytes: usize,
    newest: Option<i64>,
    counts: Counts,
}

#[expect(clippy::cast_precision_loss, reason = "display precision for a chart")]
fn adjusted(raw: u64, decimals: u8) -> f64 {
    raw as f64 / 10f64.powi(i32::from(decimals))
}

impl Tape {
    /// An empty tape that will hold at most `budget_bytes` (estimated).
    #[must_use]
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            coins: HashMap::new(),
            by_activity: BTreeSet::new(),
            budget_bytes,
            used_bytes: 0,
            newest: None,
            counts: Counts::default(),
        }
    }

    /// The newest block time applied, unix seconds. `None` before anything.
    #[must_use]
    pub const fn newest(&self) -> Option<i64> {
        self.newest
    }

    /// What has happened so far.
    #[must_use]
    pub const fn counts(&self) -> Counts {
        self.counts
    }

    /// Estimated bytes held.
    #[must_use]
    pub const fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    /// Coins held.
    #[must_use]
    pub fn coins(&self) -> usize {
        self.coins.len()
    }

    /// Notes a transaction that could not be folded.
    pub const fn note_unreadable(&mut self) {
        self.counts.unreadable += 1;
    }

    /// Notes transactions dropped because their block time never came.
    pub const fn note_untimed(&mut self, transactions: u64) {
        self.counts.untimed += transactions;
    }

    /// Applies one folded transaction, stamped with its block's time.
    pub fn apply(&mut self, slot: u64, signature: Signature, block_time: i64, decoded: Decoded) {
        self.counts.transactions += 1;
        self.counts.unpriced += decoded.unpriced as u64;
        self.newest = Some(self.newest.map_or(block_time, |n| n.max(block_time)));
        let stamp = radar_store::from_epoch(block_time);

        for launch in decoded.launches {
            self.counts.launches += 1;
            self.launch(launch, block_time);
        }
        for fill in decoded.fills {
            self.counts.fills += 1;
            self.fill(&fill, slot, signature, &stamp, block_time);
        }
        for holding in decoded.holdings {
            self.holding(&holding, block_time);
        }
        self.evict();
    }

    fn coin(&mut self, mint: Address, at: i64) -> &mut Coin {
        let coin = match self.coins.entry(mint) {
            Entry::Occupied(existing) => existing.into_mut(),
            Entry::Vacant(slot) => {
                self.by_activity.insert((at, mint));
                self.used_bytes += MINT_BYTES;
                slot.insert(Coin::new(at))
            }
        };
        if at > coin.last_active {
            self.by_activity.remove(&(coin.last_active, mint));
            coin.last_active = at;
            self.by_activity.insert((at, mint));
        }
        coin
    }

    fn launch(&mut self, launch: LaunchSeen, at: i64) {
        let coin = self.coin(launch.mint, at);
        coin.launch = Some(Launch {
            name: launch.name,
            symbol: launch.symbol,
            uri: launch.uri,
            creator: launch.creator,
            at,
        });
        // A launch is the start of the coin, so nothing before it is missing.
        coin.first_seen = coin.first_seen.min(at);
    }

    fn fill(&mut self, fill: &Fill, slot: u64, signature: Signature, stamp: &str, at: i64) {
        let (mut added, mut removed) = (0usize, 0usize);
        let coin = self.coin(fill.mint, at);

        if !coin.pools.contains(&fill.pool) {
            if coin.pools.len() == POOLS_PER_MINT {
                coin.pools.remove(0);
            }
            coin.pools.push(fill.pool);
        }

        coin.trades.push_back(MarketTrade {
            mint: fill.mint,
            ts: stamp.to_owned(),
            slot: Slot(slot),
            signature,
            side: fill.side,
            token_amount: fill.token_amount,
            quote_amount: Some(fill.quote_amount),
            quote_mint: Some(fill.quote_mint),
            price: Some(fill.price),
            trader: Some(fill.trader),
        });
        added += TRADE_BYTES;
        if coin.trades.len() > TRADES_PER_MINT {
            coin.trades.pop_front();
            removed += TRADE_BYTES;
        }

        let quote = *coin.quote.get_or_insert(fill.quote_mint);
        if quote == fill.quote_mint {
            let start = at.div_euclid(60) * 60;
            let newest_minute = coin.minutes.back().map(|m| m.start);
            if newest_minute.is_none_or(|newest| start > newest) {
                coin.minutes.push_back(Minute {
                    start,
                    open: fill.price,
                    high: fill.price,
                    low: fill.price,
                    close: fill.price,
                    quote_volume: fill.quote_amount,
                    token_volume: fill.token_amount,
                    trades: 1,
                });
                added += MINUTE_BYTES;
                if coin.minutes.len() > CANDLE_MINUTES {
                    coin.minutes.pop_front();
                    removed += MINUTE_BYTES;
                }
            } else if let Some(m) = coin.minutes.iter_mut().rev().find(|m| m.start == start) {
                // The newest minute, or an earlier one: the feed delivers slots
                // in commitment order, which can trail by a slot or two. A late
                // trade is folded into the minute it belongs to, and only the
                // newest minute's close moves, so a late fill never rewrites
                // where a finished candle ended.
                m.high = m.high.max(fill.price);
                m.low = m.low.min(fill.price);
                if Some(start) == newest_minute {
                    m.close = fill.price;
                }
                m.quote_volume += fill.quote_amount;
                m.token_volume += fill.token_amount;
                m.trades += 1;
            }
        }
        coin.bytes = (coin.bytes + added).saturating_sub(removed);
        self.used_bytes = (self.used_bytes + added).saturating_sub(removed);
    }

    fn holding(&mut self, holding: &Holding, at: i64) {
        let (mut added, mut removed) = (0usize, 0usize);
        let coin = self.coin(holding.mint, at);
        coin.decimals = Some(holding.decimals);
        if holding.amount == 0 {
            if coin.holders.remove(&holding.account).is_some() {
                removed += HOLDER_BYTES;
            }
        } else {
            let balance = HeldBalance {
                owner: holding.owner,
                amount: holding.amount,
            };
            if coin.holders.insert(holding.account, balance).is_none() {
                added += HOLDER_BYTES;
            }
            if coin.holders.len() > HOLDER_ACCOUNTS {
                if let Some(smallest) = coin
                    .holders
                    .iter()
                    .min_by_key(|(account, b)| (b.amount, **account))
                    .map(|(account, _)| *account)
                {
                    coin.holders.remove(&smallest);
                    removed += HOLDER_BYTES;
                }
                coin.holders_truncated = true;
            }
        }
        coin.bytes = (coin.bytes + added).saturating_sub(removed);
        self.used_bytes = (self.used_bytes + added).saturating_sub(removed);
    }

    /// Drops whole coins, least recently active first, until inside budget.
    fn evict(&mut self) {
        while self.used_bytes > self.budget_bytes {
            let Some(&(at, mint)) = self.by_activity.first() else {
                break;
            };
            self.by_activity.remove(&(at, mint));
            if let Some(coin) = self.coins.remove(&mint) {
                self.used_bytes = self.used_bytes.saturating_sub(coin.bytes);
                self.counts.evicted += 1;
            }
        }
    }

    /// A coin's trades with `from <= time < to`, newest first, and whether the
    /// window is complete: `false` when the kept trades start inside it,
    /// because older ones were dropped or happened before watching began.
    #[must_use]
    pub fn trades(&self, mint: &Address, from: i64, to: i64) -> (Vec<MarketTrade>, bool) {
        let Some(coin) = self.coins.get(mint) else {
            return (Vec::new(), false);
        };
        let (from_s, to_s) = (radar_store::from_epoch(from), radar_store::from_epoch(to));
        let trades: Vec<MarketTrade> = coin
            .trades
            .iter()
            .rev()
            .filter(|t| t.ts >= from_s && t.ts < to_s)
            .cloned()
            .collect();
        let dropped_inside = coin.trades.len() == TRADES_PER_MINT
            && coin.trades.front().is_some_and(|t| t.ts > from_s);
        let complete = !dropped_inside && complete_from(coin) <= from;
        (trades, complete)
    }

    /// A coin's minute candles with `from <= start < to`, oldest first, and
    /// whether the window is complete.
    #[must_use]
    pub fn minutes(&self, mint: &Address, from: i64, to: i64) -> (Vec<Minute>, bool) {
        let Some(coin) = self.coins.get(mint) else {
            return (Vec::new(), false);
        };
        let minutes = coin
            .minutes
            .iter()
            .filter(|m| m.start >= from && m.start < to)
            .copied()
            .collect();
        let dropped_inside = coin.minutes.len() == CANDLE_MINUTES
            && coin.minutes.front().is_some_and(|m| m.start > from);
        (minutes, !dropped_inside && complete_from(coin) <= from)
    }

    /// Every coin that traded in `from <= time < to`, from its minute candles.
    ///
    /// Minute resolution: a window edge that falls mid-minute counts that
    /// whole minute. The coin list ranks activity; a minute's slack at either
    /// end does not change a ranking anyone reads.
    #[must_use]
    pub fn active(&self, from: i64, to: i64) -> Vec<Activity> {
        let floor = from.div_euclid(60) * 60;
        self.coins
            .iter()
            .filter(|(_, c)| c.last_active >= floor)
            .filter_map(|(mint, coin)| {
                let window: Vec<&Minute> = coin
                    .minutes
                    .iter()
                    .filter(|m| m.start >= floor && m.start < to)
                    .collect();
                let first = window.first()?;
                let last = window.last()?;
                Some(Activity {
                    mint: *mint,
                    trades: window.iter().map(|m| m.trades).sum(),
                    token_volume: window.iter().map(|m| m.token_volume).sum(),
                    quote: coin.quote,
                    quote_volume: window.iter().map(|m| m.quote_volume).sum(),
                    open: Some(first.open),
                    close: Some(last.close),
                    launch: coin.launch.clone(),
                })
            })
            .collect()
    }

    /// The launch, if the feed saw it.
    #[must_use]
    pub fn launch_of(&self, mint: &Address) -> Option<&Launch> {
        self.coins.get(mint).and_then(|c| c.launch.as_ref())
    }

    /// The newest trade price in the coin's candle quote, with that quote.
    #[must_use]
    pub fn last_price(&self, mint: &Address) -> Option<(f64, Address)> {
        let coin = self.coins.get(mint)?;
        let quote = coin.quote?;
        coin.trades
            .iter()
            .rev()
            .find(|t| t.quote_mint == Some(quote))
            .and_then(|t| t.price.map(|p| (p, quote)))
    }

    /// A coin's holders by owning wallet, richest first, at most `limit`.
    #[must_use]
    pub fn holders(&self, mint: &Address, limit: usize) -> Option<Holders> {
        let coin = self.coins.get(mint)?;
        let decimals = coin.decimals?;
        let mut by_owner: BTreeMap<Address, u64> = BTreeMap::new();
        for (account, held) in &coin.holders {
            let owner = held.owner.unwrap_or(*account);
            let entry = by_owner.entry(owner).or_default();
            *entry = entry.saturating_add(held.amount);
        }
        let pools: HashSet<&Address> = coin.pools.iter().collect();
        let mut rows: Vec<HolderRow> = by_owner
            .into_iter()
            .map(|(owner, amount)| HolderRow {
                owner,
                balance: adjusted(amount, decimals),
                pool: pools.contains(&owner),
            })
            .collect();
        rows.sort_by(|a, b| b.balance.total_cmp(&a.balance).then_with(|| a.owner.cmp(&b.owner)));
        rows.truncate(limit);
        Some(Holders {
            rows,
            since_launch: coin.launch.is_some(),
            since: watched_from(coin),
            truncated: coin.holders_truncated,
        })
    }
}

/// When watching a coin began: its launch if the feed saw it, else its first
/// sighting.
fn watched_from(coin: &Coin) -> i64 {
    coin.launch.as_ref().map_or(coin.first_seen, |l| l.at.min(coin.first_seen))
}

/// The earliest window start this coin's record is complete from.
///
/// A coin the feed saw launch has nothing before its launch to miss, so any
/// window is complete however far back it reaches. Otherwise the record starts
/// when watching did.
fn complete_from(coin: &Coin) -> i64 {
    if coin.launch.is_some() {
        i64::MIN
    } else {
        coin.first_seen
    }
}

#[cfg(test)]
mod tests {
    use radar_store::MarketSide;

    use super::*;

    fn addr(n: u8) -> Address {
        Address::new([n; 32])
    }

    fn fill(mint: u8, price: f64, buy: bool) -> Fill {
        Fill {
            mint: addr(mint),
            side: if buy { MarketSide::Buy } else { MarketSide::Sell },
            token_amount: 10.0,
            quote_amount: 10.0 * price,
            quote_mint: crate::decode::WSOL,
            price,
            trader: addr(200),
            pool: addr(201),
        }
    }

    fn tx(fills: Vec<Fill>) -> Decoded {
        Decoded {
            fills,
            ..Decoded::default()
        }
    }

    /// A whole minute, so minute arithmetic in these tests reads plainly.
    const T0: i64 = 1_800_000_000;

    #[test]
    fn a_trade_lands_on_the_tape_and_in_its_minute() {
        let mut tape = Tape::new(usize::MAX);
        tape.apply(1, Signature::new([1; 64]), T0 + 5, tx(vec![fill(1, 2.0, true)]));
        tape.apply(2, Signature::new([2; 64]), T0 + 30, tx(vec![fill(1, 3.0, false)]));
        tape.apply(3, Signature::new([3; 64]), T0 + 61, tx(vec![fill(1, 1.0, true)]));

        let (trades, _) = tape.trades(&addr(1), T0, T0 + 120);
        assert_eq!(trades.len(), 3);
        assert_eq!(trades[0].price, Some(1.0), "newest first");

        let (minutes, _) = tape.minutes(&addr(1), T0, T0 + 120);
        assert_eq!(minutes.len(), 2);
        assert_eq!(
            (minutes[0].open, minutes[0].high, minutes[0].low, minutes[0].close),
            (2.0, 3.0, 2.0, 3.0)
        );
        assert_eq!(minutes[0].trades, 2);
        assert_eq!(minutes[1].start, T0 + 60);
    }

    #[test]
    fn the_window_is_half_open() {
        let mut tape = Tape::new(usize::MAX);
        tape.apply(1, Signature::new([1; 64]), T0, tx(vec![fill(1, 2.0, true)]));
        assert_eq!(tape.trades(&addr(1), T0, T0 + 1).0.len(), 1, "start included");
        assert_eq!(tape.trades(&addr(1), T0 - 1, T0).0.len(), 0, "end excluded");
    }

    #[test]
    fn a_window_reaching_before_watching_began_is_not_complete() {
        let mut tape = Tape::new(usize::MAX);
        tape.apply(1, Signature::new([1; 64]), T0 + 10, tx(vec![fill(1, 2.0, true)]));
        assert!(!tape.trades(&addr(1), T0, T0 + 60).1);
        assert!(tape.trades(&addr(1), T0 + 10, T0 + 60).1);
    }

    #[test]
    fn a_window_whose_oldest_trades_were_dropped_is_not_complete() {
        let mut tape = Tape::new(usize::MAX);
        for i in 0..=i64::try_from(TRADES_PER_MINT).unwrap() {
            tape.apply(1, Signature::new([1; 64]), T0 + i, tx(vec![fill(1, 2.0, true)]));
        }
        let (trades, complete) = tape.trades(&addr(1), T0, T0 + 10_000);
        assert_eq!(trades.len(), TRADES_PER_MINT);
        assert!(!complete, "the first trade was dropped, so the window is short");
    }

    #[test]
    fn a_trade_in_another_quote_goes_on_the_tape_but_not_the_chart() {
        let mut tape = Tape::new(usize::MAX);
        tape.apply(1, Signature::new([1; 64]), T0, tx(vec![fill(1, 2.0, true)]));
        let mut usdc = fill(1, 500.0, true);
        usdc.quote_mint = crate::decode::USDC.parse().unwrap();
        tape.apply(2, Signature::new([2; 64]), T0 + 1, tx(vec![usdc]));
        assert_eq!(tape.trades(&addr(1), T0, T0 + 60).0.len(), 2);
        let (minutes, _) = tape.minutes(&addr(1), T0, T0 + 60);
        assert!((minutes[0].high - 2.0).abs() < f64::EPSILON, "a USDC price never enters a SOL candle");
    }

    #[test]
    fn the_least_recently_active_coin_is_dropped_to_stay_inside_budget() {
        // Room for two coins with one trade each, plus one more trade.
        let budget = 2 * (MINT_BYTES + TRADE_BYTES + MINUTE_BYTES) + TRADE_BYTES + 1;
        let mut tape = Tape::new(budget);
        tape.apply(1, Signature::new([1; 64]), T0, tx(vec![fill(1, 1.0, true)]));
        tape.apply(2, Signature::new([2; 64]), T0 + 1, tx(vec![fill(2, 1.0, true)]));
        tape.apply(3, Signature::new([3; 64]), T0 + 2, tx(vec![fill(1, 1.0, true)]));
        tape.apply(4, Signature::new([4; 64]), T0 + 3, tx(vec![fill(3, 1.0, true)]));

        assert!(tape.used_bytes() <= budget);
        assert_eq!(tape.counts().evicted, 1);
        assert!(tape.trades(&addr(2), T0, T0 + 60).0.is_empty(), "coin 2 was idlest");
        assert!(!tape.trades(&addr(1), T0, T0 + 60).0.is_empty(), "coin 1 traded again");
        assert!(!tape.trades(&addr(3), T0, T0 + 60).0.is_empty());
    }

    #[test]
    fn holders_sum_by_owner_drop_closed_accounts_and_mark_the_pool() {
        let mut tape = Tape::new(usize::MAX);
        let holding = |account: u8, owner: u8, amount: u64| Holding {
            mint: addr(1),
            account: addr(account),
            owner: Some(addr(owner)),
            amount,
            decimals: 6,
        };
        let mut first = tx(vec![fill(1, 1.0, true)]);
        first.holdings = vec![
            holding(10, 200, 1_000_000),
            holding(11, 200, 2_000_000),
            holding(12, 201, 9_000_000),
            holding(13, 202, 500_000),
        ];
        tape.apply(1, Signature::new([1; 64]), T0, first);
        let mut second = tx(vec![]);
        second.holdings = vec![holding(13, 202, 0)];
        tape.apply(2, Signature::new([2; 64]), T0 + 1, second);

        let holders = tape.holders(&addr(1), 10).unwrap();
        let rows: Vec<(Address, f64, bool)> =
            holders.rows.iter().map(|r| (r.owner, r.balance, r.pool)).collect();
        assert_eq!(rows, vec![(addr(201), 9.0, true), (addr(200), 3.0, false)]);
        assert!(!holders.since_launch);
    }

    #[test]
    fn a_launch_names_the_coin_and_makes_its_record_complete_from_birth() {
        let mut tape = Tape::new(usize::MAX);
        let mut launch = tx(vec![]);
        launch.launches = vec![LaunchSeen {
            mint: addr(1),
            name: "Name".into(),
            symbol: "SYM".into(),
            uri: "u".into(),
            creator: addr(9),
        }];
        tape.apply(1, Signature::new([1; 64]), T0 + 30, launch);
        tape.apply(2, Signature::new([2; 64]), T0 + 40, tx(vec![fill(1, 1.0, true)]));

        assert_eq!(tape.launch_of(&addr(1)).map(|l| l.symbol.as_str()), Some("SYM"));
        // A window reaching back before the launch is still complete: there was
        // nothing to miss.
        assert!(tape.trades(&addr(1), T0, T0 + 60).1);
        assert_eq!(tape.holders(&addr(1), 10).map(|h| h.since_launch), None, "no balances yet");
    }

    #[test]
    fn the_coin_list_sums_a_window_of_minutes() {
        let mut tape = Tape::new(usize::MAX);
        tape.apply(1, Signature::new([1; 64]), T0, tx(vec![fill(1, 1.0, true)]));
        tape.apply(2, Signature::new([2; 64]), T0 + 70, tx(vec![fill(1, 4.0, true)]));
        tape.apply(3, Signature::new([3; 64]), T0 + 5, tx(vec![fill(2, 1.0, true)]));
        let mut active = tape.active(T0 + 60, T0 + 120);
        active.sort_by_key(|a| a.mint);
        assert_eq!(active.len(), 1, "coin 2 did not trade in the window");
        assert_eq!(active[0].trades, 1);
        assert_eq!((active[0].open, active[0].close), (Some(4.0), Some(4.0)));
    }
}
