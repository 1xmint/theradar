<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0020 — Conservative paper broker exposure

**Status:** source implemented; full GitHub verification pending.

Branch: feat/alpaca-paper-exposure. Base: 02a3d3d, verified by complete GitHub CI
38179746811 (PR 374). This increment extends that actual reconciliation caller.
PR 374 merged at 6ae333d with the identical tree; parent CI 38180442914 is queued.
Independent setup PR 375 passed full CI 38179947445 at 3b34115. Do not advance the
parent while this child's CI runs. No workstation tests/builds/lint/mutation jobs.

## Objective

Make broker positions and pending commitments usable by the eventual pure risk
evaluator without confusing a balance read with free capital. This is the next
step toward isolated autonomous paper execution, not an alternative end state.

## Checklist

- [x] Compute exact nine-place fractional-share and dollar quantities without f64.
- [x] Floor observed funds and ceil exposures to micro-USD; check overflow.
- [x] Retain original pending quantities through partial fills/cancellation.
- [x] Bound simple equity limit buys, long position cost/value and reserved sells.
- [x] Avoid subtracting open orders twice from broker buying power.
- [x] Refuse unknown flags/decimals, unsupported orders, ambiguous identities,
  oversold shares and changing bracket observations; publish null bounds.
- [x] Attach bounds to the existing CLI caller; add seven behavior regressions,
  including that caller. Test floor 2316.
- [ ] Read full GitHub CI and mutation diagnostics; repair actual failures.
- [ ] Integrate verified source after parent/other-child checks finish.

## Handback

CI 38180545954 at 8a75159 found test fixture type inference selecting unsized
slices for serde decoding, and a mutable reference to u64::MAX. Use explicit Vec
decode types and a local mutable total. Full run must finish before pushing the
repair; do not cancel pending jobs. The full run is now terminal with the same
baseline compilation error in build/tests/MSRV/mutation jobs. No local substitute verification.

PR 374 source 02a3d3d passed every job in CI 38179746811, including all mutation
shards. This increment is pending verification. No paper account or real broker
capture has been supplied. Unsupported states remain visible observations with
unknown bounds. Supported bounds are deliberately conservative and may overcount
partially filled orders. They exclude fees and do not reconcile durable local
claims, settlements, fills or freshness; risk_state_complete/execution_authority
remain false. No keys, deposits, orders, delegation or deployment changed.

Next: durable order/claim journal and recovery, complete fresh account state,
deterministic mandate/quote/asset checks and isolated paper order submission.
Then connect supervisor and site handover; compare held-out and forward results
net costs before separately eligible live activation. Preserve Agent decides,
adaptive risk and optional daily loss controls; do not solicit chat mandates.
