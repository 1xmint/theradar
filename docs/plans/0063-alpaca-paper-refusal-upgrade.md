<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0063 — Coordinated private paper refusal upgrade

**Status:** upgrade source and boundary tests implemented; GitHub verification pending; not deployed.

Branch feat/alpaca-paper-refusal-upgrade is stacked on remote PR 423 source
70aabc9, which passed all fourteen jobs in CI 38781016512. Its Linux release
38781098632 succeeded and all six binary hashes match BUILD-INFO. The new
paper reader hash is 47b0448dd16b89818bcf65551745814161cc7ac082e1ff9618c143b409c30a3f.
The controller and fresh-install reader pins are updated together. This Python
deployment follow-up uses that verified reader, without rebuilding Rust locally.

## Upgrade boundaries

The isolated root script accepts a reviewed two-file archive hash and fixed new
reader hash, validates regular unique members and the controller pin, and checks
known original installed reader/controller/supervisor hashes. Existing owner
configuration or policies refuse this upgrade; this procedure is deliberately
limited to the current unconfigured installation. It stops supervisor scheduling,
refuses an active tick, stops the controller/socket, rechecks owner state and
retains old payload bytes before replacing the three files. Subscription bridge,
credentials, units, web service, broker genesis and supervisor state are untouched.

An initial paused supervisor tick must succeed; original broker-journal prefix
and genesis/lock bytes must remain before scheduling resumes. Failure retains
partial installation and backups; no blind retry or bootstrap. The script does
not grant signing/trading permission or restart the website. Unit tests use fake
service calls and temporary files, not proof of actual deployed lifecycle.
Fourteen exact regressions exercise root/hash/archive/pin/owner/quiescence/
history/exclusive-backup boundaries on GitHub. No workstation tests or jobs run.

## Current handback

The owner completed the fresh Tailscale check. Existing SSH and fixed inspect
now work. October 15 inspection confirms successful paused supervisor ticks and
healthy controller/socket/timer with separate identities. Public installed hashes
match the retained original bundle exactly: reader 0c421ea6475f400cda75126214725f718ed37cede13fe68d72c5f663016494f9,
controller d0bd5acc9b0fd4f62eab980f691a7a3aa92a7990b88cb656a747b9fb404621b3,
supervisor db74d454bcaf721c21dcd94daa13f05d86d411b73a25ca4981432ed538bfdc76.
No new payload has been staged or installed; no credentials read.

Next: inspect GitHub verification, package/hash the two reviewed Python files
and verified reader, stage without overwriting unknown files, verify VPS hashes,
then ask for the exact one-time root upgrade command (outside existing fixed
passwordless grants). After application inspect paused runtime and authenticated
site, correct stale installation copy and verify explicit handover and actual
paper entry/exit/cancel/reconciliation/pause. Adaptive learning and measured
profit after costs remain unfinished; the full autonomous-trading goal is active.

## GitHub handle and fixture refinement

Draft PR 424 is attached. Source 029747d8a48c9bb2dfcfe47f13fd74978fb88038
is confirmed in progress in CI 38783431746; inspect this handle before pushing.
No prior job was canceled. A subsequent source review strengthened the tar-link
fixture: it now names a valid regular target containing the expected reader pin,
so removing the link rejection must actually accept a linked payload, rather
than fail later on an invalid empty link target. This fixture-only refinement is
committed locally while the existing run finishes; it still requires its own
GitHub verification. No upgrade archive or installer has been staged on the VPS.
The current turn made source and runtime-inspection progress; no permission or
deployed service changed. Full autonomous trading remains incomplete and active.
