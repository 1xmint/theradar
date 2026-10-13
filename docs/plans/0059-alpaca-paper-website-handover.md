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
local calls have finite deadlines and concurrency, responses are projected into
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
Ten exact reapplications require all seven tests to execute and fail; compiler
refusals are not kills. Eight frontend tests exercise mount/StrictMode, manual
save, delegated enable, optional daily cap, draft-independent pause, unknown
reads/writes and one-write-at-a-time behavior. Existing setup tests keep login
poll counting separate from the new paper account read.

All tests/build/lint/format/mutations run on GitHub, never the workstation.
Source is not runtime evidence. No controller, root configuration, supervisor or
website artifact is installed by this source change; no orders are enabled.

Remaining: isolated supervisor inference access, model proposals and entry/exit
lifecycle, reviewed installation and end-to-end paper evidence. Richer historical
features, forward/baseline comparisons, adaptive learning and complete economic
fill/cost accounting still remain for the profit goal, followed by eligible live
setup and explicit live permission. No positive expectancy has been established.
