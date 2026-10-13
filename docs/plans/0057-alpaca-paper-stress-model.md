<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0057 — Explicit paper execution assumptions

**Status:** source and regression checks prepared; GitHub verification pending; not installed or enabled.

Branch feat/alpaca-paper-stress-model, base 6b2e82c. Parent source 1819896 passed
all fourteen checks and matching release; its opening-cash repair was applied
and direct ordinary maintenance/activities checks succeeded. Parent documentation
CI 38441956057 at 6b2e82c passed all fourteen jobs before this child push. Keep
that parent stable while child CI runs. The previous turn made progress by clearing the
runtime accounting blocker. The full goal remains autonomous trading.

## Why this moves execution forward

The default collector deliberately leaves capacity, fee reserve and round-trip
cost unknown. It therefore cannot authorize orders. Alpaca's
[paper specification](https://docs.alpaca.markets/us/docs/paper-trading) says its
simulator does not check order size against available quantities and omits
important live costs. Turning its fills into live liquidity evidence would be
wrong. Requiring measured live capacity before even testing a simulated execution
loop would also prevent the paper experiment from advancing.

Add an explicitly selected private operator stress model to the fixed-host paper
preflight/submission caller. It provides a finite scenario share ceiling, positive
fee reserve and positive per-side slippage. These are declared experimental
assumptions, never model confidence, observed liquidity or measured live fees.
Only the private operator file supplies these assumptions; no model/site API
route is added. Effective handover and isolated service provisioning remain
separate work. No new handover is implied.

## Actual caller and requirements

- Optional --paper-model <private-simulation-model> on existing private commands.
  Missing selection preserves the original unknown-input refusal.
- Strict schema/version, account binding, finite positive ceiling/fees and
  positive slippage up to 10000 bps. Invalid models refuse before key loading.
- Exact u128 arithmetic; round fractional USD costs up. Charge quoted spread
  once plus two sides of fee/slippage, using the larger of ask/limit for scenario
  slippage so reduction limits cannot understate it. Overflow refuses.
- Apply only to the selected account and empty execution-bound inputs. Do not
  overwrite existing evidence or partially update state after invalid costs.
- Cash, owned shares, portfolio/order/symbol limits, session/quote checks and
  revocation remain in the existing deterministic risk and durable executor.
- Reload and compare the model before risk and after durable submission
  uncertainty; changed, removed or invalid configuration refuses before send.
  A refused second check keeps uncertainty and authorizes no retry.
- Reports include the full scenario and explicit false live capacity/cost and
  profit flags. Simulator assumptions cannot establish live expectancy.

## Verification

Four model tests cover arithmetic, strict inputs, overflow and file reloads.
Three actual collector/configured-dispatch tests cover both sides, capacity/fee/
cash/closed-policy refusals, no-model refusal, successful stub submission only
after durable uncertainty, changed-model refusal before/after reservation, and
atomic state failure. One actual CLI process test covers both commands, optional
selection, invalid scenario validation before credentials, unsupported flags and
missing history without creation. Eight exact regressions require executed
tests; compiler refusals are not kills. Tests/build/lint/mutations run on GitHub.
Do not raise the measured parent floor 2847 until actual full runner output.

## Handback

Implementation and verification harness are prepared. No child checks run yet;
no artifact, configuration, website deployment or order authority changed.
Complete the GitHub checks and inspect mutation diagnostics before provisioning.
The observation repair remains successful; it is not the final trading goal.

Remaining: effective Alpaca manual/Agent decides site controls, revocable paper
handover and isolated model-proposal/risk/entry/exit supervisor; richer historical
features, held-out/forward baselines and learning; cost/fill/fee reconciliation;
eligible live configuration and explicit live authority. All twelve retained
contextual research slots abstained. Do not manufacture an edge or weaken the
default refusal to make a paper demo look ready.
