<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0011 — Build and measure a private autonomous trader

**Status:** in progress — design/handoff prepared; all implementation items below
are unstarted. No live operation is authorised by this document.
**Date:** 2026-09-09.
**Branch:** `research/advanced-assistant` — documentation only.
**Inspected base:** `c39aabc8aafc3fcb9da2b0bf0c1329a18f75a212`.
**Planned for:** Josh's implementation handoff to Opus 5.
**Specification:** [design 0017](../design/0017-a-private-autonomous-trader.md).

## Objective

Produce a private, Solana-wide research and trading assistant that Josh can first
observe in shadow, then use under supervision, then allow to run for bounded
unattended sessions after the respective gates pass. One portfolio supervisor
uses tools and optional research specialists; deterministic policy and the separate
signer retain capital authority. Measure net portfolio improvement against credible
cheaper controls. Prepare a paid offering only if subsequent evidence supports it.

The first delivery is a **complete prospective shadow session across multiple
Solana market families and SOL/USDC routes**, with a reconstructible morning report.
It is not a buy-only demonstration, a billing system or a fleet with no consumer.

## Scope and coordination

Read [GOAL](../../GOAL.md), [AGENTS](../../AGENTS.md), design 0017 and this plan
before implementation. Design 0017 records Josh's later private-first and broad
Solana direction; its market target supersedes the older pump-first sequence for
this experiment. It does not remove the model/authority boundary or relax unknown
data handling. Keep the shipped default `Policy::CLOSED`.

This documentation task does **not** alter code, run builds/tests, spend money,
create wallets, sign transactions or deploy anything. The original workstation
restrictions remain in force for this session. All implementation and verification
below describe a subsequent authorised coding session; use CI for its required
checks and respect the owner's local-compute limits.

[Plan 0010](0010-radar-actualization.md) is owned by another workstream and is not
modified here. Reuse its shared work: item 2c coverage production writes, item 3
economic reporting/dependence, item 4 durable audit/outbox/replay and item 5c
retained evidence. Inspect what has actually landed before writing overlapping
changes. This plan does not depend on its public competition, website or
distribution work. A shared primitive needs one implementation and named callers,
not a parallel trading copy.

Before future implementation, record the actual branch/base and coordinate shared
files. Do not check out or push the branches protected in this research brief:
`main`, `feat/launch-evidence-clauses`, `docs/metered-intelligence`. This handoff is
not permission to modify those branches. Do not launch an implementation agent or
create a separate task automatically; Josh controls the handoff.

## Operating contract to settle before coding a live path

The implementation may define schemas and run signer-free experiments without
choosing a real capital budget. Live configuration must separately supply:

| Field | Required meaning |
|---|---|
| Account and wallet | Dedicated own-capital experimental identity; verified ownership and recovery path; no customer/prize funds. |
| Capital and denominations | Maximum funded principal, base reporting currency, allowed quote assets and minimum SOL operating reserve. No assumed $1 USDC valuation. |
| Risk mandate | Position, aggregate exposure, correlated-group exposure, trade size, turnover, session/daily loss and drawdown limits; maximum holding duration. |
| Intelligence budget | Per-job, daily and session caps across all agents, tools, retries and rejected candidates; independent from principal and protection reserve. |
| Session | Start/end, entry stop, renewal and unwind behaviour; no silent permanent authority or automatic refill. |
| Protection | Admitted exit routes, reduction constraints, retry/fee budget, freshness/reconciliation limits and escalation channel. |
| Evidence policy | Strategy/model/tool versions, frozen experiment, allowed sources and cohorts, timestamps, retention and read coverage. |
| Conflicts | Community-token exclusion and enforcement of GOAL's no-hold/comment rule across private trader and public analyst. |

Numbers remain **unset**, which blocks real spending rather than inviting an agent
to pick them. This does not block implementation of disabled live machinery.

## Dependency sequence

P0 starts the work. P1, P2 and P3 establish the first coherent shadow slice; P4
runs that slice, P5 evaluates architecture. After interfaces settle, P6–P8 provide
the live boundary and can proceed while prospective evidence accumulates. P9
requires all their gates plus an explicit own-money mandate. P10 follows reconciled
canaries. P11 is a separate commercial decision, not a completion prerequisite for
the private trader.

### P0 — Reconcile the current tree and freeze the experiment contract

- [ ] Reinspect the baseline and list landed shared fixes, remaining gaps and the
  implementation branch/base. Record any required GOAL/ADR amendments as explicit
  direction changes, not silent conformance deletions.
- [ ] Define typed recommendation, evidence reference, operation identity,
  portfolio snapshot, source-availability clocks and experiment manifest at their
  actual consumers. Bind each interface to the caller that will use it in P4.
