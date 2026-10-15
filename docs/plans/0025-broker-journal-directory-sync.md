<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0025 — Persist broker journal directory entries before receipts

**Status:** source and combined integration verified on GitHub; merged to parent.

Branch: feat/broker-journal-directory-sync, based on verified journal eadb500
(full CI 38182537437). Independent of combined-source CI 38183043973 at 61f82b2.
Both runs passed fully before incorporation. Source 055a773 passed CI
38183410146. No local jobs.

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
- [x] Read complete GitHub CI 38183410146 at 055a773 and exact mutation diagnostics.
  All jobs passed. One viable mutant was caught; three were unviable: the two
  receipt replacements require nonexistent Recorded::Default, and the no-op sync
  replacement leaves an unused parameter under CI's deny-warnings build.
  These compile refusals are not behavioral mutation kills. No exclusions added.
- [x] Incorporate into verified combined Alpaca 61f82b2 (CI 38183043973).
  Both source runs completed before the merge. Combined test floor is 2337.
- [x] Combined f9553a8 passed full CI 38183693452 before PR 380 advanced the
  parent to 3a4ba61. Source and parent tree identity verified.

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
