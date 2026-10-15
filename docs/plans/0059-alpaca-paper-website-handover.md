<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0059 — Alpaca paper website handover

**Status:** source verified; website deployed; private controller/supervisor remain uninstalled.

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
Source is not runtime evidence. The website artifact was subsequently deployed
with the evidence below. No private controller, root configuration or supervisor
is installed; no autonomous paper order authority is active.

Remaining: isolated supervisor inference access, model proposals and entry/exit
lifecycle, reviewed installation and end-to-end paper evidence. Richer historical
features, forward/baseline comparisons, adaptive learning and complete economic
fill/cost accounting still remain for the profit goal, followed by eligible live
setup and explicit live permission. No positive expectancy has been established.

## Earlier verification

Correction 441292f212dce10d43c2747b68bcaeab5461b40d is pushed in attached draft
PR 420. GitHub run 38466027799 completed with failure; no job was canceled.
Its lint job 115532922246 requires ok_or for the now-small static Fault, and
formatting job 115532922623 reports one final assertion-wrapping hunk. Those
exact corrections are committed locally at 2800e45. Tests job 115532922447
passed all seven paper tests but failed repo-conformance because the new
learning note lacked index row 60. The missing row and table counts are fixed;
the test was not weakened. That early failure prevented its twenty exact backend
reapplications from running, so those are still pending.

All four mutation artifacts were read, including every compiler-unviable reason:
64 total, 43 caught, 21 compiler-unviable, no survivors/timeouts. Whole-function
substitutions fail unused/dead-code checks or invalid constructor/Default types;
none count as executed coverage. Frontend job 115532921871 completed with
365 tests and all nine panel reapplications caught. Transient jobs-list 502/504
responses were followed by successful reads of the same run and its individual
jobs; no restart was inferred from an observation failure.
The next push combines the retained lint/format fixes, index correction and this
evidence. Full green verification remains required. No local tests, builds,
formatters or mutations ran.

This goal turn made source and executed verification progress. The website
caller is implemented, not yet fully verified or deployed. Continue with the
remaining GitHub evidence, then supervisor integration and the reviewed private
installation. Autonomous trading remains the full active objective.

## Supervisor integration findings

The existing alpaca-paper-propose command has a fixed experiment quantity and
only enter/wait replies; it cannot implement agent sizing or exits. The Codex
bridge currently admits guardian only; a separate supervisor needs exec-only
admission, keeping login operations guardian-only and the model UID excluded
from broker control. The fixed market-read CLI can collect SPY/QQQ snapshots;
the supervisor needs a private observation path rather than broker credentials.

Existing radar-paper-research.service and radar-context-research.service run as
guardian with the paper environment. A reviewed autonomous installation must
retire those completed campaign timers while preserving their files, so those
legacy credentialed processes do not share the website UID. The private recorder
and existing broker observation history must remain preserved. No service was
changed during this inspection.

## Current handback

Source 4116592006059769175b0cfda839439a001b03ab passed all fourteen jobs in
GitHub CI 38468036410. Tests job 115539707338 reports 2862 Rust tests, 109 Python
tests and all twenty exact backend reapplications caught. Web job 115539707196
reports 365 frontend tests and all nine panel reapplications caught. All four
generic mutation artifacts were downloaded and every compiler-unviable reason
inspected: 64 total, 43 caught, 21 compiler-unviable, no survivors/timeouts. No
compiler refusal counts as executed coverage. No check was skipped or canceled.

Matching release 38468143821 succeeded at the same 4116592 source. Downloaded
BUILD-INFO and independent local hashes agree:

- radar-serve: 8c5b14087d97df803c1dd8312207be15df7e2f3d8c43cd585259707ef364e7c3
- radar: 45bb54452d63fd198a3e61c24b690cb3aac2dd266a655d0be74cc84fa33386cf

The retained artifact directory is workstation TEMP/radar-paper-site-release-38468143821.
Only radar-serve was uploaded to the fixed /tmp/radar-serve.new staging path;
its old staged bytes matched the previously installed website, rather than an
unknown pending artifact. The remote new hash matched before sudo radar-deploy.
That fixed command returned active. Installed /usr/local/bin/radar-serve hashes
to the expected 8c5b1408 value, and localhost:8402/health reports the exact
4116592006059769175b0cfda839439a001b03ab build. The first post-deploy curl command
had a PowerShell quoting error; the corrected read completed successfully.

The service remains active in /system.slice/radar-serve.service, guardian UID/
group, MemoryMax 805306368, ProtectHome read-only, ProtectSystem strict and
NoNewPrivileges yes. An unauthenticated paper-status request returned 403.
The existing health trading=true flag describes the manual wallet feature;
it is not autonomous Alpaca authority. Direct systemd inspection reports
LoadState not-found and ActiveState inactive for the private controller socket,
controller service and supervisor service. No new model configuration was
installed; existing broker readers were not replaced.

Using the computer-use skill and the dedicated browser API, a new in-app Radar
setup tab reached Cloudflare Access, then its normal sign-in screen. The browser
needs fresh authentication. The tab is retained for handoff and the owner was
asked to sign in there; no password, session credential or API key was retrieved
or entered. Authenticated
rendering is unverified, not a successful UI check. This does not block backend
supervisor work. Do not bypass the login or infer permission from page content.

Post-deploy paper-admin inspect showed recent completed observation ticks still
successful, zero working/unresolved/terminal orders and new_order_authority false.
Journal metadata was 525021 bytes; lock/genesis markers stayed 82/110 bytes.
No journal contents, root environment contents or credentials were read.

This turn completed verification and website deployment, not the full goal.
Next implement isolated supervisor inference/data access, sizing and entry/exit
proposals, then the reviewed private installation and end-to-end paper check.
Adaptive learning, economic accounting, forward comparisons and eligible live
activation remain as listed above. The full autonomous-trading objective stays active.
