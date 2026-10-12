<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0028 — Combine verified recovery inputs and reconcile executions

**Status:** execution consistency caller verified on GitHub; fee/cash reconciliation remains pending.

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
- [x] Verify the actual caller and arithmetic on GitHub, including mutation jobs.
  `gh run view 38200285274 --json status,conclusion,jobs` verified every job passed
  at 6e555de. All 2763 tests passed; 105 mutations caught, 20 compiler-unviable,
  zero missed/timeouts. The exact alpaca_fills.rs:36:9 OR-to-AND mutant was caught.
  All four diagnostics and each unviable compiler reason were read.
  Six unit tests and one actual-command test initially raised the floor from
  2347 to 2354. CI 38199868775 measured 2763 passing tests; use that actual floor.
  First run 38199605432 at 73f450a exposed a missing required account status in
  the fixture and unnecessary Option wrapping in its response helper. The
  mutation baselines failed the same fixture; no mutants ran. Repair supplies
  status, simplifies the helper and adds a filled-status/partial-history case.
  Repair 0738e5b passed all nonmutation jobs in CI 38199868775. Mutation results:
  104 caught, 20 compiler-unviable, one survivor at alpaca_fills.rs:36:9, changing
  the first OR to AND. Its unknown-status fixture also had mismatched quantity,
  hiding the weakened status gate. The repaired case has consistent full/empty
  histories and the buy side, so only the unknown status causes refusal.
  Downloaded all four diagnostics; inspected the exact survivor diff and every
  unviable compiler reason. No exclusions or cancellations. Unviable cases are
  unused/dead code after no-op bodies, missing Default implementations, invalid
  let-chain OR syntax and useless unsigned comparisons. Reverification passed
  in 38200285274 at 6e555de, as recorded above.
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
private configuration refusals. Initial GitHub verification found the fixture,
lint and status-test gaps above; repaired code passed the entire CI at 6e555de.
This follow-up records that evidence without changing code. Read its own CI
before advancing the integration parent. The next implementation is fee/cash/
baseline economic accounting and durable claim transitions. Gross consistency and exhausted pages
do not prove final history or fees. No release API exists. The owner still needs
to create the paper account for actual response verification; independent code
work remains available. Preserve the complete autonomous goal from plan 0024.
