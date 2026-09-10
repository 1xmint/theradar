<!-- SPDX-License-Identifier: Apache-2.0 -->
# Captured Jupiter Router responses

Real bodies from `https://api.jup.ag/swap/v2/build`, captured on **2026-09-09**
with `curl`. They are here because AGENTS §1 says a reference proposes and a
capture disposes, and the Router's published TypeScript type disagreed with the
wire in three places — see below.

Committed **whole and unedited**. A trimmed capture is a description of a
capture, and the fields that would be trimmed first (`addressesByLookupTableAddress`,
the instruction account lists) are the ones carrying the fact this task turned
on: every route Jupiter returns uses address lookup tables.

## No credential was used or is present

Every capture was taken **keyless**, except `jupiter-build-401-unauthorized.json`,
which was taken with the literal string `not-a-real-key-0000`. No
`RADAR_JUPITER_API_KEY` exists on this machine and none was read, sent or
recorded. Jupiter answers keyless requests at a lower rate limit, which is
exactly why `radar_exec::route::Credentials` refuses to fall back to it: the
fallback would work, quietly.

| File | Request | Answer |
| --- | --- | --- |
| `jupiter-build-sol-usdc.json` | wSOL → USDC, 100000000 (0.1 SOL), `slippageBps=100` | 200; 5 venues, **5 lookup tables** |
| `jupiter-build-usdc-sol.json` | USDC → wSOL, 10000000 (10 USDC), `slippageBps=100&maxAccounts=20` | 200; 1 venue, **1 lookup table** |
| `jupiter-build-400-no-routes.json` | wSOL → an unlisted mint | 400 `{"error":"No routes found"}` |
| `jupiter-build-401-unauthorized.json` | wSOL → USDC with a bad key | 401 `{"code":401,"message":"Unauthorized"}` |

The `taker` on every capture is `CjfBjFVBs6QRvRTpMdKTBxZ7PZuJvHXWQKGRvR7wFbdz`,
an address Radar holds no key for. `/build` needs a taker to select
account-specific setup instructions; it does not need, and was not given, any
authority over one.

## Where the wire disagreed with the documentation

1. The documented `BuildResponse` type omits **`priceImpactPct`**. It is present
   on the wire, as a long-precision decimal *string*
   (`"0.0000320221058740248142431269"`), and as the string `"0"` when there is
   none — never a JSON number.
2. `blockhashWithMetadata` carries a third key, **`fetchedAt`**, shaped
   `{secs_since_epoch, nanos_since_epoch}`; `blockhash` is a **byte array**, not
   a base58 string.
3. `routePlan[].percent` is a **float** (`38.2`), alongside the documented
   integer `bps` (`3820`).

## What is not here

There is no capture of a legacy transaction, because `/build` has no
`asLegacyTransaction` parameter and returns no transaction at all. Reducing
`maxAccounts` to 20 did not remove the lookup tables either. That is the
measurement behind `route.rs`'s refusal to implement `Routing::build_buy`.

## Re-capturing

One request per second at most — the free tier's limit — and never with a real
key, because the result is committed to a public repository.
