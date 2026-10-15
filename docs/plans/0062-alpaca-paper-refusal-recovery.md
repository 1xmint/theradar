<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0062 — Recover from proven paper refusals

**Status:** source implemented; GitHub verification pending; not deployed.

Branch feat/alpaca-paper-refusals, parent 79fa519 (PR 422). Parent CI
38496773942 completed all fourteen jobs successfully before this branch was
created. The preceding goal turn made deployment and runtime-verification
progress: the private controller and supervisor are installed and paused.

## Behavior

Ordinary deterministic risk refusals must not strand the autonomous supervisor.
The submission executor can emit refused_before_reservation only after checking
healthy existing journal entries exclude the proposed client ID, and before any
reservation or network submission. Initial risk rejection and initial payload/
authorization mismatch use this explicit receipt. Existing client IDs, failed
policy/clock reads, and every error after reservation continue to require
inspection/reconciliation; missing broker IDs or stderr never establish safety.

The controller and supervisor validate exact receipt fields and identity, paper
venue/environment, absent broker ID, explicit false reservation/network/retry/
claim-release/fill flags, and paper-scenario limitations. A proven refusal becomes
a completed proposal_refused attempt; a subsequent tick can collect fresh inputs
and reason with a new durable identity. No old submission is retried, policy
ceiling increased, claim released, or broker journal reset. A lost IPC response
still remains unresolved, even when the executor may have refused it.

## Verification / handback

Rust tests assert the full receipt and no reservation/network effect, and ensure
a previously retained identity cannot receive the non-dispatch receipt even
when risk is currently disabled. Python tests exercise actual controller
forwarding and supervisor refusal/restart/new-identity behavior; malformed and
overclaimed receipts refuse. Exact Rust/Python reapplications run in GitHub CI,
alongside the existing unknown-response and post-reservation tests.

No local tests/builds/jobs ran. Verification and a new verified reader artifact
are pending. Installed reader/controller/supervisor remain the earlier paused
versions; never deploy this controller against the old pinned executor or
claim that source tests establish a deployed lifecycle. Next work includes
verification, a coordinated protected upgrade, accurate site status, explicit
owner handover and actual paper entry/exit/cancel/reconciliation/pause. Adaptive
learning and measured profit after costs remain unfinished. The full autonomous
trading goal remains active; live orders remain unauthorized.
