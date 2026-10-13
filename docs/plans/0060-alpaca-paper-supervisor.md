<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0060 — Isolated autonomous paper supervisor

**Status:** source implementation; GitHub verification and private installation pending.

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

- Pending GitHub Python suites and exact behavioral reapplications for durable
  dispatch, response validation, pauses, cancellation, watermark and peer roles.
- Pending GitHub systemd unit verification and all standard checks.
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
GitHub verification and the reviewed private installer. Full autonomous trading
remains the active objective; this turn continues the actual paper execution loop.
