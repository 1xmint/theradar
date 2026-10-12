<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0031 — Reconcile residual broker claims with durable cash observations

**Status:** implemented; GitHub verification pending.

Branch: feat/alpaca-economic-claims, based on 6801ee8. All 14 jobs in
`gh run view 38204871140 --json status,conclusion,jobs` passed before this change.
The preceding turn made verified progress on durable cash checkpoints. All
test/build/lint/mutation jobs run on GitHub.

## Checklist

- [x] Actual cash checkpoint caller retains complete bound broker order progress
  after matching immutable payloads, fill intervals and account-wide cash deltas.
- [x] Store progress atomically beside cash in the owned journal. Replay and writes
  reject missing/extra identities, unsubmitted orders, excessive/decreasing fills,
  changed broker bindings, terminal changes and forgotten established progress.
- [x] Derive residual buy limit cash with upward rounding, residual sell shares,
  and full fee reserves. Original intents/submission states stay immutable.
  Missing historical progress means full original claims; no retry permission.
- [x] Expose original and observed remaining claims to existing actual journal/
  cash callers. New reservations retain full claims until verified observation.
- [ ] Verify four new tests and existing regressions on GitHub: exact fractional
  residual math, monotonic complete progress, atomic replay/refusal, and actual
  cash caller for both sides across ten lifecycle states with restart checks.
- [ ] Connect pure equity risk to complete fresh cash/positions/open-order/asset/
  session/quote observations, then the separate executor and supervisor/site.
  Measure learning through forward paper operation; eligible explicit live setup
  and performance evaluation remain separate requirements for real-money trading.

## Handback

No account credentials, actual orders, deposits, deployment or activation.
GitHub verification is pending; inspect all jobs and exact mutation diagnostics.
This advances economic state rather than adding another standalone read tool.
The protected provider caller establishes facts; generic storage checks their
association, monotonicity and durability. Remaining liabilities are relative to
the last selected cash observation, not current spendable capital. Fee reserves
stay fully held; adequacy/finality, position accounting and complete risk state
remain to verify. A terminal correction refuses and preserves the prior baseline
for explicit reconciliation rather than reopening claims silently. Historical
cash events omit the new optional field and retain their old hashes. Next work
is the pure equity risk/caller boundary and isolated paper execution, not another
cash-observation command. The owner still needs the Alpaca paper account for
real-response verification; independent implementation remains available.
