// SPDX-License-Identifier: Apache-2.0
//! The market-tape collector: spends the CryptoHouse budget so `radar-serve`
//! does not have to.
//!
//! # The problem this exists to close
//!
//! CryptoHouse permits 120 queries an hour per IP, shared with `radar-follow`
//! and the hourly `--outcomes` cron on the same box. `radar-serve`'s market
//! routes used to query CryptoHouse live, once or more per HTTP request —
//! measured at 291 queries in one hour from a handful of terminal loads, well
//! past the quota, and every subsequent request failed with `QUOTA_EXCEEDED`
//! until the hour rolled over. No cache TTL fixes this: the coin list alone
//! costs two queries, so a one-minute refresh consumes the whole hourly
//! budget by itself, before a single visitor asks for anything else.
//!
//! # The fix, and its arithmetic
//!
//! Flip the direction. A collector spends the budget on a fixed schedule and
//! writes what it finds to the store; `radar-serve` reads only the store and
//! issues zero CryptoHouse queries on any request path (`radar_serve::market`).
//!
//! Two queries per pass — [`super::market::query::coin_candidates_query`] for
//! the window's active mints, then one batched
//! [`super::market::query::trades_query`] for all of them — at one pass every
//! [`PASS_INTERVAL`] (two minutes) is **60 queries an hour**, comfortably
//! inside the 120/hour quota with headroom for `radar-follow` and the hourly
//! `--outcomes` cron that share the same IP. The loop sleeps a fixed
//! [`PASS_INTERVAL`] between passes regardless of how long a pass took, so a
//! slow network round trip can only widen the interval, never narrow it below
//! what the arithmetic assumes.
//!
//! # Its own cursor
//!
//! The follow cursor (`radar_store::cursor::CURSOR_FILE`) is one file per
//! store, and two writers of one file fight over it — `radar-follow` already
//! owns it. This collector never touches that file: it keeps its own cursor
//! in [`SCOPE_DIR`], a subdirectory `radar_store::Reader`/`Writer` never look
//! inside, using the same atomic read/write the follow cursor uses so a torn
//! write here is exactly as safe as it is there.
//!
//! # What a covered window can honestly claim
//!
//! Each pass ranks mints by recent activity and only asks for trades on that
//! shortlist — the same design the live `/v1/market/coins` endpoint always
//! used, just run once and stored rather than run per request. A mint outside
//! the window's activity cut has no row here, and that is **not** the same
//! fact as "this mint had zero trades": it means the shortlist did not reach
//! it. The [`Coverage`](radar_store::Coverage) record this collector writes
//! says the window's shortlist was scanned; it does not attest every mint on
//! Solana. `radar_backfill::coverage`'s own doc comment makes the same kind of
//! disclosure for pump.fun being the store's only venue, and this is that
//! caveat's shape applied to a rank cut instead of a venue.

use radar_store::{Completion, Coverage, MarketSide, MarketTrade, ObservedSlots, Table};
use radar_types::{Address, Signature, Slot};

use crate::market::fold;

/// The decoder that produced the rows -- this crate's own version, the same
/// convention `crate::coverage::DECODER_VERSION` uses.
const DECODER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// This collector's coverage source.
pub const SOURCE: &str = "cryptohouse:market:tape";

/// The subdirectory this collector's own cursor lives under, so it can never
/// collide with `radar-follow`'s `.follow-cursor` at the store root.
pub const SCOPE_DIR: &str = "market-tape";

