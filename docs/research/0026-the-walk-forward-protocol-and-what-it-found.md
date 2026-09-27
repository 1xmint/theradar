<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0026 — The walk-forward protocol, run for the first time

**Date:** 2026-09-27
**Status:** measured. **No stratum clears the bar.** A null result, over a
population large enough to say so with intervals rather than a shrug.
**Source:** `radar features` then `radar edge` (plan 0007 items 1-2, both
built and unrun until now), run on the production guardian VPS against
`/home/guardian/radar/data/store`, watermark slot 451035678. Output kept at
`/home/guardian/radar/data/edge-runs/2026-09-27/` (also
`~/phase-i/out/` on the box during the run).
**Bears on:** [plan 0007](../plans/0007-the-learning-loops-instrument.md) items
3-4, and [plan 0014](../plans/0014-close-the-terminal-and-measure-the-edge.md)
Phase I. Closes plan 0007's open handback.

## The question this closes

Plan 0007 built the instrument — `radar features` writes a deterministic
feature table, `radar edge` runs the walk-forward protocol over it — and
never ran it against a real store, because no store existed off the
production box. This is that run.

## Commands, exactly

```
radar features --store /home/guardian/radar/data/store \
  --from 441040080 --to 445363440 \
  --out features-441040080-445363440.parquet

radar edge --features features-441040080-445363440.parquet \
  --rates docs/research/data/0024-base-rates.json
```
Binary: a Linux build of `main` at `4fdb9e3`, scp'd to the box for this run
and removed afterward (`~/phase-i/radar`, sha256 `3359c3b7...`, matching
`BUILD-INFO.txt`). `nice -n 19 ionice -c3 timeout 45m` / `timeout 10m`, single
process, no flags asking for more than one thread (neither subcommand has
one).

## The window, and why it is not the whole store

The store spans slots 441040080..451033908 (~1,110 chain-hours, ~46 days,
1,228,880 launches). The window run here is the **oldest ~20 days** of it,
441040080..445363440 — chosen after a full-store `radar features` pass was
started, watched, and killed before it finished: it was on a trajectory to
exceed both the 45-minute budget and a safe share of the box's memory (see
*The instrument, measured* below), and this repository's own rule is to run
smaller and report rather than push on. The oldest fifth was chosen over the
newest so that every launch in it has had time to clear its 24-hour
checkpoint against the current watermark.

## The instrument, measured

`radar features` costs **~2.1-2.4 GB resident** on this run, and that figure
barely moved between a 2-day trial window (45,458 rows) and this 20-day one
(595,202 rows) — both hit roughly 2 GB within the first two minutes. That
points at a cost dominated by building the creator-history index once over
however much of the store's launch table the window touches, not by the
number of output rows. On a 3.9 GB, 2-core box also running `radar-serve`,
`radar-follow` and `radar-market-tape`, that is a large fraction of total
memory; the full-store attempt pushed free memory to ~100 MB and swap usage
climbing before it was killed. `radar edge` itself is cheap: 409 MB, two
seconds, reading the parquet file rather than the store. This is a fact about
what the tool costs on this box today, not a finding about the strategy, and
it is why later runs on this box should stay windowed rather than assume the
whole store is affordable in one pass.

## What was measured

| | |
|---|---|
| horizon | 24h |
| population (eligible launches in window) | 595,202 |
| labelled rows | 181,675 (31% of the population) |
| round trip charged | 850 bps — the fresh-launch cohort's measured all-in cost (research 0019, snapshot 2026-09-03) |
| bar, for context | 456 bps — the same measurement read in the $20-$200 band (research 0022); not a second hurdle, per plan 0007 Q2 |

Of the 69% without a 24h label:

| reason | rows | % of population |
|---|---|---|
| stale entry price | 355,794 | 60% |
| the only measurement doubles as both endpoints | 32,536 | 5% |
| stale exit price | 19,807 | 3% |
| no exit measurement at all | 5,254 | 1% |
| no entry measurement | 136 | 0% |

**The trades table is still empty** (`/home/guardian/radar/data/store/trades`
holds no data, checked 2026-09-27, same finding as plan 0007 Q4 on
2026-09-06). Of the 24 features `radar features` defines, the twelve
trade-derived ones are absent for all 595,202 rows — reported as absent, not
zero, which the tool itself confirms in its own output. This run measures
only the creator-record, launch-metadata, dev-buy and decision-lane features,
as plan 0007 said it would until the recorder writes trades.

