// SPDX-License-Identifier: Apache-2.0
//! The SQL this module sends to CryptoHouse.
//!
//! Split from [`super::fold`] on the same principle [`radar_backfill::extract`]
//! already uses: everything here is a pure function from parameters to a query
//! string, so it is tested without a network, and everything that turns a row
//! into a domain type lives on the other side of the split where a fixture can
//! exercise it.
//!
//! # The technique, and the bugs in the sketch it came from
//!
//! A swap on any venue is one transaction in which the target mint moves and a
//! quote asset — wrapped SOL, USDC or USDT — moves too. The sketch this was
//! built from matched wrapped SOL with `LIKE 'So1111...%'`, which also matches
//! `So11111111111111111111111111111111111111111` — a real, active, unrelated
//! mint one character longer than wrapped SOL, confirmed live against
//! CryptoHouse on 2026-09-11. [`radar_backfill::extract::QUOTE_MINTS`] already
//! excludes it by exact match, for the same reason, so the quote set here is
//! that list rather than a pattern.
//!
//! It also summed values that had not been divided by `decimals`, dropped a
//! trade outright when the quote leg's join found nothing, and looked only at
//! wrapped SOL. Every one of those is fixed below: the query joins with a
//! **`LEFT JOIN`**, so a transaction whose quote leg is missing is not
//! discarded — [`super::fold`] is the layer that turns "no leg" into a `None`
//! price rather than a zero one — the quote side accepts any of the three
//! assets, and every raw amount keeps its `decimals` beside it so nothing is
//! displayed before it is adjusted.

use radar_backfill::extract::QUOTE_MINTS;

/// The quote mints, as a SQL `IN (...)` list.
///
/// Exact match, never `LIKE`. See the module comment for the mint a pattern
/// match would have pulled in.
fn quote_list() -> String {
    QUOTE_MINTS
        .iter()
        .map(|m| format!("'{m}'"))
        .collect::<Vec<_>>()
        .join(",")
}

/// One transaction's token leg outer-joined against its quote leg, over a
/// bounded time window.
///
/// `LEFT JOIN`: a transaction that moved `mint` but has no matching row in the
/// quote CTE still appears, with every `quote_*` column coming back as
/// ClickHouse's default for its type (`''` for the mints and authorities,
/// `'0'` for the summed amounts) rather than being dropped. [`super::fold`]
/// treats an empty `quote_mint` as "no quote leg found", never as a quote leg
/// worth zero.
///
/// Bounded on `block_timestamp` at both ends, which is what the table is
/// partitioned by — an unbounded `mint` filter alone scans the whole table
/// (measured at 16.42 billion rows for one mint's lifetime) and is refused by
/// the endpoint outright.
#[must_use]
pub fn trades_query(mint: &str, from: &str, to: &str) -> String {
    format!(
        "WITH t AS (\
           SELECT block_timestamp, block_slot, tx_signature, \
                  sum(value) AS token_value, any(decimals) AS token_decimals, \
                  any(source) AS token_source, any(destination) AS token_destination, \
                  any(authority) AS token_authority \
           FROM solana.token_transfers \
           WHERE mint = '{mint}' AND block_timestamp >= '{from}' AND block_timestamp < '{to}' \
           GROUP BY block_timestamp, block_slot, tx_signature\
         ), s AS (\
           SELECT tx_signature, sum(value) AS quote_value, any(decimals) AS quote_decimals, \
                  any(mint) AS quote_mint, any(authority) AS quote_authority \
           FROM solana.token_transfers \
           WHERE mint IN ({quotes}) AND block_timestamp >= '{from}' AND block_timestamp < '{to}' \
           GROUP BY tx_signature\
         ) \
         SELECT toString(t.block_timestamp) AS ts, toString(t.block_slot) AS slot, \
                t.tx_signature AS sig, \
                toString(t.token_value) AS token_value, toString(t.token_decimals) AS token_decimals, \
                t.token_source AS token_source, t.token_destination AS token_destination, \
                t.token_authority AS token_authority, \
                toString(s.quote_value) AS quote_value, toString(s.quote_decimals) AS quote_decimals, \
                s.quote_mint AS quote_mint, s.quote_authority AS quote_authority \
         FROM t LEFT JOIN s ON t.tx_signature = s.tx_signature \
         ORDER BY t.block_timestamp DESC",
        quotes = quote_list()
    )
}

