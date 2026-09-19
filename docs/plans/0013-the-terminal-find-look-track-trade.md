<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0013 — The terminal: find, look, track, trade

**Status:** Phase A in progress. #244 and #245 merged on 2026-09-18; #248 has
main merged in and is waiting on CI; nothing in phases B to E is built.
**Date:** 2026-09-18.
**Branch:** none; each item lands on `main` as its own pull request.
**Inspected base:** `c8fca0e` (`Remove the bot from Radar: it lives in realorrug
now (#253)`) plus `feat/live-stream` at `fd01f40`, read on 2026-09-18.
**Builds on:** [plan 0012](0012-the-public-trading-panel.md), whose P0 to P2
landed as #246. This plan supersedes two of 0012's rules; both are marked there.
**Decided by:** the owner, 2026-09-18, recorded not argued: the terminal only;
free data; a buy/sell button signed by the visitor's own wallet, no custody, no
fee.

## Context

Radar is now two things: the free public trading terminal and the private
trader. The X bot left for realorrug (#253). Josh chose on 2026-09-18:

- next phases cover **the terminal only** (edge measurement and the private
  trader stay parked);
- plan on **free data** (CryptoHouse, 120 queries/hour per IP); the paid
  Yellowstone feed (#248) stays a one-day switch, not a dependency;
- the terminal **gets a buy/sell button**, signed by the visitor's own wallet.
  Radar never holds keys or funds. No fee.

What exists (main + #248): `/` and `/token/:mint` draw a coin list, candle
chart, trade tape, holders tab, info tab and token header
(`web/src/Terminal.tsx`). Five public routes under `/v1/market/*`
(`crates/radar-serve/src/market/mod.rs`). The collector
(`crates/radar-backfill/src/market_tape.rs`) tracks 10 coins every 300 s at
~80 queries/hour and writes only `MarketTrades`. Decision routes are
operator-only (`access.rs:652-680`). Old research pages are deleted.

What is missing, measured against Axiom's loop (find, look, trade, track):

| gap | where it shows |
|---|---|
| coin names/images are always `null` on the free path | `market/mod.rs:762-822` |
| holders refused on the free path | `market/mod.rs:843-859` |
| only 10 coins | `market_tape.rs:108` |
| nothing per-wallet: no watchlist, positions, history | no route in `radar-serve` |
| no buy/sell | no swap code in `web/src`; `radar-exec` route code is operator-only |
| everything polled at 15 s, no push | `Terminal.tsx:34` |
| plan 0012 (#244), live feed (#248), lag fix (#245), #243 sit unmerged | `gh pr list` |

Plan 0012 is the base. Two of its rules are now out of date and this plan
replaces them: "do not write a line of transaction construction" (Josh
reversed it today) and P3's "a wallet's trades from CryptoHouse on request"
(breaks the later rule that nothing on a request path may query CryptoHouse).

## Phase A — clear the deck (small, first)

1. Merge #244 (plan 0012 doc), #245 (follow lag measured), #243. Merge #248:
   CI is green, and with `RADAR_STREAM_ENDPOINT` unset it changes nothing in
   production. Retarget before merging; squash only (memory: stack-merge trap).
2. Batch the patch-level Dependabot PRs (#250, #251, #252, #228). Leave the
   major bumps (#195, #196, #208–#210): vite 8 is blocked on
   `@tailwindcss/vite` per plan 0003's handback.
3. Confirm on the box whether `RADAR_CUSTOMER_ACCESS=open` is set. If not, it
   is Josh's one sudo env edit; hand him the exact line from `deploy/README.md`.

Done when: `gh pr list` shows only the major-bump PRs, and a fresh wallet can
sign in on radar.heyvera.org.

## Phase B — the free screen looks real (find + look)

All three items cost **zero** extra CryptoHouse queries on a request path.

1. **Names and images from our own launch records.** `radar-follow` already
   stores each launch's `name`, `symbol`, `uri` (`radar-store/src/event.rs:233-237`).
   Join them into `/v1/market/coins` and `/v1/market/token/{mint}` from a map
   built at the store's watermark, not per request. The image lives behind
   `uri` (off-chain JSON): the **browser** fetches it, never the server.
   Coins launched before recording began keep the honest `metadata_reason`.
2. **Holders folded from the tape we already have.** Net each trader's buys
   minus sells over `MarketTrades`. Label it what it is ("net position of
   wallets seen trading here since <time>"), per 0012's holders section.
   Reuse the shape `market/live.rs:243` already returns so the screen does
   not care which source answered.
3. **More than 10 coins, inside the budget.** First measure: run the tape at
   today's load for a day and read the quota headroom from the coverage
   table. Then widen `SHORTLIST` by batching several mints into one query
   (`IN (...)`) instead of one query per coin. Hold total under 80/hour; the
   recorder and the hourly outcomes cron share the IP. Add a "newly launched"
   list straight from the launches table (no CryptoHouse cost at all).

Done when: a screenshot with no wallet shows named coins with images, a
holders tab with rows, and more than 10 coins; `radar brief` on the box shows
CryptoHouse queries/hour still under 80.

## Phase C — track (per-wallet, isolated)

Build 0012's P3 as written for the **storage** half, and change the **read**
half:

1. `Tenant` + `TenantStore` and the watchlist, exactly per 0012 task
   9-11-0011 (per-wallet directory from the verified address; the seven-item
   rubric; independent review, because it is the isolation boundary).
2. **Positions: the browser reads its own balances** from a public Solana RPC
   (`getTokenAccountsByOwner`). The request leaves from the visitor's IP, so
   our quota is untouched, and isolation is free: nobody can ask for another
   wallet's view through us because we never serve one. Price each holding
   from `/v1/market/token`.
3. **History: from the store only.** "Your trades in this coin" filters
   `MarketTrades` by the signed-in wallet. Check first whether the tape rows
   carry the trader (0012 says backfilled rows do not; the live decoder
   does). If they do not, add the column in the collector before building the
   view. Coins we do not track say so in one sentence.
4. The private column of the screen, with 0012's distinct empty sentences
   ("no trades yet" must differ from "could not look").

Done when: two wallets in one transcript each see their own watchlist and
neither sees the other's, by address guess, id guess, no session, and expired
session: four refusals, four different messages.

## Phase D — trade (visitor-signed swap)

1. **ADR first**: Radar builds, the visitor's wallet signs and sends. No
   custody, no fee, no server-side key. Names what Radar will never do.
2. Quote: a public, rate-limited `GET /v1/market/quote` backed by the existing
   `radar-exec` router code (`crates/radar-exec/src/route.rs:243-590`), which
   already quotes Jupiter both ways or refuses (#231). Per-IP limit so a
   visitor cannot burn Jupiter's allowance for everyone.
3. Swap: the browser asks Jupiter for the unsigned transaction, shows the
   visitor exactly what they pay, receive, and the worst-case price, and only
   then calls the wallet's sign-and-send. Extend `Wallet.tsx`; sign-in code in
   `siws.ts` stays sign-in only.
4. Guard rails on the screen: slippage cap with a sane default, a refusal
   when the quote is stale or the coin has no route, the round-trip cost shown
   as a cost (`honesty.ts` already separates cost from gain).
5. **Independent review before it ships**: it touches other people's money.
   Rehearse on a throwaway wallet with a few cents; Josh does the signing.

Done when: a recorded session shows a real small buy and sell from the screen,
with the transaction ids, and the position from Phase C updating after each.

## Phase E — feels live (after people can use it)

1. Server push (one SSE stream per coin) replacing the 15 s poll; 0012's P5.
2. Chart tools; 0012's P6.
3. The paid feed switch stays where it is: `deploy/README.md` § "The live
   feed", purchase-day runbook. Revisit when visitors exist.

## Order and what can run side by side

A → B → C → D → E. Inside B the three items are independent. C.1 (storage)
and B can overlap. D.1 (the ADR) can be written during C. Nothing in D ships
before C is live, because a trade nobody can see afterwards is half a product.

## Not in this plan

The edge measurement (plan 0007 items 3–4), the private trader (plan 0011,
designs 0017/0018), `Policy::CLOSED`, the paid feed purchase, any fee, any
second chain.

## Verification, every phase

- `just check` and `just web` green; raise `MIN_WEB_TESTS` with each web
  change. Scoped cargo only on this PC; never `cargo mutants` locally — CI
  runs it `--in-diff`, so test every line touched.
- Each rubric proved by re-applying the wrong behaviour and watching a named
  test fail (repo convention).
- Each phase ends with evidence from the real site, not a local run: a
  screenshot or transcript from radar.heyvera.org after deploy
  (`ssh guardian-vps-tail "sudo radar-deploy"`), because merging and
  deploying are different acts here.

## Handback

**Stopped at:** Phase A, item 1. #244 and #245 are merged. #248 conflicted with
#253 (the bot's crates left the workspace); main was merged into the branch,
keeping the removals and keeping `radar-stream` and its probe, and
`cargo check -p radar-serve -p radar-stream --all-targets` passed locally.

**Next action:** merge #248 once CI is green, then #243 (it conflicts with main
and needs the same treatment), then the patch-level Dependabot batch.

**Do not:** build Phase D before Phase C is live; put a CryptoHouse query on a
request path; hold a key or a fee on the server side of a swap; start the
private trader or the edge measurement from this plan.
