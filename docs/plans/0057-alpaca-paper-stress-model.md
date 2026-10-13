<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0057 — Explicit paper execution assumptions

**Status:** source and matching release verified; service/site integration remains; not installed or enabled.

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
CI 38444102509 measured 2855 Rust tests; the floor now matches that output.

## Handback

Source fe21d9e is pushed in attached draft PR 418. CI 38444102509 completed: its
lint job requires Option<&Model> rather than &Option<Model>, and formatting
reported sixteen hunks. Those exact GitHub formatting edits and the reference
type/caller/test/regression correction are committed locally at e7db69a, without
suppressing warnings. All jobs are terminal; only lint and formatting failed.
The corrected source passed complete GitHub verification below.

All four generic mutation shards passed. Downloaded artifacts show 32 caught,
eight compiler-unviable, no survivors or timeouts. All eight unviable diagnostics
were read: whole-function replacements cause unused/dead-code warnings; they are
not executed-test coverage. The full test job passed: 2855 Rust tests, 98 Python
tests and all eight exact paper-model reapplications were caught by the required
executed tests. Earlier exact checks and unit verification also completed. A
corrected full run passed as recorded below; passing tests alone was not complete CI.

Corrected source a66793ad37cc52170a0f09e3a380fdc968c126f6 passed all fourteen
jobs in CI 38445546289. The actual tests job 115462841296 reports 2855 Rust tests,
98 Python tests and all eight exact paper-model reapplications caught. One log
lookup used an incorrect ID and returned 404; it was corrected using the returned
job metadata before inspecting that real log. All four new mutation artifacts
again show 32 caught and eight compiler-unviable, no survivors/timeouts. All eight
compiler diagnostics were inspected separately; none count as executed guards.

Matching release 38445537967 succeeded. Downloaded BUILD-INFO names the same
a66793a source. Independent local SHA256 agrees with its reader entry:
0c421ea6475f400cda75126214725f718ed37cede13fe68d72c5f663016494f9.
Artifact directory is the workstation TEMP/radar-paper-model-release-38445537967;
it is retained locally, not staged to the VPS or installed. The existing repaired
maintenance reader remains unchanged. This turn made progress through executed
full verification and independently hashed release, not a blocked-access turn.

Next integration evidence: the operator guard in crates/radar-serve/src/lib.rs verifies
Cloudflare identity but discards it at Ok(_); broker handover handlers will need
a request-carried verified operator proof and must refuse when Access is off.
Existing automation handlers require a Privy Solana wallet and save drafts only;
Alpaca controls must use their own pinned broker account and operator identity.
The local Codex bridge already uses a systemd-owned socket and separate
radar-agent UID. Reuse that separation pattern for a private paper broker control
service so the reasoning process never receives broker keys or handover-writing
authority. No such broker-control route or service has been added yet.
No artifact, configuration, website deployment or order authority changed.
Finish isolated control, effective site handover and the supervisor before activation.
The observation repair remains successful; it is not the final trading goal.

Remaining: effective Alpaca manual/Agent decides site controls, revocable paper
handover and isolated model-proposal/risk/entry/exit supervisor; richer historical
features, held-out/forward baselines and learning; cost/fill/fee reconciliation;
eligible live configuration and explicit live authority. All twelve retained
contextual research slots abstained. Do not manufacture an edge or weaken the
default refusal to make a paper demo look ready.
