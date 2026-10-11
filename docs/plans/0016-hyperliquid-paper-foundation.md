<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0016 — Hyperliquid paper foundation

**Status:** in progress; source requires GitHub verification.
**Branch:** feat/hyperliquid-paper-foundation.
**Base:** 64230845eb5c9452f7ff16c55c40138dd3f1bbbe.
**Date:** 2026-10-11.
**Decision:** [design 0020](../design/0020-hyperliquid-first-research-and-eligible-live-execution.md).

## Objective

Start Hyperliquid public-data research and paper evaluation, preserving Solana as
secondary. The owner requested the pivot and confirmed U.S. residence/location;
live venue eligibility remains unresolved. Preserve adaptive risk, optional daily
caps and research chat from [plan 0011](0011-private-autonomous-trader.md).

## Items

- [ ] Verify `radar perps-read` on GitHub: default testnet, explicit mainnet public
  observations, BTC/ETH/SOL metadata-index/context binding and bounded-depth books.
  Two regressions cover shuffled universe, absent/duplicate markets, mismatched
  context lengths, malformed books, transport refusal and endpoint selection.
  No exchange requests. Test floor 2290 on this base.
- [ ] Record public responses and compare schemas before decimal normalization.
  Keep tests/build/lint/mutation jobs on GitHub.
- [ ] Build WebSocket book/trade/context recording, reconnect/resync, duplicates,
  gaps, bounded retention and source/receipt clocks for point-in-time replay.
- [ ] Build realistic paper fills, spreads/queue uncertainty/latency/partial fills,
  fees, hourly funding, position/margin/liquidation accounting and frozen controls.
  Evaluate forward/out of sample before ML/RL or online strategy promotion.
- [ ] Resolve a live venue eligible for the owner's U.S. account before funding
  or delegation. Public API access is not eligibility evidence.
- [ ] Add venue-specific proposals, deterministic perps bounds, isolated signing,
  durable client IDs/nonces, order/cancel/reduce-only actions, reconciliation and
  unknown-outcome/restart/partial-fill/cancel-race recovery.
- [ ] Connect effective adaptive preferences and research chat to the controller;
  show verified equity/free collateral/exposure/net costs/reasons with direct
  pause/revoke. Solana wallet authentication is not perps-account authority.
- [ ] Verify eligible testnet and reconciled canaries before autonomous real money;
  require measured net expectancy. Optional Solana expansion follows live results.

## Handback

Solana implementation stopped for the direction discussion. Native-claims combined
integration CI 38163263101 passed every job. Separate PR 371 first failed CI
38163883799 on two reversed-argument ATA fixture calls. Correction dce1d03 was
committed/pushed after that run finished. CI 38176516659 at dce1d03 now passes
every job. PR 371 remains separate and unmerged; do not move this branch's base
while its own checks run.

This branch starts independently from the verified integration base. The actual
public-data CLI caller is implemented but not yet GitHub verified. No account
credential, funding, delegation, production service or live risk policy changed.
This is REST observation, not streaming, paper fills or autonomous execution.
The full goal stays active. Next inspect GitHub results, repair real failures and
finish the recorder. Do not reuse Solana bounds for perps, claim an unmeasured
profitability ranking or make universal venue support an activation prerequisite.

### Profit-first direction discussion

After publication of source e9f8253 / PR 372, the owner supplied an external
U.S.-venue ranking and clarified that the primary goal is the best chance of
profit after learned intelligence is applied. Pause further venue implementation.
[Design 0021](../design/0021-profit-first-venue-selection-for-a-us-account.md)
records the recommendation and its uncertainty; Alpaca-first is not yet a settled
owner decision. CI 38176947898 is running; lint has failed and must be inspected.
Keep the remaining jobs running, do not cancel or push over them. No production
or wallet action changed. Next finish verification of this existing source and
resolve the direction before adding a streaming or execution connector.

GitHub lint identified only the unreadable timestamp literal in perps_read.rs.
Group its digits without semantic changes. Keep this correction local until all
current jobs finish; then push it with the recorded direction discussion and
require fresh GitHub verification. No further venue implementation resumed.

Initial CI completed: build/tests/MSRV/web/site/dependency/format/license checks
passed. Lint rejected the timestamp literal. Mutation shards 1/2 additionally
named survivors at perps_read.rs:44:13 and :45:35 (each `||` to `&&` in the book
guard). The malformed-book fixture returned the same BTC packet for later ETH/SOL
requests, masking a bypass with a later identity refusal. Damage only the BTC
response and return correct subsequent books, so the regression must detect that
specific malformed observation. No production semantics, test count or exclusion
changes. Style/docs correction 486d838 was already pushed before reading those
two mutation diagnostics; let its new run finish before publishing this fixture
correction. Require complete GitHub verification; the prototype is not verified.
