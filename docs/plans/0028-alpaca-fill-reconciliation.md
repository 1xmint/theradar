<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0028 — Combine verified recovery inputs and reconcile executions

**Status:** combined inputs implemented; merged-tree verification and fill reconciliation pending.

Branch: feat/alpaca-fill-reconciliation. Sources d3f7975 (full CI 38184222577)
and eb70903 (full CI 38184575296) both passed before combination. Parent 3a4ba61
CI 38184135535 remains running; keep it fixed and verify this separate branch.
Previous goal turn made progress by implementing account activity pagination;
this turn verified that source and combined it with verified payload binding.
All verification runs on GitHub; no local jobs.

## Checklist

- [x] Combine verified intent binding and account activity observations, preserving
  optional journal lookup, both CLI dispatches/help and selected broker schemas.
- [x] Sum independent tests: 2337 foundation + 5 binding + 5 activities = 2347.
- [ ] Read combined GitHub CI and repair any failures before claiming integration.
- [ ] Add the actual order reconciliation caller under journal ownership: bracket
  activity collection with bound order observations and pinned account reads.
- [ ] Match execution identity, side, ticker, cumulative quantity and remaining
  quantity. Reject duplicate IDs, gaps, overlaps, overfills and unsupported shapes.
  Use cumulative quantities to establish execution intervals, independent of
  response order, instead of assuming timestamps uniquely order executions.
- [ ] Derive gross fill amounts with exact arithmetic and conservative USD bounds.
- [ ] Reconcile fees/nontrade cash and baseline account cash without attributing
  deposits/withdrawals to strategy returns. Late fees/corrections remain possible.
- [ ] Persist verified economic transitions before claim release, with replay and
  failure recovery; ambiguous submission never becomes permission to retry.
- [ ] Complete pure equity risk, isolated paper executor, supervisor/site controls
  and measured evaluation from plan 0024. Preserve the full autonomous goal.

## Handback

No orders, credentials, deployment or activation. Inputs are now combined in
source; a matching payload and exhausted pagination still do not prove fills,
cash or final fees reconciled. No release API exists. The next code change must
connect these verified inputs through the actual reconciliation caller rather
than add another unrelated observation tool. The owner still needs to create
the paper account for actual response verification. This does not block the
remaining implementation work. The parent and verified source branches stay
fixed while required jobs finish; none has been canceled or bypassed.
