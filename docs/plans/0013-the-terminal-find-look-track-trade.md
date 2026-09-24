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
on the owner. **Both of those are now deployed** -- `/health` reported build
`2d7f91d` on 2026-09-23, which is #266 and so carries #261 and #263 with it.
Phase C item 3 (#271) and item 4's trades half (#283, which replaced #272) are
merged and **deployed** -- `/health` reports build `871c6ce` since 2026-09-23.
C item 1 (#285, the watchlist) is merged and **deployed** -- `/health` reports
build `7554bb8` since 2026-09-23. Item 4's watchlist star and panel (#287) and
item 2, positions (#288, then the #289 fix), are merged and **deployed** --
`/health` reports build `d0aba33` since 2026-09-24. **Phase C is built and
live.** Nothing in D or E is built.
**Date:** 2026-09-18, handback extended 2026-09-23.
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
2. **Positions: the server reads the signed-in wallet's own balances.**
   Redesigned 2026-09-24, replacing the original "the browser reads its own
   balances" plan above: measured 2026-09-24, the public Solana node refuses
   every request that carries a browser Origin header --
   `curl -H "Origin: https://radar.heyvera.org" https://api.mainnet-beta.solana.com`
   with `getBalance` or `getTokenAccountsByOwner` answers
   `{"error":{"code":403,"message":"Access forbidden"}}`; the same request
   with no Origin succeeds; publicnode answers "Request blocked"; drpc wants a
   paid plan. So the server reads instead, behind `Tenant` exactly like the
   watchlist (no address, path, or query parameter -- a query string is
   refused `unscoped`, same as the watchlist): native SOL plus both the Token
   and Token-2022 programs' `getTokenAccountsByOwner`, all three or none (a
   partial read is refused as `unreadable_chain`, never a partial list dressed
   as complete). Priced from `/v1/market/token`'s own pricing function, not
   the HTTP route; an untracked mint is `priced: false` with a null price,
   never 0. A per-wallet cache (30 s TTL, 2048 wallets, keyed only by the
   verified address), a global cap (60 RPC calls/minute across every wallet,
   reserved 3 at a time per view), and -- fixed 2026-09-24 after review found
   the global cap alone let one busy wallet exhaust the whole minute's budget
   -- a per-wallet cap of its own (at most 2 fresh reads, 6 calls, per wallet
   per rolling 60 s, refused the same `503 busy` over it; both windows are
   checked before either is charged, so a refused read spends nothing) keep
   one visitor's reads from starving another's, since this quota is now Radar's own rather
   than free on each visitor's IP.
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
3. Swap: **amended 2026-09-24 by
   [ADR 0024](../adr/0024-radar-builds-a-visitors-swap-and-only-their-wallet-signs-it.md)**
   -- the browser cannot ask Jupiter (fee, key, and CSP, each on record), so
   Radar's server builds the unsigned v0 transaction from Jupiter's `/build`
   for the signed-in wallet only, behind `Tenant`, for every coin Jupiter
   routes. The browser shows the visitor exactly what they pay, receive, and
   the worst-case price, and only then calls the wallet's sign-and-send.
   Extend `Wallet.tsx`; sign-in code in `siws.ts` stays sign-in only. The
   button ships off behind a server switch until the owner approves a drafted
   terms page and notice.
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


**Phase C item 3 is built and green, 2026-09-20 to 09-23, unmerged.**
[#271](https://github.com/1xmint/theradar/pull/271) (`per-wallet-trade-history`,
base `main`) adds a public `GET /v1/market/history/{mint}?wallet=`. It filters
the snapshot's trades **forward** from the wallet: the tape names the trader on
only one trade in five (research 0037, measured 2026-09-20), so for the rest it
derives the wallet's own associated token account for the mint and matches buys
that were paid into it. It never inverts an account back to an owner, which
that derivation cannot do. Every row says which of the two happened,
`matched_by: "trader"` or `"receiving_account"`, and whatever still matches
neither comes back as a count, `unattributable_trades`, never as silence. The
route sits under `/v1/market/`, so it is `Audience::Public` and identity-free
by the prefix match at `access.rs:632`; the wallet is a query parameter, not a
session, and the route reads no customer store.

CI named four surviving mutants on the first push and `7f8e8a8` killed all
four: two unit tests in `market/mod.rs` (the two `Matched::as_str` words, and
the boundary where a list exactly filling the limit is **not** truncated) and
a new end-to-end test file,
[`crates/radar-serve/tests/wallet_history_from_the_tape.rs`](../../crates/radar-serve/tests/wallet_history_from_the_tape.rs),
because the not-collected gate lives in the handler rather than the fold. That
last test asserts the message contains `"collector"`: all three of the route's
`NotCollected` answers share one status and one `error` code, so only the
sentence tells them apart. Proved by re-applying all three bugs and watching
exactly those four tests fail. Run 35548436825, all 14 checks green.

**Phase C item 4's trades half is built and green, 2026-09-23, unmerged.**
[#272](https://github.com/1xmint/theradar/pull/272) (`your-trades-panel`) is
stacked on #271 and must be retargeted to `main` after #271 lands. It adds a
"Yours" tab to the terminal. Item 4 as the plan writes it needs items 1 and 2
-- the watchlist and the browser-read positions -- and neither exists, so this
is the trades column only, which is what item 3 unblocked.

Five ways for it to be empty, five different sentences, which is the whole
point of the item: not signed in, none of yours, this coin has no recorded
trades at all, Radar could not look, and Radar could not be reached. Four of
those are facts about Radar rather than about the reader's trading.
`yourTradesRefusal` in `web/src/honesty.ts` reads the server's own message to
tell the three `not_collected` refusals apart, exactly as `launchesEmptyMessage`
already does. The load-bearing case is zero rows with a non-zero
`unattributable_trades`: that reads "none of the trades Radar could attribute
are yours, but N of this coin name nobody at all", never "you made no trades".
Rule 9 on a screen, and the reason this item is worth its own line in the plan.

Two pieces landed outside the panel. `useWalletAddress` in `web/src/Wallet.tsx`
is a `useSyncExternalStore` over the stored session returning the address
string only -- the browser fires `storage` for *other* tabs, so signing in here
told this tab's panels nothing, hence the custom event. It deliberately does
not return the bearer token: nothing needing a wallet address needs the
credential with it, and handing both to every caller is how a public request
quietly starts carrying one. `transactionUrl` in `web/src/format.ts` is new
because `explorerUrl` builds an `/account/` link, and a signature given to that
page renders "not found", which reads as though the trade never happened.

Verified by re-applying four bugs: collapsing the five empty sentences into one
(9 tests failed), linking a signature through the account page (1), dropping
the unattributable warning beside a full list (1), and rendering both match
kinds with the same word (1). Restored, `Tests 124 passed (124)` and `tsc -b`
clean; `MIN_WEB_TESTS` raised 113 to 124. All 14 checks green on both PRs as of
2026-09-23.

**The earlier "next action" is done and is superseded.** #261 and #263 are
deployed. What is live is four merged commits behind `main`: #267, #268, #269
and #270. Two of those change what runs: #269 touches the collector's query
(`radar-backfill/src/market/query.rs`) and `radar-serve/src/customer.rs`, and
#270 adds to the `radar consider` CLI. #267 and #268 are documents only.

**That next action is done, 2026-09-23.** #271 squash-merged as `6705d17`.
Deleting its branch closed #272 automatically, and GitHub will not reopen a
pull request whose base branch is gone, so the same two commits were rebased
onto `main` and opened as #283, which merged as `871c6ce`. The release build
for that commit was installed by `sudo radar-deploy`; `/health` reports
`871c6ce` and the artifact's sha256 matched `BUILD-INFO.txt` on the box.

**Verified against the real site**, which is what "Verification, every phase"
asks for. `/v1/market/history/{mint}` answers JSON rather than the single-page
app's shell, so the panel now gets refusals it has words for: a mint outside
the snapshot returns 503 `not_collected` with a message, and a collected mint
(`SKRbvo6...`) returns 200 with no trades for the wallet and
`unattributable_trades: 15` -- the case the panel must not report as "you made
no trades". The shipped interface bundle contains the panel's own strings, so
the tab is live. Not checked: nobody has driven the tab in a browser while
signed in with a wallet that actually traded one of these coins.

**Still the owner's, unchanged:** whether to pay for more CryptoHouse
allowance, which 0036 frames with numbers. The recommendation on the table is
to read one day of `radar-follow` refusal counts first (`radar/outcomes.log`
and `radar/decisions.log`); that clock started 2026-09-20, so a day of it
exists now.

**Phase C item 1 is built, 2026-09-23, reviewed, merged and deployed.** The
owner decided the one question this item raised, on 2026-09-23: **nobody reads
a wallet's saved state through Radar but that wallet** -- the operator
included. There is no operator read path, so there is no second way past the
boundary.

What exists: `GET /v1/customer/watchlist`, and `PUT` / `DELETE
/v1/customer/watchlist/{mint}`, in
[`watchlist.rs`](../../crates/radar-serve/src/watchlist.rs), all behind a
`Tenant` from
[`tenant.rs`](../../crates/radar-serve/src/tenant.rs). A `Tenant` is made only
by verifying a wallet session token; the guard puts one on a request only after
the session verified *and* the wallet was admitted. A `TenantStore` is made only
from a `Tenant` and has no method that takes a wallet address; a coin is its own
type. A query string on these routes is refused (`400 unscoped`) rather than
ignored, so an address guess is told it does not work. Lists live at
`<RADAR_STATE_DIR>/customers/<address>/watchlist.json`, at most 100 coins, and
a list that exists but cannot be read is reported as unreadable, never as empty,
and is never overwritten.

Three departures from 0012's text, each argued in `tenant.rs`'s module comment:
the constructor takes a verified token rather than the guard's `Customer`,
whose fields are public and so could be written for anyone; the storage is one
JSON file per wallet in the state directory rather than `radar-store`'s
slot-partitioned Parquet writer, which cannot remove a row; and there is no
watermark, because a watchlist is the wallet's own instruction rather than an
observation of the chain.

One wording change outside the new routes: a request to a customer route that
carries no credential, or an expired or forged wallet session, now says so
(`no_session`, `session_expired`, `session_invalid`) instead of "no Cloudflare
Access assertion". Who gets in is unchanged.

Not done in this item: the star button and the watchlist tab, which are item 4.

**Item 1 is merged and live, 2026-09-23.** Independent review (Opus) failed
the first push on one gap -- no test reached the branch where a list's file
exists but cannot be read at all, so treating that as an empty list would have
survived -- and passed the second, which adds that test. It also moved two
smaller things: a failed email login on these routes is no longer told to
"sign in with your wallet again", and the file written before the rename is
named per process, because old and new servers overlap during a deploy. #285
squash-merged as `7554bb8`; the release build's sha256 matched
`BUILD-INFO.txt`, `sudo radar-deploy` installed it, and `/health` reports
`7554bb8`.

**Verified against the real site with two wallets** made for the check and
signed in through `/v1/customer/siws`: A saved a coin and read it back; B's
list was empty; B naming A in a query got `400 unscoped`; B deleting the same
coin emptied only B's list, and A's still held it; no session got
`no_session`; A's token with one character changed got `session_invalid`. Both
fresh wallets were admitted, so wallet sign-in on the box is no longer the
one-address allowlist. Their two folders stay on the box with empty lists. Not
checked live: an expired session, which takes twelve hours to make; CI's
`no_session_an_expired_one_and_a_forged_one_each_say_which` covers it.

**Item 2's original design ("the browser reads its own balances") is
superseded, 2026-09-24.** The owner approved a server-side read instead;
see item 2's text above for the design and the 403-Origin measurements that
forced the change.

**Phase C item 2 is built, 2026-09-24, PR open as a draft, not yet reviewed
or deployed.** `GET /v1/customer/positions` in
[`positions.rs`](../../crates/radar-serve/src/positions.rs), behind `Tenant`
exactly like the watchlist. New methods on `radar-onchain`'s `RpcClient`
(`balance`, `token_accounts_by_owner`) run inside `spawn_blocking`, since the
client is `ureq`-based and blocking. Response:
`{wallet, slot, read_at, age_seconds, sol: {lamports, ui_amount, price, quote,
value, priced, price_reason}, tokens: [{mint, program, amount, decimals,
ui_amount, price, quote, value, priced}]}`, `amount` as a raw-integer string
so it never crosses the wire through a float. **Corrected 2026-09-24:** a
review finding (`MarketTrade.price` is the trade's own quote-asset price --
wSOL, USDC or USDT -- never USD) meant `price_usd`/`value_usd` never belonged
in this shape; they are `price`/`value`, paired with `quote`
(`"SOL"`|`"USDC"`|`"USDT"`|`null`), a number is never presented as dollars
unless `quote` says so, and SOL itself carries `price_reason` when nothing in
the tape prices it. Same-mint accounts are summed with a checked add
(overflow refuses rather than wraps); zero balances are dropped; an untracked
mint prices as `priced: false` with a null `price`, never `0`. Per-wallet
cache: 30 s TTL, 2048 wallets,
keyed only by the verified address -- a cached answer keeps its original
`slot` while `age_seconds` grows. Global cap: 60 RPC calls/minute across all
wallets (one view costs 3), reserved atomically before the first call; over
the cap and not cached refuses `503 busy` without touching the RPC.
A failed program read refuses the whole answer `502 unreadable_chain` rather
than a partial list. New integration test file
[`a_wallets_positions_are_read_and_priced_by_radar.rs`](../../crates/radar-serve/tests/a_wallets_positions_are_read_and_priced_by_radar.rs)
(now 11 tests, no network -- a fake `Transport`), covering wallet isolation,
the `unscoped` query-string refusal, three wallet-session refusals
(`no_session`, `session_expired`, `session_invalid`) plus the
failed-email-login case (which itself reads as `no_session`, not a fourth,
distinct refusal), the one-program-failure refusal, the cache's fixed slot
and growing age, the cap's `busy` refusal without calling the transport, an
untracked mint's `priced: false`, same-mint summing with zero-balance
dropping, a SOL-quoted and a USDC-quoted trade priced and labelled
correctly, and the `Cache-Control: private, no-store` header. **Corrected
2026-09-24:** the original text here claimed "all four wallet-session
refusals," but `not_a_wallet` -- the refusal a real, Privy-configured
email-login session gets -- is not exercised by this test, nor by
`a_watchlist_is_seen_only_by_its_wallet.rs`'s equivalent test; both fakes
stop at a token shaped like a Privy one, not a working Privy verification,
so `not_a_wallet` remains untested in both places.

On the web side: a `PositionsPanel` beside `WatchlistPanel` in
`TokenHeader.tsx`, backed by `usePositions.ts` (mirrors `useWatchlist.ts`,
read-only). `isWatchlistSessionRefusal` in `honesty.ts` is now
`isWalletSessionRefusal`, shared by both routes rather than duplicated. Five
distinct sentences in `honesty.ts`'s `positionsMessage`: an invitation to
connect when signed out; "This wallet holds no tokens" (plus the SOL balance
when non-zero); a session-refusal sentence telling the reader to sign in
again; a `busy` sentence naming the rate limit; and a could-not-look sentence
that says plainly it is not evidence of what the wallet holds. `MIN_WEB_TESTS`
raised 132 to 146 (`vitest run`: 146/146). Not yet done: independent review,
merge, and a check against the real site with a wallet that actually holds
tokens.

**Item 2 is merged and live, 2026-09-24, after a one-line fix.** Independent
review (Opus) passed #288, which squash-merged as `0dbaadc` with
`MIN_WEB_TESTS` at 151 and was deployed. On the real site every read then
refused `502 unreadable_chain`: `TOKEN_2022_PROGRAM_ID` in
`radar-onchain/src/rpc.rs` was mistyped (`...PE9w6NCZt4Kwh2`, no account on
mainnet), and the public node answers `getTokenAccountsByOwner` with it as
`INVALID_PARAMS`. Every test passed because the fake `Transport` never looks
at the program id, and review read past it. #289 corrects it to
`TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb` (checked from the box:
`getAccountInfo` answers an executable program) and adds
`the_token_programs_read_are_the_ones_mainnet_runs`, which pins both ids to
`radar_pumpfun::token`'s byte constants, themselves pinned against real
accounts. It squash-merged as `d0aba33`; the release build's sha256 matched
`BUILD-INFO.txt`, `sudo radar-deploy` installed it, and `/health` reports
`d0aba33`.

**Verified against the real site with two fresh wallets** signed in through
`/v1/customer/siws`: A and B each read `200` with their own address, an empty
`tokens` list and zero lamports; B naming A in a query got `400 unscoped`; B
naming A in the path was refused by Cloudflare Access before it reached Radar;
no session got `no_session`; A's token with one character changed got
`session_invalid`; A's second read two seconds later was the cached answer
(same `slot`, `age_seconds: 2`); every answer carried `Cache-Control:
private, no-store`. Not checked live: a wallet that actually holds tokens
(both wallets were empty, and funding one is the owner's call), and an expired
session (CI covers it).

Two things this surfaced, neither a code fault:

- **SOL itself is unpriced on the live box.** The answer reads "no wrapped-SOL
  trade quoted in USDC or USDT was found in Radar's pricing window": the tape
  held no wSOL trade against a dollar coin, so nothing prices SOL in dollars.
  Balances are right; only SOL's dollar value is missing. Recording such a
  pair would widen what the tape collects, which is the owner's decision
  (Phase B item 3).
- **Positions reads through the public node.** `RADAR_RPC` is not set in
  `/etc/radar/radar.env`, so reads go to `api.mainnet-beta.solana.com` from
  the same IP as the two backfill jobs. Under load, visitors will see `busy`
  or "could not look". A private RPC URL there is the owner's edit.

**Phase D, web half — draft PR opened, 2026-09-24.** Built in worktree
`plan-0013-d-trade-web` against the server contract items 2 and 3 above
describe (`GET /v1/market/quote`, `POST /v1/customer/swap`, `/health`'s
`trading` field), authored in parallel with, not against a running instance
of, the server side of this same phase (`plan-0013-d-swap-server`) -- see the
PR body for the exact request/response shapes assumed. Two dark switches gate
`TradePanel`/`Terms.tsx`: `legal.ts`'s `TERMS_APPROVED` (hardcoded `false`,
with a test that any `[PLACEHOLDER]` bracket left in `TERMS_TEXT` forces it
false) and `/health`'s `trading` field (`useHealth.ts`'s `useTrading()`);
both must be true before either renders real content, so on `main` today the
panel stays inert regardless of what the server reports. Signing:
`@solana/web3.js` added, scoped to `sign.ts`'s single
`VersionedTransaction.deserialize()` call, chosen over the Wallet Standard
discovery registry (a bigger departure from `siws.ts`'s existing
injected-provider pattern than this warranted) and over Phantom's
undocumented-for-v0 bs58 `request()` path -- `sign.ts`'s own doc comment and
`PROGRESS-d-web.md` in that worktree carry the full reasoning.

**Gap, not fixed here:** `/terms` is wired into `App.tsx`'s real routing but
deliberately left out of `routes.ts`'s `ROUTES` table, because that table is
cross-checked by `routes.test.ts` against `crates/radar-serve/src/access.rs`'s
`audience_of`, which this worktree did not touch (Rust crates are out of
scope for a web-only phase); `access.rs` has no classification for `/terms`
yet. Add it to both in the same change, once `/terms` gets a real server-side
audience.

**Not checked:** a real quote or swap against a live server -- the parallel
`plan-0013-d-swap-server` branch building those two routes was still in
progress as this branch was written, so the shapes in `api.ts` (`Quote`,
`SwapResponse`) and the refusal codes in `honesty.ts`'s
`swapRefusalMessage` are contract, not observation; cross-check both once
that branch lands. `MIN_WEB_TESTS` raised 151 to 210 by a static count
(`it`/`it.each` cases counted by hand, since this worktree was told not to
run the web suite locally) against an estimated real total near 219 -- CI's
first run of this branch has the true number and the floor should move to
match it. Rehearsal on a throwaway wallet with a real, deployed server (item
5 above) has not happened and should gate the merge, the same as every other
phase in this plan required a real-site check before being called done.
