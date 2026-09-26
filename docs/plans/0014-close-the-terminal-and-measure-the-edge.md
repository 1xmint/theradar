<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0014 — Close the terminal, then measure the edge

**Status:** in progress.
**Branch:** `plan/0014-close-the-terminal` for this file and the small items;
each build item lands on `main` as its own pull request.
**Base:** `666d504` (`#296`, the shared ticker), read on 2026-09-25.
**Planned by:** Claude, from `origin/main`, the pull-request list, the live
`/health`, and one advisor pass; the owner chose the last phase on 2026-09-25.
**Builds on:** [plan 0013](0013-the-terminal-find-look-track-trade.md), which
this plan takes over. 0013's status paragraph says what is live; this plan
says what is left.

## Objective

Two things, in order. First, finish what 0013 started: the buy button is built
on both sides and dark, and it comes on only after the owner's legal blanks,
the Jupiter key, an independent review and a rehearsal with real cents. Second,
unpark the one measurement GOAL.md puts before everything else: run the edge
calculation over the decision records the box has been keeping since the
recorder started, and write the number down, however small the sample.

Nothing here changes the policy, holds a key or a fee on the server, or moves
the Cloudflare wall.

## Context

What is true on 2026-09-25, from `/health` and the merged list, not from memory:

- **Live build is `5a9d991`** (#295): the swap server. `trading: false`,
  `policyClosed: true`, three instruments, `paidSurface: false`.
- **#296 is merged and not deployed.** The shared ticker and
  `GET /v1/market/events` wait on the owner's `sudo radar-deploy`.
- **The buy button is dark twice over:** `TERMS_APPROVED` is hardcoded false
  in `web/src/legal.ts`, and the server refuses to start with `RADAR_TRADE=on`
  and no `RADAR_JUPITER_API_KEY`. The terms draft is PR #292 with four
  `[BRACKETS]` only the owner can fill.
- **The web's copy of the swap contract was never checked against the
  server.** #293 was written against `trade.rs`'s doc comment. Item F2 below
  is the check.
- **`/terms` is already Public on both sides** (`access.rs:627`,
  `routes.ts:69`), so 0013's "settle the audience" note is closed by reading.
- **SOL is unpriced on the live site** and `RADAR_RPC` is unset; both are
  allowance or owner questions, in Phase G.
- **The README claims agent surfaces that are behind the login wall**
  (issue #186). GOAL.md says Radar must never be wrong about what is live.
- **GOAL.md itself is stale**: it still describes the X account that moved to
  realorrug in #253, and its item 2 ("see the decisions and act on them") is
  not what a swap button gives. It is the owner's document; item F4c asks for
  the update rather than arguing with it.
- **Research 0036** capped `consider` at ten queries a run (deployed
  2026-09-20) and says its decision records "cannot be backfilled later". They
  are the input to Phase I. This plan does not shrink `consider` to make room
  for more coins on the screen.
- **The site's privacy page promises "Nothing measures you"**
  (`site/src/Privacy.tsx:56`). `site/` is cabalhunter.org, not the terminal,
  but the rule carries: any visitor counting is an owner decision with a text
  change first, and is not in this plan.
- **2026-09-26, owner:** trading is public for any wallet with no counsel
  review, and the scope is all of this plan. F4 and the order below are
  amended to match; F10–F13 are new.

## Not in scope

Anything that holds a key or a fee server-side; moving the Cloudflare wall;
the paid Yellowstone feed (`RADAR_STREAM_ENDPOINT`); a second chain;
`Policy::CLOSED`; the private trader (plan 0011); publishing Radar's decisions
on the terminal (operator-only by the owner's direction of 2026-09-11, to be
revisited when GOAL.md is updated); cutting the `consider` cron or its
candidate count; any visitor measurement; running cargo test suites locally
(owner rule 2026-09-19: CI runs them).

## Phase F — close plan 0013 on the real site

1. [ ] **Bring 0013's status up to `main` and deploy #296.** The status
   paragraph is edited in this branch. The deploy is the owner's:
   `deploy/README.md` "Every deploy after that", then `sudo radar-deploy`.
   Done when `/health` reports build `666d504` or later and
   `curl -N https://radar.heyvera.org/v1/market/events` emits ticks from
   outside.
2. [x] **Contract cross-check.** `web/src/tradeContract.test.ts` reads
   `trade.rs` and `api.ts` and fails if `Quote`, `SwapResponse` or the refusal
   vocabulary drift. Written 2026-09-25 in this branch; read-through found no
   mismatch (six server reasons, all with a sentence in `honesty.ts`; the
   field lists agree). CI green on #297 with `MIN_WEB_TESTS` 262.
3. [x] **`/terms` audience.** Already `Public` in `access.rs:627` and listed in
   `routes.ts:69` on `main`; `routes.test.ts` cross-checks it. Nothing to do.
4. [x] **Owner, legal: the terms.** ~~Owner decided 2026-09-25: counsel
   reads the draft.~~ **Reversed by the owner on 2026-09-26: no counsel;
   trading is public for any wallet.** The owner fills #292's four blanks
   (operator name, governing law, blocked regions, contact) himself and
   carries the exposure. The work moves to F10. (The `site/` privacy page
   belongs to cabalhunter.org, a different product; the terminal's own
   disclosure of what it keeps lives in its terms.)
5. [x] **Jupiter's terms, read 2026-09-25** (orch-researcher, primary pages
   only). Findings the owner and the terms draft must carry:
   - The SDK & API License Agreement
     (https://developers.jup.ag/docs/misc/sdk-api-license-agreement) §2.1
     licenses "products or services integrating the API"; nothing bars a
     free, no-fee, non-custodial front-end. **§8.4 requires a prominent
     "Powered by Jupiter" to end users**: the trade panel needs the label.
     §2.3: do not present one routing engine as another.
   - **§7.3 passes sanctions and anti-money-laundering compliance to the
     integrator**, including blocking sanctioned wallets. Radar's terms and,
     if counsel says so, its code have to carry that.
   - **Jupiter's Terms of Use (https://developers.jup.ag/docs/misc/terms-of-use)
     list the United States among blocked jurisdictions**, with Cuba, Iran,
     North Korea, Syria and sanctions-listed persons. Whether that list binds
     Radar's visitors is a counsel question; the region blanks in #292 cannot
     be filled without answering it.
   - §§12.2–12.4: the integrator's terms should say Jupiter is a back-end
     technical service, not an exchange, broker or custodian. Both documents
     disclaim failed transactions, slippage and MEV and cap Jupiter's
     liability at $100; Radar's terms should mirror the disclaimer.
   - Plans (https://developers.jup.ag/docs/portal/plans): free tier is 60
     requests a minute with a free key, 30 without; Developer $25/month for
     10 a second. Radar's tiers (30/6/6 a minute) fit the free key.
   - Not verified: whether the old `lite-api` host still answers; whether
     execute calls cost credits. Neither changes the answer above.
6. [ ] **Owner: update GOAL.md** to the 2026-09-18 direction: move or remove
   the X account section; say whether item 2 is still "see the decisions and
   act on them" or is now the swap button. Done when it is committed on `main`.
7. [ ] **Owner: the key and the switch.** In the same box session as F1,
   after F10–F12 are merged and F8 has passed: `RADAR_RPC` set to a private
   RPC URL (G4, moved here so F11's landing check and positions do not ride
   the public node), then `RADAR_JUPITER_API_KEY` and `RADAR_TRADE=on` in
   `/etc/radar/radar.env`, restart `radar-serve`. Then open
   radar.heyvera.org in Phantom and check signing shows no "unsafe site"
   warning; if it does, submit the domain to Phantom/Blowfish before F9.
   Done when `/health` reports `trading: true`, `GET /v1/market/quote`
   answers from outside, and Phantom does not warn.
8. [ ] **Independent review** (Opus, orch-reviewer) of the whole swap path,
   server and web, against 0013's D.4 and D.5: no key, no fee, slippage
   refused not clamped, the review card's numbers come from the built quote,
   cancel and failure worded apart, "Powered by Jupiter" shown, plus F10–F12:
   landing states honest ("expired" only when proven), the sanctioned
   refusal. Runs on each pull request as it is pushed. Money moves, so this
   is not optional. **Trading does not switch on until PASS.** Done when PASS
   is recorded here with findings fixed.
9. [ ] **The owner's real test.** From radar.heyvera.org with his own wallet:
   start with about 0.01 SOL of one coin, then sell it back; then any size.
   Include one SOL→token and one token→SOL (proves Jupiter's wrap/unwrap
   passes `assemble.rs`'s checks on mainnet). Transaction ids, the landing
   state shown and the positions panel updating go here. Then 0013's status
   becomes `landed`.
10. [ ] **Terms final, panel on.** #292's text moves into `web/src/legal.ts`
    with the owner's four values, plus: Jupiter is a back-end routing
    service, not a broker, exchange or custodian; the failed-trade, slippage
    and MEV disclaimer; sanctioned persons may not use it; what the terminal
    keeps (wallet address, watchlist, session token in the browser,
    positions — stored or read live, whichever the code does); Cloudflare and
    hosting named as request loggers. `TERMS_APPROVED` true, `/terms` linked
    from the panel, a persistent "Powered by Jupiter" label (F5, §8.4). #292
    closed pointing here. Done when `legal.test.ts` finds no placeholder and
    `TradePanel.test.tsx` asserts the label and the link.
11. [ ] **Did it land?** Server: `GET /v1/customer/tx/{signature}?last_valid_block_height=N`
    (Customer audience, the per-wallet limiter) answers `pending`, `landed`,
    `failed` (with the reason) or `expired`, from `getSignatureStatuses` and
    `getBlockHeight`. Web: after the wallet sends, poll it every 1.5 s up to
    about 90 s and say which. **"Expired" only when proven**: a successful
    read shows the chain past `last_valid_block_height` and no status for the
    signature; a failed or rate-limited read says "unknown, check Solscan".
    Positions refresh on `landed` only. A build older than about 60 s is
    rebuilt before the wallet is asked to sign. Done when a server test covers
    all four states against the faked chain and web tests cover each state
    and the stale-build rebuild.
12. [ ] **Sanctioned wallets refused** (Jupiter §7.3). A checked-in list of
    OFAC-listed Solana addresses, source and date in its header, checked at
    `customer/swap` only; refusal `sanctioned` with a sentence in
    `honesty.ts`. Done when a server test refuses a listed address and passes
    others.
13. [ ] **Works on a phone.** Under 1024 px the terminal stacks to one column
    with the trade panel reachable without sideways scroll; with no wallet in
    a phone browser, Connect offers "Open in Phantom" / "Open in Solflare"
    deep links to the current page. Does not hold up F7. Done when web tests
    cover the no-wallet phone state and the page is checked at 375 px and
    1280 px.

Order (2026-09-26): 10–13 in parallel as separate pull requests, merged one
at a time with CI re-run; 8 reviews each as it is pushed. 1 and 7 in one box
session after 10–12 are merged and 8 has passed. 9 after 7. Phases H, G and I
do not wait.

## Phase H — the public claims match the product

1. [ ] **Close issue #186 by making the README true.** The x402/MCP surface
   is behind Cloudflare Access and no agent reaches it today; what is free is
   `/v1/market/*` and the terminal. The wall does not move in this item. Done
   when the README states what `curl` shows and the issue is closed with the
   commit.
2. [ ] **`docs/STATE.md` gains "what is live on radar.heyvera.org"**: the
   build, the public routes, the customer routes, the dark switches, and that
   nothing has ever traded. Done when `repo-conformance` is green.
3. [ ] **Housekeeping.** Plan 0012's status line ("planned, not started") and
   plan 0011's are stale; fix them. Merge the safe dependabot PRs (#273–#282
   minus vite 8, which plan 0003 blocks on the tailwind plugin) after
   re-running CI on each (stale-green trap). Prune the worktrees under
   `.claude/worktrees/` whose branches are merged.

## Phase G — the data the terminal stands on

1. [ ] **Per-unit query counts.** Each unit logs its own CryptoHouse query
   count per run (0036's "what was not checked"), and `radar brief` shows the
   day's refusals per unit. Then read the recorder and decision logs since
   2026-09-20 and write a 0036 addendum: is anything still refused now that
   `consider` is capped?
2. [ ] **Owner decides money, framed by G1.** The box cannot run the tape,
   the recorder, the outcomes job and a useful `consider` inside 120 free
   queries an hour. (a) pay CryptoHouse for a larger allowance; (b) leave the
   screen at ten coins and SOL unpriced until there are visitors; (c) run
   `consider` over fewer candidates. Recommendation: (a) if the price is
   small, else (b). Not (c): 0036 says `consider`'s records cannot be
   backfilled and they are Phase I's input.
3. [ ] **With headroom from (a):** widen `SHORTLIST` inside the tape's 72 an
   hour and record one wSOL/USDC pair so SOL prices in dollars. **With (b):**
   price SOL from a Jupiter quote of 1 SOL to USDC once a minute in the ticker
   refresher, after F7, so SOL at least shows a dollar figure. Done when more
   than ten coins are named, or SOL is `priced: true` without CryptoHouse, and
   `radar brief` stays under 80 queries an hour.
4. [ ] **Owner:** `RADAR_RPC` set to a private RPC URL (a free tier at one of
   the RPC providers is enough for one box). Done when positions reads no
   longer hit the public node.

## Phase I — edge measurement (the owner unparked this on 2026-09-25)

Plan 0007 items 3 and 4, as written there.

1. [ ] **One windowed run on the box** of `radar-next features` then
   `radar-next edge` over the decision records since the recorder started; the
   window and the store path recorded here with the command. `Policy::CLOSED`
   untouched. Done when the run's output is saved beside the store.
2. [ ] **Research 0026:** the measured edge, over how many decisions, with
   LEARNINGS 35's caveat if the sample is small (a measured zero is not a
   verdict). Done when it is on `main`.
3. [ ] **`docs/STATE.md`'s measured-edge narrative** updated from 0026, and
   plan 0007 items 3–4 ticked with the evidence. Done when STATE.md and 0007
   agree and `repo-conformance` is green.

Not from this phase: changing the policy, starting the private trader, or any
product change from a single window.

## Verification, every phase

As 0013: evidence from radar.heyvera.org after `sudo radar-deploy`, not a
local run. CI green on every PR; `MIN_WEB_TESTS` raised with each web change;
CI re-run on any PR older than `main` before it merges.

## Open questions for Josh

1. ~~What comes after the terminal?~~ Answered 2026-09-25: edge measurement.
2. ~~F4: does counsel read the terms before the button goes live, or do you
   fill the four blanks yourself?~~ Answered 2026-09-25: counsel; reversed
   2026-09-26: no counsel, public for any wallet, the owner fills the blanks.
3. G2: pay CryptoHouse for a larger allowance, or leave the screen at ten
   coins until there are visitors? Asked with G1's numbers in hand.
4. The four terms values for F10: operator name, governing law, contact,
   blocked regions. Recommendation for regions: Jupiter's own list (United
   States, Cuba, Iran, North Korea, Syria, sanctioned persons), stated in the
   terms, not enforced by IP. If the owner is in the United States, his own
   test would sit outside Jupiter's terms as written.
5. F6, a GOAL.md wording for the owner to approve or rewrite (it is his
   document; nothing here waits on it). Line 166 "nothing has ever traded"
   and "What working would look like" item 2 become, after F9: *"2. A
   customer can connect a wallet, see the market, and buy or sell from the
   screen, with Radar never holding a key or a fee. Live since F9; Radar's
   own decisions are still not published to customers."* The X account
   section moves out (it lives in realorrug since #253). Item 1, the measured
   edge, stays first: the swap button is a tool, not a claim of edge.

## Handback

**Stopped at (2026-09-26):** #297 carries F2 and this plan with the owner's
two decisions of the day (no counsel, public for any wallet; the scope is all
of this plan). F10–F13 added.

**Next action:** F11, F12, F13 and H1/H2 as their own pull requests now; F10
when the owner sends the four terms values. F8 reviews each as pushed. Then
the owner's box session (F1 + F7), then F9.

**Do not:** turn `RADAR_TRADE` on before F10–F12 are merged and F8 has
passed; shrink `consider`; add any visitor counting; touch `Policy::CLOSED`.
