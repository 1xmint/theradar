<!-- SPDX-License-Identifier: Apache-2.0 -->
# Radar: intelligence, autonomous distribution, and an evidence newsroom

**Status:** proposed implementation handoff, 2026-09-07. Owner preferences recorded below; recommendations are not claims that implementation or legal clearance exists.

**For:** Claude Opus 5, medium effort. Implement this in independently reviewable changes. The owner requested this document so the implementation session can spend its effort building and verifying, rather than repeating discovery.

**Inspected baseline:** local `main`, `4d14032f493d01ef76d27744769efda786956fc7`. The owner's untracked brief was present; preserve it. Reconcile changes since this commit before editing. This handoff was based on source inspection, repository documents, a read-only browser inspection, and primary documentation. No production-store evaluation, live contest experiment, or Rust test run was performed for this handoff. The supporting source-inspection record is [research 0031](../research/0031-radar-handoff-inspection.md).

## 1. Outcome and decisions

Build a bot that earns repeat summons by publishing distinctive, reproducible measurements; make every reply a useful shareable receipt; give that product a memorable public home. The token remains a community/distribution experiment, not a demonstrated source of demand or trading edge.

Josh chose an **evidence-newsroom** visual direction. **The contest must be fully autonomous, with recorded decisions and developer-accessible logs when something goes wrong. No human judging belongs in this implementation.** The operator is in **Tennessee** and wants worldwide reach. Worldwide readership and legal eligibility for a monetary prize are separate things.

**Winner selection is mandatory when at least one valid entry exists.** Josh explicitly rejected withholding a winner because engagement might be farmed or verification is incomplete. He accepts farmed participation as a distribution tradeoff. Do not implement a fraud suspicion veto, a tied-week no-winner outcome, or an unresolved-score rollover. Apply the published fallback order, select a winner automatically, and show exactly what evidence and rule selected it. Actual payout still requires a valid claim and the existing deterministic payment checks; selecting a winner and confirming a payment are separate recorded events.

**X approval is settled for this task, by Josh.** He reports direct evidence from an X developer that following the applicable rules requires no additional written approval, and explicitly instructs the implementer to leave this question alone. He will handle it if it becomes a problem. Do not reopen this discussion, demand an approval request, or make it an implementation blocker. Continue observing the existing interaction rules and recording actual platform responses.

### What “ungameable” means after the owner's clarification

An open, free contest on pseudonymous X accounts cannot be proved absolutely ungameable. An attacker controlling aged accounts can produce the same observable inputs as independent users. A deterministic function must treat identical inputs identically; an LLM cannot recover the missing identity evidence. Random draws add chance and Sybil exposure, staking changes the token/legal design, and a hidden fraud score only hides the uncertainty.

The owner's settled priority is **automatic winner selection, transparent rules, bounded spending, and reproducible evidence**, accepting that accounts may farm the contest. Do not pretend the system can prove account independence, but do not turn that limitation into a winner-selection gate. Protect against implementation exploits, duplicate credit under the same account, forged claims, corrupted state, and double payment. Accepted participation under the published rule remains participation even when it looks coordinated.

Keep these boundaries:

- `Policy::CLOSED` stays closed. No trading strategy activation, custody redesign, or new model-to-signer path.
- Preserve point-in-time reads, local decoding, deny-by-default budgets, and unknown/zero distinctions.
- No token purchase requirement, holding benefit, burn-for-access, team allocation, buyback, or operator profit share. All creator fees remain earmarked for the single weekly prize and its documented rollovers.
- No token/creator social intelligence, metadata-URI fetching, personal identity inference, aggregate safety score, or accusatory verdict. Necessary X contest/account observations remain confined to contest administration.
- Telegram remains off under design 0013; do not resurrect the older brief's free-lane rollout.
- No mint, payout activation, public post, production restart, or hosting deployment is implied by preparing code and PRs. Complete the reviewable artifacts first; follow existing operational authorization for external actions.

## 2. Findings that change the implementation order

These are source/browser findings, not newly measured production results. Reproduce them with the named regression scenarios before marking them fixed.

| Priority | Evidence and consequence | Required response |
|---|---|---|
| HIGH | `radar-research` infers trade coverage from partition-file presence. A partially populated file can make missing trades look like measured inactivity; later files can affect historical coverage. | Explicit, watermarked coverage records; unknown stays unknown. |
| HIGH | The trade feature pass does not filter unsuccessful transactions. Backfill also defaults missing transaction success to true, uses the system-program address as the trader placeholder, and can have unknown ordering/amounts. | Fix observability before enabling trade-derived features. Never count placeholders as actors. |
| HIGH | Labels can disappear before fold construction; overlapping price lookbacks do not establish independent entry/exit observations. | Freeze the population before label filtering; retain censoring and price provenance. |
| HIGH | The contest can mix verified scores with unverified raw scores after a scan/budget failure. Equality in its early-stop comparison also needs a tie proof. | Use one declared scoring mode for the whole week, an explicit fallback when evidence is incomplete, and a deterministic total ordering. |
| HIGH | Fact slots bind a numeric string, not necessarily its subject or qualifier. Free prose can attach the correct number to the wrong claim. | Model selects complete typed clauses; deterministic rendering owns subject, unit, window, and limitations. |
| HIGH | The site maps several API failures to factual empty states. Home/About still contain claims stronger than the measurements, including “honest way” and “looks clean.” | Separate unavailable, empty, stale, and live; repair public language. |
| MEDIUM | At a 375px browser viewport, the public page measured 394px wide and the summon section began around 2,703px down. Mobile evidence columns are hidden. | Put the primary action on the first screen, remove horizontal overflow, and retain evidence on mobile. |

