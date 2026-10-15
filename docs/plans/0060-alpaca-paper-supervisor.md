<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0060 — Isolated autonomous paper supervisor

**Status:** source GitHub-verified; private installation and actual paper lifecycle pending.

Branch feat/alpaca-paper-supervisor, parent a3dc9a8. Parent CI 38471213998
completed successfully before child work. Website source 4116592 is deployed;
the private controller and new supervisor are not installed.

## Actual caller

The scheduled credential-free Python supervisor collects reconciled account,
positions, regular-session clock and validated SPY/QQQ IEX quotes through the
private controller. It asks the existing isolated subscription bridge for a
strict inert buy/sell/cancel/wait proposal, including adaptive share sizing.
The bridge admits the distinct supervisor UID for inference only; login stays
operator-only. Model UID remains excluded from broker control and owner policy.
The supervisor receives neither broker keys nor subscription credentials.

Before dispatch, it collects again, rejects changed/disabled handover, closed
sessions, expired decisions, unresolved orders and overlapping trades. Limits
and fees are derived from validated quotes and the root-declared stress scenario,
not model-provided prices or costs. The existing private submit CLI remains the
deterministic risk/journal/policy gate for both cash-funded entries and owned-share
exits. Cancellation binds an existing intent and records uncertainty before its
fixed paper DELETE. Acceptance never proves fills, cancellation or claim release.

An exclusive supervisor lock serializes ticks. Provisioned origin and existing
lock are mandatory; no tick initializes history. Each new attempt has a unique
identity, durable observation/proposal and a dispatching state written before
IPC. Unknown responses retain that identity across restart and cannot authorize
resubmission. A missing retained broker identity requires inspection; uncertain
cancellation must reach terminal reconciliation before another tick can proceed.
Inert pre-dispatch attempts are not reused. Historical observations enter context
only strictly before the current watermark. Storage has a byte and attempt cap;
unknown/partial entries stop rather than being deleted or silently rotated.

The systemd tick has no network address families except Unix sockets, no broker
environment and a private state directory. A dedicated radar-paper-ipc group
permits socket access; the web drop-in and operator group membership must be
provisioned together. Peer UID checks, rather than group membership, decide the
allowed operations. These units are source only, not an installed permission.

## Verification and remaining installation

- GitHub Python suites and exact behavioral reapplications verified at 48936c3
  by CI 38476247749, including durable dispatch, response validation, pauses,
  cancellation, watermark and peer roles.
- GitHub systemd unit verification and all fourteen standard checks passed at
  that source; no local tests or jobs ran.
- Pending reviewed root installation: protected pinned reader/model/config,
  separate principals/group, existing broker journal and provisioned supervisor
  origin. Preserve every existing journal byte and genesis marker.
- Retire completed guardian-UID credentialed research timers and preserve their
  data. Keep isolated market recording and observation intact.
- Restart the website only through fixed radar-deploy after verified staging;
  preserve hardening and subscription login. Verify authenticated panel, paused
  tick, explicit site handover, reconciliation and pause without inventing trades.

The loop is not a learned profitable strategy. Its accumulated quote context is
limited, IEX is not consolidated, and stress costs are assumptions. Rich history,
forward/held-out baseline comparisons, economic fill/fee accounting and adaptive
risk measurement remain required. No live broker activation is authorized here.

## Current handback

Read-only VPS paper-admin inspect at approximately 22:00 UTC on October 13 showed
successful consecutive observation ticks, zero working/unresolved/terminal or
pending-cancellation orders, and new_order_authority false. The journal was
577783 bytes; lock/genesis markers remained 82/110 bytes. This independently
confirms observation continued after the opening acknowledgement; it is not
profit evidence. No credentials or journal contents were read.

No local tests, builds, lint, format or mutations ran. Current source still needs
the reviewed private installer and actual lifecycle verification. Full autonomous trading
remains the active objective; this turn continues the actual paper execution loop.

## First GitHub verification

Source c7d4c27, draft PR 421, CI 38475644601 completed with the tests job failing
at exact supervisor regression 6 (decision expiry). Normal Python suites passed;
the original seventeen controller reapplications and first five supervisor cases
were caught. Removing the decision-age check still passed because the fixture
advanced the observation clock but left its quote watermark behind; intent
validation rejected that separate mismatch. The fixture now advances both fresh
quote watermarks as well, isolating the aged original decision. This is a test
correction, not permission to weaken the expiry rule. Remaining new cases still
need execution. A source review also changed the model-integrity fixture to
valid JSON with a different fee, so parsing cannot mask a missing digest check.
All other thirteen jobs completed successfully, inspected individually before
the next push. Python discovery ran 127 tests; the new exact suite ran 37.

Read-only installation inspection found the existing root Codex bridge hash
4a6eba67dece415085dd71e7eab248eb22d8b56d21fe423b6a606bb1ec4cfcd8,
its regular root-owned 0755 client wrapper hash
37b37f06839bbb688df9813634a9814102a2f87e72a398046c162cd9284de31a,
and old observation reader hash
3f21fc1ae3b54b2333a5b809eda916c296be0948d69dba1f9da618adec9733d2.
The research/context timers are active but their services inactive. The isolated
model user exists; supervisor user and dedicated IPC group do not. No service,
key, private directory, wrapper or existing reader was changed by these reads.

## Verified source handback

Source 48936c3c9ac8f39248f5022869fa40a53fa3284f passed all fourteen jobs in
CI 38476247749. Tests job 115567535735 ran 127 Python tests and caught all 21
new supervisor/observation/bridge regressions, including the corrected original
decision-age case. All seventeen existing controller reapplications passed;
the existing exact Rust checks completed, followed by systemd-analyze verification
of controller and supervisor units. Web job 115567535335 ran 365 tests and caught
all nine panel reapplications. No Rust behavior changed; the generic mutation
shards reported no changed Rust source and retained no mutation artifacts.
Compiler refusal is not claimed as executed coverage. No jobs were canceled.

Attached draft PR 421 is stacked on PR 420. The source is verified, not installed.
The next concrete work is one reviewed private installation bundle for controller,
supervisor, model/configuration, socket group/drop-ins and observation diagnostics.
Use the already verified 0c421ea paper stress reader; do not replace the ordinary
observation reader or re-run initialization/opening acknowledgement. Provision
supervisor origin/lock under its own service identity; root-owned dynamic-user
storage files must not be created with guessed host UIDs. Validate and preserve
the original broker-journal prefix and genesis markers across installation.

Retire the completed guardian-UID credentialed campaign timers while preserving
their files. Keep subscription credentials and its fixed client intact; replace
only the reviewed bridge after quiescence, with its original bytes retained.
Install the dedicated socket group and web drop-in together. Apply a website
restart only through fixed radar-deploy with the verified artifact and health
build/hash checks. Start paused, inspect actual local socket identity separation
and authenticated panel, then exercise explicit site handover/reconciliation/
pause without forcing a trade merely to manufacture a success report.

This turn made source, runtime-inspection and executed verification progress.
No new service, model configuration, broker credential, signer or order permission
was installed or enabled. The full autonomous-trading objective remains active;
adaptive learning and measured profit after costs remain unproven.
