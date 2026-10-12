<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0031 — Reconcile residual broker claims with durable cash observations

**Status:** residual economic claims verified on GitHub; risk/execution pending.

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
- [x] Verify four new tests and existing regressions on GitHub: exact fractional
  residual math, monotonic complete progress, atomic replay/refusal, and actual
  cash caller for both sides across ten lifecycle states with restart checks.
  CI 38205493827 at 408a4ae passed 2786 tests and every nonlint/nonmutation job.
  Lint named similar fixture bindings full/fill; rename the former completed.
  Mutations: 16 caught, 13 compiler-unviable, one missed, zero timeouts. Read all
  four artifacts and every compiler reason (unused/dead/absent Default and an
  unsupported OR let-chain). Exact survivor broker_claims.rs:53:40 changed
  filled > original to >=. The storage-only package tests lacked exact-full
  acceptance, although the CLI exercised it. Add that precise boundary to the
  validator test, retain the actual caller test, raise the observed floor to
  2786 and rerun full GitHub verification; no exclusions.
  `gh run view 38205765530 --json status,conclusion,jobs` verified all 14 jobs
  passed at 2bccaca. 2786 tests passed; 17 mutations caught, 13 compiler-unviable,
  zero missed/timeouts. Every artifact/compiler reason inspected; the exact
  :53:40 >= survivor was caught. No exclusions or cancellations.
- [ ] Connect pure equity risk to complete fresh cash/positions/open-order/asset/
  session/quote observations, then the separate executor and supervisor/site.
  Measure learning through forward paper operation; eligible explicit live setup
  and performance evaluation remain separate requirements for real-money trading.

## Handback

No account credentials, actual orders, deposits, deployment or activation.
Code at 2bccaca passed every job in CI 38205765530. This follow-up only records
evidence and clarifies reserve wording; inspect its CI before advancing the base.
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
