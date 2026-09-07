<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0009 — The evidence, the rule, and the voice

**Status:** in progress
**Branch:** one per item, all from `main`; named in each item
**Base:** `4898685`, `main` at #167
**Planned by:** Fable 5.1, plan mode, 2026-09-06, from a checkout that did not write #160–#167
**Reviews:** plan 0008's handback; design 0012 (answered here, recorded in design 0013); design 0011 (accepted with three amendments, section 4); LEARNINGS 29–31; AGENTS.md §4

## Context

The account went live at 16:16 UTC today with no model, on the free public RPC endpoint, one reply in the log, and a contest week that closes in a few hours. Plan 0008 landed the site, the claim fix and the voice work; item 3 (the scan) was superseded by an undecided design and item 5 (the public surface reviewed) never started. Josh asked for four things (design 0012) and, during this planning, sharpened them: the leaderboard should show **the evidence behind every placing and exclusion**, never the code and never a verdict word; the scoring must be **dialled in against modern engagement farms** without killing the engagement that makes a reply spread; and the whole thing should be built to **go viral, safely** — a bio that works as a noticeboard, a way for people to find good accounts, an X community, a history of who won and whether they collected.

This plan is the review he asked for and the plan that follows from it. It is written by a model that did not write the code it plans against, which is why it re-verified every claim below by reading the code, the box, or the platform's own documentation, and says which.

**Ground truth, read off the box at 17:30 UTC today** (`ssh guardian-vps-tail`, read-only):

| what | state |
|---|---|
| units | `radar-analyst`, `radar-serve`, `radar-follow` active; `radar-brief.timer`, `radar-creator-index.timer` active. **Not installed:** `radar-seven-days.timer` (the daily post cannot start on 2026-09-13), `radar-payout` (no token, correct) |
| binaries | `~/bin/radar-analyst` built 17:16 today, `~/bin/radar` 15:48, `/usr/local/bin/radar-serve` 15:42 — all post-#162; every running process holds the installed file (no `(deleted)`) |
| `/etc/radar/analyst.env` | X credential and prices set; `RADAR_X_PUBLISH=on`; **no `RADAR_RPC`** (the analyst reads the chain through the free public node); **no `RADAR_MODEL_*`** (every reply is the template); `RADAR_CONTEST_OPERATORS` **appears twice** (systemd takes the last line; nothing prints which set is in force) |
| `/etc/radar/alert.env` | absent. No alarm channel for a public bot (LEARNINGS 8) |
| the log | 2 lines = 1 reply, `fellback: NoProvider`, published. No `refusals.jsonl` (nobody refused yet) |
| the record | `2956.json` (empty week). Week 2957 closes 2026-09-07 00:00 UTC with one entry, the operator's, excluded — the summary will say "no winner" and "no token yet" |

**Three platform facts, external, each with the URL Josh should confirm in the Developer Console before anything is built on it:**

