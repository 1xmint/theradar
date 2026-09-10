<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0018 — The frozen experiment manifest

**Status:** **frozen measurement contract**, written before any outcome exists.
It fixes what will count as evidence for plan 0011's architecture decision. It
measures nothing, proves nothing, authorises no capital and permits no spending.
Every number in it is either cited to a document in this repository or marked
**unset**.
**Date:** 2026-09-09.
**Freezes:** [plan 0011](../plans/0011-private-autonomous-trader.md) P0's third
item — hypotheses, arms, cost accounting, evaluation window, loss reporting, and
the admission and stopping rule.
**Specification:** [design 0017](0017-a-private-autonomous-trader.md) §6, whose
four candidate mechanisms, fair-comparison design and fill-aware accounting rules
this document carries across and makes checkable. It does not reinvent them.
**Judged against by:** plan 0011 P5, which requires the architecture verdict to
use "the primary criterion recorded in P0, not a favourable secondary metric
afterward".

## 1. Why this exists and what "frozen" binds

Plan 0011 P5 will compare six arms and produce a number. If the criterion that
number is judged against is written after the number exists, the comparison is a
story. [`0025`](../research/0025-what-the-evidence-says-about-how-this-repository-is-run.md)
is the repository's own evidence that a rule written in advance is obeyed; this
document is the only version of that rule that can be honest, because it is being
written while nobody knows what the session will produce.

**What frozen means here, exactly.**

1. Anything in §§2–10 may be changed only by an amendment that is a **new
   numbered design document**, committed **before** the evaluation window opens
   or, if after, only together with the declaration that the open window is void
   and a fresh cohort begins. An edit to this file after a window opens is a
   defect, not an amendment.
2. The window's results are read against the version of this file recorded in the
   run bundle by content hash. A run whose bundle does not name a version of this
   file is not evaluable and is reported as such.
3. Freezing is not the same as being right. A frozen rule that turns out to be
   unmeasurable produces **inconclusive**, and inconclusive is a result. It is
   never grounds for substituting a rule that would have produced a verdict.

**What this document deliberately does not fix.** Capital, loss limits, drawdown
limits, holding duration, research budgets and the session calendar are left
**unset** by plan 0011's operating contract, which says so on purpose: "Numbers
remain **unset**, which blocks real spending rather than inviting an agent to
pick them." §11 lists every one of them and who owns it. Filling any of them in
here would read as authorisation, and it would be false.

**Inspected tree.** `main` at `7795de8`, 2026-09-09.

The session that drafted this had no `git` and read a working copy parked on a
deleted branch three commits behind, so it reported two things plan 0011 had
landed as missing: a market and quote asset on
[`Proposal`](../../crates/radar-risk/src/kernel.rs), and any reference to
[`Coverage`](../../crates/radar-store/src/coverage.rs) in
`crates/radar-backfill/src`. It was right to record that as a fact about its own
instrument rather than about the work — AGENTS §1 — and it was right to be
suspicious. **Both had in fact landed**, at `7278afc` and `7795de8`, and both are
present on `main`. §13's C-5 keeps the episode because the lesson is this
document's own subject: a file read is a fact about a checkout, not about a
project.

Nothing below depended on either being present; §10 states what the manifest
*requires* of them.

## 2. The primary criterion

**Exactly one metric decides the architecture. It is this one.**

> **Fully allocated net portfolio return, in basis points of the arm's mandated
> capital, over the frozen evaluation window — the arm's shadow-portfolio equity
> change after simulated execution costs charged at the measured round trip and
> after every research, data and inference dollar allocated to that arm,
> including rejected candidates and failed and truncated work — read as the
> paired difference between an arm and the cheaper control it must beat, over
> the same blocks and the same candidate population.**

**Direction of the test, which is half the criterion.** The burden is on the more
expensive arm. A richer architecture is adopted only if the paired difference
clears §9's three conditions. A difference that is not shown is not a difference,
and **ties go to the cheaper arm**. Design 0017 §5 gives the reason: "Give the
single agent parallel tools and equivalent cache access; otherwise a deliberately
weak baseline manufactures a fleet advantage." A symmetric test would let a
manufactured advantage win by accident; an asymmetric one cannot.

**Why this metric and not another on 0017 §6's list.**

