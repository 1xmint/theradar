<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0047 — Bind paper permission to validated broker session and price increments

**Status:** Source prepared; GitHub verification pending. Trading remains disabled.

Branch feat/alpaca-session-bounds from 926b746, stacked on PR 407. Previous turn
implemented private cancellation, source 0ef633a verified by GitHub run
38306187381: all 14 jobs, 2825 Rust and 47 Python tests; 38 mutations caught,
16 compiler-unviable, no survivors or timeouts, exact replay survivor caught.
That was concrete progress, not activation. The full autonomous trading goal
remains active.

## Work

- [x] Validate broker clock time, offset, freshness and open/close chronology.
- [x] Include broker clock age in oldest input; expose validated close in preflight.
- [x] Make pure equity authorization expire strictly before broker session close.
- [x] Reuse submission's final time check to refuse a close after durable writes.
- [x] Reject unsupported limit increments before provider reads or reservation.
- [ ] Inspect GitHub build, tests, lint, formatter and all mutation artifacts.
- [ ] Verify actual capacity/cost inputs, isolated supervisor and site handover.

The actual preflight caller previously used only is_open, ignoring the broker
clock timestamp and close. State now requires an explicit validated session
close. The pure kernel refuses missing/elapsed close and caps permission at
close minus one millisecond. Its existing final permission and re-evaluation
checks therefore also cover the session boundary after durable reservation.
No local weekday/timezone calendar substitutes for the broker's own session.
This is a selected broker observation, not proof of an atomic market snapshot,
individual-asset halt state or guaranteed execution before close.

The private clock parser accepts canonical UTC or explicit numeric offsets,
reuses the quote parser's date/nanosecond checks, and refuses malformed zones,
future/stale observations, closed sessions and unordered next open/close. It
floors observed time and close conservatively to milliseconds. The broker
[clock model](https://github.com/alpacahq/alpaca-py/blob/master/alpaca/trading/models.py)
defines timestamp, is_open, next_open and next_close. Actual response format
verification remains distinct from fixture tests.

Preflight rejects limit prices outside cents at or above $1, or ten-thousandths
below $1, without rounding, following the official
[order increments](https://docs.alpaca.markets/us/docs/orders-at-alpaca).
Nanoshares already bound quantity precision; asset fractionability checks remain.
This does not claim all broker order constraints have been implemented.

Capacity and fees/costs remain absent. Official
[paper behavior](https://docs.alpaca.markets/us/docs/paper-trading) ignores NBBO
quantity and omits regulatory fees, market impact and latency slippage. A broker
paper fill therefore cannot establish live exit capacity or live profitability.
Streaming documentation describes size in round lots, while retained snapshot
sizes alone cannot establish equivalent units or routed liquidity. No zero-cost
or unlimited-capacity shortcut was added to get orders through the kernel.

## Handback

Six new Rust tests cover offset/nanosecond parsing, freshness and chronology,
increment boundaries, actual collector refusal, pure permission close and
submission crossing close after durable writes. All jobs run on GitHub only;
verification pending. No new credentials, deployment or broker effect.
Next: verified capacity/cost model for paper execution, isolated supervisor and
effective website controls, plus forward strategy evaluation. Goal active.