1. **Since 2026-02-23, on every self-serve tier, `POST /2/tweets` as a reply is accepted only when the author of the post being replied to mentioned or quoted the bot in that post.** Replies to the bot's own posts are reported exempt ("posting your own content via the API works normally"). The bot's replies to mentions are fine. The claim prompt (a reply under the bot's own reply) and the teardown (a reply under the bot's own summary) should post, and **nothing has tested it here**, because no week has had a winner. [XDevelopers announcement](https://x.com/XDevelopers/status/2026084506822730185), [fireply's write-up](https://fireply.ai/blog/x-api-reply-restriction-2026).
2. **Follow, like and quote-post write endpoints were removed from all self-serve tiers on 2026-04-16 and are Enterprise-only**; X's automation rules prohibit automated following at every tier. An automated follow system cannot be built on this account's plan. [SocialNexis](https://socialnexis.com/guides/x-api-basic-enterprise-automation-rules), [X rate limits](https://docs.x.com/x-api/fundamentals/rate-limits).
3. **X's Automation Rules (updated April 2026) are reported to require prior written X approval for AI-powered reply bots.** The rules page returned 403 to this session; Josh should read it: <https://help.x.com/en/rules-and-policies/x-automation>. Today the account posts a deterministic template. The day a model is switched on it is unambiguously an AI reply bot. [vorplabs](https://vorplabs.com/agent-tools/x-api).

**Josh's reading of fact 1, recorded as his:** the restriction is acceptable, because a dev who wants traffic will quote the bot's reply about their coin to their own audience, and a quote is one of the two triggers X permits. It is a loop, not a limit: item 2 scores a quote from a real account at three verified points, and the weekly post names the hunters (item 8). One API detail to carry with it: a quote does not reach `GET /2/users/:id/mentions` unless its text mentions the bot, so "reply to whoever quoted us" would need a `quote_tweets` read per recent reply and a reason to say anything new. Not built; item 2's records will show how often devs quote, and that number decides whether it is worth a second reply path.

Prices on X's own page, read today: post read $0.005, **user read $0.010 per user returned**, owned read $0.001, likes read $0.001, summoned reply $0.010, post $0.015, **post with URL $0.200**, profile update $0.005, user interaction $0.015. <https://docs.x.com/x-api/getting-started/pricing>. These are claims; the console is the instrument.

## Objective

Afterwards: every number in a reply about a graduated coin is about that coin; replies have a voice and every model call is metered; the week is scored by a rule that costs a farm more than the prize and the leaderboard shows the evidence behind every placing; a winner is told in a post X will accept, claims to a wallet and not a mint, and the history page says who won, whether they claimed, and whether they were paid; the bio is a guarded noticeboard; the box says what it runs; design 0012 is answered in the repository; and research 0029 records the public surface as reviewed, with S1–S30 and their dispositions.

## Not in scope

- `Policy::CLOSED`, the kernel, `radar-signer`, `radar-exec`, the store's schema, the recorder, `radar-graph` thresholds. Nothing here touches money the trading lane could move.
- The token launch, the devnet week, the on-chain claim program. `radar-payout` is reviewed (item 10) and not installed until a wallet exists.
- The Telegram lane beyond a token from Josh. It is built.
- Anything that spends beyond the prices Josh set, without the line that says so.
- A second daily post format, or any unprompted post about a coin nobody asked about. Design 0009 §6 and the site's promise both refuse it; section 5 says what the daily post already is.
- The mint index that would give a graduated Radar-era coin its launch block. Sized in item 6 and deferred with its cost.

## Phase 0 — before any code: the box, the platform, and tonight

Nothing below needs a merge. Each line says who.

**Read off the box at 18:40 UTC on 2026-09-06, and three of these lines were wrong.** Corrections here rather than edits to the lines themselves, so the plan is not quietly rewritten into having been right.

- **0.2 has no endpoint to point at.** "Set it to the Helius endpoint the recorder uses" assumed one exists. There is none, anywhere: `radar-follow.service` carries no `EnvironmentFile` and no `Environment=`, neither `/etc/radar/analyst.env` nor `/etc/radar/radar.env` sets `RADAR_RPC` or `RADAR_RPC_URL`, and no config on the box contains the string `helius`, `quicknode` or `alchemy`. **The recorder, both hourly crons and the analyst are all on the free public node.** So 0.2 is not an env edit; it is "obtain an endpoint", and it is a purchase before it is a configuration.
- **0.3 is cosmetic, not a live hazard.** The two `RADAR_CONTEST_OPERATORS` lines are *identical* — both `2005812292693483520` — so systemd taking the last one changes nothing. The plan said "keep the one with both ids", which implied they differed. They do not, and both operator accounts are in fact covered: `operator_ids` adds `RADAR_X_USER_ID` (`1889496824328880128`, the bot) to whatever the variable lists, so the bot and the managing account are both excluded. Worth tidying; not worth a restart of its own.
- **0.4 has no source files on the box.** `~/radar` is the analyst's `WorkingDirectory` and holds the three data files it reads, but it is a checkout of `github.com/1xmint/radar.git` with no `deploy/` directory and no `crates/`. The unit files have to be copied to the box before they can be installed, which the runbook's commands assume away by starting at `deploy/`.

The 429 claim behind 0.2 is worth stating exactly rather than repeating: over the six hours to 18:40, `radar-analyst` logged **zero** rate-limit lines and `radar-follow` logged **six**, while continuing to record rows throughout. The analyst has answered one mention in its life, so its zero is a measurement of its traffic and not of the endpoint. The case for a paid endpoint is what a busy coin's signature walk needs, not an outage in the log.

- [x] 0.1 **Tonight's close, read after it happens** (session). Week 2957 closes at 00:00 UTC. Expected: `weekly:2957:0` in `posts.jsonl` with a `reply_id`, saying one summoned reply, none counted, one excluded, no winner, no token yet. If the post did not go out, the reason is on the same line.
      proof: `ssh guardian-vps-tail 'grep weekly:2957 ~/radar/data/analyst/posts.jsonl | tail -1'` and the post on the account.
- [x] 0.2 **`RADAR_RPC` on the analyst** (Josh; root edits the env, restarts the unit). The analyst is on `api.mainnet-beta.solana.com`, which rate-limits and cannot reach the history a busy coin needs. LEARNINGS 30 fixed the *name* of the variable; the box still has no value. Set it to the Helius endpoint the recorder uses.
      proof: `journalctl -u radar-analyst -n 200 | grep -c 'http status: 429'` before and after; a `radar dossier` on the box against a busy mint.
- [x] 0.3 **One `RADAR_CONTEST_OPERATORS` line** (Josh, root). Two lines are in the file; systemd keeps the last. Delete one, keep the one with both ids. Item 9 makes the daemon print the set it holds so this cannot be silent again.
      proof: `grep -c '^RADAR_CONTEST_OPERATORS=' /etc/radar/analyst.env` prints 1; after item 9, the journal's start line names two ids.
- [ ] 0.4 **`radar-seven-days.timer`** (Josh, root; the commands are in `deploy/README.md` "The two appointments"). Without it the first "seven days later" on 2026-09-13 finds no file and posts nothing.
      proof: `systemctl list-timers radar-seven-days.timer` shows a next run at 11:30 UTC.
- [x] 0.5 **`/etc/radar/alert.env`** (Josh, root; `deploy/alert.env.example`, Telegram recommended). A public bot that dies looks like a quiet week. Prove it with the empty-store command in the runbook.
- [x] 0.6 **Four platform checks, in the Developer Console and with one test post** (Josh):
      (a) a reply to the bot's own post is accepted — post one top-level from the bot's credential, reply to it, delete both;
      (b) `POST /1.1/account/update_profile` with `description` is callable on the pay-per-use plan (item 7 depends on it);
      (c) the Automation Rules page's clause on AI reply bots, and whether the account already carries the automated-account label with the managing account named (X sets it from the developer portal; if it is set, the bio need not carry the disclosure sentence);
      (d) the quote-count claim behind item 2: quote one of the bot's replies twice from one account and read `quote_count` at `https://api.x.com/2/tweets?ids=<id>&tweet.fields=public_metrics`. If it reads 2, the claim stands.
      Record the four answers in `deploy/README.md` with the date. Item 4 changes shape if (a) fails; item 7 is dropped if (b) fails; item 1's key waits on (c).
- [ ] 0.7 **The model key goes on the box only after item 1 is deployed**, because until then a model call is unmetered.

## Items

Ordered by what it costs to be without them. Items 1–5 should be on the box before 2026-09-14 00:00 UTC, when week 2958 — the first full live week — closes. A week closes under whatever rule the installed binary carries, and after item 4 the record says which.

**Who installs what.** `radar-analyst`, `radar` → `~/bin`, installed by guardian without sudo, restarted by Josh with sudo. `radar-serve` → `/usr/local/bin`, root. The site → Cloudflare Pages on merge to `main`, nothing to install. Every item names its binary.

- [x] 1. **Every model call is metered, then a model is switched on.** `feat/meter-the-voice`. `Cost::ModelCall` exists in `crates/radar-analyst/src/spend.rs` and is charged nowhere (S20; the shape of S3). `crates/radar-analyst/src/answer.rs` reserves nothing before `radar_roast::roast`, and `radar_model::Answer.cost` — computed from the provider's reported usage in `crates/radar-model/src/api_key.rs` — is discarded by `crates/radar-roast/src/voice.rs`. Change: `Reply` carries what the call was billed; `tick` in `daemon.rs` and `telegram.rs` reserve `Cost::ModelCall` when a provider is configured, before `answer`, settle at the reported cost or the reservation, release when no call reached a provider. `RADAR_MODEL_PER_CALL_USD_MICRO` on the box (already set) is what the reservation charges. Then the key, and the gate before it: `radar analyst --mentions <fixture>` offline with the provider set, twenty replies read by Josh beside their sheets, the deterministic template kept as the floor it already is.
      **Model.** The client speaks the Anthropic Messages API (`RADAR_MODEL_ENDPOINT=https://api.anthropic.com/v1/messages`, `max_tokens` 2000, no thinking parameter — works unchanged against `claude-opus-5`, `claude-sonnet-5` or `claude-haiku-4-5`). Josh has an OpenAI key ready. Two paths: **(A)** an Anthropic key — configuration only, the same evening; **(B)** an OpenAI-shaped `Provider` in `crates/radar-model/src/` (Chat Completions body, `usage.prompt_tokens`/`completion_tokens`, tested against a stub like `api_key.rs` is) — one session, then his key. **Josh chose (B) on 2026-09-06.**
      binary: `radar-analyst`. proof: with a stub provider, `spend.spent_today()` rises by the model price on one answered mention and does not on a refusal; `grep -c '"fellback":"NoProvider"' replies.jsonl` stops growing after the key.
- [x] 2. **The score is verified engagement, and the leaderboard shows the evidence.** `feat/verified-score`. Answers Josh's "dialled in against farms, without losing engagement".
      **The finding (S16).** `3·reposts + 3·quotes + 1·likes + 1·replies` over `public_metrics` — but on X one account can quote or reply to the same post without limit; only a repost and a like are one per account. A single account can farm the top score for nothing. Farms sell the rest: likes from about $2 per 100, reposts from about $1.50, from aged accounts with bios and posting histories, so an age floor alone does not stop a bought winner; it raises the price per point from cents to dollars. [prices](https://onpattison.com/news/2026/mar/23/safe-sites-to-buy-twitter-likes/). The prize is tens of dollars, so no engagement rule makes farming unprofitable. What a rule can do is (i) count only what is one-per-account, (ii) count only accounts that cost money to fake, (iii) publish the measurement so a bought week is visible, and (iv) let the operator void a week with the evidence on the page. Design 0011 argued (iii) and (iv); this item builds them into the rule rather than beside it.
      **The rule.** `score = 3·reposts + 3·quoters + 1·likes`, counted over **verified engagers**: distinct accounts at least 30 days old at close (the floor entrants already meet, applied symmetrically). Replies weigh zero — they are conversation, not spread, and the bot's own claim prompt is one. Raw `public_metrics` stay on the record and the page as evidence. `crates/radar-contest/src/score.rs`: `Metrics` gains `verified: Option<Verified { reposts, quoters, likes, engagers, engagers_under_30d }>`, `score()` prefers `verified` and falls back to raw only for entries below the scan; `Rules` gains `min_engager_age_days: 30`; `Rules` and the weights are written into the record as `Record.rule` (`serde(default)` — the 2956 bytes test in `ledger.rs` is the enforcement).
      **The scan, bounded.** At close, walk the raw ranking from the top: read `GET /2/tweets/:id/retweeted_by`, `/liking_users` and `/quote_tweets` (authors) with `user.fields=created_at,public_metrics`, one page of 100 each, for entry k; compute its verified score; stop when the best verified score is at least the next entry's raw score (raw ≥ verified always, so nothing below can win). Ordinary week: one to three entries, six to nine calls, at most 300 user resources — about $3 at X's listed $0.010 per user, metered as a new `Cost::UserRead` per resource returned (S3's unmetered close reads are metered in the same change: `metrics` and `accounts` as `Cost::PostRead`/`UserRead`). `crates/radar-analyst/src/x.rs` gains `engagers(reply_id)`; `contest.rs::close_if_due` runs the walk; `close` stays pure and takes the verified sets as input.
      **What is published, per entry, on the leaderboard and the history page:** raw reposts/quotes/likes/replies; verified reposts/quoters/likes; engagers counted; how many were under 30 days old; the entrant's own account age. Counts. Never "botted", never "farmed", never a threshold beyond the published 30 days. The cluster measurements design 0011 wanted (created in the same week, engaged with last week's winner) are recorded from the same reads and shown as counts; they become a rule only by an ADR after four closed weeks, which is 0011 phase 2 unchanged.
      **The operator's veto, on the record.** `Record.voided: Option<Voided { at, reason }>` written by a new `radar contest void --week N --reason "..."`: the week pays nobody, the pool rolls over, and the history page prints the reason beside the evidence. Design 0007 §6.2 promised "if the first winner is obviously bought, the rule changes and the change is recorded"; this is the mechanism that keeps the pool from paying a farm while the rule catches up, and it is visible rather than an unpaid week that looks unclaimed.
      **The site** (`site/src/Leaderboard.tsx`): the rule text becomes the verified rule with a dated note; the table gains the evidence columns (collapsed on a phone); the empty-state sentence "The account is not live" is replaced, because it is.
      binary: `radar-analyst` (the close) and `radar-serve` (`/v1/public/leaderboard` carries the fields); the site by merge. Josh's decision (Q1). proof: re-apply by scoring raw and a fixture where one account quoted thirty times beats one repost; with the rule, it does not. The walk stops where the bound says: a fixture where entry 2's raw score is below entry 1's verified score makes exactly one scan. `just ci`; mutants on `score.rs` and the walk. Two instruments: the verified score of week 2958's winner against a hand count of that reply's reposts.
- [x] 3. **A benign refusal no longer costs the week.** `fix/refusal-kinds`. **S17:** `contest.rs::close` excludes any summoner with *any* refusal in the week, and `daemon.rs` writes every kind — `AlreadyAnswered` (somebody asked about the same coin an hour ago), `GlobalDaily` (the account's cap was spent), `Unconfigured`, `SelfOrIgnored`. So an honest entrant who asks about a popular coin is excluded from the prize for the week, and an attacker with a few accounts can burn the global cap early on Monday and have every later summoner excluded all week. Design 0007 meant the burst. Change: `RefusalLine` gains `kind` (the `Refused` variant name, `serde(default)`, absent on old lines — there are none on the box); `close` counts only `SummonerDaily`. The site's rule text says "an account that hit its daily cap that week sits the week out" instead of "anyone the admission gate refused". Also **S21**: `Gate::new(limits, vec!["radar"])` in `daemon.rs` ignores a summoner literally named `radar`, never the bot's numeric id; pass `x.user_id()`.
      binary: `radar-analyst`. proof: re-apply by counting every kind; the test where an `AlreadyAnswered` refusal still ranks the entrant fails. `just ci`.
- [x] 4. **The winner is told in a post X will accept, and a claim can only name a wallet.** `fix/the-claim-reaches-the-winner`. Three parts.
      (a) **Where the prompt goes.** Today `prompt_claim_if_due` posts under the bot's own winning reply. If Phase 0.6(a) shows X refuses a reply to our own post, the prompt moves under the **winner's summons**, whose author mentioned the bot in that post — the one reply X guarantees — which also lands in the winner's notifications. The contest `Entry` gains `mention_id` (`serde(default)`) written at close from the log's `mention_id`; `try_claim`'s predicate is unchanged (`parent == claim_prompt`). If 0.6(a) passes, the prompt stays where it is and this part is a one-line note; either way the first real prompt is read on the account and recorded in the handback (S28).
      (b) **A claim names a wallet.** `try_claim` takes the first base58 run in the reply — a winner who writes "for <mint>, pay me at <wallet>" claims the mint. Before recording, the analyst reads `getAccountInfo(address)` through the `RpcClient` it already holds: absent, or owned by the system program, is a wallet; anything else (a mint, a token account, a PDA) is refused, not recorded, and answered once under the claim with a fixed template — "That address is a program account, not a wallet. Reply again with a wallet address." — through `weekly::check` like every post. Refused means not recorded, so the winner can try again. (S19; plan 0008's S2, at claim time where the winner can act on it.)
      (c) **`Refusal::NotAWallet` at payout**, defence in depth: `radar-payout::pay` reads the owner of the claimed address before signing, and refuses. The history page shows it.
      binary: `radar-analyst` (a, b), `radar-payout` (c; not installed until a wallet exists). proof: re-apply (b) by dropping the owner check: a claim naming a mint is recorded. A fake chain returning a token-program owner refuses; a null account is accepted.
- [x] 5. **The history page, the evidence, and the handle bug.** `site/history`. Josh's ask: who won, whether they collected, whether they were paid, and the evidence behind it.
      **The document.** `/v1/public/weeks` in `crates/radar-serve/src/public.rs`, by exact path in `access::audience_of` (the test table gains the row): every closed week newest first — week, entries counted, excluded counts by reason, the winner (`handle`, id link, reply URL, score, the evidence from item 2, account age), the rule the week was scored under, the claim (`claimed_at` or "unclaimed, window closes <date>" or "unclaimed, rolled over"), the payout (signature, lamports) or why none (unclaimed / below floor / voided with the reason / not yet paid). Read from `records_in`, never the store. Excluded accounts stay counts; named accounts are winners only.
      **The page.** `site/src/History.tsx`, route `/history`, in the nav as "Past weeks". Every row checkable: the reply link, the transaction link through `solscanTx`, the claim reply link.
      **The bug (S4, the half #162 left).** `site/src/Leaderboard.tsx:698` and `Pool.tsx:934` render `@{summoner}`, the numeric id. `api.ts`'s `Entry` has no `handle`, so the site never read the field `public.rs` has sent since #162. Render `handle` when present and `userHref(id)` as the link either way (a handle can be reassigned; the id cannot — S27).
      **Wording that no longer matches the code:** Home "It does not post about coins unprompted" and About "It replies only when it is mentioned" — both false since the weekly teardown and the daily post; say "It never picks a coin to post about. Every coin it names, somebody asked it about." The leaderboard rule text per items 2 and 3.
      binary: `radar-serve`; the site by merge. proof: `just site` with the floor raised; `empty.test.tsx` gains the history page's honest empty state and a rendered claimed-and-paid row; `curl -s https://radar.heyvera.org/v1/public/weeks | head -c 400` through the edge; the leaderboard shows a handle, not `@1234567890`.
- [x] 6. **A graduated coin gets the facts that are true of it.** `fix/graduated-coins`. People ask about coins they have heard of, which skews graduated, and today the sheet for one carries nothing about the coin. Verified in the code: `crates/radar-onchain/src/dossier.rs::oldest_launch` refuses the launch block when the signature walk truncates at three pages, and the creator comes only from the launch block, so no creator record either.
      (a) **The creator from the curve account.** `radar_pumpfun::curve::BondingCurve` parses `creator` from the account (`curve.rs:123`), and the dossier already reads that account. Carry `creator` on `CurveFacts`; `FactSheet::build` in `crates/radar-roast/src/sheet.rs` looks the creator up when the launch block is unreadable. That is the headline — "93 launches by this creator, none filled a curve" — for any pump.fun coin, graduated or not.
      (b) **The slot.** `Dossier.read_at` is set only from the launch block, so a graduated coin's reply has no slot and the fidelity check authorises none. Take `context.slot` from the `getAccountInfo` result in `rpc.rs::account`.
      (c) **Two lines that are false on a complete curve.** `push_curve` prints "exit capacity: none — cannot size into this at all" for a coin that trades on PumpSwap, and the curve's fee tier for a coin that pays the AMM ladder. On `complete`, say "graduated off the curve; it trades on the AMM, which Radar does not price" and print no curve fee.
      (d) The "more history than the page budget allows" phrase says so in Radar's words on the sheet.
      Deferred with its cost: the launch block of a graduated Radar-era coin needs its launch slot; the store holds it for 550k mints; a sorted fixed-width `mint → slot` file with a seek, built by the creator-index job, is about 22 MB and one to two sessions; `getBlock` on that slot is one heavy call. Not this plan.
      binary: `radar-analyst` and `radar`. proof: a dossier with `launch: None`, `curve: Some(complete, creator)` and an index carrying that creator yields a sheet with the creator record and a headline; the reply contains neither "cannot size" nor a curve fee. Re-apply each. Then the box: `radar roast <a graduated mint>` prints the creator's record.
- [x] 7. **The bio as a noticeboard, guarded.** `feat/bio-noticeboard`. Josh reaffirmed this after the constraint was raised, so it is his decision, recorded. The consequence, once: a bio write is a public statement with no version history, on the one field that carries the automation disclosure; so the writer is built to make dropping it impossible rather than unlikely.
      **Only if Phase 0.6(b) shows `POST /1.1/account/update_profile` works on this plan** (X lists profile updates at $0.005). a new `bio` module under `crates/radar-analyst/`: a template of at most 160 characters — the disclosure line first, fixed, unless the account carries X's automated label (0.6(c)); then one of: "Week of <date> leads: @<handle>, <n> pts" mid-week, "Won the week of <date>: @<handle>. Claim: reply to the prompt under your post by <date>" after close, "Paid <n> SOL to the week's winner" after payout. Every render passes `forbidden::check` and `fidelity::check` against the record's authorised numbers, and a test asserts the disclosure survives every branch. Written through the same record-before-say path (`posts.jsonl` under `bio:<date>`), at most once an hour, and only when the text changed. The mid-week leader needs a mid-week metrics read of the week's replies (`Cost::PostRead` each, about $0.25 a day at fifty entries); it is raw, unverified engagement and the bio says "leads", never "wins".
      binary: `radar-analyst`. proof: the rendered bio is at most 160 characters in every branch and always starts with the disclosure; re-apply by dropping the disclosure and the test fails; on the box, the profile shows the line within an hour of the close.
- [x] 8. **The bot can name its own site, and names its hunters.** `feat/the-post-carries-the-link`. Josh's decision (Q2). `forbidden.rs` refuses "cabal" because a reply must not imply an identity the count cannot see (research 0012); the side effect is that no post can carry `cabalhunter.org` and the weekly result ends "Rule and leaderboard: on the site" with no link. Change: `forbidden::check` masks the exact literal `cabalhunter.org` (case-insensitive) before scanning, so every other "cabal" is still refused. The weekly summary and the claim prompt carry `cabalhunter.org/leaderboard` and `cabalhunter.org/history`; replies stay link-free, because a post with a URL is listed at $0.200 against $0.010 — two a week at most, and the price is Josh's to confirm.
      **The follow substitute.** Ask 2 cannot be built (fact 2 above). What Josh wanted from it — good accounts for people to look at, and interaction that spreads — is the hunter board: `hunter-<week>.json` is written at every close and served nowhere. Serve it as `/v1/public/hunters`, render it as a second tab on the leaderboard, and name the week's top three hunters by handle in the weekly summary ("Top hunters: @a, @b, @c"), which reaches their notifications and is the badge design 0009 L4 called the real prize. Handles are on the record since #162.
      binary: `radar-analyst`, `radar-serve`; the site by merge. proof: `forbidden::check("see cabalhunter.org/history")` is empty and `forbidden::check("a cabal ran it")` is not; the summary fixture stays under 280 characters with the link and three handles; the hunters tab renders from a fixture.
- [x] 9. **The box says what it runs.** `feat/build-sha`. The trap that caught us twice. (a) `release-linux.yml` passes `GITHUB_SHA` as `RADAR_BUILD_SHA`; every binary embeds it with `option_env!` and prints it on start; `radar-serve`'s `/health` carries it. (b) `radar brief` gains `binaries`: for each unit, whether the running executable is the installed file (`/proc/<pid>/exe` not `(deleted)`) and whether its sha256 is listed in `~/bin/BUILD-INFO.txt`, which the runbook now installs beside the binaries. Unknown when the file is absent; fails only when a deploy is half done, which is the one state worth an alarm. (c) The analyst prints the operator set on start ("operators: 2 ids") beside its posture, which is what makes Phase 0.3 checkable.
      binary: all four, and `BUILD-INFO.txt`. proof: `readlink /proc/$(pgrep -x radar-analyst)/exe`; `curl -s localhost:8402/health | grep -o '"build":"[0-9a-f]*"'`; the brief line goes red when a binary is replaced and not restarted, then green.
- [x] 10. **The payout, reviewed before a wallet exists.** `fix/payout-order-and-floor`. Read in full (`crates/radar-payout/src/lib.rs`, `main.rs`, `crates/radar-cli/src/contest.rs`). What holds: three refusals pinned by re-applied tests, read-back before the ledger says paid, the fallback through the same verify. What does not (S18): `--due` pays every claimed unpaid week in `records_in`'s **directory order**, and each payment is everything above the reserve, so with two due weeks whichever is listed first takes the vault. Sort ascending; the earliest due week takes the vault, and the runbook says so. Design 0007 J2 and 0009 L4 both say a **0.1 SOL floor with rollover** and the code has no floor: add `Refusal::BelowFloor { floor, collected }` from `RADAR_PAYOUT_FLOOR_LAMPORTS` (unset means no floor, and the daemon says which), the week stays unpaid, the pool rolls, the history page says "below the floor, rolled over". One runbook line: stop `radar-payout.timer` before a hand payment, or the timer and `record-payout` race (S29).
      binary: `radar-payout` (not installed until the wallet). Josh's decision on the floor (Q5). proof: two due weeks in a temp directory pay the earlier; a vault below the floor refuses and records nothing.
- [x] 11. **Design 0012 answered, in the repository.** `docs/design-0013`. A new design 0013, *The four asks answered*, each with Josh's words, the constraint, the platform fact, and the decision:
      1. bio → **yes, guarded** (item 7), his decision after the concern.
      2. follow/unfollow → **cannot be built** on this plan (Enterprise-only since 2026-04-16; prohibited by the automation rules at every tier); the substitute is item 8's hunters. If X ever opens follows on self-serve, it is an ADR, because a follow is a verdict about a person.
      3. unprompted posts → the two appointments **are** the automated reports; the daily one starts 2026-09-13 once Phase 0.4 is done; no third format until the daily post has four weeks of measured engagement (design 0009 §6 stands); the site's promise is reworded (item 5) so it is true.
      4. better replies → the model (item 1), the focus parser and the parent read (item 12), and the second reply in the thread (design 0010 V3) after the model has run a week.
      Plus the AI-reply-bot approval clause (fact 3): read by Josh, and either applied for or recorded as not applicable, before item 1's key.
      proof: `cargo test -p repo-conformance`; every path named exists.
- [ ] 12. **Replies answer what was asked, without the mention reaching the model.** `feat/focus-and-parent`. After item 1 has run a week. (a) `mention::read` gains `Focus`: a closed vocabulary parsed from the mention by keyword — creator, launch block, cost, graduation — passed to the model as one fixed phrase ("the asker's question, as Radar read it: the creator's record") and to the template as the lead fact. A stranger chooses among Radar's own labels and supplies no text; `an_adversarial_mention_cannot_change_the_reply` gains the case that the sheet is identical and only the order differs. (b) Design 0007 B4: a mention naming nothing that is a reply reads its parent once (`GET /2/tweets/:id`, `Cost::PostRead`) through the same parser; it is how people actually ask ("@thecabalhunter what about this one?" under a coin's post). Gate: `journalctl -u radar-analyst | grep -c -- '-> Nothing'` over the first week says how often it happens; build if it is more than a handful.
      binary: `radar-analyst`. proof: the fixture page whose mention holds no mint and whose parent holds one produces a reply; whose parent holds an instruction produces nothing; the meter is charged once.
- [x] 13. **Research 0029: the public surface reviewed.** `docs/0029`. Plan 0008 item 5, written last so it cites what landed. S1–S15 re-verified at the tip, and S16–S30 from this review:

      | # | finding | severity | disposition |
      |---|---|---|---|
      | S16 | quotes and replies are unbounded per account, so one account farms the top score for nothing | high | item 2 |
      | S17 | any refusal, including "somebody asked first", excludes an entrant for the week; the global cap can be burned to exclude everyone | high | item 3 |
      | S18 | `--due` pays the whole vault to whichever due week the directory lists first; no floor | medium, latent | item 10 |
      | S19 | a claim that mentions the coin before the wallet claims the mint; a program-owned recipient is paid | high | item 4 |
      | S20 | the model call is metered nowhere | medium | item 1 |
      | S21 | the gate's ignore list is the literal `radar`, never the bot's id | low | item 3 |
      | S22 | the analyst runs on the free public RPC | medium | Phase 0.2 |
      | S23 | `RADAR_CONTEST_OPERATORS` set twice; nothing prints the set in force | low | Phase 0.3, item 9 |
      | S24 | no alert channel on a public bot | medium | Phase 0.5 |
      | S25 | the seven-days timer is not installed | low | Phase 0.4 |
      | S26 | the public leaderboard folds the whole reply log per request; the edge cache is per URL, so a query string busts it | low | a Cloudflare cache rule ignoring the query string (Josh); recorded |
      | S27 | the leaderboard links by handle, which can be reassigned | low | item 5 |
      | S28 | the claim prompt may be refused by X's reply rule, and it does not mention the winner | high until tested | Phase 0.6(a), item 4 |
      | S29 | the payout timer and a hand payment can both send | low | item 10, runbook |
      | S30 | AI reply bots may need X's written approval | unknown | Phase 0.6(c), item 11 |

      proof: `cargo test -p repo-conformance`.
- [x] 14. **Documents changed in the same commits as the behaviour**: `docs/STATE.md` (the analyst's live state, the rule, the voice, the model's meter); `deploy/README.md` (the verification table dated 2026-08-25 lists neither the analyst nor the seven-days units; `RADAR_RPC`; `BUILD-INFO.txt` beside the binaries; the four console answers; the payout floor and the timer-versus-hand line); `README.md` crate rows for `radar-contest` and `radar-analyst`; a new LEARNINGS entry only if Phase 0.6(a) fails -- "a mechanism built against a platform rule that had already changed" -- and not otherwise; and the two memory notes outside the repo -- production truth, and the analyst being built -- are both stale now that the analyst runs and the account is live.

## Growth, sorted by what can be measured

Josh asked for the version that goes viral. Every line here either has a number that decides it or names the one that will.

- **The receipt is the product.** The "seven days later" post starts 2026-09-13 (Phase 0.4). It is the one thing nobody else can post. Measure its engagement against the replies' for four weeks before adding any format (design 0009 §6).
- **Answer the coins people ask about.** Item 6. The measurement: the share of replies whose sheet carries a creator record, before and after (`grep -c 'tokens this creator' replies.jsonl` against the line count).
- **A voice.** Item 1. Measure the model reply's engagement against the template's over the same weeks; the log carries `fellback`, so the split is free.
- **The link in the post.** Item 8. Measure `/v1/public/*` requests from the weekly post's day against a weekday (the edge's analytics).
- **An X community.** Yes, and by hand: Josh creates "Cabal Hunter" in the app; the bot's account joins; summons happen in the room and the weekly result is cross-posted there. Posting into a community by API needs `community_id` on `POST /2/tweets`, which is on the docs and unverified on this plan — Phase 0.6 gains (e): one test post into the community. If it works, `weekly.rs` posts the summary there too, priced as a post. The measurement is members and summons per member per week, read from the log by author.
- **The hunters.** Item 8 names them; the tab shows them. Being named by the account is the badge, and it costs nothing.
- **Telegram.** Built; a token from Josh switches it on; free volume for people who want twenty coins a day, and none of it is in the record.
- **Refused, with the reason:** an unprompted post about a coin nobody asked about (the promise on two pages, and the touting line); automated follows (impossible and prohibited); engagement pods or reciprocal likes (the thing item 2 exists to price out); any post that names a token's price (ADR 0013 constraint 5).

## Design 0011, decided

**Accepted, with three amendments.** Its placement argument is right: the leaderboard is the product, and an exclusion at payout is a private correction to a public error. Its three unverified numbers held up: the per-entrant scan really is a few hundred metered user reads a week at scale, the walk down the ranking really is a handful, and the baseline really is empty (one dry-run week, and the account went live today). What it missed, and what this plan changes:

1. The rule it scans is farmable from one account at zero cost (S16), so the scan is folded **into** the rule as the verified score (item 2) rather than measured beside a rule that stays wrong.
2. The engager reads bill per user returned at ten times the post-read price, so the scan is bounded to one page per endpoint and stops at the arithmetic bound, and every read is metered.
3. Publish the measurement and never the verdict — kept exactly. Phase 2 (a cluster threshold) stays an ADR after four closed weeks. The operator's veto with a published reason (item 2) is added, because 0011's own weakness — "raises the cost, does not work" — needs a lever that is visible when it fires.

The claim-address check stays at payout and gains its twin at claim time (item 4), where the winner can act on a refusal.

## Open questions for Josh

Answered 2026-09-06 unless noted.

- **Q1** The verified rule in item 2 — reposts and quoters ×3, likes ×1, over engagers at least 30 days old, replies zero, with the evidence published and an operator veto on the record. **Assumed: yes.**
- **Q2** The exact `cabalhunter.org` allowed in the two weekly posts, at the URL price. **Assumed: yes.**
- **Q3** The model. **Answered: build the OpenAI provider (path B), and use the key Josh already has.** Item 1 carries it.
- **Q4** The bio noticeboard: the disclosure line first unless X's automated label is set on the account. **Assumed: build it if 0.6(b) passes.**
- **Q5** The payout floor, 0.1 SOL as designs 0007 and 0009 say. **Assumed: build it, before the wallet.**
- **Q6** The X community, created by hand, the bot posting into it if the API allows. **Assumed: yes.**
- **Q7** Voiding a week is the operator's decision with a public reason, written by a command on the box. **Assumed: yes.**

## Verification

- **Re-apply the bug** for every rule: the verified score (2), the refusal kinds (3), the wallet check at claim and payout (4), the graduated curve lines (6), the disclosure in the bio (7), the masked literal (8), the due-week order and the floor (10).
- **Two instruments compared:** week 2958's verified winner against a hand count of the reply's reposts and likes; the graduated-coin sheet's creator against the creator index's own row; the brief's `binaries` line against `sha256sum` by hand.
- **The platform, not a screenshot of intent:** Phase 0.6's four answers written in the runbook with the date; the first real claim prompt read on the account.
- **The browser** (design 0008 §10) for items 5 and 8: every route at 375 and 1280, console clean.
- **Mutants in CI** on each item's changed behaviour; a survivor becomes a test or a `.cargo/mutants.toml` line with its reason.
- `just ci`, `just site`, `cargo test -p repo-conformance` on every commit that touches a document.
- **Deploy is not merge.** Every item above names its binary; the handback records the sha installed and the `readlink` that proved it running.

## Only Josh can do

Phase 0.2–0.6; the model key (after item 1); the Cloudflare cache rule (S26); the community; the automation-rules read; the restarts.

## Handback

**Item 1, 2026-09-06 (Opus 5).** Josh answered Q3 with path B: the OpenAI provider is built rather than an Anthropic key configured. Started with the meter, because the plan's own ordering says a key must not reach the box before a call is charged for.

One departure from the plan's wording, and it is the reason rule 9 exists. The plan said `Reply` carries `cost: Option<MicroUsd>`. `Option` cannot tell **no call was made** apart from **a call was made and the provider did not report its cost**, and those settle in opposite directions: the first releases the reservation, the second charges it. `radar_model::Answer.cost` already carries that ambiguity safely because its caller knows a call happened; a reply that fell back does not. So `voice::Reply` carries [`Billed`](../../crates/radar-roast/src/voice.rs) — `NoCall`, `Reported(MicroUsd)`, `Unreported` — and the meter reads it instead of inferring from `fellback`.

**Josh's follow-up, mid-session:** keep Sonnet 5 available to switch to later, use OpenAI now while there are credits there. Nothing had to be built for the first half — [`api_key.rs`](../../crates/radar-model/src/api_key.rs) already speaks Anthropic's Messages shape, so `claude-sonnet-5` was always three environment variables away. So the OpenAI provider was built as a **sibling** rather than a replacement, sharing the endpoint, model-name and two price variables. Moving between vendors is an env edit and a restart; there is no build in it. Setting two keys at once stays refused rather than resolved — with three providers the refusal now names the ones it found, because "more than one is set" sends an operator to read a file they have only just edited.

Deliberately **not** built: a runtime chain that falls back from one vendor to the other. The fallback on an unreachable provider is the deterministic template, which is rule 8 and is a working product. A second live credential on the box would be a second bill and a second thing to rotate, and it would contradict the refusal above.

**Item 3, 2026-09-06 (Opus 5).** `RefusalKind` is typed rather than a string, because `why` is a sentence written for a person and is the wrong thing to branch on. `None` -- a line written before kinds existed -- is counted the **old** way: rule 9, absent is unknown rather than benign, and a harmless default would silently re-admit an account that really had burst the gate. S21 came with it.

Both merged: #168 at `914a0fd`, #169 at `81e422a`.

**#171, unplanned, found while writing the commands to put a key on the box.** `daemon.rs` built its provider with `radar_model::from_vars(&env).ok()`, throwing the reason away. `Selection` names every missing variable at once precisely because an operator setting a key up has no other signal, and that message reached nobody: a key with a price missing produced an account that answered exactly as it had the day before. Rule 8 has two halves there and `.ok()` collapsed them — an unconfigured provider is a resting state, a mis-configured one is a mistake.

**Item 5, in part.** The `@1234567890` bug (S4, S27) is fixed and merged at #172. `public.rs` had sent `handle` since #162; `api.ts` never declared the field. The handle is shown, the numeric id is linked, and with no handle the id is shown bare — `@1234567890` reads as a name somebody chose. `public.rs` now also sends `"handle": null` on the open-week path rather than omitting the key, because the site types both shapes of that document with one interface. **The history page and `/v1/public/weeks` are not built.**

**The model, decided 2026-09-06.** `gpt-5.6-luna`, $0.20/$1.20 per million, about 59 cents a month at the 50-reply cap. Two findings behind it, both measured rather than assumed:

- **Fifteen of the sixteen OpenAI models released in 2026 reason by default**, and reasoning tokens bill at the *output* rate while never reaching the reply. On a three-sentence write from a fixed sheet that is the whole bill and none of the product, and it fails silently: the model can spend the entire `max_completion_tokens` ceiling thinking, return empty `content`, and be charged for a reply that then ships the template. `RADAR_MODEL_REASONING_EFFORT` (#173) is the guard.
- **The prices can come from a catalog, and I said they could not.** `models.dev/api.json` is MIT-licensed and provider-agnostic. "A default price is a spending decision made by whoever wrote the code" is an argument against hard-coding, not against automation — copying two numbers by hand is the same numbers with a transcription error available. `radar model-prices` (#174) reads the catalog and prints the lines; `--check` reports drift. **The daemon still never calls it**: a third party's number inside the budget's own accounting would under-count silently when it went stale, which is that rule's failure relocated rather than removed.

**Unverified, and it will be settled by one curl before the key goes on the box.** `openai.rs` sends the system prompt as `{"role": "system"}`. OpenAI's current docs list `developer`, `user`, `assistant` for the Responses API and do not say whether Chat Completions still maps `system` on a 2026 model. It fails safe — a rejected role is a 400, so `Unreachable::Refused`, the template, and a `fellback` line in the log — but it would look like the model simply not working while being paid for.

**A process error, recorded because it was mine.** #172 changes the public site, and merging deploys it. I told Josh it was his call and then armed auto-merge on it by accident while arming two others. It merged. The visible impact was nil — the leaderboard has no entries, so no name renders either way — but the approval was not given and the deploy was not reversible by the time I noticed.

---

### The rest of the plan, 2026-09-06 to 2026-09-07 (Opus 5)

**Every item is built.** What is left is on the box or on the platform, and it is listed under *For Josh* below.

**Item 2 — the rule.** #179, #180, #181, #182. The score prefers `Verified` — distinct accounts that reposted, quoted or liked, each counted once and each old enough to meet the same floor an entrant meets — and keeps the raw metrics beside it as evidence. `RADAR_X_PRICE_USER_READ` is a **sixth required price**, and `Prices::from_vars` is all-or-nothing: adding a seventh takes the analyst silent until the box is told the number. That is rule 8 working and it is the thing to know before adding one.

**Item 4 — the claim.** #183. The prompt goes under the winner's **own summons**, which is the one reply X guarantees, with the bot's winning reply as the fallback for the single week closed before mention ids were recorded. A claim naming a mint is refused at claim time and again before a signature.

**Item 6 — the graduated coin.** #184. Three things wrong, all the same mistake: an empty read treated as a measurement of zero. The creator lookup sat inside the launch-block arm, so a coin with real history — every coin anybody asks about — got no creator record at all. The curve said "cannot size into this at all" about a coin trading fine on an AMM. And `read_at` came only from the launch block, so every figure was published with no slot beside it. `RpcClient::account` returns the slot with the bytes now, in one struct, so dropping it is no longer the path of least resistance.

**Item 5 — the history page.** #185. `/v1/public/weeks` and `site/src/History.tsx` at `/history`. "Not paid" has four causes and `payout.state` says which; `rule: null` reads "not recorded", never today's numbers. **The winner's account age is not on it**, and the document says so rather than dropping it quietly: `Standing` carries it at close and nothing writes it down, so publishing an age read today would be a different number about a different moment.

**Item 8 — the link and the hunters.** #187. `forbidden::check` masks the exact literal `cabalhunter.org` before scanning, so the account can name its own site and `cabalhunters.org`, `cabalhunter.org.evil.example` and "a cabal ran it" are all still refused. The summary was at 280 to the character, so the claim window is a date now rather than a timestamp — the same trim `claim_prompt` already made. The hunters get a **third post in the thread**, because three handles do not fit and a post cut off mid-figure is a wrong figure.

**Items 9 and 10 — provenance, order, floor.** #188. `RADAR_BUILD_SHA` via `option_env!`, on `/health` and the analyst's start line; `radar brief`'s `binaries` check reports **FAIL** on a binary replaced under a live process and **????** when it cannot look. `--due` sorts ascending (S18 — the directory's order decided who got the vault), and `Refusal::BelowFloor` rolls a thin week over.

**Items 11, 13, 14 — the documents.** #189 and #190. Design 0014 answers design 0012's four asks. Research 0029 carries S1–S30 with dispositions, re-verified at `9c98617`. `STATE.md`, `deploy/README.md` and the two `README.md` crate rows now describe what runs.

**ADR 0014, unplanned, from a question Josh asked mid-session.** Can the Codex/ChatGPT subscription carry the LLM layer? No, and ADR 0004's existing "private-use-only" line was not a sufficient answer, because its reason was *"not a foundation for a sold product"* and this account is not sold. Checked rather than recalled: the vendor splits the two sign-in methods by **use** — plan sign-in for interactive work, API keys for automation — and the usage stops being personal the moment a system calls while the account holder is not in the loop. Reliability would bite first anyway: the quota is a five-hour rolling window shared with the browser and the IDE, and a rate limit reached is silence rather than a refusal the meter can see.

**Unplanned, found while adding the third post.** `announce_week` reserved **one** `Cost::Post` for the whole thread. That was already short by the teardown and would have been short by two. A meter that under-reports is worse than none, because the day's cap is computed from it. `reserve_thread` prices a post and a reply for each one after it and returns as many as the budget covers; the thread is cut to fit rather than posted in full and charged for part.

**What the mutation runs caught, which is the part worth reading.** Twenty-one survivors across five branches, and none of them was a missing assertion on a happy path:

- The claim prompt's `==` inverted posts under a **losing entrant's** summons — the winner is never told and somebody who did not win is publicly invited to claim. It survived because the only test reaching that code had one entrant, so the winner and the first non-winner were the same row.
- The floor's `<=` withholds a prize from a week that reached the bar exactly; `==` refuses only the amount equal to the floor and pays everything under it, which is the opposite of a floor. Both survived because the tests were in the *other* crate and `cargo mutants` runs each crate's own suite.
- `>` in the thread-shortfall notice prints an alarm on every ordinary week and stays silent on the one week that actually lost a post.
- The exclusion counter's `+=` — every reason appeared once in the fixture, and 1 is a fixed point of `-=` and `*=` alike.

Four entries were added to `.cargo/mutants.toml`, each with the argument written out and each applied by hand first. One is worth naming because it is neither of that file's two categories: `build_sha -> None` is unobservable because `option_env!` is fixed when the crate is compiled and no test compiles it twice — not equivalent, since a release build would lose the commit from three places. A test for `running_exe` that returned early on Windows and asserted `None` on Linux **was written, ran green under both mutations, and was deleted**: a non-root process cannot `readlink` another user's `/proc/<pid>/exe`, so both versions return `None`. It was the check that cannot fail, and keeping it would have been worse than the entry.

### Phase 0.6(b) and (c), answered by probe

**0.6(b): yes.** `POST /1.1/account/update_profile` returns **200** on this account's plan, established on 2026-09-07 by a deliberately non-destructive probe: read the profile, write the `description` back byte-identical, read it again and diff. The profile was unchanged field for field, which also confirms — rather than assumes — that v1.1 updates only the parameters supplied and leaves `name`, `url` and `location` alone.

**It took two attempts, and the first one is worth recording.** The probe returned `403` with code 326, *"this account is temporarily locked"* — on an account that had posted the weekly summary normally twenty minutes earlier and was reading mentions throughout. Josh cleared the lock and the same probe returned 200. So a refusal on this endpoint says nothing about whether the account can still post, and the two capabilities are gated separately.

**0.6(c): the account carries X's own automated label** (`Automated by @1xmint_`, read off the live profile the same day). So the bio's fixed lead does not have to be the disclosure and is free to be the account's own copy.

**Item 7 is therefore built.** `crates/radar-analyst/src/bio.rs`, off unless `RADAR_BIO_LEAD` is set. The design departs from the plan's wording in one place and it is the hazard the plan itself named: the plan said the disclosure line goes first, fixed. What is fixed is a **lead the operator supplies**, because the failure that cannot be undone is not a missing disclosure — X's label covers that here — it is an unconfigured instance overwriting the account's real copy with a status line. Unset means never written; the daemon says on every start whether it is on and what it will write.

### Phase 0.1, read after it happened — and it had not

The plan asked for one thing at the end: read tonight's close. It did not happen, and the reason is the most valuable finding of the session.

**Week 2957 closed at 00:00 UTC on 2026-09-07 and no record was written.** The daemon was alive, polling, and had been failing every five minutes for ninety minutes:

```
radar-analyst: cannot write the week's record: Read-only file system (os error 30)
```

`deploy/radar-analyst.service` grants `ProtectSystem=strict` with **one** `ReadWritePaths` entry, the analyst's own directory. `Paths::under` puts the contest's directory *beside* it rather than inside it — deliberately, because `radar-serve` reads the same records as `RADAR_CONTEST_DIR` and one ledger with two readers must not be two locations. So the analyst could write its log, its cursor and its spend ledger, and could not write the thing the entire contest is made of. **It could not have closed a week. Ever.**

**Nothing showed for six days**, and that is the part worth carrying. A directory written on every tick fails on the first tick, while somebody is watching a deploy. A directory written once a week fails at the one moment nobody is watching — and until then every available signal says the deployment is correct. `radar brief` was reporting a five-day-old record as healthy, because on a Wednesday that is what healthy looks like.

Fixed at three levels in #191, because none of them holds the property alone: the unit file (every future install, nothing already installed), a start-up probe in the analyst (fixes the *timing* — the failure now arrives at a restart), and `brief::contest`'s probe **before** the records are read (a start-up line scrolls away; the brief runs on a timer). Both probes write and remove a file rather than reading permissions, because the directory was `0755` and owned by the right user and the kernel refused the write anyway. LEARNINGS 32.

**Stopped at:** every item merged except #191, which is in CI. Items 1–6, 8–11, 13 and 14 are on `main`.

**Next action:** none in the repository. The list below is the box and the platform.

---

## For Josh

Seven things, and only these. Everything else is done, and the first one is the only urgent one.

0. **Reinstall the analyst unit and restart it** — this is the one that costs a week if it waits. #191 fixes `ReadWritePaths`; the file on the box is the old one, so the contest still cannot close.

   ```
   sudo install -D -m644 ~/radar/deploy/radar-analyst.service /etc/systemd/system/radar-analyst.service
   sudo systemctl daemon-reload && sudo systemctl restart radar-analyst
   ```

   The next tick then closes week 2957, and the start-up line will say so if anything is still unwritable. Pull the repo on the box first — `~/radar` is a checkout.

1. **Install `radar-seven-days.timer`** (root; commands in `deploy/README.md`, "The two appointments"). Without it the first "seven days later" post finds no file and posts nothing. S25.
2. **Read X's Automation Rules** at <https://help.x.com/en/rules-and-policies/x-automation> and either apply or record it as not applicable. The page returned 403 to this session, so it is a report and not a verified fact — and it stopped being hypothetical the moment the model provider was configured. S30, ADR 0014.
3. **A Cloudflare cache rule ignoring the query string on `/v1/public/*`.** The edge caches per URL, so `?x=1`, `?x=2` each miss. Worth doing before a link goes wide; no code change would help. S26.
4. **Set `RADAR_BIO_LEAD`, or leave item 7 off.** Phase 0.6(b) is answered — `POST /1.1/account/update_profile` returned **200** on 2026-09-07, so the bio noticeboard is built and merged. It writes nothing until this is set, because a bio write overwrites the only copy of whatever the profile says. **Copy the account's current bio into it first**, or the first write replaces it and there is nowhere to read it back from. `deploy/analyst.env.example` carries the shape.
5. **Deploy the merged binaries and install `BUILD-INFO.txt` beside them** (`scp ./dist/BUILD-INFO.txt guardian-vps-tail:~/bin/`). `radar brief`'s new `binaries` check reads it; without it that line reports `????`, which alarms.
6. **Set `RADAR_PAYOUT_FLOOR_LAMPORTS`** when the token exists. `100000000` is 0.1 SOL, the figure design 0007 J2 and design 0009 L4 both name. Unset means no floor, and the process prints which it is using on every run.

**Two things that will only be settled by the first real week.** The claim prompt has never been posted, because no week has had a winner (S28) — if X refuses it, that is a `LEARNINGS.md` entry and not a surprise. And item 12's gate: `journalctl -u radar-analyst | grep -c -- '-> Nothing'` over a week says whether the focus parser is worth building. Build it only if that is more than a handful; if people are not asking that way, it is a rule 4 surface nobody needed.
