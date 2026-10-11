<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0024 — Integrate the paper foundation, then complete autonomous execution

**Status:** combined source in preparation; GitHub verification pending.

Branch: feat/alpaca-paper-integration. Parent remains integration 6ae333d until
all relevant child checks finish. All tests/builds/lint/mutations run on GitHub.
The preceding goal turns made progress through source fixes and verified CI.

## Evidence brought together

| Piece | Verified source | Full passing CI |
|---|---|---|
| Private setup guidance | 3b34115 | 38179947445 |
| Conservative exposure | 25f0b49 | 38180737966 |
| Client-ID recovery reads | 11a9107 | 38182273530 |
| Durable intent journal | eadb500 | 38182537437 |

Asset observations at ba4e58a are under CI 38182759739; require its final full
result before incorporating. Preserve both shared BrokerOrder lifecycle fields
and exposure field visibility. Keep every CLI dispatch, report and historical
journal hash behavior. The combined independent test floor is 2331 before the
four asset tests, then 2335. Do not replace a sum with either child's floor.

## Checklist

- [x] Combine verified setup, exposure, recovery and journal source on a separate
  branch without moving the parent used by the remaining child CI.
- [x] Resolve overlapping order schema, command/help and documentation additions.
- [ ] Incorporate asset observations only after their entire CI passes.
- [ ] Read combined GitHub CI, including all mutation shards, and repair failures.
- [ ] Advance the shared parent only from the verified combined tree.
- [ ] Produce and deploy a verified artifact through the fixed deployment path.

## Remaining work toward the full goal

1. Connect the owner's paper account privately and verify actual account, asset,
   order and IEX data responses. No keys in chat or in the model/web environment.
2. Bind durable order payloads to broker observations; reconcile fills, fees,
   cash movements and local claims through restarts before releasing claims.
   Verify directory-entry durability before the executor relies on new logs.
3. Complete the pure equity risk verdict and separate paper executor: owner
   mandate, fresh asset/session/quote checks, exact long-only cash/share sizing,
   cost/edge bounds, idempotent order submission, cancellation and recovery.
4. Connect the unattended supervisor and website: Alpaca balance, manual options
   and per-option Agent decides checkboxes, adaptive risk, optional daily stop,
   explicit handover, pause/revoke, research chat and visible decision/outcome log.
   Model output remains an inert proposal and external posts remain untrusted data.
5. Run held-out and forward evaluation with realistic capital, costs and tradable
   baselines. Learning must be measured. Separately establish live eligibility,
   revocable authority and actual live behavior before claiming full completion.

## Handback

This integration is not an executor and does not activate trading. Individual
green runs do not prove their merged tree; full combined verification is required.
No paper credentials, actual orders, deposits, paid data, deployment or delegation
changed. The owner's account creation is still outstanding. The goal remains
fully autonomous trading, including the site and learning loop, not read-only
tools or an isolated paper order. Preserve the accepted Alpaca-first direction.
