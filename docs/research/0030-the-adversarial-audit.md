<!-- SPDX-License-Identifier: Apache-2.0 -->
# Research 0030 — The adversarial audit

**Status:** **complete for the findings it closes, open for the four it does
not.** Every row below has a disposition, and every disposition names either a
merged change or the reason nothing was changed.

**What this is.** On 2026-09-07 the repository was read against itself: every
design, ADR, research note and LEARNINGS entry, then the code they describe,
then the production box, then the external facts the product depends on. The
review was written to disagree with the documents rather than to summarise them.
Twenty-four findings came out of it, one critical enough to invalidate the
guarantee the whole custody design rests on.

**Written last on purpose**, in `0029`'s shape. A review written before the
fixes is a list of worries; this one cites what landed, so a reader can check
each line rather than take it.

**Written against:** `main` at `634d384` (#193), and the deploy files that
describe the box. Where a finding is about the running box rather than about the
tree, the row says so — Tailscale SSH wanted a browser login during this session,
so **the live box was not re-read here**. Findings H1, H2, H9 and C2 are
therefore recorded as the review found them and not re-verified.

**Every PR cited below merged on 2026-09-07**, in the order 198, 200, 199, 201,
202, 203, 204, 205 — the signer first because it is the critical one, the monitor
second because its LEARNINGS entry numbers after the signer's. So a disposition
reading "fixed, #N" is on `main` and can be read there rather than taken.

**Written by:** Claude Opus 5, 2026-09-07.

---

## How to read the severity column

Severity is about **what a stranger or a failure could make happen**, not about
how hard the fix was.

- **Critical** — the property the design is built on does not hold.
- **High** — money, a public falsehood about a named person or coin, or an
  outage a stranger can cause for free.
- **Medium** — wrong in a way that costs money or attention but not trust.
- **Latent** — the code is wrong and the state that triggers it has not occurred
  yet, usually because nothing trades and no token exists.

---

## Critical

| # | finding | disposition |
|---|---|---|
| C1 | **The signer did not bound the size of a swap.** `verify::lamports_transferred` sums system-program `Transfer` instructions and a swap makes none: a pump.fun buy carries its lamports in instruction data the signer never decoded, so every buy scored **zero** against every ceiling — the authorisation's, the operator's policy, and `Canary`'s dust bound alike. AGENTS.md rule 1 says the signer re-derives the transaction and checks it against the authorisation's bounds; for the only kind of transaction Radar would ever sign, it did not. Latent, because `Policy::CLOSED` means nothing signs. | **fixed**, [#198](https://github.com/hey-vera/radar/pull/198). `verify::lamports_bought` decodes every pump.fun instruction through `radar_decode` and adds each buy's lamports to the spent total. Buys only — a sell's lamport field is a *minimum output*, not a spend, and counting it would refuse a large exit for being large. An unreadable venue instruction is `UnreadableVenueInstruction`, not a spend of zero. LEARNINGS 33. |
| C2 | **The account is an AI reply bot without X's written approval.** X's Automation Rules require prior written approval for AI-powered reply bots; the OpenAI provider went on the box on 2026-09-06 and the account was temporarily locked (code 326) the same day. | **open, and Josh's.** Nothing in this repository can close it. Recorded in [`docs/STATE.md`](../STATE.md) and in the only-Josh list. The one thing the code could do about it landed: `Unreachable::Refused` now keeps 300 bytes of the platform's body, so the *next* lock says which lock it is instead of arriving as an empty status ([#201](https://github.com/hey-vera/radar/pull/201)). |
| C3 | **The gate was a free daily denial of service and a contest race.** `global_daily` (50 on the box) was spent in arrival order with nothing spreading it out, so seventeen throwaway accounts asking about three real mints each at 00:01 UTC silenced the account for everyone until midnight — every mention individually legitimate, no rule broken. Separately, `AlreadyAnswered` carried the existing reply's id and nothing read it, so a duplicate got silence — and because the first asker's reply is their contest entry, a script that asks first about every trending launch owned the entry on the hottest coins. | **fixed**, [#201](https://github.com/hey-vera/radar/pull/201). An hourly token bucket under the daily cap: a burst of ten, then `global_daily / 24` refilled continuously. A bucket rather than a window, because a window resets on a boundary and fifty either side of it is a schedule that is decorative. A duplicate now gets a pointer reply. The contest itself stays unpromoted until the token exists (design 0009 L1–L6). |

---

## High

| # | finding | disposition |
|---|---|---|
| H1 | **The health monitor resolved its paths against the wrong root.** `deploy/radar-brief.service` set no `WorkingDirectory`, so systemd started it in `/` and three relative literals pointed at the root of the filesystem. Two of the three wrong lines it produced were **`[ok]`**. | **fixed**, [#200](https://github.com/hey-vera/radar/pull/200). `Tree::around` derives the analyst, contest and creator-index paths from `--store`, mirroring `Paths::under` rather than restating it. `WorkingDirectory=` and `ReadWritePaths=` added as belt. LEARNINGS 34. |
| H2 | **Merge is not deploy, third occurrence.** The site ships on merge and `radar-serve` is installed by hand, so the History page called an endpoint on a server four merges old. | **half fixed**, [#200](https://github.com/hey-vera/radar/pull/200). `radar brief`'s `deployed` check compares `/health`'s `build` to `BUILD-INFO.txt`'s commit and fails on a mismatch, so the drift is *noticed*. Design 0010 A-4 — ship by tag, the box pulls and verifies — is the actual answer and is still unbuilt. |
| H3 | **"Seven days later" would publish false statements about named coins.** The store measures a token at 1 h, 6 h and 24 h after launch and never again, so for any coin older than a day when asked about — most coins people ask about — the latest outcome predates the reply and `last_transfer_slot <= read_at_slot` is true *by arithmetic*. The post said a coin "had no transfer since we answered" while it traded on the AMM. `held_bps` was first-fill-to-24h, labelled as if a week had passed. | **fixed**, [#203](https://github.com/hey-vera/radar/pull/203). `quiet_since_reply` requires the outcome to be newer than the reply; every clause names its horizon; a fourth checkpoint at seven days is measured **for mints in the reply log only**, so the post can say something about a week without giving 35,000 daily launches a query each. |
| H4 | **The site's headline was an unmeasured claim.** "Most launches are coordinated", with the card directly underneath measuring the opposite: 70.5% of launches pay one to three recipients and 0.02% of those are bought out instantly (`0024`). | **fixed**, [#204](https://github.com/hey-vera/radar/pull/204). The hero states a measurement the card is evidence *for*. `figures.test.ts` asserts the shape cannot come back, reading published copy with comments stripped. |
| H5 | **The fidelity check bound numbers to nothing.** A literal passes if *any* authorised value rounds to it at the literal's own precision, and `authorised()` harvests every numeral out of the rendered block — so with fifteen figures on a sheet, most small integers are authorised by something. "3 launches by this creator" passes off a 2.9% population share. Number words are not scanned at all. | **fixed**, [#202](https://github.com/hey-vera/radar/pull/202). Fact-slot rendering: the model writes `[F1]`-style tags and **no digits**, and Radar substitutes its own rendered strings. `fidelity::check` stays as the second lock and now passes by construction, which is the point — a check that can only fail when something upstream is broken is one whose silence is informative. |
| H6 | **The reply loop lost state and dropped questions.** The gate was in-memory only under `Restart=always`; a log-write failure broke the loop and then advanced the cursor over the whole page; a refused reply reservation skipped a mention silently; `$TICKER` mentions were printed and never answered; `ureq`'s free functions carry no timeout on a single-threaded loop; `Unreachable::Refused` discarded the platform's body. | **fixed**, [#201](https://github.com/hey-vera/radar/pull/201). All six. `Gate::restore` rebuilds from `replies.jsonl`; the cursor advances only over mentions that finished; a spent reply budget stops rather than skips; tickers are answered and gated on the symbol; one `ureq::Agent` with a 15 s global timeout; 300 bytes of refusal body kept. |
| H7 | **The trading lane would fail three ways before losing money honestly.** (a) `signer_client` hard-coded `now_slot: 0`, which is inside every window, so ADR 0008's staleness check was unreachable from production. (b) `read_positions` filtered on `opened_at` and admitted a `closed_at` from the future — the one watermark leak in the *permissive* direction. (c) No compute budget, `skipPreflight`, no simulation, no reconcile. (d) `mint_authority` was parsed and gated nothing. (e) `execute()` never reads `authorization.action`. | **(a), (b), (d) fixed**; (c) and (e) **open**. (a) [#198](https://github.com/hey-vera/radar/pull/198); (b) [#199](https://github.com/hey-vera/radar/pull/199); (d) [#198](https://github.com/hey-vera/radar/pull/198), as `can_be_diluted` — deliberately not folded into `can_be_stopped`, because a live mint authority does not cancel the sale, it makes what is sold worth less, and one name for both would be a name that lies. **(c) is left unbuilt on purpose**: nothing should be sending until an edge exists, and building a send path now is building against a lane the repository's own research measures at 0 bps. **(e) is open and cheap** — `execute()` always builds a buy — and is recorded here rather than fixed, because it was outside the scope this session was asked for. |
| H8 | **The recorder's cursor was the one state file not written atomically.** Every other one stages and renames; `store::write_cursor` was a plain `fs::write`. A torn cursor reads as `None`, which is indistinguishable from a store that was never followed, and the follower restarts near now and skips the gap in silence. | **fixed**, [#199](https://github.com/hey-vera/radar/pull/199). Temp and rename. |
| H9 | **The public origin was a two-core box with no edge cache, no rate limit and a chatty health page.** `/health` is reachable by anybody and carried the day's model spend and the provider's last refusal verbatim; `RADAR_BIND` fell back to `:8080` on a typo. | **partly fixed**, [#204](https://github.com/hey-vera/radar/pull/204). The spend and the refusal text moved to `/v1/store`, which is the operator's; `RADAR_BIND` refuses to start on a value it cannot parse. **The Cloudflare cache and rate rules are Josh's** and are not in this repository — `cf-cache-status: DYNAMIC` on every `/v1/public/*` probe means the 60-second `Cache-Control` has no cache rule to apply it. |
| H10 | **The account's own posts skipped the sanitiser, and the week close read before it metered.** `weekly::check` ran the two checks over raw text while every *reply* went through `render::for_publication`, so both were defeatable by one zero-width character and the 280 cap was unenforced. `scan_ranking` read three engager pages and then asked the meter; `x.metrics` and `x.accounts` were not metered at all. | **fixed**, [#201](https://github.com/hey-vera/radar/pull/201). Sanitise, then check, then publish — and what is logged is what goes out. The scan reserves a bound before the read and settles at what it billed; both close-time reads are metered. |

---

## Medium

| # | finding | disposition |
|---|---|---|
| M1 | `forbidden::check` is a 31-phrase substring list, and none of its phrases was the word a sharp model reaches for. | **fixed**, [#202](https://github.com/hey-vera/radar/pull/202). `honeypot`, `exit liquidity`, `dumped on`, `100x`, `bullish`, `looks clean` and the typographic apostrophe in `don't buy`. Stated in the code as the weaker half of the defence: a substring list can only refuse what somebody thought of. |
| M2 | The base-rate snapshot's age was never checked by the daemon; a stale distribution would be published forever. | **fixed**, [#204](https://github.com/hey-vera/radar/pull/204). Dropped past `STALE_AFTER_DAYS`, using the crate's own threshold rather than a second number. |
| M3 | `FactSheet::authorised()` harvests every numeral from label text, so adding a label with a figure in it authorises that figure. | **made irrelevant** rather than fixed, [#202](https://github.com/hey-vera/radar/pull/202). The model can no longer write a digit, so what `authorised()` admits is no longer the thing standing between a fabrication and the timeline. The function is unchanged and its looseness is now a property of the second lock. |
| M4 | `State::Leads` — the mid-week bio ranking — is never constructed. | **open.** The bio is off until somebody says what it may keep (#193), so a state nothing reaches is consistent with that. Recorded so it is not mistaken for working. |
| M5 | `radar consider` spent ~320 Jupiter calls an hour on a lane `Policy::CLOSED` will not let trade. | **fixed**, [#205](https://github.com/hey-vera/radar/pull/205). Curve pricing by default: two account reads per candidate, and one aggregator call per pass for the SOL price. Measured against Jupiter on a live mint — two lamports apart across a 50,000× size range, and a near-constant 1.2-point difference in the *impact* figure, which is the fee riding in the dust reference. |
| M6 | `RADAR_STATE_DIR` and `RADAR_SIGNER_POLICY` are both required to start and were absent from the env examples. | **fixed**, [#204](https://github.com/hey-vera/radar/pull/204). LEARNINGS 30's shape. |
| M7 | `dependabot.yml` watched `/web` and not `/site`; `mutants-shards` was the one checkout keeping its push token. | **fixed**, [#204](https://github.com/hey-vera/radar/pull/204). `/site` is the half with an audience and was the half nothing watched; `mutants-shards` is the job that runs deliberately damaged source hundreds of times. |
| M8 | `SECURITY.md` stated three things that are no longer true; `deny.toml` claims a daily run no workflow schedules. | **fixed here.** See below. |
| M9 | `radar-exec/src/lib.rs` names a `reconcile` stage that does not exist. | **fixed here.** The doc names the five modules the crate has. |
| M10 | The recorder and both crons run on the free CryptoHouse endpoint (7×429 in 24 h); `simulate_exit` is priced at the x402 floor while making eight Jupiter calls. | **open, and partly Josh's.** A paid tier is a spending decision. The instrument's own cost dropped with M5's default and its price was not changed, which is now conservative in the right direction. |
| M11 | No HSTS in the Caddy block; `radar-follow` and `radar-analyst` lacked `MemoryDenyWriteExecute`. | **fixed**, [#204](https://github.com/hey-vera/radar/pull/204). |
| M12 | `now_unix()` returns 0 on a bad clock, and every token then reads as unexpired. | **open.** Not reached today — the tokens it gates are the customer lane's, which has no customers — and the fix is a decision about what a serving process does with a clock it cannot read. Recorded rather than guessed at. |
| M13 | `features.rs` has 24 features, not the 23 every document says. | **open.** A documentation count, checked here and left for whoever next edits those documents; changing the number in five places without re-reading the feature list would be the kind of edit that makes a document *look* maintained. |

---

## What was found and deliberately not done

Three things, each with its reason, because a review that silently drops its own
recommendations is a review nobody can audit.

**`catch_unwind` around each per-tick step of the analyst daemon.** The plan
asked for it. With the gate now restored from disk, a panic under
`Restart=always` costs one tick and no state, while `AssertUnwindSafe` over the
daemon's mutable state buys resilience by discarding the invariant that made the
state worth trusting. Not built; recorded here.

**Deleting the custody lane** (`radar-customer`, the Privy and Turnkey signers,
the wallet routes, `web/src/Wallet.tsx`, `siws` — some 2,400 lines with zero
customers). AGENTS.md §5 says a layer nothing depends on is a document that
compiles, and that is the honest reading. Deleting it is the owner's decision
and not a reviewer's, so it stays and this is the record that it was considered.

**Moving the whole agent block off `/health`.** The plan asked for it; it would
leave `radar brief`'s agent check permanently unable to see, and a monitor that
alarms forever is one that gets ignored — which is the failure the brief was
already having this week. Split by sensitivity instead: the two fields worth
hiding moved, the health signal stayed.

---

## What this review changed about the documents

`SECURITY.md` said three things that were true when written and are not now:

1. *"There is no spend meter in the running system."* `radar_analyst::spend::Spend`
   meters every mention read, model call, reply and post, and `radar-agent`
   carries its own ledger. Both persist across a restart.
2. *"Property and fuzz testing are absent."* `radar-risk` has carried `proptest`
   as a dev-dependency since the kernel was written.
3. *"The public server has no authentication."* `radar_serve::access` decides an
   audience per exact path, and the operator surface is behind Cloudflare Access.

`deny.toml` says `cargo-deny` runs "on every pull request and daily". No workflow
in `.github/workflows/` carries a `schedule:`. The claim is corrected to what
runs rather than the schedule being added, because adding one is a decision about
CI minutes and about who reads the result — LEARNINGS 21's shape.

None of these was a lie when it was written. Each is what happens to a document
that states a *state* rather than a way to check one, which is LEARNINGS 13, and
this file is the fourth time that entry has been paid for.