/// Raw transfers for one mint, quote legs excluded, over a bounded window —
/// what [`super::fold::fold_holders`] folds into balances.
///
/// Includes `MintTo`, `Burn` and their checked variants so supply is
/// conserved rather than only ever growing: a fold that counted transfers
/// alone would show a burned balance as still held.
#[must_use]
pub fn holder_transfers_query(mint: &str, from: &str, to: &str) -> String {
    format!(
        "SELECT toString(block_timestamp) AS ts, source, destination, \
                toString(value) AS value, toString(decimals) AS decimals, transfer_type \
         FROM solana.token_transfers \
         WHERE mint = '{mint}' AND block_timestamp >= '{from}' AND block_timestamp < '{to}' \
         ORDER BY block_timestamp ASC"
    )
}

/// Which mints traded at all in a window, ranked by transaction count.
///
/// One row per mint however many transfers the window holds — the aggregate
/// is cheap the same way [`radar_backfill::prices`] is, because the cost is
/// the window scan and barely depends on how many mints come back. This is
/// the shortlist [`coin_prices_query`] then prices; pricing every mint that
/// moved without first ranking them would ask CryptoHouse for as many
/// candle-style joins as there are mints in the window, most of which nobody
/// asked to see.
#[must_use]
pub fn coin_candidates_query(from: &str, to: &str, limit: usize) -> String {
    format!(
        "SELECT mint, toString(count(DISTINCT tx_signature)) AS tx_count, \
                toString(sum(value)) AS token_volume, \
                toString(any(decimals)) AS token_decimals \
         FROM solana.token_transfers \
         WHERE block_timestamp >= '{from}' AND block_timestamp < '{to}' \
           AND mint NOT IN ({quotes}) \
         GROUP BY mint \
         ORDER BY tx_count DESC \
         LIMIT {limit}",
        quotes = quote_list()
    )
}