Sample prior-audit verification: the signer now calls `lamports_bought`, with a named over-spend regression test; `radar-roast` has the tag substitution path. Preserve these improvements. Do not repeat research 0030's entire sweep or claim its tests were rerun here.

## 3. Intelligence: repair the instrument, then add evidence

### 3.1 Honest event and coverage inputs

Modify the existing store/backfill/research path, not a new analytics crate.

- Represent transaction success, actor identity, ordering, and realised amounts with explicit known/unknown status. New records must not encode unknown as success, system-program actor, or a valid index. Treat legacy sentinels as unknown on read. Version the recording schema; preserve old readable records and do not rewrite production history in place.
- Failed transactions remain available as failed-attempt evidence but do not contribute to successful buyers, transfers, acquired inventory, or curve progress.
- Record completed ingestion ranges with scope (table, mint/filter where applicable), slot interval, source, collection watermark, decoder version, and completion status. A completed zero-event query differs from an absent query. A file existing proves neither.
- Accept a feature row only when its decision time is at or before the table watermark. Coverage used by replay must itself have existed at that watermark.
- Add a bounded partition/range reader: `--from`/`--to` must restrict the initial disk read, not merely filter a fully loaded table afterward. Preserve `AsOf` filtering inside each selected partition.
- Do not turn on venue-wide trade ingestion. The current follower records lifecycle events, and the trade backfill has substantial source-volume constraints. Start with a reproducible mint cohort and bounded captures; report calls, bytes, wall time, memory, completeness, and ingestion lag before proposing a recurring workload.

**Acceptance:** a partial partition cannot yield zero trades; a complete empty interval can; failed swaps and legacy actor placeholders never inflate participants; adding future rows or coverage records cannot change earlier output; a boundary-crossing row whose decision time is in the future is excluded; bounded reads demonstrably avoid unrelated partitions.

### 3.2 A defensible research protocol

Retain the existing `radar features` and `radar edge` instruments. There are already 24 features at this baseline; do not rebuild them from a stale count in prose.

1. Freeze the eligible launch population and its five chronological fold boundaries before examining labels. Keep all launches in cohort accounting, including missing labels, failed exits, migration, and incomplete observation. Do not claim coverage of all Solana launches from pump.fun records.
2. Store feature cutoff, label-observation slots/times, quote/fill source, endpoint freshness, maturity, and exclusion reason. Distinguish a descriptive price proxy from an executable return. The same old fill in overlapping lookbacks is not a new exit.
3. Purge using actual label availability at fit time. Preserve the embargo and make its units explicit; a slot approximation is not an exact wall-clock day. Equal slot values cannot straddle fit/test boundaries.
4. Fit thresholds and choose candidates using training data only. Freeze the selected candidate before either holdout is evaluated. Version the grammar, trials, cost snapshot, horizon, seed, and dataset digest. Reusing a holdout after tuning makes it development data; require a later untouched window for a confirmatory claim.
5. Report descriptive signal quality separately from trading economics: conditional frequencies, absolute differences, intervals, denominators, missingness, and coverage by period. No fabricated return for a missing exit. Report observable-cohort results plus explicit missing-outcome sensitivity; do not upgrade them to population trading edge.
6. Use time-block resampling for uncertainty and a creator-grouped sensitivity analysis; correlated launches are not independent Bernoulli trials. Report the fixed rules before searched strata. Charge the applicable measured round trip once, preserving the current fresh-launch versus notional-band distinction.

The first production run must follow these repairs. Record exact command, commit, UTC date, watermark, input hashes, feature availability, and output in the next unused research note. If access or budget is missing, leave that specific measurement pending and complete the other work; never replace it with invented results.

**Acceptance:** planted future data is refused; identical history yields identical bytes; missing labels cannot move fold boundaries; duplicated old fills cannot establish exit freshness; noise does not acquire a confirmatory label through repeated holdout selection; fit-only signal vanishes on holdouts; engineered signal survives; costs are not subtracted twice. A null or inconclusive result is valid and cannot change `Policy::CLOSED`.

### 3.3 What to measure beyond recipient count

Recipient count is a proxy for early acquisition structure. It does not identify intent, beneficial ownership, or capital committed before creation. Preserve it as a dated distributional fact while replacing causal wording.

Implement these two families first, behind evidence admission:

