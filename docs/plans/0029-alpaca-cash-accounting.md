<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0029 — Connect account-wide cash deltas to retained execution history

**Status:** implemented; GitHub verification pending.

Branch: feat/alpaca-cash-accounting, based on 79a9207. Parent PR 384 code 6e555de
passed full CI 38200285274; its documentation follow-up 79a9207 passed every job
in CI 38200674505 before this change. Keep that base fixed while child CI runs.
The previous goal turn made concrete progress through execution reconciliation,
regression repair and verified tests/mutations. All jobs run on GitHub.

## Checklist

- [x] Actual alpaca-cash-reconcile caller holds the verified journal owner while
  binding every retained order around activity reads; reuse payload/interval checks.
- [x] Revalidate a prior private activity report, account pin, USD cash, selected
  bracket equality, pagination, identities and monotonic host times. Bound input
  to 16 MiB; reject future/overlapping baselines before evaluating cash.
- [x] Refuse missing/changed prior records, unbound fills, duplicate broker order
  IDs, unknown new cash activity types, invalid amounts and inconsistent cash.
- [x] Separate trade cash flow, CSD/CSW funding and observed USD FEE cash effects.
  Sum exact signed products first; use outward nano-USD bounds without a tolerance.
- [x] Keep profit, final fees, complete economic reconciliation, claim release and
  execution authority unproven. Preserve all original claims and journal contents.
- [ ] Verify on GitHub: eight unit tests plus one actual-command test raise the
  observed floor from 2763 to 2772. Cover both sides, signed fractional bounds,
  changed/missing/unknown history, baseline shape/size/time, multiorder ownership,
  duplicate broker IDs, every read failure, exact arithmetic and private setup.
  First CI 38201628804 at e79f641 passed 2772 tests and build/web/site/MSRV/
  dependency/license/format jobs. Lint found a 107-line process test; share its
  repeated refusal assertions rather than bypass the lint. Mutations: 80 caught,
  24 compiler-unviable, three survivors. Inspected every unviable reason and the
  exact diffs at alpaca_cash.rs:214:14 (floor < 0 to == / <=) and :311:42
  (future-baseline > to >=). Add valid zero cash and negative fractional cash
  cases. Move the same future predicate into the loader with an explicit host
  cutoff so equal/past/future boundaries are deterministic tests. Reverify on
  GitHub; no exclusions or cancellations. Unviable cases were unused/dead code,
  absent Default and ambiguous numeric types after deleted match arms.
- [ ] Connect protected durable baseline/checkpoints and economic claim transitions
  with restart/error replay; late events/corrections must remain recoverable.
- [ ] Complete pure equity risk, isolated executor, supervisor/site handover and
  held-out/forward evaluation from plan 0024 before autonomous activation.

## Handback

No keys, actual orders, deposit, deployment or activation. The new caller connects
existing verified observation tools to account-wide cash arithmetic; this is not
an available-capital verdict or a claim-release path. A baseline is a prior private
alpaca-activities-read JSON report in protected operator storage, not authenticated
provenance or a durable checkpoint. Cash-flow consistency is not measured P&L.
Initial new nontrade support deliberately covers CSD, CSW and FEE; other new types
refuse visibly. Actual broker precision/booking and late fees remain to verify.
The first GitHub run found the lint/test gaps recorded above; repairs are in
source and await full reverification. The next action is verification, then
durable economic state and the executor. The owner still needs to create the
paper account for actual responses; independent implementation remains available.