- [ ] Freeze the initial hypothesis set, benchmark arms, cost accounting,
  evaluation window, loss reporting and admission/stopping rule before outcomes.

**Acceptance:** a reviewer can trace one candidate from source to recommendation
to shadow portfolio and report, identify every clock/budget, and show that the
shadow process cannot reach a live signer. Do not create unused framework crates.

### P1 — Broad discovery and honest point-in-time collection

**Inspect/reuse:** [backfill](../../crates/radar-backfill/src/main.rs),
[store coverage](../../crates/radar-store/src/coverage.rs),
[features](../../crates/radar-research/src/features.rs),
[decoder](../../crates/radar-decode), and plan 0010 item 2c.

- [ ] Add a market registry covering the design's pump, Raydium, Orca, Meteora,
  Jupiter and other-discovered states. Track program/pool/mint identities, quote
  assets, migrations and capability progression with unsupported reasons.
- [ ] Connect a live raw-data recorder, distinct from the delayed backfill. Own
  the decoder; do not buy parsed transactions. Keep direct execution RPC separate
  from any analysis acquisition/settlement lane.
- [ ] Record source event time, slot, capture/available time, gaps, successful
  transaction status and cohort filters. Define filtered completeness at readers;
  never claim an observed sample is the whole venue.
  - Partly done 2026-09-09 on `agent/9-9-0005a-coverage-writer`, which closes
    plan 0010 item 2c: both backfill paths now write a
    [`Coverage`](../../crates/radar-store/src/coverage.rs) record per window per
    table, and a completed window that returned no rows is recorded as
    `ObservedSlots::Nothing` rather than skipped or written as slots zero to
    zero. Proved by
    [`an_empty_window_is_not_an_unvisited_one`](../../crates/radar-store/tests/an_empty_window_is_not_an_unvisited_one.rs).
    **Still open here:** these records carry `filter: None`, which is exact only
    while a table holds one venue — the moment a second venue writes into
    `Table::Trades`, the venue has to move into `filter` and the reader contract
    for filtered completeness has to be defined with it. Capture and available
    time are not recorded; a window attests only the slots its own rows landed
    at, because there is no epoch-to-slot conversion and none may be invented.
- [ ] Start the prospective cohort with at least two pool families and both SOL
  and USDC routes; publish broad discovery gaps and the remaining adapter queue.
- [ ] Meter call credits, bandwidth, local decoding time, stored bytes, freshness
  and deep-history requests. Enforce quotas and retention before unbounded scans.

**Acceptance evidence:** captured records from both families and quote assets,
plus delayed arrival, gap, failed transaction, migration and missing-decoder cases.
A replay before availability cannot see the later record or a future-enriched
cache entry. A filtered capture cannot satisfy an unfiltered completeness request.
The report shows provider/decoder scope and collection cost, not only row count.

### P2 — Reconciled portfolio and durable operation model

**Inspect/reuse:** [stored positions](../../crates/radar-store/src/position.rs),
[portfolio strategy](../../crates/radar-strategy/src/portfolio.rs),
[consider](../../crates/radar-cli/src/consider.rs),
[journal](../../crates/radar-journal), and plan 0010 item 4.

- [ ] Represent wallet, raw mint quantities, verified decimals, SOL/wrapped SOL,
  quote balances, cash, fees/rent/tips, partial fills, pending reservations and
  valuation uncertainty. Preserve unknown versus zero for holdings and losses.
- [ ] Define deterministic event application and idempotent operation identities
  for fills, failures, restarts, deposits and withdrawals. Carry evidence IDs and
  commitment/finality states; reserve atomically across concurrent decisions.
- [ ] Persist state transitions with one durable operation record before effects;
  derive or reconcile the portfolio from that record and observed chain balances.
- [ ] Connect the shadow portfolio to the same accounting rules the live caller
  will use, with simulated versus actual fills explicitly distinguished.

**Acceptance evidence:** restart with pending reservation; duplicate event;
concurrent requests exceeding combined capital; partial fill and fees; missing
inventory read; SOL/USDC conversion; wrapped SOL rent/refund; external balance
change; stale/absent valuation. None creates money, drops an unknown position or
frees a reservation while an earlier submission may still land.

### P3 — Bounded investigative supervisor with one consumer

**Inspect/reuse:** [agent](../../crates/radar-agent),
[chat](../../crates/radar-serve/src/chat.rs),
[evidence](../../crates/radar-serve/src/evidence.rs),
[consider](../../crates/radar-cli/src/consider.rs) and
[conformance](../../crates/repo-conformance/src/lib.rs).

