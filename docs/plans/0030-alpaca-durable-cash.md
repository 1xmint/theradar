<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0030 — Retain broker cash checkpoints beside order claims

**Status:** durable cash caller verified on GitHub; economic transitions/execution pending.

Branch: feat/alpaca-durable-cash, based on 2c62e4c. All 14 jobs in
`gh run view 38202895943 --json status,conclusion,jobs` passed on that base.
The previous turn verified cash accounting and the documentation follow-up;
this turn connects provider reads to durable state. Tests/jobs run on GitHub.

## Checklist

- [x] Actual alpaca-cash-checkpoint caller creates genesis from private pinned
  broker reads before any retained order. Subsequent calls revalidate the last
  checkpoint and use the verified account-wide order/activity cash caller.
- [x] Retain normalized selected observations in the same owned, verified chain
  as immutable order claims. File and directory sync precede baseline advancement.
  Poison on uncertain writes; replay restores the observed baseline and claims.
- [x] Refuse changed/disappeared history, unsupported new events, failed reads,
  cash mismatches and invalid checkpoint identity/time. Rejections retain the last
  baseline so late fees and valid new history can be reconciled later.
- [x] Verify ten new tests on GitHub: durable replay/idempotency, illegal history,
  hash coverage, domain separation, sync failure, poison, actual caller deposits/
  late fees/read failures, retained-order claims and command credential refusal.
  First CI 38204170897 at b972e76 found the new BrokerCash stage missing from
  the audit label match. Build/test/lint and shard 0's unmutated baseline refused
  compilation; add the explicit audit label and rerun the full GitHub checks.
  CI 38204357039 at 486b762 passed 2782 tests, build/MSRV/web/site/format/dependency/
  license checks and every mutation shard: 45 caught, 14 compiler-unviable,
  zero missed/timeouts. All four artifacts and compiler reasons inspected
  (absent Default, unused arguments, dead code). Lint found three missing test
  statement semicolons; correct those and raise the observed test floor to 2782.
  Full verification of that repair remains pending; no mutation exclusions.
  `gh run view 38204609543 --json status,conclusion,jobs` verified all 14 jobs
  passed at 2ab0628. 2782 tests passed; 45 mutations caught, 14 compiler-unviable,
  zero missed/timeouts. Inspected every shard and compiler reason; no exclusions.
- [ ] Add durable economic claim transitions and pure equity risk, then wire the
  separate executor and supervisor/site handover for autonomous paper operation.
  Forward paper evidence and eligible explicit live setup gate real money later.

## Handback

No actual broker account, credentials, order, deployment or activation. Code at
2ab0628 passed every job in CI 38204609543. This documentation follow-up records
that evidence; inspect its own CI before advancing the parent branch.
This is a durable observation baseline, not complete settlement, profit or
available capital. All claims remain held and all authority flags remain false.
The generic storage accepts caller-normalized facts; the private caller validates
their selected broker shape/economics before writes and on use after replay.
Protected stable paths, a durably provisioned directory and honest fsync storage
remain prerequisites. This does not detect whole-host rollback or valid-prefix
replacement without an independently trusted checkpoint. Legacy journals with
orders but no cash genesis refuse automatic bootstrap; do not overwrite them.
Next connect economic claim transitions to the retained observations and risk
state; avoid another disconnected observation command. Owner account creation
is needed for real-response validation, not for independent implementation.
