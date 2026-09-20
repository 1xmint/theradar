<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0036 — The hourly `consider` run eats the whole CryptoHouse allowance

**Date:** 2026-09-20
**Status:** measured. The shared CryptoHouse allowance of **120 queries per hour
per IP** is exhausted every hour by the `radar consider` cron at minute 37. The
launch recorder `radar-follow` was refused **91** times in 24 hours and the
market tape **21** times — and **every single one of those refusals fell between
minute 40 and minute 59**, never earlier. The starved process is the one whose
data cannot be recovered.
**Source:** `journalctl` on the production box, read over Tailscale on
2026-09-20, covering the preceding 24 hours. Counts are of `QUOTA_EXCEEDED`
lines per unit. No paid source, no synthetic load.
**Bears on:** [plan 0013](../plans/0013-the-terminal-find-look-track-trade.md),
whose Phase B item 3 asks for more than ten coins — this says there is no
allowance left to widen into.
**Answered in part:** point 1 of *What follows* is built and deployed —
`consider` declares a budget and stops when it is spent. See *The half of
this that is fixed*, below. Point 2 is still open and still the owner'''s.

## The question this closes

Plan 0013 listed an unchecked question: whether the CryptoHouse refusals stopped
after #258 raised the `radar-follow` idle interval to 90 seconds. **They did
not.** Slowing the recorder down did not help, because the recorder was never
what spent the allowance.

## What the allowance is

CryptoHouse's free tier permits 120 queries per hour, counted **per IP address**,
not per process or per credential. Every Radar unit on the box draws from one
bucket. The refusal is explicit about the shared count:

```
Quota for user 'crypto' for 3600s has been exceeded: queries = 133/120
```

That line is the whole finding in miniature: the box asked for 133 where 120 were
allowed, and CryptoHouse refused the thirteen that arrived last.

## Who was refused, over 24 hours

| Unit | `QUOTA_EXCEEDED` in 24h | Has a declared ceiling |
|---|---|---|
| `radar-follow` (launch recorder) | 91 | no |
| `radar consider` (hourly cron) | 82 | **no** |
| `radar-market-tape` | 21 | yes — 72/hour |
| `radar-backfill --outcomes` | 4 | no |

## The measurement that names the culprit

Counts alone would not settle blame; a busy hour could starve anything. The
timing does.

**Every follow and tape refusal in the 24 hours sampled occurred between minute
40 and minute 59 of its hour. Not one occurred before minute 40.**

The box's crontab runs `radar consider --store ... --cap 40 --record` at **minute
37**. So the allowance survives the first thirty-seven minutes of every hour
intact, and is gone within about three minutes of `consider` starting. Nothing
else on the box changes at that moment.

## Why `consider` spends so much

`consider` is the only CryptoHouse caller on the box with **no ceiling of any
kind**. It asks for up to 40 candidates a run (`--cap 40`), and each candidate
costs more than one query: `coordination_of` reads both a shape and a bundle
window, and `paid_tier` reads authorities as well when a prevalence table is
loaded. On that reading a single run can ask for roughly 76 queries — more than
half the hour's entire allowance, spent in one burst, by one process, for one
feature.

By contrast `radar-market-tape` already budgets itself, in
`crates/radar-backfill/src/market_tape.rs`:

```rust
pub const PER_PASS: u32 = (80 * PASS_INTERVAL) / 3_600;
```

Eighty queries an hour, divided across passes, enforced at the single point every
query passes through rather than by counting the queries the code appears to
write. That distinction matters here because CryptoHouse's thousand-row cap turns
one written query into several real ones — the module's own doc comment records
getting this arithmetic wrong once already.

## A correction

Earlier in the day I recommended batching the market tape's per-coin requests, on
the assumption that asking about each coin separately was the expensive thing.
**That was wrong.** The tape already batches, and already holds itself to 72
queries an hour — it is the best-behaved caller on the box. The recommendation
was made before the log was read, and the log reversed it.

## What follows

**The fix is to give `consider` the same declared ceiling the tape already has**,
not to cut the cron. `consider`'s decision records only accumulate forward and a
gap in them cannot be backfilled later; neither can a missed launch. A ceiling
preserves both and is reversible by changing one constant.

**But an honest ceiling is small enough to be a product decision, not a tuning
detail.** Of 120 queries an hour, the tape holds 80. That leaves about 40 for the
recorder, the outcomes job and `consider` together. If the recorder keeps half —
and it should, being the only one whose loss is permanent — `consider` is left
with something near ten queries a run, which at three queries a candidate serves
three of the forty candidates it is asked for. Budgeting it does not make it
work; it makes it **fail honestly and stop damaging its neighbours**.

So two things are true at once, and the second is the owner's call, not an
implementation detail:

1. `consider` must not be allowed to spend an allowance it does not own. That is
   a defect and it should be fixed regardless.
2. At 120 queries an hour the box cannot run all four units at the sizes
   currently asked for. Either `consider` runs less often or over fewer
   candidates, or the allowance has to grow.

**Phase B item 3 of plan 0013 — showing more than ten coins — is blocked by the
second of those.** The `SHORTLIST` of ten is not an arbitrary cap; widening it
raises the tape's query cost, and there is no headroom to raise it into. That
item cannot proceed on measurement, not on effort.

## The half of this that is fixed

**Deployed 2026-09-20.** Merged as `2d7f91d` and installed to
`/home/guardian/bin/radar` on `clawguard`, which is the path the `37 * * * *`
cron entry names, so the ceiling took effect on the next hourly run with no
restart. `consider`'''s CryptoHouse reads now go through a
declared budget of ten queries a run, enforced at the single point every query
passes through, and a run that hits the ceiling says so rather than reporting a
short pass as a complete one — in `radar session` and as a `query budget` line
in `radar brief`. Rule 9: *considered three candidates* and *considered three
and stopped because the allowance was spent* are now different sentences.

This fixes point 1 above and **does not touch point 2**. Ten queries still buys
about three of the forty candidates the cron asks for. The budget stops
`consider` starving `radar-follow`; it does not make `consider` able to do its
job at its current size. Choosing between a smaller run and a larger allowance
is still the owner'''s decision, and the recommendation recorded with the change
is to watch one day of recorder data before making it — the refusal count
going to roughly zero is free evidence that changing two things at once
would destroy. That day starts 2026-09-20; `radar/outcomes.log` and
`radar/decisions.log` on the box are where it lands.

## What was not checked

The per-candidate query cost of three is read from the call sites, not counted
from a log — the units do not log their own query counts, which is itself worth
fixing. The 24-hour window is one day, on one box, and was not repeated. Whether
CryptoHouse counts a refused query against the allowance is unknown, and it
changes how quickly a starved hour recovers.
