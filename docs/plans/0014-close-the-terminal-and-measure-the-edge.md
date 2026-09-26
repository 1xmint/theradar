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
  (`site/src/Privacy.tsx:56`). Any visitor counting is an owner decision with
  a text change first, and is not in this plan.

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
2. [ ] **Contract cross-check.** `web/src/tradeContract.test.ts` reads
   `trade.rs` and `api.ts` and fails if `Quote`, `SwapResponse` or the refusal
   vocabulary drift. Written 2026-09-25 in this branch; read-through found no
   mismatch (six server reasons, all with a sentence in `honesty.ts`; the
   field lists agree). Done when CI is green on this branch and
   `MIN_WEB_TESTS` is 262.
3. [x] **`/terms` audience.** Already `Public` in `access.rs:627` and listed in
   `routes.ts:69` on `main`; `routes.test.ts` cross-checks it. Nothing to do.
4. [ ] **Owner, legal: the terms.** PR #292 has four blanks: operator name,
   governing law, region blocks, contact. Recommendation: counsel reads the
   draft before the button goes live; a public buy button on memecoins may be
   a financial promotion in some places, and the region list has to be
   deliberate. The draft must also say what the terminal keeps about a
   visitor (wallet address, session, watchlist, positions), which the site's
   privacy page currently denies. Engineering then merges the terms, sets
   `TERMS_APPROVED` true, and links the terms from the panel. Done when the
   owner or counsel has signed off in the PR, `legal.test.ts` finds no
   placeholder, and `/terms` renders on the site.
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
7. [ ] **Owner: the key and the switch.** `RADAR_JUPITER_API_KEY` and
   `RADAR_TRADE=on` in `/etc/radar/radar.env`, restart `radar-serve`. Only
   after items 4 and 5. Done when `/health` reports `trading: true` and
   `GET /v1/market/quote` answers from outside.
8. [ ] **Independent review** (Opus, orch-reviewer) of the whole swap path,
   server and web, against 0013's D.4 and D.5: no key, no fee, slippage
   refused not clamped, the review card's numbers come from the built quote,
   cancel and failure worded apart, "Powered by Jupiter" shown. Money moves,
   so this is not optional. Done when PASS is recorded here with findings fixed.
9. [ ] **Rehearsal.** A throwaway wallet with a few cents; Josh signs one buy
   and one sell from the screen. Transaction ids and the positions panel
   updating go here. Then 0013's status becomes `landed`.

Order: 1, 2 and 5 now. 4, 6 and 7 are the owner's and gate 8 and 9. Phases H
and G do not wait on them.

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
2. F4: does counsel read the terms before the button goes live, or do you fill
   the four blanks yourself and accept the exposure? F5's findings (the
   "Powered by Jupiter" label, the sanctions pass-through, and the United
   States on Jupiter's own blocked list) are the reason to ask.
3. G2: pay CryptoHouse for a larger allowance, or leave the screen at ten
   coins until there are visitors? Asked with G1's numbers in hand.

## Handback

**Stopped at:** F2's test and this plan written on
`plan/0014-close-the-terminal`, not yet pushed; F5 read; F3 closed by reading.

**Next action:** push the branch, open the PR, wait for CI. Then H1 and H2 as
their own PRs while the owner works F1's deploy, F4 and F7.

**Do not:** turn `RADAR_TRADE` on before F4, F5's label and F8; shrink
`consider`; add any visitor counting; touch `Policy::CLOSED`.
