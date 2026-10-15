<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0064 — Accurate paper readiness copy

**Status:** GitHub-verified, deployed through fixed script and authenticated rendering verified; trading remains paused.

Branch feat/alpaca-paper-readiness-copy starts from verified PR 423 source
70aabc9. It is independent of the protected upgrade in PR 424, which is verified
and awaiting owner root application. No upgrade payload or reader pin is changed here.

The setup footer previously asserted that the supervisor still needed
installation and that execution was disabled, regardless of runtime or owner
handover. October 15 fixed-helper inspection proves a supervisor is installed
and running paused. Use a neutral readiness heading and describe the separate
operational verification needed before unattended trading. Keep the distinction
between owner permission and evidence of a running agent, and keep Solana
settings as separate drafts. This static copy does not claim a healthy lifecycle,
active trading, learned edge or profitability.

Update the existing render expectation for the renamed heading. No new test is
needed for this reversible copy change; existing frontend checks run on GitHub.
No workstation tests/jobs ran. Deployment uses a GitHub artifact and fixed
radar-deploy with hash and health-build checks; the authenticated rendering is
verified in Brave below. The broader autonomous-trading goal remains active.

## Verified deployment and handback

Source 1dcf60e2ea592d447c4498f040f1744d21a6deae passed all fourteen jobs in
CI 38785773799; every final job status was inspected. Web job 116597265978
reports 365 tests and all nine existing paper-panel reapplications caught.
Linux release 38785790062 succeeded for that exact commit; all six downloaded
binaries match BUILD-INFO. Only radar-serve was deployed. Its hash is
dcfca7a5bb43e1dac5bccc36863114e9310a912b8c67bfdf460c8aaa4cb8ec56.

Before publishing to fixed /tmp/radar-serve.new, its old regular, nonsymlink
artifact was checked against the known old 8c5b1408 hash. The new artifact was
published atomically and rehashed, then sudo -n radar-deploy returned active.
Installed website hash matches the release, and localhost health reports build
1dcf60e2ea592d447c4498f040f1744d21a6deae and status ok. User/Group guardian,
SupplementaryGroups radar-paper-ipc, MemoryMax 805306368, ProtectHome read-only,
ProtectSystem strict and NoNewPrivileges yes are preserved. Health's legacy
trading feature flag is not autonomous Alpaca permission.

The old Brave task-tab binding had expired; a new task tab in the same Brave
browser (3), ID 1377638513, binding readinessTab, reached Cloudflare sign-in.
One normal provider click used the existing session and returned to Radar; no
credential was retrieved, entered or moved. A transient AX timeout was resolved
through documented screenshot observation, followed by a fresh full AX tree.
The authenticated page shows the new readiness heading and lifecycle text,
ChatGPT Linked, simulated cash/equity 100000 USD, zero positions, regular session
Open and handover disabled (broker clock 2026-10-15T14:43:37-04:00). No preference
was saved and Enable was not clicked.

Proof images are workstation TEMP/radar-readiness-1dcf60e.jpg (full page) and
TEMP/radar-readiness-footer-1dcf60e.jpg (visible footer). The task tab is marked
for handoff; mark it again when continuing. No local tests/builds/jobs ran.
PR 425 is attached and its description records verified/deployed behavior.

The separate protected rejection-recovery upgrade in PR 424 is fully verified
and staged with matching hashes, but owner root application output is still
pending. Existing installed reader/controller/supervisor hashes were still the
old versions at this turn's initial check; do not run or retry the installer on
the owner's behalf. Continue from its exact requested command and independently
verify new public hashes and paused runtime after success. Full paper order
lifecycle, adaptive learning and performance after costs remain unproven. This
turn made deployment and authenticated runtime-verification progress; the full
autonomous-trading goal stays active.
