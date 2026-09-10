<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0023 — A session record is what a replay would need

**Date:** 2026-09-10
**Status:** accepted, and **implemented in the same change**.
**Decides:** what one `radar consider` run writes down about itself, where it
lives, and what a report built from it is forbidden to say.
**Amends:** [`crates/radar-types/src/session.rs`](../../crates/radar-types/src/session.rs)
(new), [`crates/radar-cli/src/session.rs`](../../crates/radar-cli/src/session.rs)
(new), [`crates/radar-cli/src/consider.rs`](../../crates/radar-cli/src/consider.rs)
and [`crates/radar-cli/src/main.rs`](../../crates/radar-cli/src/main.rs).
**Does not add** `radar audit replay`. See §5.

## Context

`radar consider` ran hourly on the box and printed a funnel. The numbers were
true while they were on screen and gone afterwards, so the morning question —
*what did it do overnight, and how much of what it did not do was the market
rather than the instrument* — could only be answered by running the pass again
against a world that had moved.

Three specific things were unanswerable the next day.

1. **Whether a zero was a measurement.** A pass over a window nobody had
   collected printed `considering: 0` in exactly the same characters as a pass
   over a quiet market. The store has held the distinction since
   [`Coverage`](../../crates/radar-store/src/coverage.rs) landed —
   *ran and observed nothing* versus *never ran* — and nothing read it.

2. **What the run refused.** The strategy's tally was printed and dropped, the
   kernel's reasons were printed and dropped, and two `continue` statements
   discarded candidates without counting them at all: an in-window mint no
   candidate could be assembled from, and a candidate that vanished after the
   exit probe had already been paid for. Design 0017 §6 asks for the opposite —
   *"Do not discard excluded candidates from measurement"* — and a proposal rate
   whose denominator has a hole in it measures the hole.

3. **What it cost.** Every paid call was made and none was counted. Design 0018
   §7 charges an arm for the work that produced nothing, precisely so that
   *"an arm cannot improve its number by discarding what it wasted"*.

## Decision

**One durable record per run**, written to `<store>/sessions/<key>.json` when
`--record` is given, and rendered by `radar report --store <dir>`.

The key is the watermark then the wall-clock start, both zero-padded, so a
directory listing sorts into the order a reader wants without anything being
opened, and two runs at one watermark — which is what the hourly cron produces
whenever the recorder is behind — stay distinguishable.

**JSON, not a store table.** One row per run holding nested tallies whose reason
codes outlive the code that raised them. Flattening that into Arrow would either
lose the nesting or add a column per refusal reason, and the store's promise is
that DuckDB reads its Parquet directly. The per-candidate
[`Decision`](../../crates/radar-store/src/decision.rs) rows stay where they are;
this records the *run* they were taken in.

**The vocabulary lives in `radar-types`, the disk and the page in `radar-cli`.**
The record is a type about a decision session, beside
[`Portfolio`](../../crates/radar-types/src/portfolio.rs); the writer is
`consider` and the reader is `report`, both of which are the operator surface.

## What the report is forbidden to say

Every one of these is a place a zero could be printed and is not. The type
system carries four of them and a test carries the fifth.

| Absence | What is printed | Held by |
|---|---|---|
| No completed collection covers the considered window | *unknown*, and a paragraph saying the counts describe the recorder | `WindowCoverage::Unattested`, read by `render` before any count |
| A table nobody ever collected | *nothing attests this table* | `CoverageState::NeverAttested`, and every table appears |
| A holding nobody could price | the equity total is removed, not shrunk, and its reason is named | `EquityTotal::of` is the only constructor |
| The account could not be read at all | *the account could not be read*, and no rows | `AccountView::Unreadable` |
| Nobody priced the run's calls | *unknown*, with the reason | `MoneySpent::Unmeasured`, which is also the `Default` |
| No earliest eligible entry | which of the four conditions could not be dated | `EarliestEntry::Unknown` |
| A stage lost candidates | the residual, signed | `Funnel::unaccounted` |

**A completed collection that observed nothing still prints its zero.** Without
that the first row is not honesty, it is a refusal to report.

**No entry slot is named, and today none can be.** Design 0017 §6 requires the
earliest eligible entry to be after evidence, after reasoning, after policy and
after a transaction could have been built and landed. Under the shipped
`Policy::CLOSED` there was no eligible slot at all, which is a different answer
from *we could have and chose not to*. Under an open policy there would still be
no measured build-and-land latency on this instance, and the "~2.5 slots a
second" figures in this tree are bucketing heuristics — using one to name an
entry boundary would fabricate the number the whole fill rests on.

**Visibility is availability time, not event time.** A launch's `visible_at` is
the collection watermark of the earliest completed range whose observed span
contains it, and `Unattested` when nothing says. Substituting the launch slot is
exactly the fill at the launch price for a decision taken forty minutes later.

## Consequences

- `radar consider` reads the account on every run rather than only when
  something was proposed, because the report needs it either way. The error is
  still raised at the same point — after the early return for an empty proposal
  list — so an unreadable inventory stops the **sizing** and only the sizing.
- Three exits from the pass that used to `return Ok(())` silently now keep a
  record. A report assembled only from runs that got furthest would be a sample
  selecting itself.
- A run without `--record` writes nothing and says so. A read-only command that
  quietly starts writing to a production store is the thing `record_target`
  already exists to prevent.

## What this does not do

**It is not `radar audit replay`, and it does not add it.**
[`audit.rs`](../../crates/radar-cli/src/audit.rs) says why the subcommand is
absent: a replay needs the recorded *inputs* to a decision, and one built on
what exists would re-derive the verdict from today's world and call the
difference a divergence. This record is the **run-level half** of those inputs —
the watermark, the versions, the policy, the coverage the decision rested on,
the spend it took and the clocks it ran against. The per-candidate half is the
fact snapshot, and it is still missing. Building toward the input is not the
same as shipping the check.

**It is not the prospective loop.** No cohort, no scheduler, no comparison arm,
no statistic. Plan 0011 P5 and [design 0018](../design/0018-the-frozen-experiment-manifest.md)
govern those, and this record exists to make them possible rather than to
anticipate them.

**Design 0018's cost books are not filled in.** §7 asks for direct, marginal and
fully allocated economics. This records the call counts each of them would be
computed from, per kind, including the calls that failed and the calls that
answered and changed nothing. It does not record dollars, because §11 leaves
provider and model selection unset and no rate table exists to multiply the
counts by. `MoneySpent::Measured` is in the type for the day one does.