/// Groups a batched query's rows by mint and folds each group on its own.
///
/// **Grouping has to happen before folding, not inside it.**
/// [`fold::fold_tape`]'s pool detection assumes every row it sees belongs to
/// one mint's activity — mixing two mints' transfers into one call would let
/// one mint's pool compete with another's for the same frequency count, and
/// [`super::market::query::trades_query`] batches exactly that many mints into
/// one result set to fit the query budget. So this is the seam that undoes the
/// batching before the existing, unmodified fold sees it.
///
/// A row whose `mint`, `signature` or `slot` does not parse is dropped rather
/// than guessed at, the same discipline [`crate::extract::events_from_rows`]
/// applies to a chain event — those three are never null by construction in
/// the stored schema, so a bad value in one is not a fact this collector may
/// invent. `quote_mint` and `trader` are optional and a parse failure there is
/// read as absent, never as a reason to drop an otherwise-good trade.
#[must_use]
pub fn fold_market_trades(rows: &[fold::TapeRow]) -> Vec<MarketTrade> {
    let mut by_mint: std::collections::BTreeMap<&str, Vec<fold::TapeRow>> =
        std::collections::BTreeMap::new();
    for row in rows {
        by_mint
            .entry(row.mint.as_str())
            .or_default()
            .push(row.clone());
    }

    let mut out = Vec::new();
    for (mint, group) in by_mint {
        let Ok(mint) = mint.parse::<Address>() else {
            continue;
        };
        for trade in fold::fold_tape(&group) {
            let Ok(signature) = trade.signature.parse::<Signature>() else {
                continue;
            };
            out.push(MarketTrade {
                mint,
                ts: trade.ts,
                slot: Slot(trade.slot),
                signature,
                side: match trade.side {
                    fold::Side::Buy => MarketSide::Buy,
                    fold::Side::Sell => MarketSide::Sell,
                    fold::Side::Unknown => MarketSide::Unknown,
                },
                token_amount: trade.token_amount,
                quote_amount: trade.quote_amount,
                quote_mint: trade.quote_mint.as_deref().and_then(|m| m.parse().ok()),
                price: trade.price,
                trader: trade.trader.as_deref().and_then(|a| a.parse().ok()),
            });
        }
    }
    out
}

/// The highest slot among a pass's trades, or `None` when it collected none.
///
/// The market-tape analogue of `crate::coverage::highest_slot`, kept separate
/// because it walks `&[MarketTrade]` rather than `&[Event]` -- there is no
/// envelope here to read a slot off through a shared accessor.
#[must_use]
pub fn highest_slot(trades: &[MarketTrade]) -> Option<Slot> {
    trades.iter().map(|t| t.slot).max()
}

