<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0032 — Bind durable paper cash checkpoints to share quantities

**Status:** implementation ready for GitHub verification; execution pending.

Branch: feat/alpaca-position-accounting, based on ceae1c2. All 14 jobs in
`gh run view 38206083004 --json status,conclusion,jobs` passed at that base.
The preceding goal turn verified the pending run and provided account setup
instructions; independent implementation is available while signup is pending.

## Checklist

- [x] Extend the existing cash checkpoint caller, rather than another operator
  command, to bracket account-wide cash/fill collection with pinned accounts
  and two normalized position-quantity reads.
- [x] Persist selected long US equity asset identities and exact nanoshares
  atomically with cash and order progress. Retained host times cover all reads.
- [x] Compare prior quantities plus new bound fill deltas with current holdings.
  Reject unexplained additions/disappearances, identity changes, unsupported or
  duplicate positions, visible races and invalid/missing historical baselines.
- [x] Add exact fractional/open/close/order-invariance and invalid-shape tests;
  exercise actual checkpoint rejection/restart/recovery and buy/sell lifecycles.
- [ ] Inspect all GitHub jobs and mutation artifacts, repair actual findings.
  `gh run view 38206870134 --json status,conclusion,jobs` at af1c74b:
  2789 tests passed; all jobs passed except lint. The lint failure requires a
  nested/wildcard pattern in one test fixture; simplify it to (_, true).
  All four mutation artifacts inspected: 22 caught, 5 compiler-unviable,
  zero missed/timeouts. Each unviable function-body substitution produced unused
  argument errors (checkpoint, validate, read, retained, reconcile); all reasons
  and mutation diffs inspected. No exclusions. Raise the observed test floor to
  2789 and rerun full GitHub verification after the pattern/EOF cleanup.
- [ ] Connect fresh complete state to pure equity risk, isolated paper executor,
  supervisor, owner handover controls and forward learning. Account credentials
  and actual-response verification remain required before paper activation.

## Handback

No orders, credentials, deposits, deployment or activation. Tests and mutations
passed at af1c74b; lint repair and full re-verification pending. Position
quantities are selected observations, not valuations or proof
of atomicity. Genesis records the owner's opening holdings; it does not infer
their purchase cost or profitability. Later fills are already bound and checked
by the existing cash/execution caller before their share deltas are applied.
Unknown activity types still refuse in cash reconciliation. Corporate actions,
transfers and historical corrections need explicit supported reconciliation;
they never become unexplained free shares. Old journals remain readable, but a
cash checkpoint without share quantities cannot be silently upgraded: preserve
it for explicit migration/reconciliation. This work closes a necessary account
state gap for risk/execution; it does not itself grant trading permission.
