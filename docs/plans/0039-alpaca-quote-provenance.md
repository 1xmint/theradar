<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0039 — IEX quote provenance in actual preflight

**Status:** GitHub verified; no VPS release or autonomous activation.

Branch: feat/alpaca-quote-provenance, based on 6fda92e. Previous turn made
verified progress: bounded recorder passed all 14 GitHub jobs at c99bb62 and
was uploaded with matching hashes. Administrator installation remains pending.
Parent documentation run 38269361587 subsequently passed all 14 jobs, inspected
with gh run view --json jobs before this branch's checks.

## Checklist

- [x] Reject absent or unsupported exchange, conditions and tape in the shared
  quote validator consumed by actual preflight and the offline CLI.
- [x] Retain checked metadata in quote output; keep size units and execution
  authority separate from the supported metadata pattern.
- [x] Exercise independent invalid fields, missing fields and supported tapes,
  plus actual preflight refusal without reserving an intent.
- [x] Inspect all GitHub jobs and exact mutation results.

## Handback

No workstation jobs. The adapter supports both exchange codes V, precisely
condition array [R], and tape A/B/C. The owner reported V/R/B for SPY and V/R/C
for QQQ in the retained actual capture. This deliberately narrow pattern is an
adapter boundary, not a universal definition of valid exchange quotes or proof
of a regular trading session. Official Alpaca schema supplies bx/ax/c/z fields;
the [metadata endpoint](https://docs.alpaca.markets/us/reference/stockmetaconditions-1)
documents tape-specific mappings, but unauthenticated direct requests returned
401. No mapping was invented from a failed request.

The shared validator's callers already gate capture availability, quote age and
spread. They now reject unsupported metadata too. No capacity or fee defaults,
new permission or order submission. Actual preflight continues to refuse unknown
capacity/costs. The protected recorder is unchanged and keeps all raw metadata;
this Rust change needs a new verified CLI release before any VPS use.

Alpaca's [paper rules](https://docs.alpaca.markets/us/docs/paper-trading) say paper
fills use NBBO, ignore available quantities and omit regulatory fees, latency
slippage and queue effects. Consequently a successful simulated fill cannot
validate displayed size or routed capacity, and IEX-only research cannot be
represented as the simulator's complete execution feed. Forward evaluation
needs explicitly labelled cost/capacity assumptions, not zero costs inferred
from simulator fills. Live eligibility and meaningful results remain separate.

Next: resolve size normalization and paper cost model, protected execution
supervisor/cancellation, forward evaluation, and website handover controls.

Initial run 38269906722 at 58d7409 found one fmt-only fixture line-wrap issue;
applied the exact GitHub formatter diff, without running a workstation job.
All four mutation shards completed: nine mutants, seven caught, two unviable,
no misses/timeouts. Inspected shard 0 artifact outcomes and both compiler logs:
collect_state's default return fails because State has no Default implementation;
validate's default return fails denied dead-code warnings after removing its
parser callers. These are compiler refusals, not test kills. The run finished
with 13 jobs passed and fmt failed; tests reported 2,806 Rust and 27 Python tests.
Correction f5a0d38 was pushed only after terminal status. Its GitHub run
38270384888 passed all 14 jobs. Inspected every job via gh run view --json jobs,
tests job 114882896648, and all three mutation-diagnostics artifacts: 2,806 Rust
tests, 27 Python tests, all 32 existing targeted Python mutations caught, and
systemd unit verification passed. Rust mutations again totaled nine, seven
caught and two unviable for the same inspected compiler reasons, zero misses or
timeouts; shard 3 had zero mutants. All seven changed metadata guard mutations
were test kills (three != inversions, three || to &&, and tape-negation removal).

Code proof is f5a0d3856d380d06ef899207c5f17860322d4405. This final handback
update is documentation only; its run is separate from code verification.
Draft PR 400 is attached to the chat and stacked on PR 399. No new release build,
VPS binary replacement, order or execution authority. Recorder installation
still needs the owner's already supplied sudo command; ordinary SSH inspected
timer inactive and worker LoadState=not-found during this turn. The sudo password
is never requested in chat. The objective remains fully autonomous trading;
validated quote metadata alone does not complete it.
