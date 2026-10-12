<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0039 — IEX quote provenance in actual preflight

**Status:** Implemented; GitHub verification pending.

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
- [ ] Inspect all GitHub jobs and exact mutation results.

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
parser callers. These are compiler refusals, not test kills. Remaining checks
must finish before the correction push; final all-job verification is pending.
