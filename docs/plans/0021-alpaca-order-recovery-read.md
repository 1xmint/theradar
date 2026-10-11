<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0021 — Paper order recovery observations

**Status:** source implemented; full GitHub verification pending.

Branch: feat/alpaca-order-recovery-read. Base: integration 6ae333d, identical tree
to 02a3d3d verified in complete CI 38179746811. Exposure PR 376 is independent;
its checks must finish before any parent movement. Complete exposure CI
38180737966 now passed at 25f0b49, including every mutation shard. Parent CI
38180442914 is still running; do not advance it. No workstation jobs.

## Objective

Recovery after an ambiguous submission needs a broker lookup using the retained
client order ID, including orders absent from the open-order list. Build that
actual CLI caller without treating missing or terminal observations as authority
to retry or release capital. Preserve the autonomous Alpaca-first objective.

## Checklist

- [x] Actual paper-only CLI order lookup between pinned account reads.
- [x] Retain selected fill and lifecycle fields without float conversion.
- [x] Bind returned client ID; refuse missing/wrong identity and malformed bodies.
- [x] Report HTTP 404 as not visible; refuse other HTTP errors and redirects.
- [x] Add five unit regressions and one process regression; floor 2315 on this
  independent base (add seven exposure tests on eventual integration).
- [ ] Read full GitHub CI, including exact mutation diagnostics, and fix failures.
- [ ] Integrate only after parent and independent-child checks finish.

## Handback

CI 38181116618 found one fixture assertion comparing normalized nested legs
against sparse raw JSON. Optional absent fields deliberately serialize as null;
assert the retained leg count, identity and status instead of raw shape equality.
This fixes the fixture expectation, preserving unknown-field semantics.

The order lookup is read-only and grants no execution authority. It neither
retries orders nor releases claims on HTTP 404, canceled/expired/filled status or
any other observation. Account snapshots remain sequential, not atomic; timestamp
strings are preserved but not validated as fresh. Query-safe IDs are Radar's
subset, not a broker-wide format claim. Credentials are the existing private
paper env variables; no live URL override or order-submission route exists.
No account/keys supplied, no deployment or trading activation performed.

Next: durable intent/client-ID journal, exclusive ownership and record-before-
submission ordering, then bound payload lookup and fill/cash reconciliation before
reservations can be released. Connect pure mandate/quote/asset checks and isolated
paper execution, supervisor and site handover. Preserve adaptive risk, per-option
Agent decides and optional daily loss controls, with no chat numeric mandate.