- It is the only candidate that can be **negative while the reasoning is right**,
  which is precisely the question P5 asks. Design 0017 §12: "Positive selection
  with negative fully allocated net results falsifies the deployment choice, not
  necessarily the reasoning mechanism." The architecture decision *is* a
  deployment decision, so the criterion must be the one that can fail that way.
- **Gross return cannot see the question.** Research spend is the entire
  difference between one agent and a fleet. A criterion excluding it would let
  the fleet win by spending, and 0017 §6's "equal dollars and deadline"
  comparison exists to stop exactly that.
- **Risk-adjusted ratios are refused as primary.** These return distributions are
  heavy-tailed with a point mass at zero — [`edge.rs`](../../crates/radar-research/src/edge.rs)
  estimates a median's standard error from the interquartile range rather than a
  variance for that stated reason — so a Sharpe-like ratio is dominated by a
  handful of rows. Design 0017 §6 is explicit that "deflated Sharpe is one
  diagnostic, not a permission token."
- **Drawdown and tail loss are constraints, not the objective.** They void an
  arm rather than rank it (§8, §10). GOAL: "'Over time' rules out a strategy that
  wins often and ruins you once." A constraint traded off against return is not a
  constraint.
- **Dollar P&L is the same quantity unnormalised.** Basis points of mandated
  capital keeps the figure on the same scale as the 456 bps bar (§6), so a
  research bill and a trading result can be read against the same number.

**Secondaries are diagnostic only and may never be promoted.** Gross return
before costs; maximum drawdown; tail loss; turnover; capacity and impact-limited
size; concentration; stale exposure; failed exits; refusal rate by reason code;
latency from evidence-ready to earliest eligible fill; research cost per
decision; hit rate; deflated Sharpe.

> **No secondary metric may become the criterion after an outcome exists.** If
> the primary is inconclusive and a secondary looks favourable, the finding is
> *inconclusive* and the secondary is a lead for a **new hypothesis, a new
> version and a fresh future cohort** — never a re-reading of the window that
> produced it.

**Two different objects both called "primary", and they are not the same.**
Design 0017 §6 asks each *hypothesis* to record a "primary metric"; plan 0011 P5
asks for the *architecture* "primary criterion". This manifest keeps them apart:
the hypothesis-level metric (§4) decides whether a mechanism is worth carrying;
the architecture criterion above decides which arm runs. Neither substitutes for
the other. See §12, contradiction C-2.

## 3. The arms

Six, as design 0017 §6 and plan 0011 P5 both list them.

| Arm | What it is | Cheaper control it must beat |
|---|---|---|
| A0 **Cash** | No position. Denominated explicitly; USDC cash and SOL exposure are two different benchmarks and both are reported. | — |
| A1 **Deterministic** | The current shipped strategy at a frozen version, run under the shadow-only policy. | A0 |
| A2 **Statistical** | A simple fitted model over recorded features, chosen and frozen by [`edge.rs`](../../crates/radar-research/src/edge.rs)'s existing protocol on the fitting folds only. | A1 |
| A3 **Single agent** | One strong supervisor with parallel read tools, the full tool allowlist and the same dossier cache. | A2 |
| A4 **Agent + specialists** | A3 plus the three bounded specialist roles of 0017 §4. | A3 |
| A5 **Critic** (optional) | A4 plus an independent critic. Experimental arm, not a committee. | A4 |

**Each arm beats the arm above it or it does not run.** The chain is fixed here
so no arm may pick a favourable comparator later. A4 beating A2 while losing to
A3 is A3 winning.

**How an arm is frozen.** An arm identity is the tuple, recorded in the run
bundle at window open and hashed:

```
arm = (name, code commit, model id and version string, prompt hash,
       tool allowlist hash, memory/dossier policy, budget caps,
       policy version, cost-rate table version, population filter hash,
       sizing rule version, horizon set)
```

**Any field changing produces a new arm version and a fresh cohort.** Not an
amended arm, not a continued window. This is the same rule 0017 §6 states for
strategies — "a revised strategy starts a fresh version and future cohort" —
applied to the thing actually being compared.

**The baseline is not allowed to be weak, and this is checkable.** A3 receives,
by construction and not by intention: the identical tool allowlist, the ability
to issue tool calls in parallel, the identical dossier cache and watermark rules,
the identical wall-clock deadline, and — in the equal-dollars comparison — the
identical total spend cap. Design 0017 §6 also requires recording "truncation and
timeouts instead of granting only the fleet an extension", so:

> **If any arm was truncated by a limit another arm in the same pair did not
> reach, that block is reported as unsound for that pair and excluded from the
> primary criterion, and the exclusion is counted in the denominator.** A
> comparison in which only one side ran out of room measures the room.

## 4. The hypotheses

Design 0017 §6's four mechanisms, carried across unchanged in substance and
completed with the fields 0017 requires: "population, inputs and their
availability, horizon, entries/exits, sizing, risk settings, costs, null model,
primary metric, stopping rule and allowed revisions". **A hypothesis without a
falsifier is not admitted**, and the falsifiers below are 0017's own.

**Fields common to all four**, frozen here so they are not free parameters:

- **Horizons.** 1 h, 6 h and 24 h — the checkpoints the store actually measures
  ([`0030`](../research/0030-the-adversarial-audit.md) H3). A seven-day horizon
  exists only for mints in the analyst's reply log and **may not be claimed for a
  general cohort**; 0030 records the false statement that assuming otherwise
  produced.
- **Entry timing.** The earliest eligible entry is after evidence arrives,
  reasoning completes, policy approves and a transaction could have been built
  and landed (0017 §6). Never the launch price for a decision taken later.
- **Sizing.** The smaller of the mandated per-position cap (**unset**, §11) and
  the exit-simulation capacity at the frozen impact budget — `max_impact_bps: 100`
  in [`exit.rs`](../../crates/radar-sim/src/exit.rs)'s `Search::DEFAULT`. The size
  at an impact equal to the round trip is recorded **as a sensitivity only**,
  because [`0022`](../research/0022-capacity-was-a-budget-not-a-ceiling.md) shows
  the $31 figure is that budget's output rather than a venue wall. An absent
  capacity is refused, never treated as unlimited — [`kernel.rs`](../../crates/radar-risk/src/kernel.rs)
  and AGENTS §4 rule 9.
- **Risk settings.** One frozen policy version, identical across every arm, under
  a shadow-only policy in a signer-free process (0017 §6; plan 0011 P4). The
  shipped `Policy::CLOSED` default does not move.
- **Hypothesis-level primary metric.** Median net return per decision in basis
  points after the charged round trip (§6), over decision-eligible candidates —
  the quantity [`edge.rs`](../../crates/radar-research/src/edge.rs) already
  computes as `median_net`, so the mechanism question is answered by the
  instrument the repository has rather than a new one.
- **Costs.** §7. Charged to the arm that requested the work, including work that
  produced nothing.

