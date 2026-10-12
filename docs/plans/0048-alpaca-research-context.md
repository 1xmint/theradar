<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0048 — Give the subscription analyst bounded historical snapshot context

**Status:** Source prepared; GitHub verification pending. No deployment or activation.

Branch feat/alpaca-research-context from 9e0f3fe, stacked on PR 408. Previous
turn added session/price bounds and repaired GitHub findings. Its repair run
38307990221 was confirmed live at continuation, not restarted or cancelled.
The full autonomous trading goal remains active.

Parent repair run completed: source 9e0f3fe, GitHub 38307990221 passed all
14 jobs; 2831 Rust and 47 Python tests, six exact regression reapplications,
58 mutations caught and 12 compiler-unviable, no survivors/timeouts. All four
artifacts and refusal diagnostics inspected. Test floor rises to observed 2831.

## Work

- [x] Opt in only with the fixed experiment strategy_id snapshot-context-v1.
- [x] Feed validated completed minute and previous daily bars into the real model caller.
- [x] Preserve numeric digits and reject impossible OHLC, time and volume inputs.
- [x] Gate by capture availability, then recheck minute freshness at model completion.
- [x] Retain original raw snapshot in the decision for replay/scoring validation.
- [x] Keep context untrusted and explicitly incomplete; no execution authority.
- [ ] Inspect GitHub build, tests, lint, format and mutation artifacts.
- [ ] Schedule/deploy a separate regular-session campaign after verification.

The installed quote-only campaign's first wait decision explicitly lacked
directional evidence. Its owner-reported first outcome was unscoreable from a
stale exit; four later slots failed at that report's watermark. Changing that
frozen campaign's inputs after the fact would spoil the comparison. The new
mode therefore uses an explicit strategy version. Other strategy identifiers
keep their original quote-only observations and trusted prompt.

The actual caller remains radar alpaca-paper-propose, with its existing frozen
experiment file, retained capture and costs. For snapshot-context-v1 it requires
the requested symbol's minuteBar and prevDailyBar before contacting the model.
Alpaca's [snapshot API](https://docs.alpaca.markets/us/reference/stocksnapshots-1)
provides those bars in the same response already recorded by Radar. No extra
provider subscription, credential path or new network endpoint is introduced.

Prices use the existing exact decimal parser, positive volume is parsed as an
integer and reported without asserting consolidated coverage. OHLC must contain
both open and close. Minute eligibility starts at its timestamp plus 60 seconds;
previous-day eligibility requires 26 hours, conservatively covering 25-hour DST
days without claiming a 26-hour broker daily interval. Neither period may extend
past capture availability. The minute end must satisfy the experiment's quote
age bound at observation and completion. Original raw response, fixed strategy
ID, assumptions and observation/freeze watermarks are retained by the existing
decision format; replay validates this context again before scoring.

Model receives four fenced observations, including the bounded context; two
bars are not learned predictive evidence or a profitable strategy. Missing or
bad context refuses the new mode instead of silently falling back to one quote.
The request cannot set position size, grant permission or execute. Bar volume
does not populate the execution kernel's capacity field or certify exit liquidity.

## Handback

Initial GitHub run 38308803627 completed with failures: the context injection
test accidentally searched for words already present in the trusted instruction
("change permissions"). It now uses a unique injected marker and asserts that
the trusted instruction is identical with and without context. Four formatting
hunks and three process-fixture lint findings were corrected from runner logs.
All four mutation shards stopped at that same failing unmutated baseline;
no mutation result is claimed. Build, MSRV, site, web, licence and cargo-deny
passed. Repair verification must complete before deployment.

Three new Rust unit tests cover exact/context time boundaries, malformed bars,
versioned decision replay and completion-age refusal. Existing real CLI/provider
process test now also proves missing context refuses before the model, valid
context reaches the fenced prompt, and the decision preserves the original raw
bars. GitHub verification pending; no workstation jobs, deployment or orders.
Next: separate regular-session forward cohort, longer historical feature series
and held-out comparisons, alongside capacity/cost inputs, isolated supervision
and website handover. Goal active; no measured profitable edge claimed.
