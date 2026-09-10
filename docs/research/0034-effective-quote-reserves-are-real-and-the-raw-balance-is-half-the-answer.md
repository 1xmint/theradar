<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0034 — Effective quote reserves are real, and the raw balance is half the answer

**Date:** 2026-09-10
**Status:** measured. The PumpSwap constant product runs on
`quote_vault_balance + virtual_quote_reserves`, confirmed against two accepted
mainnet buys to within one part in thirty million. Pricing off the raw vault
balance alone is wrong by a factor of **2.1** on the pool tested.
**Chain:** three transactions on pool
`C4mLt6fs2dL2W1oovZAT9QpM3tpL6CA7DZ8hqHU9Ldqb`, read with `getTransaction` at
slots 445,487,551 and 445,487,727 on 2026-09-10 via the public endpoint. No
credential, no paid source.
**Builds on:** [0033](0033-the-pumpswap-pool-account-has-eight-lengths.md), which
proved the field is sixteen bytes and signed, and
[0028](0028-the-fee-after-graduation-is-a-ladder.md).

## The question this closes

The vendor says: *"Use effective quote reserves (not the raw quote-vault token
balance) wherever you quote, price, or index a pool."* It does not say what
effective means. Until now that made every PumpSwap price a guess, and this
vendor's documentation has already been wrong twice about this exact program —
it claimed `virtual_quote_reserves` is zero on all pools (it is not), and implied
pools quote only in SOL (five of ten captured quote in something else).

So the reading was taken from behaviour, not from the sentence.

## The formula

For a buy, with every quantity read from the swap's own event log:

```
base_out = (B_post + base_out) x quote_in / (Q_post + V)
```

where `B_post` and `Q_post` are the pool's base and quote reserves **after** the
trade, `quote_in` is the quote amount in, and `V` is `virtual_quote_reserves`.

Two things in that are worth stating separately, because either alone would
produce a plausible wrong number.

**The reserves in the event are post-trade.** The pre-trade base reserve is
recovered by adding the output back. Reading them as pre-trade gives an answer
that is close enough to look right and is not.

**The quote side adds `V`.** That is what "effective" means. Nothing subtracts,
and `V` does not move when trades do — 0033 observed the same value at two slots
6,066 apart.

## Shown working

| | trade A | trade B |
|---|---:|---:|
| signature | `4U2CKHsA…` | `5tBNczMH…` |
| observed `base_amount_out` | 319,155,070,204 | 359,919,974,734 |
| **predicted, effective (`Q+V`)** | **319,155,065,960** | **359,919,963,994** |
| ratio to observed | **1.000000** | **1.000000** |
| predicted, raw balance only | 673,153,819,575 | 758,566,432,976 |
| ratio to observed | 2.109 | 2.108 |

The residual is 4,244 and 10,740 respectively — about one part in 30 million,
positive in both cases, which is what integer truncation in the arithmetic above
produces. It is not a modelling gap.

The raw-balance model is not slightly wrong. It is wrong by more than double, in
the direction that would have Radar expect twice the tokens it actually receives.

## The numbers behind the table

Trade A: `B_post` 533,344,254,484,778 · `Q_post` 15,853,682,783 ·
`quote_in` 19,997,562 · `V` 17,584,505,289.
Trade B: `B_post` 532,984,334,510,044 · `Q_post` 15,876,258,798 ·
`quote_in` 22,580,532 · `V` 17,584,505,289.

On this pool `V` is 111% of the raw quote balance, which is why the error is so
large. A pool with `V = 0` would price identically under both models, and that is
the trap: **a decoder tested only against `V = 0` pools looks correct.**

## Where the field lives in the event

The buy event carries discriminator `3e2f370aa503dc2a` and is **417 bytes**.
`V` is the `u64` at **offset 392** — the appended tail the vendor describes, and
its value matches the pool account's own field exactly. Offsets used above, all
`u64` little-endian: base out at 16, base reserve at 48, quote reserve at 56,
quote in at 64.

**Events have more than one shape, exactly as the account does.** A third
transaction on the same pool carried discriminator `d67ef2d1bd81d1df` at **256
bytes**, with no field at offset 392 to read. A decoder that assumes 417 bytes
panics or reads garbage there. 0033 found eight account lengths; this is the same
lesson on the event side, and it was found by a decoder attempt failing loudly
rather than quietly.

## What Radar must refuse

- **A pool whose event shape is unknown.** Refuse by discriminator and length,
  never read past the end.
- **A quote asset with no dollar price.** Six of ten captured pools quote in
  something other than SOL, one in USDC, and five in other SPL mints including
  another `…pump` token. Effective reserves give a price *in the quote asset*.
  Turning that into dollars needs a chain of quotes, and each link is a way to be
  wrong. An unvaluable holding is unknown, not zero.
- **Any mint carrying a Token-2022 extension that changes transfer amounts.**
  `crates/radar-pumpfun/src/token.rs` already refuses six by name.

## The fees, added 2026-09-10

The section below replaces this document's original "fees not settled" note. Same
two trades, all eight fee-bearing fields read out and checked.

**Fees are outside the product.** The value at offset 64 enters the constant
product untouched — that is what makes the match exact — and the fee amounts are
computed from it separately. A quoter that deducts a fee before applying the
curve gets a smaller answer than the chain gives.

**Two rates, both stored in the event, both rounding up.**

| | rate | rate at | amount at | trade A | trade B |
|---|---:|---|---|---:|---:|
| liquidity fee | 93 bps | offset 88 | offset 96 | 185,978 | 209,999 |
| creator fee | 30 bps | offset 344 | offset 352 | 59,993 | 67,742 |

Both amounts reproduce exactly as `ceil(input x rate / 10000)` on both trades.
Rounding **up** — the same direction 0028 found in the fee ladder, and the
direction that costs the taker rather than the pool.

And the two sum to a difference already present in the record:

```
offset 96 + offset 352  ==  offset 104 - offset 112
245,971 == 245,971   (trade A)
277,741 == 277,741   (trade B)
```

Exact on both. That identity is what ties the fee fields to the pair at 104 and
112, whatever those two are named.

## What is still not established

**Offset 384** is 46.50 basis points of the pool input on both trades — exactly
half the liquidity rate on trade A, and one unit off half on trade B, which is
rounding. It is consistent with a protocol share taken out of the liquidity fee,
and that is a guess, not a reading. **Offset 376 is 5,000 on both trades** and is
not a rate of the input at all. Neither is reconciled and neither is needed to
quote.

Nothing here names offsets 104 and 112. The identity above constrains their
difference and says nothing about what either one is.

The 2026-07-20 program upgrade report remains unverified. Both trades here are
from after it, so they say nothing about how older data should be read — and a
price computed with today's reading may be wrong on older pools.