- [ ] Add read-only evidence tools and a bounded request/response loop; the
  current fixed evidence bundle is not sufficient. Resolve every returned fact
  to its source, availability, scope and unknowns.
- [ ] Put orchestration at the outer caller. Keep the model-facing contract inert
  and preserve forbidden authority dependencies. The deterministic adapter alone
  validates recommendation shape and constructs a proposal for risk evaluation.
- [ ] Support abstain, investigate, enter, hold and reduce recommendations with
  expiry, strategy version, evidence and invalidation conditions. An amount from
  model output is a requested bound, never permission.
- [ ] Reserve aggregate spend and enforce depth, fan-out, retries and deadline.
  Add watermark-aware reusable dossiers and event-triggered refresh.
- [ ] Preserve original untrusted content as data through summaries and child
  results; reject tool requests outside the approved source surface.

**Acceptance evidence:** prompt injection in token/social content; fake tool
instructions; evidence arriving after the deadline; invalid/stale recommendation;
tool outage; exhausted budget; cache from a later watermark. The loop abstains or
returns bounded uncertainty and cannot construct authority, edit policy, reset
spend or lose the cost of failed work.

### P4 — Complete prospective shadow session and morning report

- [ ] Wire P1–P3 through a private, signer-free caller, including exit proposals
  and portfolio effects. A hypothetical policy may admit simulated actions; this
  must not change the public default or provide a hidden live switch.
- [ ] Capture opportunity time, evidence-ready time, reasoning duration,
  validation/route time and earliest eligible fill. Capture realistic quotes and
  exit state after that time, not the earlier price the model would like to get.
- [ ] Produce a report with equity, realised/unrealised results, all costs,
  controls, inventory, refusals, coverage gaps, unknown fills and protection state.
- [ ] Supply replay with the full recorded inputs and versions. Label decision
  replay, simulated fill replay and real execution evidence separately.

**Acceptance evidence:** one end-to-end prospective multi-family session can be
explained offline from the retained bundle. Force a missing exit, stale quote,
model timeout and no-trade session; each appears honestly in the denominator and
report. This is the first useful delivery and G1 instrument, not proof of profit.

### P5 — Decide whether specialists and learned features earn their place

- [ ] Compare cash, deterministic and simple statistical controls with a strong
  single agent, conditional specialists and an optional critic. Give the single
  agent parallel tools and comparable memory.
- [ ] Run equal-evidence and equal-dollar/deadline comparisons; record every
  attempt/version, truncated job and excluded candidate. Attribute shared versus
  incremental collection cost transparently.
- [ ] Freeze time-separated evaluation, overlap purging/embargo, time-block
  uncertainty and creator/funder sensitivity. Include historical-name masking and
  fresh prospective confirmation; do not call masking a complete leakage cure.
- [ ] Measure net portfolio utility, latency sensitivity, turnover, drawdown,
  tail losses, capacity and all research costs. Choose architecture using the
  primary criterion recorded in P0, not a favourable secondary metric afterward.
- [ ] Only after a recurring hypothesis earns attention, add a sandboxed
  research-to-feature candidate and compare it with its expensive parent process.
  Review and freeze promotion; never let live agents deploy their own changes.

**Acceptance evidence:** a reproducible comparative report with population and
missingness denominators, complete experiment ledger and uncertainty. If the fleet
does not improve the chosen net metric, keep it optional or remove it. If nothing
passes economic admission, continue hypothesis work; do not market an edge or
quietly lower the threshold.

### P6 — Authenticate authority and make risk action-aware

