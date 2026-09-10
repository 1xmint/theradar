<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0033 — The PumpSwap pool account has eight lengths, and five of them are the field boundaries

**Date:** 2026-09-09
**Chain:** ten `Pool` accounts read with `getAccountInfo` at slots 445,767,146
through 445,767,268; a census of every account owned by
`pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` carrying the `Pool` discriminator,
read with `getProgramAccounts` at about slot 445,766,000; the buy
`5HxVtAB7…eTxZ8WG` re-read with `getTransaction`
**Capture:** [`pumpswap_pools.json`](../../crates/radar-pumpfun/tests/fixtures/pumpswap_pools.json),
asserted by [`the_pool_layout_is_what_mainnet_holds.rs`](../../crates/radar-pumpfun/tests/the_pool_layout_is_what_mainnet_holds.rs)
**Status:** measured. The `Pool` layout is now read from bytes rather than from
the vendor's README, and the two disagree in three places. No price, no impact
and no capacity are established here; the reserves are not in this account.

## Why this was looked at

[Research 0028](0028-the-fee-after-graduation-is-a-ladder.md) §"What this does
not establish" recorded that "the pool accounts were read for a stored
high-water mark and none was recognised in the 58 bytes after `coin_creator`".
The vendor has since published field names covering eighteen of those bytes. The
costing at 9-9-0014 then made the pool layout the one piece where a capture had
to dispose, because the same vendor's IDL has twice been short about this same
program family (LEARNINGS 25) and a plausible-but-wrong field offset produces
confident wrong prices rather than an error.

## Finding 1 — eight lengths, and five of them are the field boundaries

`getProgramAccounts` over the PumpSwap program, filtered to the `Pool`
discriminator and counted by the `space` the RPC reports, returned **1,374,123**
accounts at these lengths:

| bytes | accounts | what the field order says it holds |
|---|---|---|
| 211 | 37,625 | through `lp_supply` |
| 243 | 74,457 | `+ coin_creator` |
| 244 | 40,647 | `+ is_mayhem_mode` |
| 245 | 101,699 | `+ is_cashback_coin` |
| 261 | 10,813 | `+ virtual_quote_reserves` |
| 270 | 268 | the above and nine zero bytes |
| 300 | 507,226 | the above and thirty-nine zero bytes |
| 301 | 601,388 | the above and forty zero bytes |

Five of the eight land **exactly** on the cumulative field boundaries of the
vendor's published order. That is not a coincidence a wrong layout produces, and
it is the strongest evidence in this note. The program's own `extend_account`
instruction is the mechanism: a pool is grown when it needs the newer fields, so
the shorter accounts are pools that have not been.

**It also settles a field width the README does not state.** `245 + 16 = 261`
and **no account is 253 bytes long**, so `virtual_quote_reserves` is sixteen
bytes wide, not eight. The IDL agrees — it declares the type `i128` — and this
is the rare case where the chain confirms the reference rather than contradicting
it. A `u64` reading would have produced the same number on every pool observed
today, because the top eight bytes are zero on all 601,357 accounts of length
301 that were counted, and would have been wrong the first time one was not.

The consequence for the parser is that the layout is a **prefix ladder**. A
211-byte pool is a real pool with no `coin_creator`, which is a different fact
from a `coin_creator` of all zeroes — and both occur, one capture of each. Rule
9's shape: the later fields are `Option`, and a length that stops *inside* a
field is refused rather than half-read.

## Finding 2 — three places the capture disagrees with the README

1. **`virtual_quote_reserves` is not zero.** The vendor's README says the value
   is "currently zero across all pools". Two of the six captures long enough to
   hold the field carry 17,584,505,289 and 17,584,505,417; the other four carry
   zero. Whatever the field means, it is read and not assumed. The IDL's own
   comment — "for non-boost pools, value is 0" — is consistent with what was
   seen and the README is not.
