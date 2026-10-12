<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0024 — Integrate the paper foundation, then complete autonomous execution

**Status:** combined source and parent verified; autonomous execution remains pending.

Branch: feat/alpaca-paper-integration. Full CI 38183693452 passed at f9553a8.
PR 380 merged to integration parent 3a4ba61; both trees are
70aa0602901a56ea2862294d4866a11670105a8d. Parent CI 38184135535 passed fully.
The source base stayed f9553a8 while intent-binding PR 382 ran and passed.
All tests/builds/lint/mutations run on GitHub.
The preceding goal turns made progress through source fixes and verified CI.

## Evidence brought together

| Piece | Verified source | Full passing CI |
|---|---|---|
| Private setup guidance | 3b34115 | 38179947445 |
| Conservative exposure | 25f0b49 | 38180737966 |
| Client-ID recovery reads | 11a9107 | 38182273530 |
| Durable intent journal | eadb500 | 38182537437 |
| Equity asset observations | ba4e58a | 38182759739 |
| Broker directory sync | 055a773 | 38183410146 |

Asset observations passed full CI before incorporation. Preserve both shared
BrokerOrder lifecycle fields
and exposure field visibility. Keep every CLI dispatch, report and historical
journal hash behavior. The combined independent test floor is 2331 before the
four asset tests, then 2335. Directory sync adds two, bringing the floor to 2337.
Do not replace a sum with either child's floor.

## Checklist

- [x] Combine verified setup, exposure, recovery and journal source on a separate
  branch without moving the parent used by the remaining child CI.
- [x] Resolve overlapping order schema, command/help and documentation additions.
- [x] Incorporate asset observations only after their entire CI passes.
- [x] Initial combined 61f82b2 passed full CI 38183043973, including all mutation shards.
- [x] Incorporate directory sync only after full source CI 38183410146 passed.
- [x] Combined directory-sync f9553a8 passed all CI 38183693452 jobs.
- [x] Advance the parent via PR 380 only after full source CI; verified tree identity.
- [x] Read parent CI 38184135535: all jobs passed at 3a4ba61.
- [ ] Produce and deploy a verified artifact through the fixed deployment path.

## Remaining work toward the full goal

1. Connect the owner's paper account privately and verify actual account, asset,
   order and IEX data responses. No keys in chat or in the model/web environment.
2. Bind durable order payloads to broker observations; reconcile fills, fees,
   cash movements and local claims through restarts before releasing claims.
   Directory-sync source and its combined tree are verified.
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