/// The one coverage record a pass writes.
///
/// One record, not one per mint: the window's shortlist is a single scan of
/// `Table::MarketTrades`, the same way a lifecycle window is a single scan of
/// launches. `filter` stays `None` — see the module doc comment for exactly
/// what that can and cannot be read to claim.
#[must_use]
pub fn coverage_record(status: Completion, trades: &[MarketTrade], recorded_at: Slot) -> Coverage {
    Coverage {
        recorded_at,
        table: Table::MarketTrades,
        filter: None,
        observed: ObservedSlots::over(trades.iter().map(|t| t.slot)),
        source: SOURCE.to_owned(),
        decoder_version: DECODER_VERSION.to_owned(),
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `seed` becomes a valid base58 signature via `Signature::new`, since
    /// `fold_market_trades` parses `sig` and a placeholder string like `"a1"`
    /// is not valid base58 -- it would be dropped as malformed, silently
    /// emptying every test that used one.
    fn row(mint: &str, seed: u8, quote_mint: &str, quote_value: &str) -> fold::TapeRow {
        fold::TapeRow {
            mint: mint.to_owned(),
            ts: "2026-09-11 17:35:00.000000".to_owned(),
            slot: "441251921".to_owned(),
            sig: Signature::new([seed; 64]).to_string(),
            token_value: "56626".to_owned(),
            token_decimals: "6".to_owned(),
            token_source: "POOL111111111111111111111111111111111111".to_owned(),
            token_destination: "TRADER11111111111111111111111111111111111".to_owned(),
            token_authority: "POOLAUTH1111111111111111111111111111111111".to_owned(),
            quote_value: quote_value.to_owned(),
            quote_decimals: "9".to_owned(),
            quote_mint: quote_mint.to_owned(),
            quote_authority: "TRADERWALLET111111111111111111111111111111".to_owned(),
        }
    }

    const MINT_A: &str = "5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump";
    const MINT_B: &str = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM";
    const SOL: &str = "So11111111111111111111111111111111111111112";

    #[test]
    fn a_batched_result_is_split_by_mint_before_folding() {
        // Re-applying the bug this seam exists to avoid: folding the combined
        // rows in one call would let mint A's pool and mint B's pool compete
        // for the same frequency count, since `detect_pool` has no idea two
        // mints are present.
        let rows = vec![row(MINT_A, 1, SOL, "10000"), row(MINT_B, 2, SOL, "20000")];
        let trades = fold_market_trades(&rows);
        assert_eq!(trades.len(), 2);
        let mints: std::collections::BTreeSet<String> =
            trades.iter().map(|t| t.mint.to_string()).collect();
        assert!(mints.contains(MINT_A));
        assert!(mints.contains(MINT_B));
    }

    #[test]
    fn each_mints_pool_is_detected_from_its_own_rows_only() {
        // Three trades against the same pool for mint A, one lone trade for
        // mint B. If the two mints' rows were folded together, mint A's pool
        // account would dominate the frequency count across both, and mint
        // B's single trade -- which alone cannot establish a pool -- would
        // wrongly inherit a side from an account it never actually traded
        // against.
        let mut a_rows: Vec<fold::TapeRow> = (0..3u8)
            .map(|i| {
                let mut r = row(MINT_A, i, SOL, "10000");
                r.token_destination = format!("TRADER-{i}");
                r
            })
            .collect();
        a_rows.push(row(MINT_B, 200, SOL, "5000"));

        let trades = fold_market_trades(&a_rows);
        let a_trades: Vec<_> = trades
            .iter()
            .filter(|t| t.mint.to_string() == MINT_A)
            .collect();
        let b_trades: Vec<_> = trades
            .iter()
            .filter(|t| t.mint.to_string() == MINT_B)
            .collect();
        assert_eq!(a_trades.len(), 3);
        assert!(
            a_trades.iter().all(|t| t.side == MarketSide::Buy),
            "mint A's own pool is an unambiguous plurality of its own rows"
        );
        assert_eq!(b_trades.len(), 1);
        assert_eq!(
            b_trades[0].side,
            MarketSide::Unknown,
            "a single trade cannot establish a pool on its own"
        );
    }

    #[test]
    fn a_row_naming_an_unparseable_mint_is_dropped_not_guessed() {
        let rows = vec![row("not-a-real-mint", 1, SOL, "10000")];
        assert!(fold_market_trades(&rows).is_empty());
    }

    #[test]
    fn a_quoteless_trade_still_survives_the_grouping_seam() {
        let rows = vec![row(MINT_A, 1, "", "0")];
        let trades = fold_market_trades(&rows);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].quote_amount, None);
        assert_eq!(trades[0].price, None);
    }

    #[test]
    fn a_pass_that_collects_nothing_records_no_slot_at_all() {
        let record = coverage_record(Completion::Complete, &[], Slot(500));
        assert_eq!(
            record.observed,
            ObservedSlots::Nothing,
            "a window that ran and found nothing must be distinguishable from one nobody ran"
        );
        assert_eq!(record.status, Completion::Complete);
        assert_eq!(record.table, Table::MarketTrades);
        assert_eq!(record.filter, None);
    }

    #[test]
    fn a_pass_that_collects_something_spans_exactly_its_own_slots() {
        let rows = vec![row(MINT_A, 1, SOL, "10000"), row(MINT_B, 2, SOL, "20000")];
        let mut trades = fold_market_trades(&rows);
        // Force distinct slots so the span is not vacuously a single point,
        // found by mint rather than by index so this does not depend on
        // whatever internal order grouping happens to produce.
        for t in &mut trades {
            t.slot = if t.mint.to_string() == MINT_A {
                Slot(100)
            } else {
                Slot(400)
            };
        }
        assert_eq!(highest_slot(&trades), Some(Slot(400)));
        let record = coverage_record(Completion::Complete, &trades, Slot(500));
        assert_eq!(
            record.observed,
            ObservedSlots::Span {
                from: Slot(100),
                to: Slot(400)
            }
        );
    }
}