**A. Transaction-local acquisition structure.** Count successful launch-slot buy transactions, distinct token accounts, and distinct owners where historical owner evidence exists; record which acquisitions occur in the same transaction, shared fee payers/signers, and ordered transaction positions. Report transfer fan-out separately from actual purchases. A creator-only transfer to many accounts must not become many independent buys.

**B. Inventory and flow reconciliation.** Where raw balance deltas and complete windows exist, record gross acquired/sold units, net acquired inventory, and recipient inventory retained at the stated checkpoint. Compute curve progress from evidenced reserve changes, including sells, rather than cumulative buy inputs. If the inputs only support buy flow, label that measure “cumulative recovered buy flow,” never depth. Account for quote asset, token decimals/extensions, fees, and internal transfers; reject an unsupported layout rather than infer amounts from instruction limits.

Candidate research beyond design 0010's list: same-transaction buy-and-distribute patterns; early-recipient inventory retained versus transferred/sold; repeated signer/payer motifs across launches; observed capital recycling that produces large gross flow with small net exposure. These are **hypotheses**, not existing detectors or proof of a cabal. Shared services create false associations; missing windows break conservation; secondary transfers can obscure ownership.

Every published family needs a capture-backed positive fixture, ordinary counterexample, evasive variant, exact observation window, source coverage, dated population comparison, and a caller in the sheet/research path. Admit new facts through design 0010's test, updated for whole-clause rendering. Existing data cannot backfill balances it never recorded.

