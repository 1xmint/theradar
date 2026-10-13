<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0055 — Diagnose retained paper maintenance without repeating genesis

**Status:** admin helper GitHub verified, installed and exercised; JNLC activity identified; accounting repair pending.

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
- [x] GitHub 38430389176 verified a69084f; installed helper hash and exact sudo
  grant inspected; direct stop/inspect/activities/maintain calls exercised.
- [ ] Fix the demonstrated cause; verify changed behavior on GitHub before deployment.
- [ ] Restore successful observation maintenance and inspect actual runtime proof.

## Handback

No accounting repair has been made. The agent stopped the failing observation
timer through the installed helper; no in-flight worker was killed. Preserve the
existing journal, first checkpoint and initialization fence. Do not turn a
repeated setup command into a recovery path. The JNLC activity is identified;
its relationship to retained opening cash still needs controlled reconciliation.
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
and reject syntax/import failures as kills. No local tests, builds, lint or
mutation jobs. Implementation a69084ff5f7b0e545ac91d665bde8868ba2fc2f5 passed
all fourteen GitHub jobs at 38430389176, including 2842 Rust/92 Python tests,
all sixteen new administrative reapplications, prior checks and unit syntax.
All four Rust shard logs explicitly report no changed Rust files; no artifacts
or Rust test kills are inferred from those skipped workloads.

Packaged exact committed bytes; independent local/VPS hashes match:

- /tmp/radar-paper-admin.py:
  e740c7a992f7e7ac91f843c6b8a1a3fc25a4fac41e0033e9f91b62071d75e1bf
- /tmp/radar-alpaca-admin-setup.py:
  8f55942044ada32d7d05dd604b30130aa328ef94bacfd85d98170e4ea8b3dceb

Owner completed the one-time installation. Independent SSH verified the installed
helper matches the first hash, root:root 0755, and sudo -n -l lists exactly the
five new command entries. Direct passwordless stop and inspect succeeded; timer
is inactive and orders.jsonl remains a regular 162228-byte file. Genesis/fence
remain regular 110/82-byte files. Activities succeeded under its bounded private
unit. Maintain independently reproduced the same unsupported-activity refusal.
Draft PR 416 is attached, stacked on PR 415. Later documentation checks are
separate from the implementation proof.

## Actual activity and next repair boundary

The selected paper report shows stable ACTIVE USD account cash/equity 100000,
one executed JNLC net_amount=100000 dated 2026-10-12, ID
20261012000000000::dde9ccf3-cf7e-4926-aa97-6f522edcc4ff, no trade/share fields,
and exhausted pagination. [Alpaca documents JNLC as a cash journal](https://docs.alpaca.markets/us/docs/account-activities).
The report contains no keys, and no credential file contents were read by the
agent. This is consistent with a late opening-funding record, not proof of
new money or profit. Do not simply classify it as a fresh 100000 deposit or skip
unknown zero-cash events. Verify the retained opening balance and introduce a
controlled opening-history reconciliation before advancing the baseline.
Preserve all original records and keep trading authority closed. Ordinary future
cash changes must still match supported activity deltas; an acknowledgement of
opening history must not become a general reconciliation bypass.