2. **The two token programs are not in this account.** The documented field
   order accounts for every byte through 261 and the rest is zero, so
   `base_token_program` and `quote_token_program` are what the instruction
   account list already said they were: accounts a caller passes, being the
   owner programs of the two mints. They must be read from the mints, and the
   capture records which each pool uses.
3. **A non-SOL quote mint exists, and it is common.** The vendor's
   `pump-public-docs` README said on 2026-09-09 that "no quote mint other than native SOL can be used",
   while the venue's fee page said USDC has been live since 2026-05-21. Pool
   `82zcJ16FYLuqbjxbdHKbD3F7YigdhBe6YHTTvsErNHB` quotes in
   `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`, which is USDC. **Six of the
   ten captures quote in something other than SOL** — the rest in `pump` and
   other mints. **The fee page is right and the README is wrong.** Design 0010's
   claims table carries a row reading "USDC-quoted pump.fun coins … treat as
   unverified"; it is verified now, and that row is not this task's to edit.

## Finding 3 — the two token programs are neither equal nor constant

Across the ten captures, read from each mint's owner:

| | SPL Token | Token-2022 |
|---|---|---|
| base side | 8 pools | 2 pools |
| quote side | 7 pools | 3 pools |

**Five of the ten** use a different program on each side, and which side carries
which varies: `C4mLt6fs…` is Token-2022 base against an SPL Token quote, and
`6xsdRpzd…` is the reverse. Token-2022's transfer-fee extension means the amount
that reaches a vault is not the amount in the instruction, so a quote that
assumed either side would be wrong by a fee. They are recorded as two separate
facts per pool and nothing here assumes they match.

## Finding 4 — the discriminator, captured then derived

Every one of the 1,374,123 accounts begins `f1 9a 6d 04 11 b1 6d bc`. That is
also `sha256("account:Pool")[..8]`, which the test recomputes rather than
trusting. So Anchor's account rule holds here — recorded as confirmed, in the
order the evidence arrived: captured first, derived second.

## What this does not establish

- **Any price.** The reserves are the balances of the two token accounts this
  struct names, and no SPL or Token-2022 token-account parser exists in this
  repository. The vendor's rule is
  `effective_quote_reserves = quote vault balance + virtual_quote_reserves`, and
  only the second term is in hand.
- **What `virtual_quote_reserves` means.** The two captures that carry a
  non-zero value differ by 128, on pools whose LP supplies differ by four
  thousandths of a percent and whose sizes are not alike; two further pools
  sampled but not fixtured carried 17,584,505,288 and 17,584,505,289. A
  per-pool virtual reserve would not naively cluster like that. The field is
  read; it is not understood, and nothing prices against it.
- **The 40 trailing bytes.** Zero in every capture, and the parser refuses a
  non-zero byte there rather than ignoring it. Why the program allocates them is
  unknown.
- **Whether the length ladder is complete.** Eight lengths is a census taken at
  one moment. A ninth would be refused as a `PartialField` if it landed inside a
  field and accepted if it landed on a boundary with zero padding.
- **The instruction table.** Still derived from the vendor's IDL names
  (`radar_decode::pumpswap`), still uncaptured. Unchanged by this note.
- **Plan 0011 P1.** Design 0017 §3 puts the curve and PumpSwap on one row. This
  does not satisfy "two pool families"; Raydium CPMM is what counts it.

## Reproducing

Read-only against the public endpoint, no credential, nothing signed.
[`scripts/probe/capture_pumpswap_pools.py`](../../scripts/probe/capture_pumpswap_pools.py)
rewrites the fixture. The census was this call, counted by `space`:

```bash
curl -s https://api.mainnet-beta.solana.com -H 'content-type: application/json' -d \
  '{"jsonrpc":"2.0","id":1,"method":"getProgramAccounts","params":[
     "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA",
     {"encoding":"base64","dataSlice":{"offset":0,"length":0},
      "filters":[{"memcmp":{"offset":0,"bytes":"hQrXeCntzbV"}}]}]}'
```
