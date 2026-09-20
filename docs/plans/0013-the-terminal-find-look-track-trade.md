<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0013 — The terminal: find, look, track, trade

**Status:** Phase A done except its one owner step. Merged on 2026-09-18: #244,
#245, #248, #243 and the patch-level updates #228, #250, #251, #252. Wallet
sign-in on the box is still `allowlist:` one address; opening it is the owner's
edit to `/etc/radar/radar.env`. Phase B item 1 merged as #257 and is live.
Two repairs landed on the way and are deployed: #258 (the market tape gives up a
gap it can never close) and #259 (market routes read a shared snapshot instead
of the whole trade table per request). Item 2 merged as #261 and is **not yet
deployed** -- the box's `radar-serve` restart needs an interactive sudo
password, so it is the owner's step. Item 3's "newly launched" list merged as #263
(`/v1/market/launches`, a "New" tab), also **not yet deployed**. That leaves
item 3's `SHORTLIST` widening as the only unbuilt part of Phase B, and it waits
on the owner. Nothing in C to E is built.
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
3. ~~Confirm on the box whether `RADAR_CUSTOMER_ACCESS=open` is set.~~
   **Done 2026-09-20.** It is already set in `/etc/radar/radar.env`; no sudo
   edit is needed. Note that `/health`'s `paidSurface` is a *different*
   switch — reading it does not tell you whether customer access is open.

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
3. **More than 10 coins, inside the budget.** The "newly launched" list
   straight from the launches table is built and merged (#263, no CryptoHouse
   cost at all). **The `SHORTLIST` widening is blocked on a decision, not on
   effort** — the measurement it was waiting for has been taken. The day of
   logs this item asked for was read on 2026-09-20 and
   there is no headroom to widen into: the hourly `consider` cron already
   spends the whole 120/hour allowance at minute 37, and the recorder is
   refused 91 times a day as a result —
   [0036](../research/0036-the-hourly-consider-run-eats-the-whole-cryptohouse-allowance.md).
   The batching this item proposes would not help either: the tape already
   batches and already holds itself to 72/hour. Widening waits on the owner
   deciding between a smaller `consider` and a larger allowance.

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
   `MarketTrades` by the signed-in wallet. **The check this item asked for is
   done, and the answer blocks the item:** the tape carries the trader on one
   trade in five. Every sell has one; only a buy paid in *wrapped* SOL has
   one, and most retail buys pay in native SOL, which leaves no row in
   `solana.token_transfers` for the query to read an authority from. Measured
   2026-09-20 at 20 of 100 live trades, partitioning perfectly — see
   [0037](../research/0037-four-buys-in-five-have-no-trader-because-the-buyer-paid-in-native-sol.md),
   **and unblocked by it.** Built naively, this view would show a wallet its
   sells, hide four of its five buys, and say nothing about the difference —
   a missing trade rendered exactly like a trade never made. The way through
   costs no CryptoHouse queries at all: a buy's `token_destination` is the
   buyer's token account, and
   `radar_pumpfun::pda::associated_token_account(wallet, mint, token_program)`
   derives that address locally from a wallet that is already signed in. So
   filter sells on `trader` and buys on the derived account. The one cost is
   that `market/fold.rs` currently discards `token_destination`, so the store
   needs a column for it — and it must be a new field, not `trader`, which a
   token account would turn into the placeholder its doc comment forbids. Say
   plainly that a buy routed through an unusual account may be missing. Coins
   we do not track say so in one sentence.
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

**Stopped at:** Phase A merged. Merging #244 after #246 put two dead links on
`main` (plan 0012 named `Scoreboard.tsx` and `PricePath.tsx`, which #246 had
deleted) and the link check failed every branch until #255 fixed it. The lesson:
this ruleset is `strict:false`, so a green PR can be stale; re-run CI on an old
docs PR before merging it.