/// The first and last priced fill for each of a shortlist of mints.
///
/// Aggregated all the way down to one row per mint with `argMin`/`argMax`
/// over time, so the per-trade rows the two CTEs build are never part of the
/// result the row cap counts — only the final `GROUP BY mint` is. `mints`
/// must already be shortlisted: this is quadratic in nothing, but a query
/// naming every mint that ever moved would be a very long `IN (...)` for no
/// reason [`coin_candidates_query`] does not already serve.
///
/// # Panics
///
/// Never on `mints` content — every entry has already been through
/// [`radar_types::Address::from_str`] by the caller, so it cannot carry a
/// quote or a statement terminator. Panics only if `mints` is empty, which is
/// a caller bug: an empty `IN ()` is invalid SQL and asking CryptoHouse to
/// reject it would waste a round trip finding that out.
#[must_use]
pub fn coin_prices_query(mints: &[String], from: &str, to: &str) -> String {
    assert!(!mints.is_empty(), "coin_prices_query needs a shortlist");
    let list = mints
        .iter()
        .map(|m| format!("'{m}'"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "WITH t AS (\
           SELECT mint, tx_signature, block_timestamp AS ts, \
                  sum(value) AS token_value, any(decimals) AS token_decimals \
           FROM solana.token_transfers \
           WHERE mint IN ({list}) AND block_timestamp >= '{from}' AND block_timestamp < '{to}' \
           GROUP BY mint, tx_signature, ts\
         ), s AS (\
           SELECT tx_signature, sum(value) AS quote_value, any(decimals) AS quote_decimals, \
                  any(mint) AS quote_mint \
           FROM solana.token_transfers \
           WHERE mint IN ({quotes}) AND block_timestamp >= '{from}' AND block_timestamp < '{to}' \
           GROUP BY tx_signature\
         ), joined AS (\
           SELECT t.mint AS mint, t.ts AS ts, t.token_value AS token_value, \
                  t.token_decimals AS token_decimals, s.quote_value AS quote_value, \
                  s.quote_decimals AS quote_decimals, s.quote_mint AS quote_mint \
           FROM t INNER JOIN s ON t.tx_signature = s.tx_signature\
         ) \
         SELECT mint, toString(count()) AS trade_count, \
                toString(argMin(token_value, ts)) AS first_token_value, \
                toString(argMin(token_decimals, ts)) AS first_token_decimals, \
                toString(argMin(quote_value, ts)) AS first_quote_value, \
                toString(argMin(quote_decimals, ts)) AS first_quote_decimals, \
                toString(argMax(token_value, ts)) AS last_token_value, \
                toString(argMax(token_decimals, ts)) AS last_token_decimals, \
                toString(argMax(quote_value, ts)) AS last_quote_value, \
                toString(argMax(quote_decimals, ts)) AS last_quote_decimals, \
                any(quote_mint) AS quote_mint, \
                toString(sum(quote_value)) AS quote_volume, \
                toString(any(quote_decimals)) AS quote_volume_decimals \
         FROM joined \
         GROUP BY mint",
        quotes = quote_list()
    )
}

/// The earliest published metadata for a mint — name, symbol, creator.
///
/// `solana.tokens` carries no `decimals` column; that comes back from
/// [`trades_query`] or [`holder_transfers_query`] instead, when either has run
/// recently enough to have seen the mint. Unbounded on time deliberately:
/// metadata is published once, near a token's creation, and a window a caller
/// would have to guess defeats the purpose of asking for it by mint. Measured
/// against the live endpoint on 2026-09-11 at 0.6-3s for a single `mint`
/// lookup — the table is 30 million rows, not the multi-billion-row transfer
/// log the other queries are bounded against.
#[must_use]
pub fn token_metadata_query(mint: &str) -> String {
    format!(
        "SELECT toString(block_timestamp) AS ts, name, symbol, uri, creators, update_authority \
         FROM solana.tokens WHERE mint = '{mint}' ORDER BY block_timestamp ASC LIMIT 1"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINT: &str = "5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump";

    #[test]
    fn the_quote_side_is_an_exact_list_never_a_pattern() {
        let sql = trades_query(MINT, "2026-09-11 17:00:00", "2026-09-11 17:05:00");
        // The bug: `LIKE 'So1111...%'` matches this real, unrelated mint too
        // -- one character longer than wrapped SOL, confirmed live.
        assert!(!sql.contains("LIKE"), "must not pattern-match the quote leg");
        for quote in QUOTE_MINTS {
            assert!(sql.contains(quote), "missing quote mint {quote}: {sql}");
        }
    }

    #[test]
    fn the_trade_query_left_joins_so_a_missing_quote_leg_is_not_dropped() {
        let sql = trades_query(MINT, "2026-09-11 17:00:00", "2026-09-11 17:05:00");
        assert!(
            sql.contains("LEFT JOIN"),
            "an INNER JOIN silently drops a transaction with no quote leg: {sql}"
        );
        assert!(sql.contains(MINT));
        assert!(sql.contains("2026-09-11 17:00:00"));
        assert!(sql.contains("2026-09-11 17:05:00"));
        // Decimals travel with every raw amount, never assumed.
        assert!(sql.contains("token_decimals"));
        assert!(sql.contains("quote_decimals"));
    }

    #[test]
    fn the_holder_query_keeps_mints_and_burns_so_supply_is_conserved() {
        let sql = holder_transfers_query(MINT, "2026-09-11 00:00:00", "2026-09-11 01:00:00");
        // No transfer_type filter: excluding MintTo or Burn here would show a
        // burned balance as still held.
        assert!(!sql.contains("transfer_type="), "{sql}");
        assert!(sql.contains("transfer_type"));
    }

    #[test]
    fn the_candidate_query_excludes_the_quote_mints_themselves() {
        let sql = coin_candidates_query("2026-09-11 17:00:00", "2026-09-11 17:05:00", 50);
        assert!(sql.contains("NOT IN"));
        for quote in QUOTE_MINTS {
            assert!(sql.contains(quote));
        }
        assert!(sql.contains("LIMIT 50"));
    }

    #[test]
    #[should_panic(expected = "shortlist")]
    fn pricing_an_empty_shortlist_is_a_caller_bug_not_a_query() {
        let _ = coin_prices_query(&[], "2026-09-11 17:00:00", "2026-09-11 17:05:00");
    }

    #[test]
    fn the_price_query_aggregates_down_to_one_row_per_mint() {
        let sql = coin_prices_query(
            &[MINT.to_owned()],
            "2026-09-11 17:00:00",
            "2026-09-11 17:05:00",
        );
        assert!(sql.contains("GROUP BY mint"));
        assert!(sql.contains("argMin"));
        assert!(sql.contains("argMax"));
        assert!(sql.contains(MINT));
    }

    #[test]
    fn the_metadata_query_has_no_time_bound() {
        // Deliberately unbounded -- see the doc comment for why this table is
        // the one exception, and the measurement backing it.
        let sql = token_metadata_query(MINT);
        assert!(!sql.contains("block_timestamp >="));
        assert!(sql.contains(MINT));
        assert!(sql.contains("LIMIT 1"));
    }
}