**Inspect/reuse:** [risk kernel](../../crates/radar-risk/src/kernel.rs),
[signer main](../../crates/radar-signer/src/main.rs),
[authorization-key boundary](../adr/0007-the-privy-authorization-key-lives-in-the-signer-process.md)
and [signer's own policy](../adr/0008-the-signer-holds-its-own-policy.md).

- [ ] Specify the trusted authorization issuer, authenticated intent, replay
  prevention, independent expiry state and atomic reservation/consumption. Keep
  secret authority outside model, serve and untrusted executor surfaces. Document
  exactly which compromise the design resists and which remains possible.
- [ ] Keep the kernel pure by passing validated snapshots and time explicitly.
  Add entry pause, bounded protective reduction and hard revocation semantics.
- [ ] Prove reduction from reconciled inventory and intended transaction effects;
  constrain proceeds destination and fees. A `reduce_only` flag cannot bypass loss,
  position or transfer policy by itself.
- [ ] Connect direct operator controls and persistent loss/failure counters.
  Require a reviewed explicit change to production no-signing conformance for the
  private caller; preserve the default-closed and model-dependency properties.

**Acceptance evidence:** forged issuer, reused authorization, expired intent,
caller-supplied stale time, concurrent overspend, forged reduction, unknown book,
revoked session and model outage. Reapply the wrong behaviour for the changed
security rules in the subsequent authorised verification workflow. A check that
only matches caller-supplied bounds is not reported as issuer authentication.

### P7 — Verify broad routes and complete buy/sell semantics

**Inspect/reuse:** [route](../../crates/radar-exec/src/route.rs),
[pipeline](../../crates/radar-exec/src/pipeline.rs),
[decoder](../../crates/radar-decode), and
[legacy-transaction ADR](../adr/0003-legacy-transactions-because-the-signer-must-be-able-to-read-them.md).

- [ ] Upgrade the routing contract deliberately from the inspected old Jupiter
  API, recording current request/response captures and provider access costs.
  Support arbitrary admitted input/output mints and both buys and sells.
- [ ] Add/verify adapters across the design's market families. Broad discovery
  does not grant live admission; publish progress and refusal reasons per family.
- [ ] Design and review versioned transaction/lookup-table resolution without
  weakening exact-byte verification or signer isolation. Authenticate resolved
  accounts; refuse unknown formats or unverified lookup state.
- [ ] Bind source/destination accounts, owners, mints, amounts, minimum output,
  fees/tips and ancillary instructions to the intended action. Inspect relevant
  CPI semantics, token extensions and program upgrades.
- [ ] Validate quotes and round trips at intended sizes; compare direct/aggregate
  routes and measure excluded router opportunity without signing unverified bytes.

**Acceptance evidence:** captured accepted transactions plus wrong mint, wrong
destination/owner, extra transfer, excessive fee/tip, changed route, modified
lookup mapping, Token-2022 hook/fee, expired quote and unsupported upgrade cases.
Show SOL and USDC entries and reductions across each claimed live-admitted family.
Devnet/simulation support does not become a mainnet support claim automatically.

### P8 — Durable submission, reconciliation and overnight protection

**Inspect/reuse:** [execution](../../crates/radar-exec),
[journal](../../crates/radar-journal), [audit](../../crates/radar-cli/src/audit.rs)
and plan 0010 item 4. Reuse its durable-effect primitive, with trading-specific
signature/validity and reconciliation semantics.

- [ ] Persist intent, reservation and recoverable signed transaction identity
  before broadcast. Separate not-sent from submission-unknown, submitted,
  confirmed, finalized and reconciled. Persist failures and attempts.
- [ ] Reconcile signature history and chain balances before a fresh replacement
  order; distinguish same-byte rebroadcast from re-signing. No blind timeout retry.
- [ ] Implement deterministic exit monitoring with fresh data and independent
  budget/fee reserve for a bounded session. Loss pauses entries without disabling
  every legitimate reduction; total revocation disables all new signatures.
- [ ] Add startup reconciliation, service health, reserve warnings, maximum
  holding time, session-end behaviour and a configured independent alert channel.
- [ ] Demonstrate owner recovery and revocation without model or Radar UI health.
  Vendor outage/recovery tests use the actual selected wallet configuration.

**Fault matrix required before G2 passes:** crash before reservation, after
reservation, after signing, after broadcast but before response, and after
confirmation but before local commit; RPC disagreement; expired blockhash with
uncertain history; duplicate event; partial fill; vanishing liquidity; model/credit
outage; signer/host outage; depleted fee reserve; restart during revocation. For
each, show actual remaining authority, capital reservation and operator-visible
state. A journal write failure prevents a new external effect.

### P9 — Supervised own-money canary, disabled until its gate passes

- [ ] Package a disabled private caller and a concrete proposed session manifest.
  Confirm P1–P8 evidence and economic admission; list supported routes and unknowns.
- [ ] Resolve the operator's wallet, principal, loss, fee/protection and research
  budgets, duration and conflict policy through explicit owner authority. Confirm
  global no-hold/comment handling, community-token exclusion and recovery.
- [ ] After that authorisation only, execute bounded supervised canaries, record
  actual entry/exit balances and complete reconciliation; compare with shadow
  assumptions and investigate every cost/latency discrepancy.

**Acceptance:** no unexplained balance difference or orphaned pending operation;
actual fees/fills are reflected in the experiment; independent pause/revoke works.
One successful buy does not pass. This plan alone does not permit the deposit,
signing, provider purchase or live run. Calibration despite inconclusive economics
needs a separate explicit experiment decision, not an automatic exception.

### P10 — Bounded unattended private sessions and iteration

- [ ] Run only after repeated supervised reconciliation and the protection/recovery
  drills pass. Set a funded time horizon and human escalation expectations.
- [ ] Compare daily and cumulative actual results with controls, costs and the
  registered risk mandate; preserve failed nights and open inventory in the record.
- [ ] Pause on violated operational or economic gates. Version a proposed fix and
  return it through shadow/canary as appropriate; never silently retune in place.
- [ ] Increase capital or session scope only through a new explicit mandate and
  capacity review. No autonomous top-up, loss-chasing or self-expanded universe.

**Acceptance:** repeated future sessions show the claimed operating behaviour and
complete accounting. Profitability is reported with uncertainty and all costs;
neither overnight uptime nor a winning streak establishes a commercial edge.

### P11 — Optional commercial decision after private evidence

- [ ] Evaluate capacity/crowding and customer outcomes after service fees; compare
  research-only value with autonomous value using actual pilot behaviour.
- [ ] Price managed operation plus capped intelligence and optional top-ups from
  measured workloads. Preserve free hosted tools, past research/export, direct
  control and the funded protection horizon when new AI spend stops.
- [ ] Obtain capability-by-country legal and provider review for the real service,
  including signing/recovery, outputs, public analyst/token conflicts and terms.
  Tennessee location is not worldwide authorisation.
- [ ] Only with a separately authorised pilot, measure voluntary continued use,
  replenishment/renewal, support burden, contribution and net customer utility.
  Do not count idle credit balances, incentives or operator P&L as retention.

**Acceptance:** a deliberate launch, revision or no-product decision. P11 is not
permission for this implementer to recruit, charge or publish claims. See design
0017 §11 and research 0032 for the commercial hypotheses and legal questions.

## Evidence ledger and review standard

Every completed item must carry the implementation commit, exact verification
command/CI job, outcome and deployment state. Proposed acceptance cases above are
requirements for future verification, **not names of tests that already exist**.
Do not tick a task merely because a module compiles or a vendor example works.

Use focused checks for changed behaviour and the repository's required CI gates.
For security/accounting changes, demonstrate the regression catches the relevant
wrong behaviour, following AGENTS. Do not run local builds/tests in this research
session or weaken checks to fit the workstation. Keep source-inspected findings,
offline validation, simulation, live captures and economic inference labelled.

Stop expanding verification after the relevant required checks pass unless new
changes/failures justify it. The goal is reliable evidence and a useful trader,
not an ever-growing checklist. If CI is unavailable for an implementation gate,
report the specific unavailable evidence and follow the owner's workflow.

## Where this plan is weakest

The hardest estimates are real-time cross-venue coverage, signer verification for
versioned/multi-hop transactions and failure recovery. They may dominate the build.
Architecture ranking is unmeasured; P5 can legitimately remove the fleet. Provider
quotes, actual model/data consumption, initial capital and legal scope are unset.
None should be invented by the implementer to make a milestone look complete.

P1–P4 must remain a narrow end-to-end delivery within a broad market architecture.
If they turn into a full-chain warehouse or general agent platform before the first
shadow report, reduce implementation breadth while preserving the explicit market
registry and multiple-family/SOL-USDC requirement. Do not retreat silently to
pump.fun-only data and call the owner's scope finished.

## Handback

**Stopped at:** design and implementation handoff only. Design 0017 and this plan
record the private-first direction; research 0032 links the superseding sequence.
All P0–P11 implementation checkboxes remain open. Source inspection used
`c39aabc8aafc3fcb9da2b0bf0c1329a18f75a212`; no runtime trading claim follows from it.

**Documentation verification, 2026-09-09:** PowerShell file/link inspection found
83 local references resolving across the three documents, with SPDX and status
headers present; all 26 source footnotes are defined and referenced. The scenario
arithmetic gives $0.09 per pass and $9/$36 per day for one/four passes across 100
candidates. The staged diff is reviewed before commit and checked with
`git diff --cached --check`. Only the design, this supplemental plan and the
research supersession note belong to this round. No build/test, paid API,
wallet, production or protected-plan change was performed.

**Next action:** the implementation session reads this plan and design 0017, then
does P0 against the actual current tree. Deliver P1–P4's multi-venue shadow loop
before billing or customer-wallet work. Reuse shared fixes already landed by the
other session. Record each completion with command, outcome and commit here.

**Do not:** edit plan 0010 from this handoff; treat 0 bps as proof future AI cannot
work; extrapolate pump.fun costs to every venue; open `Policy::CLOSED`; acquire
funds/keys or start real trading without the separate gate; promise returns,
perfect stops or full live market coverage before evidence; merge the research PR.
