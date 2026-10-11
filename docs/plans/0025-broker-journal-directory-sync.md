<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0025 — Persist broker journal directory entries before receipts

**Status:** source implemented; GitHub verification pending.

Branch: feat/broker-journal-directory-sync, based on verified journal eadb500
(full CI 38182537437). Independent of combined-source CI 38183043973 at 61f82b2;
keep both parent branches fixed until relevant checks finish. No local jobs.

## Objective and checklist

The planned executor must not submit after a file-only sync on a newly created
journal. Linux fsync does not necessarily persist the parent directory entry.
Actual callers are BrokerLog reserve/submit, via Journal::record_broker.

- [x] File append/sync, then parent directory sync, then return the receipt.
- [x] Sync on every broker record, including replayed owners after a failed sync.
- [x] Propagate sync errors without a receipt; preserve existing owner poisoning.
- [x] Keep historical nonbroker storage and hashes unchanged.
- [x] Add two regressions for ordering and uncertain post-write sync failure.
  Independent test floor 2320 (2318 + 2).
- [ ] Read complete GitHub CI and exact mutation diagnostics; repair failures.
- [ ] Integrate with the verified combined Alpaca branch only after checks finish.

## Handback

No credentials, orders, deployment or activation. Source adds a synchronous
post-append seam to test failure after a real file write. Both reserved and
submission-unknown receipts wait for directory sync. A deliberately failed sync
returns an error while fresh replay still finds the durable claim; it is not
permission to retry. The existing broker owner poisons any failed record path.

The helper uses standard File open/sync APIs. Unsupported filesystems/platforms
refuse instead of bypassing sync. Protected, existing, durably provisioned parent
directories remain an operational requirement. Tests verify ordering, error
propagation and recovery on the runner filesystem, not actual power loss or
hardware write-cache honesty. Reference: the Linux man-pages fsync source cited
in design 0022. This closes the missing directory-sync operation; full autonomous
execution, risk checks, economic reconciliation, supervisor/site and evaluation
remain required by plan 0024.