**The fit fold held nothing.** The protocol's first three folds (fitted as
one) span slots 441040725..443172994, and every one of them reports **0
labelled rows out of ~119,040 population each** — not a small number, zero.
The test folds immediately after (443172996..445363440) are populated
normally (78,794 and 102,881 labelled rows). This was not investigated
further inside this run — it says something about outcome-backfill coverage
of the earliest days of the store, not about the strategy, and it is why the
fitted half of the protocol below found nothing to fit on. It is recorded
here as an open question, not explained.

## What was found

**Fitted strata: nothing.** `radar edge` reports the whole grammar tried (the
count printed is `0`, meaning no candidate held the scaled fitting-fold row
floor — see plan 0007 Q3 item 1) and its own diagnosis: *"nothing in the
grammar held enough of the fitting period to be testable, which is a fact
about the table rather than a result."* With zero labelled rows in the fit
fold, this is exactly what the harness should say, not a surprise.

**Fixed strata (`creator_edge`'s thresholds and the refusal signals'
complements), tested without fitting:**

| stratum | test0 n / paid / Wilson≥ | test1 n / paid / Wilson≥ | gross | net | cleared? |
|---|---|---|---|---|---|
| `creator_prior_launches≥5, creator_prior_organic≥1, creator_launches_per_day<10` | 3,651 / 118 / 0.027 | 7,183 / 231 / 0.028 | 0 bps | −850 bps | no |
| refused: a record and no organic graduation | 20,985 / 236 / 0.010 | 32,919 / 374 / 0.010 | 0 bps | −850 bps | no |
| refused: launching too fast | 32,609 / 422 / 0.012 | 41,489 / 533 / 0.012 | 0 bps | −850 bps | no |
| refused: launch block at 10 recipients or above | no rows | 89 / 6 / 0.031 | 0 bps | −850 bps | no |

Every stratum's **median gross return over the labelled rows was exactly zero
bps** — the point-mass-at-zero shape research 0017 already warned a median
alone cannot distinguish from a real null, which is why acceptance here also
requires the Wilson lower bound of the paid share to clear one half. None of
the four came close: the highest lower bound measured was 0.031, meaning at
most about 3% of rows in that stratum are known to have paid, a long way from
the 50% the bar requires. None cleared both test folds. None cleared one.

## The verdict

**Nothing found, over 181,675 labelled decisions at the 24-hour horizon,
against a bar of 456 bps (charged as 850 bps round trip).** This is the
walk-forward protocol's first real answer, and it agrees in direction with
research 0017's 0 bps measured on the selection edge: no stratum this table
can name — fitted or the four fixed candidates already in production code —
clears the bar out of sample.

**What this does and does not settle**, per LEARNINGS 35 and AGENTS.md §1's
zero rule. It settles what the shipped policy already does: `Policy::CLOSED`
refuses every proposal today, and this measurement gives no reason to change
that — the refusal readings above (creator-record and cadence signals) are
close to what the product currently sells as its refusal logic, and they
still show no edge. It does **not** settle that no edge exists in this data:
twelve of twenty-four features are absent because the trades table is empty,
the fit fold held zero labelled rows for reasons not yet diagnosed, and the
window is less than half the store. A measured zero from an instrument
running on 31% label coverage and none of its trade-derived features is a
statement about what could be seen, not a ceiling on what is there. The
single highest-value repair plan 0007 already named — getting the recorder to
write trades — is unchanged by this run and still outside its scope.

## Not checked here

- The rest of the store (the newer ~26 days) was not run, to stay inside the
  memory and time budget described above.
- The 6-hour horizon (`--horizon 6h`) was not run; only 55 of 595,202 rows in
  this window carry a 6h label, too few to be worth a second pass.
- No sensitivity run against a different `--cost-band` (the 456 bps band
  itself, for instance) was taken; the default 850 bps fresh-launch charge is
  the one this repository holds is correct for this population (plan 0007
  Q2), so a lower-cost sensitivity run was not run and would only make
  acceptance easier for the wrong reason.
- Why the fit fold's population carries zero labels was not investigated;
  it needs a separate look at outcome-backfill coverage for the store's
  earliest days, not a re-run of this protocol.
