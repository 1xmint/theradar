<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0028 — Combine verified recovery inputs and reconcile executions

**Status:** combined inputs verified; execution consistency caller implemented, GitHub verification pending.

Branch: feat/alpaca-fill-reconciliation. Sources d3f7975 (full CI 38184222577)
and eb70903 (full CI 38184575296) both passed before combination. Parent 3a4ba61
CI 38184135535 and combined-tree CI 38184924414 at 5effe91 both passed fully.
Previous goal turn made progress by implementing account activity pagination;
this turn verified that source and combined it with verified payload binding.
All verification runs on GitHub; no local jobs.

## Checklist

- [x] Combine verified intent binding and account activity observations, preserving
  optional journal lookup, both CLI dispatches/help and selected broker schemas.
- [x] Sum independent tests: 2337 foundation + 5 binding + 5 activities = 2347.
- [x] Read combined GitHub CI 38184924414: every job passed.
- [x] Add the actual order reconciliation caller under journal ownership: bracket
  activity collection with bound order observations and pinned account reads.
- [x] Match execution identity, side, ticker, cumulative quantity and remaining
  quantity. Reject duplicate IDs, gaps, overlaps, overfills and unsupported shapes.
  Use cumulative quantities to establish execution intervals, independent of
  response order, instead of assuming timestamps uniquely order executions.
- [x] Derive gross fill amounts with exact arithmetic and conservative USD bounds.
- [ ] Verify the actual caller and arithmetic on GitHub, including mutation jobs.
  Six unit tests and one actual-command test raise the floor from 2347 to 2354.
- [ ] Reconcile fees/nontrade cash and baseline account cash without attributing
  deposits/withdrawals to strategy returns. Late fees/corrections remain possible.
- [ ] Persist verified economic transitions before claim release, with replay and
  failure recovery; ambiguous submission never becomes permission to retry.
- [ ] Complete pure equity risk, isolated paper executor, supervisor/site controls
  and measured evaluation from plan 0024. Preserve the full autonomous goal.

## Handback

No orders, credentials, deployment or activation. The actual command now connects
the inputs and preserves full claims. Tests cover both sides, partial/full/empty
executions, interval order, rounding, gross range, malformed/missing fields,
unsupported status, all nine read stages, visible account/order changes and
private configuration refusals. These tests are written, not yet verified.
The next step is GitHub verification and repair, then fee/cash/baseline economic
accounting and durable claim transitions. Gross consistency and exhausted pages
do not prove final history or fees. No release API exists. The owner still needs
to create the paper account for actual response verification; independent code
work remains available. Preserve the complete autonomous goal from plan 0024.
