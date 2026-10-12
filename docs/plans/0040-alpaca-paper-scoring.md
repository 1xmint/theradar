<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0040 — Cost-aware offline equity outcome scoring

**Status:** Implemented; GitHub verification pending.

Branch: feat/alpaca-paper-scoring, based on 1c609e7. Previous turn made verified
progress with actual preflight quote metadata checks. Parent documentation run
38270900873 passed all 14 jobs, inspected before branching.

## Checklist

- [x] Add actual offline CLI consuming two retained captures with independent
  availability, symbol, metadata, age and spread checks.
- [x] Require explicit versioned research assumptions: size units, participation,
  fees and adverse slippage; retain assumptions with the computed outcome.
- [x] Score entry ask to later exit bid using exact nanoshares/micro-USD arithmetic.
- [x] Reject excess assumed participation and overflow; preserve negative results.
- [x] Exercise real CLI without credentials, future capture refusal, unsupported
  symbols, quantity limits, rounding, chronology and friction boundaries.
- [ ] Inspect all GitHub checks and mutation diagnostics.

## Handback

Caller is radar alpaca-paper-score, not a disconnected library. No workstation
jobs. This is the outcome scorer required before comparing learned decisions
with tradable baselines. Both recorded quote inputs go through the existing
capture availability gate and the shared IEX quote validator.

The explicit displayed_size_unit=shares is a research assumption, not new feed
verification. Assumed capacity is floor(min(entry ask size, exit bid size) times
participation fraction), converted from millionths to nanoshares. No multiplying
old round lots by 100. Quantity above this assumed capacity refuses; it never
creates a broker risk-state capacity. The future exit participates only in
outcome scoring, not in an entry decision or position-sizing claim.

Entry cash notional rounds up; exit proceeds round down. Each side's adverse
slippage rounds up; a fixed fee is charged twice. Entry ask and exit bid already
include spread, so it is not charged again. Zero friction is accepted only as an
explicit sensitivity assumption, never a missing-data default. A zero quantity,
missing model identity, unsupported units, participation outside 1..10,000 bps,
slippage above 10,000 bps, arithmetic overflow, missing quote amounts or an exit
not later than entry refuse. Model values are not recommended live fees or limits.

Example research assumptions (illustrative stress values, not market estimates):

```json
{
  "model_id": "sensitivity-example-1",
  "displayed_size_unit": "shares",
  "participation_bps": 1000,
  "slippage_bps_per_side": 1,
  "fee_per_side_microusd": 7,
  "max_quote_age_ms": 1000,
  "max_spread_bps": 10
}
```

Output labels offline_simulation, capacity_verified=false,
realized_broker_pnl=false and execution_authority=false. It retains both quote
reports and all assumptions. It does not claim execution, realistic fill
probabilities, tradability/session eligibility, a frozen experiment or learned
edge. Later evaluation must freeze decisions/assumptions before outcomes and
report unscoreable cases, not silently select winners. Alpaca's
[paper rules](https://docs.alpaca.markets/us/docs/paper-trading) omit fees and
liquidity limits; this tool exposes those assumptions rather than treating broker
paper fills as verification. No credentials, network, service change or orders.

Next: score recorded forward decisions and matched baselines, resolve actual
execution inputs, protected broker supervisor/recovery, and website handover.

Initial verification at 66742ad: GitHub run 38271926517 completed all 14 jobs.
Twelve passed, including 2,810 Rust tests, 27 Python tests and all 32 targeted
Python mutations. Formatting and one assert_is_empty lint failed; the exact
formatter changes and explicit empty-vector assertion are corrected for rerun.
All four Rust mutation artifacts were inspected: 38 caught, seven compiler
unviable, zero missed or timed out. Main replacement caused denied dead code;
amount replacements caused unused quote/field; bounded replacements unused
value; score replacement unused json/arguments and dead helpers; run replacement
unused Path/args and dead helpers. These are compiler refusals, not test kills.
