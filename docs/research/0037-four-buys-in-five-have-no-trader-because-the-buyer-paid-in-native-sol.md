<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0037 — Four buys in five have no trader, because the buyer paid in native SOL

**Date:** 2026-09-20
**Status:** measured and explained. Of 100 consecutive live trades, the side is
resolved on **100**, the price is resolved on **100**, and the trader is
resolved on **20**. The 80 with no trader are exactly the 80 whose quote leg is
native SOL rather than wrapped SOL. This is not a sampling accident: the two
groups partition the sample perfectly, with no row on either diagonal.
**Source:** `/v1/market/trades/3B27GPDowS5vJWKw9jvhR4BhCXJGSFR5igKxyGqZpump`
on radar.heyvera.org, read 2026-09-20. 100 rows, 100 distinct signatures.
**Bears on:** [plan 0013](../plans/0013-the-terminal-find-look-track-trade.md)
Phase C item 3, which says to check whether the tape rows carry the trader
before building the per-wallet history view. They do, for one trade in five.

## The measurement

Cross-tabulating side, quote mint and trader presence over the 100 rows leaves
three cells and no others:

| count | side | quote mint | trader |
| ----: | ---- | ---------- | ------ |
| 80 | buy | `So1111…111` | absent |
| 19 | sell | `So1111…112` (wrapped SOL) | present |
| 1 | buy | `So1111…112` (wrapped SOL) | present |

## `So1111…111` is behaving as SOL, and the comment saying otherwise is wrong

`crates/radar-backfill/src/market/query.rs` says `So1111…111` is "a real,
active, unrelated mint", confirmed live on 2026-09-11, and that
`QUOTE_MINTS` "excludes it". Both halves mislead. `QUOTE_MINTS` is a list of
mints that are never the *subject* of a trade, and `trades_query` reuses that
same list as the set of acceptable *quote* assets — so a mint put in the list
to be excluded from one role is thereby admitted to the other.

That would be a serious defect if the mint were unrelated, because its amount
would set a price that is then rendered as a SOL price. It is not unrelated.
Within this one coin and one window, prices quoted in each spelling agree:

| quote mint | n | min | median | max |
| ---------- | -: | --- | ------ | --- |
| `So1111…111` | 80 | 3.550e-04 | 3.554e-04 | 3.555e-04 |
| `So1111…112` | 20 | 3.529e-04 | 3.532e-04 | 3.549e-04 |

Eighty rows landing within 0.15% of each other and within 0.6% of the
wrapped-SOL rows is not what an unrelated asset produces. Treat the prices as
sound and the comment as the thing to correct. The cheap confirmation, if
someone wants certainty rather than strong evidence, is one CryptoHouse query
for that mint's supply and decimals — but not while the allowance is as tight
as [0036](0036-the-hourly-consider-run-eats-the-whole-cryptohouse-allowance.md)
describes.

## Why the trader goes missing, in the code

`market/fold.rs::side_and_trader` takes the trader from `token_authority` on a
sell and from `quote_authority` on a buy. That asymmetry is correct, and it is
forced:

- On a **sell**, the person sending the coin is the trader, and
  `solana.token_transfers.authority` is the signer of the sending leg. So
  `ends.sender_authority` is the trader. This always works.
- On a **buy**, the person sending the coin is the *pool*. The trader is the
  receiver, and an SPL transfer row does not name the receiver's owner — only
  the receiving token account. `trades_query` sets `'' AS leg_authority` on
  every destination row for exactly this reason. So the buy has to find the
  trader on the other leg, as whoever paid the quote asset.
- That fallback works only when the quote leg **is an SPL transfer**. A buyer
  paying with wrapped SOL leaves a row, so the trader is found — that is the
  single buy in the table above. A buyer paying with native SOL leaves no row
  in `solana.token_transfers` at all, so `qmint.payer` is empty and
  `quote_authority` comes back blank.

Most retail pump.fun buys pay in native SOL. Hence four in five.

## What follows

1. **Correct the comment in `query.rs`.** As written it will send the next
   reader hunting a pricing bug that the numbers above rule out. This costs
   nothing and should not wait for the rest.
2. **Decide whether to recover the buyer, and accept that it costs rows.** Two
   candidates, neither free:
   - Join `tx_signature` against the transactions table and take the fee
     payer. On a retail buy the fee payer is the trader. This is one more
     table in the same HTTP request, so it spends no extra query against the
     120-per-hour allowance — but it scans more rows, and CryptoHouse's
     thousand-row cap is what turns one written query into several real ones.
     That cap is the scarcity 0036 just finished fixing, so this is not a
     free change.
   - Resolve the receiving token account to its owner. Correct in every case,
     including a buy routed through an aggregator, and strictly more expensive.
3. **Do not build Phase C item 3's per-wallet history on today's data.** A view
   that filters trades by the signed-in wallet would show that wallet its
   sells and hide four of its five buys, with nothing on screen to say a buy
   was dropped rather than never made. That is the failure rule 9 names: an
   absence rendering identically to a zero. Until the buyer is recoverable,
   either the view waits, or it says in its own words that it shows sells and
   the minority of buys paid in wrapped SOL.

The owner's decision is item 2, and it is not urgent: nothing is wrong on
screen today, and no data is being lost that a later query cannot recover,
because the trades sit in `solana.token_transfers` for as long as CryptoHouse
keeps them. That is the opposite of the launch recorder in 0036.
