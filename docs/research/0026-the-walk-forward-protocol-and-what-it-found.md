<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0026 — The walk-forward protocol, run for the first time

**Date:** 2026-09-28
**Status:** measured. **No stratum clears the bar, and every one large enough
to read loses money before costs.** Over 15,867 labelled launches from
2026-08-31 to 2026-09-13, at the 24-hour horizon. The labelled rows are a
biased 4% of the population, so this is a result about coins still trading a
day after launch, not about every launch — see *What the sample is*.
**Source:** `radar features` then `radar edge` (plan 0007 items 1-2), run on
the production guardian VPS against `/home/guardian/radar/data/store`,
watermark slot 451198556, with the Linux release build of `main` at `ca2ef05`
(sha256 `a564e917…`, matching that build's `BUILD-INFO.txt`). Output kept at
`/home/guardian/radar/data/edge-runs/2026-09-28/`.
**Bears on:** [plan 0007](../plans/0007-the-learning-loops-instrument.md) items
3-4, and [plan 0014](../plans/0014-close-the-terminal-and-measure-the-edge.md)
Phase I. Closes plan 0007's open handback.

## The question this closes

Plan 0007 built the instrument — `radar features` writes a deterministic
feature table, `radar edge` runs the walk-forward protocol over it — and never
ran it against a real store, because no store exists off the production box.
This is that run. It took three attempts; the first two were faults in the
instrument and its inputs, and they are recorded below because each one would
have been published as a finding had it not been checked.

## Commands, exactly

```
radar features --store /home/guardian/radar/data/store \
  --from 443100000 --to 446800000 \
  --out out/features-443100000-446800000.parquet

radar edge --features out/features-443100000-446800000.parquet \
  --rates /home/guardian/radar/docs/research/data/0024-base-rates.json
```

Each under `nice -n 19 ionice -c3 timeout`, one process at a time, on a box
also serving `radar-serve`, `radar-follow` and `radar-market-tape`.

## The window, and why this one

Slots 443,100,000 to 446,800,000: launches from about 2026-08-31 to
2026-09-13. Two bounds set it.

- **It starts after 2026-08-31** because `window_peak_price`, the field that
  says whether any fills happened near a price reading, only exists from then.
  Without it every exit price is stale by definition.
- **It ends before 2026-09-13** because the outcome recorder stopped measuring
  reliably from then (*Attempt two*, below). Launches after it mostly have no
  24-hour reading, and never will.

## The instrument, measured

| | `radar features` | `radar edge` |
|---|---|---|
| wall time | 401 s | 4.25 s |
| peak resident memory | 2,268,808 kB | 224,152 kB |

`radar features` sits near 2.2 GB on every window tried, from 2 days to 20: its
cost is building the creator-history index, not writing rows. On a 3.9 GB box
that is most of the free memory. A whole-store pass was started on
2026-09-27 and killed before it finished, with swap climbing. Runs on this box
stay windowed.

## What the sample is

| | |
|---|---|
| horizon | 24h (216,000 slots; about 19 hours of real time at the ~11,300 slots an hour this store's slot times show, not 24) |
| population (eligible launches in window) | 386,908 |
| labelled rows | 15,867 (4%) |
| round trip charged | 850 bps — the fresh-launch cohort's measured all-in cost (research 0019, snapshot 2026-09-03) |
| bar, for context | 456 bps — the same measurement read in the $20-$200 band (research 0022); not a second hurdle, per plan 0007 Q2 |

Why the other 96% carry no label:

| reason | rows | % of population |
|---|---|---|
| stale exit — no fills in the six hours before the 24h reading | 224,893 | 58% |
| no reading within 9,000 slots after the 24h horizon | 109,212 | 28% |
| stale entry — no fills near T = launch + 6,000 slots | 36,794 | 10% |
| no entry reading | 142 | 0% |

**This is the most important fact in the document.** A label exists only when
the coin was still trading on both sides of the hold. More than half the
population had stopped trading by the 24h reading. The labelled rows are
therefore the survivors.

*Inference, not measured:* a coin nobody trades for six hours has most likely
collapsed, so its real exit would be worse than a survivor's. If so, the
returns below flatter every stratum, and the negative verdict is the
conservative reading, not an artefact of the bias. What cannot be said from
this run is what a buyer of the non-survivors would have got — the table has
no price for them.

**The trades table covers none of the window**, so the twelve trade-derived
features of the twenty-four `radar features` defines are absent for every row
— reported as absent, not zero. This run measures only the creator-record,
launch-metadata, dev-buy and decision-lane features.

## What was found

Folds, after the purge and embargo (rows used, then labelled of population).
The first three are fitted as one; the last two are the tests.

| fold | slots | rows used | labelled / population |
|---|---|---|---|
| 0 | 443,100,030 – 443,693,061 | 1,861 | 1,861 / 77,381 |
| 1 | 443,693,064 – 444,336,737 | 2,548 | 2,548 / 77,381 |
| 2 | 444,336,739 – 445,210,890 | 2,888 | 4,153 / 77,381 |
| 3 | 445,210,899 – 445,937,233 | 2,798 | 3,813 / 77,381 |
| 4 | 445,937,275 – 446,799,976 | 2,565 | 3,492 / 77,384 |

A stratum clears only if, on **both** test folds, it has at least 100 rows,
its median net return is above its own standard error, and the Wilson lower
bound on the share of rows that made money after the 850 bps is above one
half.

**Fitted.** 44,042 strata tried, the whole grammar. The best on the fitting
folds was *creator has no prior launches, the symbol has been used 30 or more
times before, and the metadata host has been used fewer than 277,928 times*:

| fold | n | median gross | median net | paid | Wilson ≥ |
|---|---|---|---|---|---|
| fit | 382 | −575.6 bps | −1,425.6 bps | 102 | 0.225 |
| test 0 | 52 | −402.1 bps | −1,252.1 bps | 16 | 0.199 |
| test 1 | 43 | −193.2 bps | −1,043.2 bps | 8 | 0.097 |

The *best* of 44,042 candidates loses money before costs on the period it was
chosen from. It fails every condition on both test folds.

**Fixed, not fitted** — `creator_edge`'s thresholds and the refusal signals
already in production code:

| stratum | fold | n | median gross | median net | paid | Wilson ≥ |
|---|---|---|---|---|---|---|
| prior launches ≥ 5, prior organic ≥ 1, < 10 launches a day | test 0 | 432 | −693.5 | −1,543.5 | 85 | 0.162 |
| | test 1 | 334 | −1,499.0 | −2,349.0 | 45 | 0.102 |
| refused: a record and no organic graduation | test 0 | 507 | −319.5 | −1,169.5 | 119 | 0.200 |
| | test 1 | 495 | −485.6 | −1,335.6 | 77 | 0.126 |
| refused: launching too fast (≥ 10 a day) | test 0 | 889 | −489.3 | −1,339.3 | 208 | 0.207 |
| | test 1 | 838 | −741.5 | −1,591.5 | 127 | 0.129 |
| refused: launch block at ≥ 10 recipients | test 0 | 9 | −792.7 | −1,642.7 | 3 | 0.121 |
| | test 1 | 4 | +19.7 | −830.3 | 1 | 0.046 |

(bps throughout.) None cleared either test fold. The one positive gross
figure is four rows.

## The verdict

**Nothing found.** No stratum the table can name — the best of 44,042 fitted
ones, `creator_edge`'s own thresholds, or the refusal signals — makes money
out of sample at the 24-hour horizon, and every stratum with more than nine
rows has a negative median *before* the 850 bps round trip. That agrees in
direction with research 0017's 0 bps on the selection edge, and it is worse:
0017 measured no edge, this measures a loss among the coins that survived.

Notice that `creator_edge`'s "good creator" stratum did worse on test 1
(−1,499 bps gross) than the two groups Radar refuses. On this window the
creator record does not separate winners from losers among survivors.

**What this does and does not settle**, per LEARNINGS 35 and AGENTS.md §1's
zero rule. It settles that `Policy::CLOSED` stays closed: nothing here is a
reason to let the kernel authorise a buy. It does **not** settle that no edge
exists in this data. Twelve of twenty-four features are absent because the
trades table is empty; the labels cover 4% of launches and only the survivors
among them; the horizon is about 19 real hours, not 24; and the window is
thirteen days. It is a statement about what this instrument can see today.
The highest-value repair is unchanged from plan 0007: get the recorder to
write trades.

## The two attempts that did not count

**Attempt one, 2026-09-27, `main` at `4fdb9e3`, window 441,040,080 –
445,363,440.** It reported a 0 bps median gross in every stratum and zero
labelled rows in the fitting folds. Both were the label, not the market. The
exit was chosen as the last reading *at or before* the horizon, which found an
earlier checkpoint's reading, and the freshness check let an exit whose price
window overlapped the entry's own fill through. The entry's price was read
back twice. LEARNINGS 36; fixed by
[#312](https://github.com/1xmint/theradar/pull/312) (`ca2ef05`). Its output
is at `/home/guardian/radar/data/edge-runs/2026-09-27/` and must not be
quoted as a measurement.

**Attempt two, 2026-09-28, `ca2ef05`, window 446,580,000 – 450,900,000
(about 2026-09-13 to 09-27).** Only 1% of launches carried a label, the last
test fold none at all. The outcome recorder (the hourly
`radar-backfill --outcomes` cron) had been failing: of 864 logged runs, 285
failed — query timeouts from about 2026-09-13, nearly every run from about
09-19, and CryptoHouse's ten-billion-row limit on every run from about 09-22.
Its transfer query was bounded by the earliest launch in the whole store, so
it grew every day until it could not finish. `radar brief` kept reporting the
outcomes check as ok throughout. The fix is
[#314](https://github.com/1xmint/theradar/pull/314). A 24h label needs a
reading within 9,000 slots of the horizon, so most launches from those two
weeks have lost theirs for good. Its logs are kept alongside this run's, as
`run2-*.log`.

## Not checked here

- **The non-survivors' returns.** The 58% with a stale exit have no price to
  score; this run cannot say what they would have returned.
- **Launches after 2026-09-13**, until #314 is deployed and the recorder has
  caught up.
- **The 6-hour horizon.** Refused for every row by construction since #312: its
  exit window overlaps the entry's.
- **A lower cost charge.** 850 bps is the charge this repository holds right for
  this population (plan 0007 Q2). Every gross median above is negative, so no
  cost band would change the verdict.