| | **H1 structure + flow** | **H2 migration** | **H3 liquidity shocks** | **H4 narrative provenance** |
|---|---|---|---|---|
| **Population** | Launches decoded and quoted in the window, both quote assets, every admitted pool family | Tokens observed crossing a curve-to-pool migration inside the window | Pools with an observed reserve, bin or tick change beyond a frozen threshold | Candidates for which a timestamped first-seen external post exists |
| **Inputs, and when each is usable** | Creator/funder graph **as known then**; successful trades; independent-wallet uncertainty; liquidity; executable exits. Gated on availability time, not event time | Curve-to-pool identity; migration timing; holder change; pool state; SOL and USDC routes | Point-in-time reserves/bins/ticks; route quality; flow; later actual or conservative fills | First-seen posts, edits and deletions where available; source lineage; timestamped chain reaction |
| **Entry** | At earliest eligible fill after the score crosses the frozen threshold | Same, conditioned on migration observed | Same, conditioned on the shock being observed and a route quoted | Same, conditioned on the post being *usable*, not merely existing |
| **Exit** | At the horizon, or earlier on the frozen protective rule | Same | Same, plus the exit-stress variant | Same |
| **Null model** | The same population scored by structure alone, and by flow alone; plus [`0017`](../research/0017-a-control-that-could-have-been-traded.md)'s untouched-population control, matched on token age at entry and holding period | The same thesis expressed **without** the migration input, and a random-entry control at matched times and sizes | The same decisions delayed by the observed reasoning-duration distribution, and filled at conservative quotes | A **chain-only** control on an identical population and timing |
| **Falsifier (0017's own)** | "Reject if improvement vanishes after creator/time grouping or costs." | "Reject if post-migration opportunity is already priced before Radar can act." | "Reject if latency sweep or exit stress erases it." | "Reject if chain-only controls match it after paid collection and timing." |
| **Permitted revisions before the window closes** | None to the threshold, the feature set or the population filter. Fixing an instrument defect that changes a denominator voids the window (§9 D) rather than amending it | As H1 | As H1 | As H1, and paid social collection may not be increased mid-window |

**A note on what these are.** 0017 §6 calls them "research proposals, not signals
known to work or recommendations to buy particular tokens", and
[`0011`](../research/0011-graduation-predicts-volatility-not-profit.md) is the
standing correction that a signal predicting graduation is a reason to stay away
rather than a reason to buy. Nothing here predicts a return until the window says
so.

## 5. The two comparisons

Both are run. They answer different questions and neither substitutes.

- **Equal evidence.** Every arm sees the same candidate population, the same
  dossiers, the same watermark and the same deadline. Isolates whether reasoning
  topology helps at all.
- **Equal dollars and deadline.** Every arm gets the same total spend cap and the
  same wall clock. Measures whether specialist collection earns its total cost.

**They can disagree, and that is informative rather than a failure.** An arm that
wins on equal evidence and loses on equal dollars is a better reasoner that is not
worth deploying at the tested budget; the manifest's verdict in that case is
**not adopted**, because the primary criterion (§2) is the fully allocated one and
the equal-dollars comparison is where it is measured. The equal-evidence result is
reported beside it as the diagnostic that says *why*.

**Replay is for debugging and cheap rejection, not for the central claim.** 0017
§6: "Historical replay ... cannot fully remove pretrained knowledge of later token
outcomes ... rely principally on new prospective cohorts for the central edge
claim." Event time, observed slot, provider timestamp, capture time and earliest
usable time are five separate clocks and are recorded separately. Identifier
masking is a **sensitivity check only** and is never described as a leakage cure.

## 6. The bar, and what it is a bar for

The cost side is measured and dated. It is not universal.

| bps | what it is | source | scope |
|---|---|---|---|
| **456** | the bar a strategy clears before one trade is worth making | [`0022`](../research/0022-capacity-was-a-budget-not-a-ceiling.md), 2026-09-01 | all-in round trip in the **$20–$200** notional band, used as the fee rate in `s* = (r − a) / 2b` |
| **850** | the all-in round trip the kernel assumes, and separately the edge at which an optimal size of even **$59** appears | [`0019`](../research/0019-the-round-trip-is-not-one-number.md), [`0022`](../research/0022-capacity-was-a-budget-not-a-ceiling.md) | **fresh-launch pump.fun tokens** — the cohort Radar's own trades belong to |
| **0** | the current deterministic instrument's measured selection edge | [`0017`](../research/0017-a-control-that-could-have-been-traded.md), corrected 2026-08-30 | 990 proposals against 38,461 untouched tokens, median across four matched strata |

**These are dated pump.fun measurements, not universal Solana costs.** Design
0017 §1 says so in terms — "They are neither universal Solana costs nor an
estimate of future AI performance" — and GOAL's own reader note lists the bar and
the measured edge among the four things most likely to go stale. Consequently:

> **Every venue and every strategy carries its own measured hurdle.** A candidate
> on a venue whose round trip has not been measured on that venue is **recorded,
> scored, and excluded from the primary criterion**, with the exclusion counted in
> the denominator as `HurdleUnmeasured`. It is not charged 456, and it is not
> charged zero. AGENTS §4 rule 9: absent is not zero, and unknown is not safe.

**Which cost is charged, by default.** The fresh-launch **850** is the default
charge, as [`radar edge`](../../crates/radar-research/src/edge.rs) already does,
with the notional band offered as a sensitivity. [`STATE`](../STATE.md) records
why: 0019 "explicitly declines to lower the constant ... because a cost estimate
rounded down launders a trade past the gate that should have refused it."

**And zero is a measurement about the instrument.** The 0 bps figure describes the
deterministic strategy of 2026-08-30. It is not a prediction about any arm in §3.
AGENTS §2: "An absent measurement is a fact about the instrument, never a verdict
on an idea." LEARNINGS 35 is the entry that was paid for getting this backwards.

## 7. Cost accounting

**Research spend counts, including work that produced nothing.** Rejected
candidates, abandoned investigations, timed-out jobs, retries and every child call
in the decision tree are charged to the arm that requested them. An arm cannot
improve its number by discarding what it wasted.

**Three books, all reported.**

1. **Direct cost.** Tokens, tool calls and paid reads attributable to a decision.
2. **Marginal economics.** Direct cost only, shared collection excluded.
3. **Fully allocated economics.** Direct cost plus a share of collection —
   recorder, decoding, storage, RPC, hosting — allocated across arms **in
   proportion to decision-eligible candidates scored**. Any allocation key is
   arbitrary; this one is frozen here so that it cannot be chosen after the
   numbers are known.

> **The primary criterion uses the fully allocated book.** Marginal is reported
> beside it as a diagnostic and may not be substituted for it. Design 0017 §6:
> "Allocate common collection cost transparently when comparing deployments; show
> marginal and fully allocated economics."

The cheap deterministic pre-filter's cost is charged to every arm consuming its
output, on the same key, and its **missed-opportunity rate is measured** rather
than assumed — excluded candidates stay in the denominator (0017 §6).

**The cost scenario is an illustration and must not harden into a budget.**
Design 0017 §6, from [`0032`](../research/0032-an-advanced-assistant-worth-buying-again.md)'s
dated supplier table (Anthropic's published $3/M input and $15/M output for Sonnet
4.6, checked 2026-09-09):

> 20k input and 2k output tokens cost **$0.09** a pass; four passes **$0.36**; at
> 100 evaluated candidates a day that is **$9 versus $36 a day**, or **90 versus
> 360 basis points a day on a $1,000 portfolio**, before data, execution and
> hosting.

**This is a scenario, not a measurement, not a workload requirement, not a model
selection and not Radar's provider bill.** One arithmetic observation is worth
making about it, clearly labelled as arithmetic on an illustration: *if* those
figures were measured, research cost alone would run at 90–360 bps a day against
a per-trade bar of 456 bps — which is why the fully allocated book is the primary
one. **Nothing in §11 is filled in by this paragraph.** Actual token, call and
byte consumption is booked from the run, per arm, per decision.

## 8. Loss reporting, denominators and dependence

**Loss reporting.** Realised and unrealised are reported separately, per arm, per
block, in the base reporting currency (**unset**, §11), with gross trading result
stated separately from cost so that "a useful model with an uneconomic
deployment is diagnosable" (0017 §6). Missing or unexitable inventory **stays in
the report** under a stated conservative valuation and stress rule; it is never
dropped as an absent label. Failed exits, stale exposure and no-trade blocks are
data, not gaps. A no-trade block is a data point with a value.

**Every denominator is reported, per arm and per block.** Discovered; observed;
decoded; quoted; round-trip verified; live-admitted — the progression 0017 §3
defines, with a reason for stopping at each stage. Then: decision-eligible;
scored; refused with reason code; dropped with reason; labelled per horizon; and
surviving the purge and the embargo. Absent labels are reported by the six reasons
the store already records ([`STATE`](../STATE.md), 2026-09-08), "because a sample
missing labels for want of any measurement is a different sample from one missing
them because every exit price was stale."

**Dependence.** Trades are not independent.

- **Time blocks.** Uncertainty on the primary criterion comes from a moving-block
  bootstrap (Künsch 1989)[^1] with block length at least the widest label horizon,
  24 hours — the same span as `EMBARGO_SLOTS` (216,000 slots) in
  [`edge.rs`](../../crates/radar-research/src/edge.rs), so no two blocks share a
  label window. The resample count is fixed before the window opens and recorded;
  at least 10,000, the usual floor for a 95% endpoint, stated as convention with
  its reason rather than as a measurement.
- **Clustering.** Additionally on creator, and on funder where the funder is
  resolvable. **It often will not be**:
  [`0024`](../research/0024-the-spike-became-a-hump-and-the-signal-moved.md)
  records that recipients are token accounts rather than people, and
  [`0012`](../research/0012-recipient-sets-cannot-recur-authorities-can.md) that
  recipient sets cannot recur across launches. Where funder clustering is
  unavailable the report **says so**, and says that the interval is therefore
  likely too narrow.
- **Wilson bound.** The share-of-blocks condition in §9 uses the Wilson 95% lower
  bound (Wilson 1927)[^2], as `Reading::wilson_lower` already does.
- **Deflated Sharpe** (Bailey and López de Prado, 2014)[^3] is reported with the
  ledger's trial count and is **a diagnostic, not a permission token** (0017 §6).

**The experiment ledger.** Every arm, hypothesis, version, attempt, truncated job
and excluded candidate is recorded, including the ones that produced nothing. The
ledger's trial count is what multiplicity is judged against, and GOAL names the
failure it prevents: "Letting a user hunt for the stratum where the numbers look
good is how a null result gets sold as a strategy."

## 9. The stopping rule

Quoted as a block because it is meant to be quoted back.

> **A. The window closes on a pre-registered condition, never on a result.** The
> evaluation window closes at the first of: (i) every arm has produced at least
> **100** decision-eligible candidates in each of **5** time blocks, and the last
> block's labels have matured plus one embargo period; (ii) the owner's calendar
> limit (**unset**); (iii) the owner's research budget cap (**unset**) is
> exhausted. The floors of 100 rows and 5 blocks are
> [`edge.rs`](../../crates/radar-research/src/edge.rs)'s `MIN_ROWS` and `FOLDS`,
> already enforced in code, and are not lowered to make a window close. **The
> window does not close because a result looks good, and it is not extended
> because one does not.**
>
> **B. No interim promotion.** The primary criterion may not be computed on
> partial data by anyone able to change an arm. Operational health — crash rates,
> coverage, budget burn, refusal counts — may be inspected at any time.
>
> **C. A richer arm beats its control only if all three hold.** Per pair, on the
> paired difference in fully allocated net basis points:
>
> 1. **Enough evidence.** At least 5 blocks, each holding at least 100
>    decision-eligible candidates **for both arms**.
> 2. **Measurably above zero.** The difference exceeds one standard error of
>    itself, from the block bootstrap of §8. A difference that merely looks
>    positive inside its own noise has not been shown to be.
> 3. **More than half the blocks favour the richer arm**, at the Wilson 95%
>    lower bound. A mean over a few blocks is a report about those blocks.
>
> All three, or the richer arm does not win. Ties go to the cheaper arm.
>
> **D. Early stop, which voids an arm or a window and never rescues one.** Any of
> these halts the window: a breach of the owner's loss or drawdown mandate; the
> discovery of an instrument defect that changes a denominator; a cost-rate,
> model, prompt, tool, threshold, sizing, horizon or population-filter change to
> any arm. In every case the window is **void**, the partial result is recorded in
> the ledger as an attempt, and a fresh version and a fresh future cohort begin.
> A void window's data is never pooled with its successor's.
>
> **E. Inconclusive stays inconclusive.** Extending a closed window, pooling
> cohorts, or re-reading the same cohort under a new stratum are each a **new
> trial** and enter the ledger. None of them converts an inconclusive result into
> a verdict.
>
> **F. Clearing this rule permits nothing to be bought.** It selects an
> architecture. Money is §10, and §10 is still not permission.

**What this rule costs, said plainly.** It is strict enough that the most likely
outcome of the first window is *inconclusive*, particularly on candidate arrival
rates that nobody has measured (§11). That is the intended failure direction. A
rule loose enough to guarantee a verdict would produce one whether or not there
was anything there, which is the thing
[`0030`](../research/0030-the-adversarial-audit.md) found this repository doing in
four separate places.

## 10. Admission — what would permit a supervised own-money canary

Pre-registered, per 0017 §6 and plan 0011's G3. **All of these, before any canary
is proposed:**

1. **Positive net expectancy at the proposed size under conservative cost and
   latency assumptions.** Conservative means: the fresh-launch **850** charged
   rather than 456 (§6); the slower tail of the observed latency distribution
   rather than its median; conservative valuation for unexitable inventory; and
   the venue's own measured hurdle where the venue is not pump.fun.
2. **Incremental value over the chosen cheaper control**, by §9 C, against the
   control named in §3 and not one chosen afterwards.
3. **Loss inside the mandate.** No block in the window breached the owner's loss
   or drawdown limits (**unset**, §11), and tail loss is reported with its
   uncertainty.
4. **The lower bound stays positive under the dependence treatment of §8**, and a
   sensitivity is reported for the clustering that was unavailable.
5. **G2 passed.** Admission here is economic only. Design 0017 §11: "Technical
   correctness, economic promise, statistical confidence and permission to risk
   money are separate gates."
6. **Conflicts excluded.** The community token, prize funds and any mint the
   public analyst has commented on are excluded from every arm's mandate and
   counted in the discovery denominator as excluded — GOAL's no-hold rule and
   [ADR 0013](../adr/0013-a-community-token-exists-and-radar-holds-none-of-it.md).
   Read-only shadow scoring of them is not blocked (0017 §11).

**No trade count and no p-value alone establishes readiness** (0017 §6). **An
inconclusive result stays inconclusive.** A revised strategy starts a fresh
version and a fresh future cohort. And clearing all six does not fund anything:
the wallet, principal, limits and session remain the owner's separate decision.

## 11. What is unset, and who owns it

**Owner's, and deliberately blank** — plan 0011's operating contract:

| Unset | Consequence of it staying unset |
|---|---|
| Mandated capital and base reporting currency | Arms run at a normalised notional; no dollar figure is quotable |
| Allowed quote assets and minimum SOL reserve | The SOL/USDC arm split cannot be finalised |
| Per-position, aggregate and correlated-group caps | Sizing falls back to capacity alone, which is not a mandate |
| Session and daily loss limits, drawdown limit | §10 condition 3 cannot be evaluated |
| Maximum holding duration | The protective exit rule is incomplete |
| Research budget: per job, per day, per window, per arm | §9 A (iii) has no value and the equal-dollars comparison has no cap |
| Calendar limit for the window | §9 A (ii) has no value |
| Provider and model selection, hence real cost rates | §7 books a scenario until a bill exists |

**Unmeasured, and nobody's to choose** — these are measurements the window itself
or prior work must produce:

- The per-venue round trip for any venue other than pump.fun. **Nothing decodes
  Raydium, Orca or Meteora**: `crates/radar-decode/src` holds
  [`pumpfun.rs`](../../crates/radar-decode/src/pumpfun.rs) and no other venue
  module, read 2026-09-09.
- The arrival rate of decision-eligible candidates, and therefore the window's
  actual calendar length.
- Per-arm truncation, timeout and tool-failure rates.
- Real token, call and byte consumption per decision.

**None of these may be invented to make a milestone look complete.** Plan 0011
says so of its own numbers, and it applies here with more force, because a guessed
number in a frozen manifest is indistinguishable from a result.

## 12. What this manifest requires of data that does not exist yet

Design 0017 §3: "The first shadow cohort must include multiple pool families and
both SOL and USDC routes." That is a **precondition on opening the window**, not a
thing the window can discover.

> **Before the window opens**, at least two pool families and both SOL and USDC
> routes must have reached *round-trip verified* in 0017 §3's progression, each
> with its own measured hurdle (§6). Until then the window cannot open on the
> Solana-wide question.

**If only pump.fun is decoded when the window would otherwise open**, the
permitted fallback is narrow and must be declared: run a **pump.fun-only cohort**,
register it as a different and narrower experiment with its own manifest version,
and refuse to generalise from it. Plan 0011 is explicit about the failure this
prevents: "Do not retreat silently to pump.fun-only data and call the owner's
scope finished."

**The consequence worth stating.** 0017 §10 ranks the multi-venue shadow loop as
the fastest way to test the actual thesis, and §3 makes it depend on decoder work
that is unstarted. The fastest test is gated on the slowest input. That is a
sequencing fact, not an argument for opening the window early.

## 13. Contradictions recorded, not resolved

Per the brief: where the source documents disagree, this section records the
disagreement rather than smoothing it.

**C-1. GOAL and design 0017 disagree about when venues come.** GOAL: broadening
"is still worth doing *after* an edge exists, because venues make a working edge
bigger and do not create one." Design 0017 §1 supersedes that paragraph "**for
this private experiment**" on Josh's Solana-wide instruction, and says it is
recording a conflict rather than rewriting GOAL. **This manifest follows 0017**,
and §6's per-venue hurdle rule is what keeps GOAL's caution operative: breadth
does not lower the bar, it multiplies the number of bars that must be measured.

**C-2. Two objects are both called "primary".** 0017 §6 asks each hypothesis to
record a "primary metric"; plan 0011 P5 asks for one "primary criterion" for the
architecture. They are different objects at different levels, and reading them as
one would let a hypothesis-level median stand in for a portfolio-level decision.
**Separated in §2 and §4; not merged.**

**C-3. Plan 0011 P5 leaves an escape that 0017 §10 does not.** P5: "If the fleet
does not improve the chosen net metric, keep it optional or remove it." 0017 §10,
rank 8: "Add only if it wins the controlled comparison." "Keep it optional" would
let a non-winning arm survive as a default. **Recorded, and this manifest takes
the stricter reading**: an arm that does not clear §9 C is **not deployed**, and
"optional" may only mean retained as an experimental arm at zero live weight.

**C-4. The number 850 does two jobs.** [`STATE`](../STATE.md)'s reconciliation
table calls it "the **measured all-in round trip** the kernel assumes" for
fresh-launch pump.fun tokens; 0022 and GOAL also use 850 as the **edge** at which
an optimal size of even $59 appears. A cost charged and an edge required are not
the same quantity, and 0032 already warns not to "add overlapping all-in estimates
together". **Every figure in this experiment that quotes 850 must say which of the
two it means.** §6 states both rows separately for that reason.

**C-5. Withdrawn — it was a stale checkout, not a contradiction.** This entry
originally recorded that P0's market vocabulary and P1's coverage writer were not
visible on disk in either tree, against a claim that both had landed. Checked
2026-09-09 after the fact: both had landed and were merged to `main` at `7278afc`
and `7795de8`. The working copy that was read was parked on a deleted branch at
`f13deb9`, three commits behind, and the second tree read is a worktree of the
same repository and therefore equally stale.

Kept rather than deleted, because the mistake is instructive and this document is
about not fooling ourselves: **a file read is a fact about a checkout, not about a
project.** Confirm with `git rev-parse HEAD` against `origin/main`, or read
through `git show origin/main:<path>`, before reporting that something is absent.
`grep` on a working tree answers a different question from the one being asked.

The manifest does not depend on it either way.

## 14. Where this manifest is weakest

**The primary criterion may be unmeasurable at the capital that is set.** If the
mandated principal is small, the fully allocated research bill dominates the
trading result and every arm reads negative — which is a true statement about the
deployment and a useless one about the reasoning. The gross secondary exists to
diagnose exactly that, and §2 forbids promoting it. If that is the outcome, the
honest finding is "no architecture is deployable at this capital", and the next
question is the owner's, not the experiment's.

**Five blocks of a hundred may be unreachable.** The floors are lifted from code
that already enforces them on a single-venue store with 483,629 launches. A
multi-venue shadow cohort filtered to decision-eligible candidates could be far
thinner, and §9 forbids lowering the floor to close the window. This manifest
would then produce *inconclusive* repeatedly, at real cost. That is the price of
the rule, chosen deliberately.

**The dependence treatment may still be too narrow.** Funder clustering is likely
unavailable (§8), creator clustering alone leaves shared-funder correlation in the
residual, and the interval will therefore read tighter than the truth. The report
must say so every time. This is a known weakness with no fix inside this window.

**Nothing here has been run.** No build, no test and no measurement was performed
for this document; it was written from source and document inspection on
2026-09-09, against a tree whose commit this session could not establish. Every
figure it carries is cited to another document, and the arithmetic in §7 is on an
illustration.

## Sources

Repository documents are cited above with their own dates. External statistical
methods were checked 2026-09-09.

[^1]: Hans R. Künsch, [The Jackknife and the Bootstrap for General Stationary Observations](https://projecteuclid.org/journals/annals-of-statistics/volume-17/issue-3/The-Jackknife-and-the-Bootstrap-for-General-Stationary-Observations/10.1214/aos/1176347265.full), *The Annals of Statistics* 17(3), 1989, 1217–1241, DOI 10.1214/aos/1176347265. "Bootstrap replicates are constructed by selecting blocks of length l randomly with replacement among the blocks of observations."
[^2]: Edwin B. Wilson, [Probable Inference, the Law of Succession, and Statistical Inference](https://www.tandfonline.com/doi/abs/10.1080/01621459.1927.10502953), *Journal of the American Statistical Association* 22(158), 1927, 209–212. The interval `Reading::wilson_lower` computes.
[^3]: David H. Bailey and Marcos López de Prado, [The Deflated Sharpe Ratio: Correcting for Selection Bias, Backtest Overfitting, and Non-Normality](https://www.davidhbailey.com/dhbpapers/deflated-sharpe.pdf), 2014-07-31. Cited by design 0017 for the same purpose and with the same limitation.
