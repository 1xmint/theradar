<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0055 — Diagnose retained paper maintenance without repeating genesis

**Status:** unsupported cash activity demonstrated; bounded admin helper prepared; GitHub verification pending.

Branch fix/alpaca-maintenance-diagnostics, base f83a2c8. Parent documentation
CI 38350010462 passed; implementation proof remains 36ec9f2/38349170488.

## Evidence

Owner reported the setup wrapper's generic failure. Independent SSH inspection
found initialization already succeeded October 13 05:11:32 UTC (01:11 Eastern),
followed by successful recurring observation ticks with zero retained orders.
The first logged maintenance failure is 07:12:06 UTC (03:12 Eastern). At
16:06 UTC the timer was still active/waiting and the worker repeatedly failed.
The installed reader, worker and staged installer match the reviewed hashes.
The generic setup failure alone does not show which later invocation failed;
do not infer initial installation failure or repeat initialization.

Root-only state/credentials are inaccessible to the SSH user. Passwordless sudo
allows true and selected unrelated service commands, not private state reads or
paper maintenance administration. No unrelated sudo permission is a workaround.
Owner was asked to stop the observation timer and run the existing verified CLI
in a separate private transient unit, using the same dynamic user/state and
paper EnvironmentFile. This cannot create orders or perform genesis. The first
paste joined both commands into one; systemctl rejected the combined options.
A single diagnostic command then returned unsupported new cash activity;
reconciliation unresolved. The reader and journal were usable; a new unsupported
broker event blocks advancement. The specific activity type/amount is still
unknown. An isolated selected activity read was requested; the owner instead
asked to grant administrative access so the agent can perform these tasks.

Separate context research completed all twelve slots as decisions according to
its service report at 16:09 UTC. Independently read first/last retained decisions
are wait, citing insufficient context after fixed assumed costs. This proves
the scheduled model path ran; it does not establish profitability or learning.
Cost-adjusted exit scoring still needs access to the private recorder tape.

## Work

- [x] Inspect initialization, first maintenance successes/failure and installed hashes.
- [x] Owner private diagnostic returned the exact unsupported-activity refusal.
- [ ] Verify/install fixed administrative helper, then read the actual new activity.
- [ ] Fix the demonstrated cause; verify changed behavior on GitHub before deployment.
- [ ] Restore successful observation maintenance and inspect actual runtime proof.

## Handback

No accounting repair or service change has been made by the agent. Preserve the
existing journal, first checkpoint and initialization fence. Do not turn a
repeated setup command into a recovery path. The unsupported activity type and
its proper cash treatment remain unknown; do not broaden the allowlist by guess.
Continue the paper execution model, entry/exit supervisor and effective site
handover once this demonstrated runtime failure is resolved. No broker order or
live authority has been enabled.

## Owner-authorized administrative access

The owner requested direct command access instead of repeated private sudo
diagnostics. Prepared deploy/radar-paper-admin.py with five exact actions:
inspect, activities, maintain, stop, start. It accepts exactly one fixed action,
requires root, pins the paper account and protected observation configuration,
and runs fixed systemd/CLI commands with a minimal environment. Its Python
shebang uses isolated mode. No shell, path override, reset, genesis, submit,
cancel flag, credentials read or owner-risk edit is accepted.

Diagnostics execute under bounded private dynamic services. Only PID 1 loads
the paper EnvironmentFile. Maintenance uses the existing dynamic identity and
private state; the CLI owns retained history. Stop halts timer scheduling and
lets a current observation finish. Start requires observation config and a
successful service probe before enabling the timer. Inspection reports selected
unit/journal status and state-file metadata, never credential or journal contents.

deploy/setup-alpaca-admin.py exclusively installs the reviewed helper and five
exact guardian sudoers command entries. It verifies bounded no-follow source
bytes against the owner command's hash, protected root directories and absent
targets, then validates sudoers with visudo before publishing either target.
Partial installation is retained for inspection; it never overwrites a prior
grant or changes services. This is a one-time private sudo installation, not a
grant of arbitrary root execution or trading authority.

Six helper/six installer tests include actual sudoers syntax validation on the
GitHub runner. Sixteen targeted mutations require six executed tests per group
and reject syntax/import failures as kills. Verification/staging/installation
remain pending. Once installed, use sudo -n /usr/local/sbin/radar-paper-admin
activities to identify the actual event, then repair accounting and verify it
on GitHub. No local tests, builds, lint or mutation jobs.