**Outage, 2026-09-19, and what fixed it.** `radar.heyvera.org` stopped
answering: `curl` to `/` on the box timed out at 5 s with `radar-serve` at 171%
CPU. Every market request re-read and re-sorted the whole trade table (about
1,750 files, 1.5M rows). #259 builds one snapshot off the request path,
refreshed every 20 s (launches every 300 s), and routes read that. Measured on
build `1540b49` after `sudo radar-deploy`: `/v1/market/coins` 0.31 s on the box
and 0.68 s from outside; 117 CPU ticks in 60 s (6,000 is one core); 145 MB
resident. 3 of 19 listed coins carry a name; the rest predate the recorder.

**Checked since:** the CryptoHouse refusals did **not** stop after #258 raised
the follow idle to 90 s. 24 hours of logs, read 2026-09-20: the recorder was
refused 91 times, all of them between minute 40 and minute 59 of the hour, which
is the `consider` cron at minute 37 draining what is left —
[0036](../research/0036-the-hourly-consider-run-eats-the-whole-cryptohouse-allowance.md).

**Not checked:** a fresh wallet signing in.

**Rule from the owner, 2026-09-19:** tests run on GitHub CI, not on the
workstation. Locally: `cargo fmt` and scoped `cargo clippy` only.

**Item 2 is merged, 2026-09-20.** #261 (`feat/holders-from-the-tape`) went in
at `0ff1f9a`: `/v1/market/holders/{mint}` nets each wallet's buys minus sells
from the snapshot, fact `net_traded_in_window`. All 14 checks passed, mutation
shards included, and the branch was one commit behind `main` on a file it does
not touch (`radar-backfill/src/main.rs`), so the stale-green trap above did not
apply. It is merged and **not deployed**: installing `radar-serve` and
restarting it prompt for `guardian`'s sudo password, which cannot be run
unattended, and `deploy/README.md` calls that boundary a feature.

**Item 3's first half is merged, 2026-09-20.** #263 adds a public
`GET /v1/market/launches` and a "New" tab, read off the snapshot's launch index
at zero CryptoHouse cost. `LaunchInfo` now carries the launch **slot**: the
`Envelope` holds no wall-clock timestamp, so the ordering and the response's
own window are in slots (`from_slot`/`to_slot`), not the timestamp window the
trade-backed routes use. One change reached wider than the route: `web/src/api.ts`'s
`get()` now prefers the server's `message` over its `error` code when both are
present, because the two `Degradation::NotCollected` answers this route can
give -- an empty launch index, and a snapshot that could not be built -- share
one code by design, and the screen has to tell them apart. Every panel's failure
text now reads as a sentence rather than a code.

**Next action:** the owner deploys #261 and #263 together by the
`deploy/README.md` "Every deploy after that" procedure, then checks two things
on radar.heyvera.org: the holders tab on a traded coin, and the "New" tab. That
is the evidence Phase B is verified against the real site, not a local run.
Item 3's widening waits on a decision
that 0036 now frames with numbers: at 120 CryptoHouse queries an hour the box
cannot run all four units at the sizes currently asked for, so either
`consider` runs less often or over fewer candidates, or the allowance grows.
Separately and regardless of that answer, `consider` now **has** a declared
query ceiling -- ten queries a run, the same pattern `market_tape.rs` enforces
-- so it cannot spend an allowance it does not own, and a run that hits the
ceiling says so instead of reporting a short pass as a complete one (#266,
merged 2026-09-20). The "newly launched" list does not wait on any of this.

**The budget fix probably does not need the privileged half of the deploy, and
that is worth checking before scheduling one.** It ships in the `radar` binary,
which `deploy/README.md`'s table places in `~/bin` — an unprivileged
`install` -- while only `radar-serve` lives in `/usr/local/bin` and needs the
interactive `sudo`. `radar-serve` is what #261 and #263 change. If the
`consider` cron invokes `~/bin/radar`, the new binary takes effect on the next
run with no restart at all, because cron starts a fresh process every time.

**Both settled 2026-09-20.** The cron entry names `/home/guardian/bin/radar`,
the unprivileged path, so replacing that binary *is* the deploy and no restart
is involved. And `RADAR_CUSTOMER_ACCESS=open` was already set, so the owner
step Phase C was waiting on never existed.

**Do not:** build Phase D before Phase C is live; put a CryptoHouse query on a
request path; hold a key or a fee on the server side of a swap; start the
private trader or the edge measurement from this plan.
