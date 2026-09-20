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
- That fallback needs the quote leg to have a **source** row, because
  `qmint.payer` is `argMin(authority, net)` and only a source row carries an
  authority. A buyer paying with wrapped SOL sends it from a token account of
  their own, so there is a source row and the trader is found — that is the
  single buy in the table above.

Most retail pump.fun buys pay in SOL that never sits in a token account of the
buyer's. Hence four in five.

**What is proved, and what is inferred.** Proved: the quote leg of those 80
buys produces a correct price, so rows for `...111` exist in
`solana.token_transfers` — an earlier draft of this document said they do not,
which the prices above refute. Proved: `quote_authority` is blank on all of
them. Inferred, and the two candidates are not distinguished by anything
measured here:

- the transaction has only the **destination** row for the quote mint — the
  pool receiving — and `trades_query` hardcodes `'' AS leg_authority` on every
  destination row, so `argMin` has nothing but a blank to pick; or
- a source row exists and CryptoHouse leaves `authority` empty on it.

Both lead to the same place: **the trader is not on the quote leg of these
buys, and no rearrangement of this query puts it there.** The choice between
them only changes which fix is cheapest, and one query for the raw transfer
rows of a single signature would settle it before anyone spends effort.

## What follows

**Decided 2026-09-20**, the owner having handed both calls over.

1. **Correct the comment in `query.rs`.** Done in the same change. As written
   it would send the next reader hunting a pricing bug the numbers rule out.
2. **Recover the buyer by deriving the token account, not by widening the
   query.** This reverses the recommendation the first draft of this document
   made, and the reason is that the first draft missed something already in
   the tree. On a buy, `ends.receiver` — selected as `token_destination` —
   *is* the buyer's token account. It is not a wallet, so it cannot answer
   "who traded this". But the per-wallet view does not need that direction. It
   starts from a wallet that is already signed in, and
   `radar_pumpfun::pda::associated_token_account(owner, mint, token_program)`
   turns a wallet and a mint into exactly that address, locally, for free.
   Match it against `token_destination` and the buy is the signed-in wallet's.

   So the two candidates the first draft weighed — joining the transactions
   table for the fee payer, or resolving the token account to its owner — are
   both dropped. Each spends rows against the CryptoHouse allowance that 0036
   just finished protecting, and neither is needed.

   What it costs instead: `token_destination` is computed in `market/fold.rs`
   and then thrown away, so the store must keep it. That is a column, not a
   query. It is *not* the `trader` field — a token account written where a
   wallet is expected is the placeholder that field's own doc comment forbids.

   What it does not cover: a buyer whose receiving account is not the derived
   associated one. Rare for retail, real for some routers, and the view must
   say so rather than let those trades vanish quietly.
3. **Phase C item 3 is unblocked by point 2, and is the next thing to build.**
   The view filters on `trader == wallet` for sells and the derived token
   account for buys, and states in its own words that a trade routed through
   an unusual account may be missing. Sells alone, silently, remains the one
   thing it must not do.

The remaining open question is not in this document. It is
[0036](0036-the-hourly-consider-run-eats-the-whole-cryptohouse-allowance.md)
point 2, the size of the allowance, and the half of it that costs money stays
with the owner.
