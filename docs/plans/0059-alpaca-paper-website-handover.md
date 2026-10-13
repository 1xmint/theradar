<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0059 — Alpaca paper website handover

**Status:** source implemented; GitHub verification pending; not deployed.

Branch feat/alpaca-paper-site, parent 8870441. Parent CI 38461222557 completed
all fourteen checks successfully before this child push. Previous goal turn
made progress with the private controller and its full GitHub verification;
this turn connects its actual owner-facing caller. Keep the parent stable.

## Effective caller

The operator guard now carries verified Access identity in a request-only proof.
Access off, unsigned email headers and customer authentication cannot construct
that proof. The separate /v1/paper routes require it for reads and changes; they
derive the controller actor from that proof, never client JSON. A paper-only
local socket client sends only status/configure/pause, with no order route,
broker credentials or arbitrary executable/path. The web UID still cannot submit
through the controller. The supervisor still cannot change owner policy.

Writes require strict JSON, browser same-origin Fetch Metadata and a non-simple
control header. These browser checks prevent cross-site use of an operator
cookie; they never replace verified authentication. Request bodies are bounded,
local calls have finite connect/I/O timeouts and concurrency, responses are projected into
selected typed fields, and errors never echo private controller/vendor text.
Unexpected live, execution, lifecycle, profit or change acknowledgement fields
refuse. Each change carries the current generation; unknown outcomes are not
automatically retried. Handler responses are private/no-store.

The setup page now includes a distinct Alpaca panel, independent of Privy.
It reads simulated cash/equity, positions and session status without enabling
anything. Manual amounts and per-option Agent decides choices feed the real
private policy control. Saving explicitly disables handover. Enabling permission
is a separate click. Pause does not require valid unsaved draft amounts and does
not promise cancellation or liquidation. Every successful write reloads status;
an unresolved write disables further changes until an explicit reload. Optional
daily loss selection explains that unknown loss measurement currently blocks
entries. The page never calls the granted permission a running autonomous agent.

## Verification and handback

Seven new Rust tests exercise the real Access guard with an existing genuinely
signed RSA fixture, real HTTP extraction/CSRF/body bounds, injected controller
responses and an actual local Unix socket. Customer guard coverage adds the
paper status route. Access-off integration explicitly refuses paper status.
Nineteen exact reapplications require all seven unit tests to execute and fail;
one more verifies actual app-route wiring through its integration test without
relying on a dead-code compiler refusal. Compiler refusals are not kills.
Eight frontend tests exercise mount/StrictMode, manual
save, delegated enable, optional daily cap, draft-independent pause, unknown
reads/writes and one-write-at-a-time behavior. Existing setup tests keep login
poll counting separate from the new paper account read.
Nine frontend reapplications must fail through all eight panel tests; parse/
import failures do not count. Initial GitHub web job 115524620518 measured
365 frontend tests, including those eight; the frontend floor now matches.
Initial mutation shard 0 found two survivors at original 9391352 paper.rs:14:22
and :14:29 (each multiplication in the response bound). The test had derived its
oversized payload from that same production constant, so a broken bound changed
both implementation and expectation. Its replacement pins the independent
4194304-byte protocol limit and checks the exact boundary plus one byte above.
The response reader now enforces that byte cap once, with Take(LIMIT); an
oversized frame cannot include its ending newline and fails framing. The exact
byte-gate regression enlarges that one bound rather than disabling a redundant
post-read length check. Both request boundary cases are independent as well.
Both exact arithmetic faults are now included in the required reapplications;
corrected execution remains pending. No exclusion was added for these survivors.
Shard 2 found status survivors at original paper.rs:153:37 (&& to ||) and :153:59
(== to !=). Valid initial generation-zero/disabled and generation-seven/granted
status cases now isolate those refusals, with both exact faults added to the
reapplications. Source 9391352's tests job 115524620739 passed 2862 Rust tests,
109 Python tests and its original ten website regressions. The Rust floor now
matches that measured count. Complete CI is not green: lint/format and mutation
corrections still require a new run after every original shard is terminal.

Original CI 38463563767 is terminal failure. All four mutation artifacts were
downloaded and every compiler-unviable diagnostic inspected: 70 mutants total,
40 caught, nine missed, 21 compiler-unviable, no timeouts. Unviable cases were
unused/dead-code whole-function substitutions, invalid generic constructors,
Identity lacking Default, and a cast/comparison parsing error; they do not count
as executed coverage. No mutation job was canceled or bypassed.

Shard 3's exact original request survivors at paper.rs:217:22 (> to == and >=)
now have independent 65536/65537-byte cases. Its :220:51 (+ to -) read-bound
fault is reapplied as Take(LIMIT - 1). Its :221:29 inclusive response refusal is
reapplied against the independent maximum-length valid response. The other
:220:51 (+ to *) case reduced Take(LIMIT + 1) to Take(LIMIT), equivalent under
the existing newline and length checks. The production reader now uses that
simpler single byte gate and removes the redundant post-read check; there is no
excluded operator or equivalent-mutant exemption. Corrected verification remains
pending. Initial source passed tests and all 365 frontend tests; lint/format and
the genuine coverage gaps are corrected locally for the next full run.

All tests/build/lint/format/mutations run on GitHub, never the workstation.
Source is not runtime evidence. No controller, root configuration, supervisor or
website artifact is installed by this source change; no orders are enabled.

Remaining: isolated supervisor inference access, model proposals and entry/exit
lifecycle, reviewed installation and end-to-end paper evidence. Richer historical
features, forward/baseline comparisons, adaptive learning and complete economic
fill/cost accounting still remain for the profit goal, followed by eligible live
setup and explicit live permission. No positive expectancy has been established.

## Current handback

Correction 441292f212dce10d43c2747b68bcaeab5461b40d is pushed in attached draft
PR 420. GitHub run 38466027799 is confirmed live; do not restart/cancel it or
push over its remaining tests and mutation shards. Its lint job 115532922246
requires ok_or for the now-small static Fault, and formatting job 115532922623
reports one final assertion-wrapping hunk. Those exact corrections are retained
locally, unpushed while this run is live. Tests job 115532922447 and all four
mutation jobs were still in progress at the latest check; read their terminal
results and artifacts before pushing the local correction. Frontend job
115532921871 completed successfully: its actual log reports 365 tests and all
nine panel reapplications caught. A transient jobs-list HTTP 502 was followed
by a successful read of the same still-live run; nothing was restarted.
No local tests, builds, formatters or mutations ran.

This goal turn made source and executed verification progress. The website
caller is implemented, not yet fully verified or deployed. Continue with the
remaining GitHub evidence, then supervisor integration and the reviewed private
installation. Autonomous trading remains the full active objective.