**Jito boundary:** ordered, successful transactions in one slot are compatible with bundling, not sufficient evidence of bundle membership. Tips are not proof either. Use “adjacent successful transactions” when that is what is known. A stronger Jito claim requires recorded bundle-status evidence identifying those signatures, with provider provenance. Include the documented uncle/rebroadcast exception; never infer atomicity merely from the final observed block. [Jito documentation](https://docs.jito.wtf/lowlatencytxnsend/)

**Adversary matrix:** test extra dust recipients, split wallets, delayed buys, interleaved transactions, shared infrastructure, circular transfers, failed transactions, missing balance deltas, unsupported quote assets, and API truncation. Mark each fact as evadable, costly to evade, or invariant only within its measured window. Do not call address-count heuristics structural guarantees.

### 3.4 Replies and permanent receipts

Extend `Fact` with a stable kind, typed scope, measurement time/watermark, source references, completeness, and complete deterministic clause renderings. Maintain explicit schema versions.

The model chooses a bounded list of fact tags, their order, and a vetted voice variant. It cannot write a subject, number, negation, comparison, or verdict into public prose. Preserve the existing tag exception and digit refusal; no free text outside the accepted output grammar reaches publication. Use short dry connectors written in code. Example shape: token-specific observation; dated comparison when supported; source link. Generic costs lead only when they are the strongest genuinely coin-specific fact.

Create an immutable receipt from the exact fact snapshot and final rendered reply before publishing. Include the mint, observed time, measurement clauses, unknowns, evidence links, versions, and supersession/correction links. Public views contain no private account data or internal errors. Preserve the historical statement when newer evidence arrives; issue a correction record rather than silently overwriting it.

Add a strictly validated receipt-ID route to the public file-backed API and a site receipt view. Requests read a prepared receipt, never scan the production store or run arbitrary RPC. Reuse current caching/access conventions. Generate a downloadable share card from that same receipt, with visible date, scope, and source; use a brand-only default OG image so social previews cannot repeat stale measurements. Text-plus-receipt-link is the v1 X format; additional paid media APIs are not required.

## 4. Fully autonomous contest: verified evidence relay

### 4.1 Chosen mechanism

Reward distribution of an existing evidence receipt, not owning the first question about a coin. Preserve one contest and creator-fee funding; supersede ADR 0013's automatic-summons-entry and engagement formula explicitly in a new ADR. No judge, model fraud verdict, random draw, or token holding participates in eligibility or ranking.

An entrant explicitly quotes a published bot receipt post, mentions the bot, and includes `#hunt`. The receipt must already be in Radar's publication log. Existing mention ingress establishes identity and consent. Arbitrary quoted links do not choose a network destination. The account may nominate one active quote per UTC week; a later valid nomination replaces its own earlier nomination before the cutoff. Store every replacement. Any user can relay the same receipt, so an earlier summoner owns no exclusive prize opportunity. Keep ordinary summons free of contest entry requirements.

Use the existing UTC week and numeric X user IDs. Operator exclusions and the existing winner cooldown remain, checked against local records at nomination acceptance. Remove account-age and inferred-human-quality requirements from the new rules: Josh accepts farmed participation, and an unavailable external age lookup must not decide whether a locally recorded valid entrant can win. Retain required legal-entry conditions once settled by counsel; this is not permission to invent eligibility. A nomination requires an authenticated platform author, a recognized receipt, and the explicit entry action. Known invalid entries never become eligible merely because data are missing elsewhere.

The preferred score is the number of **distinct X accounts observed reposting or quoting the nominated post**, deduplicated across both actions. Each supporting account contributes at most one unit per entry. Likes, replies, views, repeated quote posts from the same account, the entrant itself, and operator accounts contribute zero in this mode. Audience overlap across entries is allowed. Farms of different accounts are not disqualified, and the public label is “observed sharing accounts,” never “verified people.”

There is no minimum engagement requirement and no prize increase based on engagement. One valid entry can win with zero shares. Break ties by earliest creation time of the account's active nominated quote, then numeric post ID ascending, then numeric account ID ascending. Parse IDs numerically with a type that cannot overflow their supported format; do not compare decimal IDs lexicographically. These are disclosed deterministic rules, not a random draw. Earlier nomination is a tie advantage, but does not give the first asker exclusive ownership of a coin or its receipt. An account replacing its nomination takes the new quote's timestamp.

### 4.2 A published fallback always chooses a valid winner

At week close, freeze active valid nominations and rules from the durable local journal. Collect engagement during the following 24-hour close window, publishing each observation interval. Do not describe it as an instantaneous, complete end-of-week snapshot: the platform does not provide that guarantee. Persist the first successful raw counter capture per nomination; retry missing captures and incomplete supporter pages within the window without overwriting an already frozen capture.

At the deadline select exactly one scoring mode for **every** valid nomination in the week, using the first applicable mode:

1. **Observed accounts:** all entries' repost/quote pagination completed, all returned actions have usable author IDs, and no unresolved source/schema error remains. Score the union of sharing-account IDs after the explicit self/operator exclusions. Completion means the allowed API enumeration completed, not proof that the platform exposed every real interaction.
2. **Reported actions:** if mode 1 is unavailable but every entry has a frozen raw counter capture, score `reposts + quote_posts` from those captures, using checked arithmetic. Likes, replies, and views still do not count. This mode counts actions and may count repeat actions by the same account; say that openly. Never describe its scores as distinct accounts or verified engagement.
3. **Recorded entry order:** if some entry lacks usable counters, choose the earliest valid active nominated quote using the tie order above. Engagement scores remain `null`; do not fabricate zero or silently rank unread entries below successfully fetched ones.

Within modes 1 and 2, sort by score descending and then the same deterministic tie order. Mode 3 sorts by that order alone. A week with at least one valid entry therefore always has one winner, including an all-zero week or a total engagement-API outage. A week with no valid entrants records `no_entries`; never manufacture a winner. Corrupted/missing entry history is a system fault, not ordinary incomplete engagement: restore/replay it before declaring a result.

Create a winner certificate containing the complete frozen candidate set, chosen mode, fallback reason, observed scores or nulls, tie comparisons, rules version, input hashes, and the selected account. Do not mix modes within a ranking. Do not keep the current `best >= next_raw` short circuit unless a future change formally proves that it preserves the exact total ordering and whole-week mode choice. Full metered collection is the v1 default.

Retry transport/rate-limit failures within the close window using the existing spend meter, bounded backoff, and provider retry guidance. On recovery after downtime, settle every due unclosed week in order from its recorded inputs; do not skip older weeks. Once the collection deadline passes, do not wait for more engagement evidence or improve a completed week's result with later counters. Select using the applicable fallback automatically. Monday's appointment may say counting is in progress; after the deadline the board shows the winner and scoring mode. If X is unavailable, queue the permitted result announcement while the public record still establishes the winner.

**Accepted tradeoff:** farms can win. They may also try to provoke an API failure to obtain the published fallback, which can favor an early nomination. This is not hidden or blocked by an invented confidence threshold: it is the cost of the owner's guaranteed-selection preference under unavailable evidence. Measure mode frequency and attack scenarios. Do not secretly change thresholds or retrospectively disqualify a winner because their participation looks farmed. Nothing here authorizes Radar to create fake accounts, buy engagement, or encourage violations of X's rules.

**Acceptance:** every nonempty valid candidate set yields exactly one winner, independent of input order, in each mode. Test one entrant, all-zero scores, tied positive scores, numeric-ID tie ordering, repeated actions by one supporter, many distinct farm accounts, one failed page, exhausted budget, missing counters, full API outage, restart at the deadline, overdue weeks, nomination replacement, and receipt reuse by different entrants. Partial reads must switch the entire week to the documented fallback, never mix units or demote an unread entrant to zero. Missing account age is diagnostic only. An LLM response cannot select, veto, or alter the winner. A selected winner without a valid claim remains visible and cannot cause an unsafe payment.

### 4.3 Payout and compatibility

Version rules at week opening. Closed legacy records retain their original formula and display; do not reinterpret history under the new rule. Add explicit states for collecting, counting, winner selected, no entries, claim pending, submitted, confirmed, and failed. Store scoring mode and fallback reason separately. Ties and incomplete engagement lead to a winner, not separate terminal no-winner states. Do not hide corruption or unreadable records by skipping them.

A winner certificate is inert data. Existing deterministic payout policy must separately establish a permitted week, valid winner/claim, eligibility configuration, correct destination, available earmarked fees, reserve/floor, replay protection, and absence of a prior payout. Missing legal eligibility configuration leaves monetary operation off. Keep the selected winner visible if payment awaits a claim, a configured floor, or recovery from an RPC error; do not erase the winner or relabel the week empty. Preserve existing disclosed claim-expiry/fund-rollover behavior unless counsel's settled rules require a change. Keep creator-fee funds separate from operational spend; no rollover becomes operator income.

Claim intake remains an explicit winner-initiated interaction bound to the exact week and claim action. A regular mint summons cannot become a destination claim. Do not send a second unsolicited claim-prompt reply to an interaction already answered; use the public result/claim instructions and an explicitly initiated claim request. Reconcile transaction status before any retry that could pay twice.

## 5. Audit logs, replay, and autonomous recovery

This is a release requirement, not optional telemetry. Extend the existing logs/ledgers instead of adding a new logging service or database.

### Durable event contract

Use an append-only structured journal with schema version, monotonically increasing sequence, stable event ID, correlation ID, UTC time, duration, build SHA, rule/decoder/model version, redacted configuration hash, and previous-event hash. Correlate mention, receipt, nomination, week, claim, and payout IDs across components.

Record stages and typed outcomes: received; parsed; admitted/refused; input fetched/incomplete; fact built/withheld; model accepted/fallback; publication prepared/submitted/confirmed/uncertain; contest candidate scored; scoring mode selected; winner selected with tie/fallback reasons; claim accepted/refused; payout prepared/submitted/confirmed/reconciled. Keep developer diagnostics separate from public reason text.

Persist the inputs necessary to reproduce a decision: raw on-chain captures or references to immutable captures, normalized X evidence IDs and eligibility fields, page-completeness markers, fact snapshot, rule settings, model request/response when applicable, exact public text, budget reservation/settlement, HTTP status/provider request ID, and bounded redacted error information. Never log credentials, signing keys, authorization headers, full credential-bearing URLs, or chain-of-thought. Untrusted content is escaped and remains data.

The durable intent and its evidence must exist before publishing or paying. A journal-write failure blocks that effect and reports through stderr/systemd and the existing operator alert channel. Use atomic checkpoint replacement and validate the journal on restart. A torn final write is distinguishable from an empty history; a gap, malformed record, or bad hash is a visible fault.

### External effects and restart behavior

Use a persistent outbox/state machine keyed by the operation, not by retry attempt. A restart must resume the same work without recharging completed reservations, skipping mentions, reopening weeks, or awarding again.

For Solana payouts, persist the signed transaction's expected signature and validity bounds before broadcast, then reconcile that signature through configured RPC. Do not sign a replacement until expiry/nonconfirmation is established under the payout policy. An ambiguous response is `uncertain`, not failure and permission to resend a different payment.

For X publication, use confirmed returned post IDs and bounded reconciliation against the account's own permitted records. If the API provides neither an idempotency guarantee nor enough evidence to distinguish acceptance from loss, preserve `uncertain` and block blind duplicate posting. Autonomous does not mean claiming exactly-once effects from an API that cannot prove them.

### Developer tools and retention

Add these **new CLI commands** to the existing CLI:

- `radar audit explain --id <correlation-id>`: chronological stages, decision reasons, evidence/config/build hashes, side effects, and recovery state.
- `radar audit replay --id <correlation-id>`: offline deterministic replay of facts, admission, scoring, and payout-policy decisions; compare canonical output hashes and explain any differences. Reuse the recorded model response; do not claim a fresh model call is deterministic.
- `radar audit verify --from <sequence> --to <sequence>`: integrity and missing-artifact checks; corruption must exit nonzero.
- `radar audit export --week <week>`: a redacted incident/review bundle with a manifest and replay instructions.

Retain small decision manifests, rendered public facts, contest rules, and payout receipts indefinitely unless a legal deletion obligation applies. Default bulky/private capture retention to 30 days, with explicit expiry markers: after expiry replay must say evidence expired, never pass silently. Counsel must settle X-content deletion and retention obligations before enabling that collection. Retention that removes a required unresolved-payout artifact is prohibited. Storage quotas fail closed for new effectful work; they cannot silently prune active audit evidence.

Hash chaining detects alteration relative to a trusted checkpoint; it does not resist a host attacker who rewrites the entire chain. Prepare off-host checkpoint/backup integration using the existing operator backup destination. If none is configured, surface `backup_unconfigured`; do not invent a destination or claim tamper-proof logs. Test restoration from an actual backup artifact before launch.

Alert on journal failure, prolonged publish uncertainty, a winner not selected by the deadline after a healthy scheduler tick, payout uncertainty, ingestion lag, repeated provider refusal, and backup failure. Record fallback scoring as an operating metric; it must not block selection. Deduplicate incident alerts and emit recovery once. Public endpoints must expose concise service states, not raw provider messages or spend details.

**Fault tests:** crash before/after each durable boundary; accepted external effect with lost response; duplicate input and replay; disk full; torn checkpoint/journal; corrupted evidence; provider 429/403/timeout; process restart mid-pagination; missing account age; transaction expiry; repeat claim; changed destination; restored backup. Each must leave an explainable state and never create a duplicate payout or falsely confirmed publication.

## 6. A site people can understand and share

Keep React, Vite, Tailwind, existing routing, Geist, and Cloudflare Pages. This is an editorial redesign of the current site, not a framework migration or trading terminal.

**Art direction:** ink background `#0B0D10`, paper text `#F1EEE6`, muted text `#A8ADB7`, restrained amber `#F2B544`, slate rules. Large tabular figures, strong headline hierarchy, thin dividers, minimal rounding, generous space. No red/green safety coding, skull/scammer imagery, spinning radar, fake terminal output, invented endorsements, or animated live counters.

**Layout:** 1,200px maximum content width; 24px desktop/16px mobile gutters. Desktop uses an editorial hero with one primary measurement and a featured receipt. Mobile stacks the headline, one measurement, and contract-address input/“Ask on X” action inside the first 812px viewport. The action opens an X composer; it does not submit or pretend to perform an instant on-site investigation.

Primary navigation: Receipts, How it works, Community, plus Ask on X. Use an accessible mobile menu, not a squeezed six-link row. Preserve existing route URLs; place History, Leaderboard, Pool, and Token under Community and link them in the footer. Their money/state content stays discoverable without displacing the product's primary action.

Home order: current product statement and action; one real evidence receipt; what the measurement means and cannot establish; actual follow-up evidence when available; compact community/token state; methodology/footer. The hero must use a checked measurement. If current data are absent, use “Read the launch. Keep the receipt.” with an explicitly dated example, never a fake current count.

Use a shared resource state across pages: loading, live, stale with observation time, confirmed empty with reason, unavailable. A successful response asserting no token is an empty state; a failed request is unavailable. Any fallback fixture must say “dated example” visibly. Align instant-graduation copy to the actual classifier window, not “inside the launch block” when the measurement allows more slots.

Receipt cards show one primary fact, a short explanation, date/window, and source link. Full receipt pages expose denominators, provenance, unknowns, and corrections. Downloadable images use the same data object and show the measurement date. About, Home, Token, metadata, and OG copy share consistent launch-status wording. Preserve honest pre-token Pool/Token states. Fetch the existing hunter endpoint only where its measured basis is explained; do not turn it into a leaderboard of “good people.”

On mobile, ranking evidence expands per row; do not hide it or tell users to rotate. Show the scoring mode, score units or an explicit unavailable score, tie/fallback reason, rules version, known exclusions, and payout evidence in plain language. Separate “no entrants,” “counting,” “winner selected,” “payment pending,” and “not launched.” A fallback-selected winner receives the same clear winner placement, with the reason visible rather than hidden in developer logs.

**Winner History: public wallet and payment evidence — Josh's explicit requirement.** Every historical winner must show their publicly submitted payout wallet address once claimed, with a copy button and a validated Solscan account link. A shortened display is acceptable only if the full address is available by expansion and copying, including on mobile. Label it “Payout address submitted by winner”; do not imply ownership of unrelated wallets or expose private claim information. The claim flow must clearly tell the winner that this address and its payout transaction will appear publicly in History.

For a completed payout, display the actual recipient address, exact SOL amount, transaction signature with a validated Solscan transaction link, confirmation/finalization status, and recorded payment time when available. Derive these fields from the persisted payout/reconciliation evidence, not a current profile or a mutable address setting. Check that the successful transaction's recipient and amount match the authorized award before marking it paid. A transaction signature alone, a submission attempt, or a claimed wallet is not proof of payment. Label submitted, pending, failed, and confirmed payments distinctly; show “Not submitted yet” for an unclaimed address. Preserve each week's original recipient even if the winner supplies a different address for a later week.

Reuse the existing History claim/payout fields (`claim.address`, `payout.recipient`, `payout.signature`, amount and time) and transaction-link helpers; extend the versioned public record only for missing reconciliation status/evidence. Never make visitors connect a wallet to inspect payment history. Legacy records missing a recipient or confirmation detail must say that evidence is unavailable, without inventing an address or downgrading a separately evidenced payment to an unpaid claim.

**History acceptance:** a confirmed award renders the matching recipient, amount, and transaction link; copied text is the full address; invalid addresses/signatures cannot produce executable or arbitrary links; pending/failed transactions never render as confirmed; a changed later claim cannot rewrite an old payout; recipient/amount mismatches cannot be labeled paid; missing legacy fields remain explicit; wallet and transaction evidence remain usable at 375px width. Include these cases in the History/API tests and browser review.

Motion: opacity/translation only, 120–180ms transitions, and no essential information behind animation. Respect reduced motion. Keyboard navigation, visible focus, 44px touch targets, readable contrast, and zero horizontal overflow at 320/375/390px are acceptance criteria. Check desktop at 1,440px and 200% zoom. Use a production build for visual QA and save dated screenshots. Maintain `figures.test.ts`, add the missing receipt/image truth bindings, run `just site`, and review actual rendered states rather than counting tests.

## 7. Distribution, Tennessee, and launch gates

### The loop and what can be measured

The useful loop is **receipt seen → new person summons → specific answer → receipt relayed → another person summons**. Token attention is a possible branch, not a necessary step. Buying the token confers no product benefit under the chosen design, so there is no established utility-to-token-demand coefficient.

Let `K` be newly acquired, nonoperator summoner accounts attributable to existing users' receipts divided by the originating active summoner accounts in a fixed cohort window. Impressions are not unique people; shares are not acquisitions; repeated summons are retention. Report an observed attribution lower bound and unassigned acquisitions separately. Never equate growth in both series with causation.

Instrument first successful summon, repeat summon within 7/30 days, canonical receipt ID, quoted-parent attribution, candidate campaign code, refusal reason, reply latency, receipt availability, and whether a token-specific measured clause was published. Unknown attribution remains unknown. No browser fingerprinting, wallet/person linkage, or scraping of unrelated social activity. Do not buy a new analytics service for this v1.

For the first 200: prepare one pinned how-to post and a small set of real receipt examples, give each share a direct summon action, and use the existing daily/weekly appointments as return reasons. Organic supporters and the operator can share these assets manually; the bot cannot create its own seed audience by unsolicited replies. Do not fabricate outreach or send it without explicit authorization. Keep the existing demand gate's exact definition; do not count contest entrants, impressions, or dry runs as distinct summoners.

For 20,000: the existing 50-replies/day setting would take at least 400 days to produce 20,000 unique successful replies even if every slot served a newcomer. That is arithmetic on the documented setting, not a verified current production limit. Instrument demand rejected by the cap; cost and load-test the required operating budget. Use precomputed receipts and edge caching so reading an answer is cheap. Do not silently raise caps or claim an unmeasured referral coefficient. Site visits and relays can scale before unique answered summons do.

Run the existing formats for four weeks before adding a new scheduled format. Report cohorts, denominators, cost per successful answer, return rate, and attributable acquisition. Mark experiment thresholds as operating choices, not empirical laws.

### Legal and platform work with precise outputs

Prepare a Tennessee counsel packet covering the actual architecture, sample receipts and share copy, explicit autonomous rules, creator-fee money flow, operator interests, claim flow, and global exposure. Obtain written answers on: Tennessee prize/chance/consideration and noncash promotional activity; federal/state securities and financial promotion treatment; whether fee-funded prizes/ongoing product work change the token analysis; eligible countries/states/ages; official rules including guaranteed selection, deterministic ties, scoring fallbacks, and claim-expiry/fund-rollover treatment; sanctions/payment screening; tax reporting and unclaimed awards; entity/responsibility; privacy/X-data retention; defamation and correction handling. Neither “free entry,” zero voluntary holdings, nor “not financial advice” settles these questions.

Tennessee's published money-transmitter guidance distinguishes virtual currency from some sovereign-currency activities; it is not a blanket launch exemption. The SEC meme-coin staff statement is limited and is not approval of this token/prize arrangement. Older Tennessee AG opinions explain why chance/consideration require attention, but do not establish current clearance for this design. Start with [Tennessee DFI](https://www.tn.gov/tdfi/mortgage-consumer-lending/money-transmitter.html), [SEC staff statement](https://www.sec.gov/newsroom/speeches-statements/staff-statement-meme-coins), and [Tennessee AG opinion 16-013](https://www.tn.gov/content/dam/tn/attorneygeneral/documents/ops/2016/op16-013.pdf); have counsel verify current law and intervening changes.

Do not promise worldwide prize eligibility. Support an explicit permitted-jurisdiction configuration; unset means cash operation disabled. The audience can remain global while monetary entry/claim restrictions follow the settled rules. Do not pretend an X profile location establishes residency.

**Do not reopen X written approval; Josh's decision in section 1 controls this handoff.** Do not file requests or contact X on his behalf. Preserve mention/quote-triggered interaction, clear automation/operator disclosure, existing publication controls, and platform-error logging. Add an explicit persistent opt-out command through mention ingress and retain the one-reply-per-interaction boundary. Do not build the older proposed unsolicited second investigative reply. These are product behaviors, not a new approval workflow.

Contest rules must discourage multiple-account participation and repetitive posting; [X promotion guidelines](https://help.x.com/en/rules-and-policies/x-contest-rules) do not establish legal clearance. The automatic evidence relay is subject to this review too.

Before any future mint, re-verify accepted pump.fun transaction layouts, fee schedules/recipient settings, and Token-2022 behavior against captures and [pump.fun's published interface](https://github.com/pump-fun/pump-public-docs), [Solana RPC](https://solana.com/docs/rpc/http/getblock), and [Token Extensions](https://solana.com/docs/tokens/extensions). No dev purchase/allocation does not guarantee that strangers cannot buy in the same launch block; record actual observed recipients instead of promising control over other users. Unsolicited token transfers and how the zero-holding policy handles them also need a settled procedure. This handoff does not choose a token name, create a key, or mint.

## 8. Ordered PRs and completion evidence

Use one branch/PR per coherent item. Rebase/reconcile after preceding work lands; do not pile dependent changes into an unreviewable branch. Proposed branch names below are new, not existing refs.

| Order | Branch | Deliverable and decisive proof |
|---|---|---|
| 1 | `docs/radar-next-phase` | Record this design, updated owner decisions, source evidence, and new ADRs for autonomous relay scoring, clause selection, and audit semantics. Preserve earlier decisions as superseded, not silently rewritten. |
| 2 | `fix/observable-research-inputs` | Known/unknown event fields, legacy handling, coverage manifests, bounded reads; failures/sentinels/partial data/future coverage regressions. |
| 3 | `fix/research-cohorts-and-labels` | Frozen cohorts, label provenance/censoring, train-only selection and reproducible reports; leakage/noise/synthetic-edge tests. |
| 4 | `feat/replayable-audit` | Journal, input artifacts, outbox, explain/replay/verify/export commands; crash, corruption, retention, and duplicate-effect tests. |
| 5 | `feat/launch-evidence-receipts` | Two admitted evidence families, complete-clause model selection, receipt persistence/API; capture/counterexample/adversarial wording tests. Keep unsupported measurements unavailable. |
| 6 | `fix/contest-verification` | Replace mixed verified/raw ranking with explicit whole-week modes and deterministic ties; rate-limit/budget/partial-pagination/candidate-order tests. Coordinate activation with the versioned new rules rather than changing an open week's terms. |
| 7 | `feat/autonomous-evidence-relay` | Explicit quote nominations, versioned scoring/state, automated settlement/claims under existing payout policy; historical compatibility and adversarial tournament fixtures. Cash activation remains gated. |
| 8 | `feat/evidence-newsroom` | First-screen summon flow, permanent receipts/share cards, honest resource states and mobile evidence; figure tests and browser screenshots across live/empty/stale/error states. |
| 9 | `feat/distribution-observability` | Cohort reporting, attribution lower bounds, latency/fallback/denial metrics, operator diagnostics and draft share assets; known synthetic journeys match expected counts. |
| 10 | `docs/measured-launch-readiness` | Run the repaired bounded research pilot when access/budget permits, document actual results, dry-run autonomous weeks/failures, prepare deployment/rollback and Tennessee counsel packets, update state and handback. |

For each changed behavioral property, add a named regression and reintroduce the wrong behavior to prove it fails. Do not run broad local mutation jobs or parallel cargo processes. Follow `AGENTS.md`'s workstation constraints, run scoped checks during editing, `just check` before each push, and the site pipeline for frontend changes. Inspect required CI shards and do not push over an in-flight required run. New commands/types above are specifications to implement, not commands that already passed.

Publish a per-PR result record: commit, command, observed output, mutation/reintroduced-bug result, remaining uncertainty, and whether deployed. A PR merge is not deployment. Backend additions must be deployed and verified before the site requires them; preserve compatibility with old records during rollout. Rollback may stop new effects and restore a compatible binary but cannot discard confirmed payouts or published history.

The final handback must identify completed work, pending external gates, and precise resume commands. Include a verdict on strongest/weakest components, hidden risk, missed opportunity, what to preserve/rewrite, and distance from the acceptance bar. Do not claim world-class from test counts alone.

## 9. Where this recommendation is weakest

- **The autonomous cash contest accepts farmed participation.** The algorithm proves consistency with recorded rules, not independence or honesty of participants. The owner prioritizes selecting a winner over refusing one under uncertain engagement. Fallback-mode manipulation and the early-nomination tie advantage are explicit costs; test and log them without imposing a secret no-winner veto. Farmed activity may inflate visible engagement without producing repeat users or durable demand, so report those outcomes separately.
- **The new intelligence is not measured yet.** Raw-data availability, historical owner resolution, and capture cost may make some families unavailable. The correct deliverable in that case is an instrumented unknown and a bounded next capture, not a misleading substitute statistic.
- **A strong reply is not yet evidence of demand.** The first 200 need an external seed audience; autonomous posting cannot summon users into existence. There is no measured K and no demonstrated token-demand mechanism.
- **Legal certainty is external.** Tennessee operation does not erase other jurisdictions, and the combination of creator fees, prizes, and ongoing product promotion matters more than the meme label. Code can enforce settled conditions; it cannot provide the missing legal opinion.
- **Sharper rendering trades expressive freedom for semantic safety.** Test whether clause choice and dry deterministic variants are sufficiently distinctive. Do not reopen unconstrained claims merely to improve the joke.
- **The evidence guarantee is bounded by the recording trust boundary.** Provider omissions, corrupted historical inputs, a compromised host, and legally required evidence deletion all limit what replay proves. State those limits alongside successful tests.

The strongest thing to preserve is the deterministic separation between observation, decision, and authority. The biggest hidden risk is treating incomplete observations as facts and mistaking consistency checks for instrument accuracy. The biggest missed opportunity is a permanent, shareable, coin-specific evidence receipt that readers can verify without trusting the bot. That is the product to make excellent first.
