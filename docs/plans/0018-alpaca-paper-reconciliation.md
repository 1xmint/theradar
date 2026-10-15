<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0018 — Paper account and pending-order observations

**Status:** implementation in progress; GitHub verification required.

Branch: feat/alpaca-paper-reconciliation.
Base: integration 9fa709d, identical tree to source 5c51176 verified by complete
GitHub CI 38179026608. Parent CI may run, but do not move the parent while this
child's checks are running. Tests, builds, lint and mutations run on GitHub only.

## Objective

Feed eventual autonomous paper execution with pinned account identity and visible
pending orders. A balance read without pending orders must not imply free capital.
Preserve the accepted Alpaca-first experiment and the site's Agent decides UX.
Owner still needs to create the paper account; no secrets requested in chat.

## This increment

- [x] Add actual CLI caller alpaca-reconcile with an explicit expected account ID.
- [x] Read account/orders/positions/clock/orders/account from the fixed paper host.
- [x] Retain partial fills, pending cancellation/replacement and nested legs.
- [x] Compare selected account/order fields, canonicalise order identity order,
  reject duplicate/blank IDs and full 500-order pages, and refuse partial reports.
- [x] Add four unit and one process regression; floor 2309.
- [ ] Read complete GitHub build/test/lint/MSRV/mutation verification and fix failures.
- [ ] Integrate only after complete verification; read the parent's existing CI
  before merging and require fresh integrated verification afterward.

## Remaining autonomous work

Derive complete, conservative pending exposure and reconcile durable local claims,
broker positions/orders/fills and account cash. Handle restarts and ambiguous
submission before releasing reservations. Bind fresh quotes, asset eligibility,
market hours, costs and the owner's site mandate to a pure broker risk evaluator.
Then isolate paper execution credentials from model reasoning, add idempotent
orders, cancellation/fill recovery and supervisor/site handover. Validate forward
performance net costs before separate eligible live setup. No universal daily
loss cap or numeric chat mandate replaces the agreed adaptive website controls.

## Handback

The reader returns observations and change flags, not a reconciled economic risk
state. Even unchanged selected fields and below-limit responses are not atomic
or complete. Unknown order types/statuses/legs retain their fields; this does not
make them supported execution inputs. Account read values stay decimal strings.
No live keys, deposits, trades, policy activation, production deployment or paid
data. GitHub verification pending; do not claim this increment verified yet.
