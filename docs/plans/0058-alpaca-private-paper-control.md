<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0058 — Private paper handover control

**Status:** source implemented; GitHub verification pending; not installed.

Branch feat/alpaca-paper-control, parent d1bf9d7. Parent documentation CI
38447606174 completed all fourteen jobs successfully before this branch. Its
verified paper reader release is pinned by SHA256 in the controller. The
opening-cash acknowledgement and ordinary observation remain successful; this
change does not replace their reader or initialize broker history.

## Caller and authority

The systemd socket activates deploy/paper-control.py with the private observation
state directory and paper credentials. Linux peer credentials distinguish the
existing guardian web UID, a separate radar-paper-supervisor UID and the excluded
radar-agent reasoning UID. The web process may set/revoke standing permission,
but cannot submit. The supervisor may propose an intent, but cannot change
permission. Neither receives broker credentials through this protocol. The
existing Rust CLI collects broker facts, applies the deterministic risk kernel,
persists uncertainty and checks revocation again before the fixed paper POST.

Root-owned configuration pins the account, operator email, model file hash and
reader binary. The authenticated web caller must supply its verified operator
identity; once bound, its subject cannot change through IPC. The future website
handler must derive that identity from verified Cloudflare Access, never request
body fields, and refuse when Access is off. This change supplies the private
controller, not that handler, a supervisor or an installation command.

## Handover behavior

Manual capital/trade fields and per-option Agent decides flags become standing
ceilings bounded by fresh observed equity. Delegated ceilings permit the agent
to choose actual allocation within the wallet bound; they do not constitute a
learned sizing strategy. Daily loss stops remain optional. Selecting one still
refuses entries while the existing collector lacks a verified daily loss meter.
No unknown P&L is changed to zero. Initial state is disabled.

Owner changes require the current generation and publish a new policy file.
Pause closes the prior file before publishing the new revision. Resume cannot
reopen an old in-flight proposal's file. Generation files are created exclusively;
partial publication preserves evidence and refuses uncertain reuse. Refreshing
equity before a submission checks the owner generation again. Existing broker
history and its observation lock are required, never created or reset here.
Pause does not cancel existing orders or liquidate positions, and cannot undo a
broker effect already accepted before the final policy check.

The protocol uses bounded single-line strict JSON, socket peer credentials,
fixed subprocess arguments, a minimal broker child environment, bounded storage
and private errors. Submission failures are not retried or erased. A successful
submission response is not a fill or profit. Status explicitly reports the agent
lifecycle unverified. The controller is paper-only.

## Verification and handback

Eleven tests exercise actual temporary files, file locks and socket peer
credentials, with stubbed broker CLI calls and synthetic history bytes. They
cover role/identity refusal, stale revisions, decimal bounds, pause/resume,
revocation during collection/submission, partial publication, process/response
failures, existing history, protected configuration and response privacy. The
history stand-in is not a new proof of Rust journal reconciliation; the actual
CLI and durable executor have their separately verified parent tests.

Seventeen exact regression reapplications require all eleven tests to execute
and fail; syntax/import failures do not count. Redundant peer guards and both
standing-permission checks are mutated together where a single removed guard
would retain the property. Generic Rust mutation checks remain unchanged.
All tests, unit verification and jobs run on GitHub. No local tests were run.

Remaining for usable autonomous paper operation: reviewed private installation,
verified-identity site controls and client, isolated supervisor access to the
reasoning bridge, model proposal/entry/exit lifecycle, and operational evidence.
Richer historical features and held-out/forward comparisons, realistic fill/cost
accounting and learning remain necessary for the profit goal. All twelve earlier
context decisions abstained; no positive expectancy has been established.
No service, website, credentials or trading authority changed in this source step.
