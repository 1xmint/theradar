<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0062 — Recover from proven paper refusals

**Status:** source GitHub-verified and release artifact hashed; protected deployment pending.

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

## First GitHub run

Source 511d9ea, draft PR 423, CI 38780392681 completed all jobs without
cancellation. Rust tests passed 2863 and Python discovery passed 138. Tests then
stopped at the existing controller regression harness: its hardcoded count was
still eleven after adding the twelfth controller test. The actual injected peer
authorization bug was caught by that twelfth-suite run; update the count, never
weaken the assertion. New exact refusal suites were not reached in this run.

Lint requested let-else for the new initial-risk branch; it is corrected.
Cargo-deny reported newly published RUSTSEC-2026-0346 through 0349 affecting
rustls 0.23.45, with 0.23.46 as the patched version. The official sparse registry
at https://index.crates.io/ru/st/rustls confirms 0.23.46 is not yanked, has the
same dependency requirements, and checksum
48e13bd8c0e9365c43cfa5c9e8f9ad49d3c8444926c9aac819e0e4dc503c8fdf.
The minimal lockfile version/checksum update accompanies the next verification;
no advisory is ignored. This dependency change is necessary to pass the release
security gate. No local Cargo process ran.

All four generic mutation shards completed: two caught and two unviable, zero
missed/timeouts. Retained diagnostics were read: replacing execute made existing
helpers/arguments unused, while replacing refused made its arguments unused,
both rejected under deny-warnings. Those compiler refusals are not executed
behavioral coverage. The exact refusal reapplications still need execution.

VPS inspection attempted through the existing SSH alias now requires a fresh
Tailscale SSH check. The owner was asked to finish that browser sign-in. The
pending SSH session was not restarted; no deployed services or permission changed
during this turn. Full objective remains active, with source and verification
progress rather than autonomous-trading completion.

## Corrected verification in flight

Source 70aabc9eef7898df75b4760faa75da1012d74206 is in CI 38781016512.
Thirteen jobs, including lint and cargo-deny, completed successfully; tests job
116580915820 remains authoritatively in progress at this checkpoint. Do not push
over it. Its four generic shards report two caught and two unviable; do not count
the unviable results as executed coverage. The full refusal suites are not yet
claimed successful. Linux release 38781098632, build job 116581201476, is also
confirmed in progress for the same source commit. No artifact has been installed.

The pending SSH session ended with an authoritative connection timeout. No SSH
command was restarted merely because observing it took time. Owner completion
of the fresh Tailscale SSH sign-in is still pending. Next turn should inspect
these same CI/release handles, fix any actual failures, read the executed exact
regression outputs and artifact receipts, then prepare the coordinated upgrade.
This turn made source and verification progress; the full goal remains active.

## Release receipt verified

Release 38781098632 completed successfully for
70aabc9eef7898df75b4760faa75da1012d74206. Downloaded artifact directory is
workstation TEMP/radar-refusal-release-38781098632. BUILD-INFO records that exact
commit and build time 2026-10-15T17:58:59Z. Every one of its six named binaries
was independently hashed and matched the manifest. The paper reader radar hash is
47b0448dd16b89818bcf65551745814161cc7ac082e1ff9618c143b409c30a3f;
radar-serve is bc2e75f439d10dd1209173ddef0aa5091743cb6f120b557c77756e5f3d47bc1e.
Downloading and hashing are not local execution; no workstation jobs ran.

CI 38781016512 tests job 116580915820 remains confirmed in progress after the
release completed. Do not restart, cancel, push over it, or treat its duration as
failure. No new test success is claimed. A coordinated upgrade must replace the
reader and pin in paper-control.py together with paper-supervisor.py, validate
the currently installed old hashes, require paused/unconfigured owner authority,
preserve broker/supervisor state, retain old bytes and start with a paused check.
The one-time installer must not be rerun against existing state. Runtime access
still needs the outstanding Tailscale sign-in. This continuation verified the
release artifact and waited on a confirmed live test handle; the goal is active.

## Completed verification

CI 38781016512 completed all fourteen jobs successfully at source 70aabc9.
Tests job 116580915820 reports 2863 Rust tests and 138 Python tests, all 27
supervisor/controller/bridge reapplications caught, all three new exact Rust
refusal reapplications caught, and systemd unit verification. Its output was
read after completion. The owner finished Tailscale authentication; the existing
SSH alias and fixed private inspect helper now work. Independent October 15
inspection again shows successful paused ticks, separate principals and active
controller/socket/timer. No order permission or service changed in that read.

Follow-up branch feat/alpaca-paper-refusal-upgrade carries the protected upgrade
work and these documentation receipts, stacked on the verified remote PR 423
source 70aabc9. The local documentation-only commits were not pushed over its
live test run. See the next upgrade plan for outstanding verification/deployment.
