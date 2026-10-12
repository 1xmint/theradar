<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0052 — Isolated scheduled paper maintenance worker

**Status:** prepared for GitHub verification; not provisioned or active.

Branch: feat/alpaca-maintenance-worker, base a92807a. The previous turn added
the actual maintenance CLI. During this turn its repaired source 15a5546
passed 2841 Rust and 59 Python tests and all prior exact regressions on GitHub
38314374131; only a test-helper lint diagnostic failed. Four mutation artifacts
showed three caught, five compiler-unviable and no survivors/timeouts. Parent
a92807a repairs that diagnostic and adds four exact ordering reapplications.
GitHub 38315068491 then passed all fourteen jobs, 2841 Rust/59 Python tests,
all four new exact ordering checks and prior regressions. All four mutation
artifacts inspected: three caught, five compiler-unviable, no survivors/timeouts.
Parent proof is recorded on this child branch to preserve the verified head.

## Work

- [ ] Verify fixed-path worker, protected owner configuration and minimal
  broker credential environment; no model or new-order subprocess exists.
- [ ] Verify exclusive ticks, storage bounds and refusal to recreate missing
  history. A missing provisioned journal requires operator recovery/bootstrap.
- [ ] Verify response identity/environment/authority checks, seven temporary-file
  tests and eleven executed worker mutations on GitHub.
- [ ] Verify service/timer syntax on GitHub.
- [x] Inspect parent proof: gh run view 38315068491, tests log and four artifacts
  at source a92807a; all required checks passed.
- [ ] Provision a verified binary, protected config and durable journal genesis
  before installing/enabling the worker. No owner sudo command is ready yet.

The actual entry point is deploy/maintain-alpaca-paper.py with no CLI arguments.
It reads /etc/radar/alpaca-maintenance.json through bounded nonblocking no-follow
regular-file reads. Root ownership and no group/other write permission are
required. The exact schema pins the account UUID, one to ten activity pages and
observe/cancel_working mode. The example defaults to observation and a placeholder
UUID. No host, executable, model or arbitrary command can be selected in config.

The worker exclusively locks its fixed state directory, refuses nonregular
entries and missing orders.jsonl, and reserves four MiB below a 256 MiB cap and
4096-file limit. It invokes only the fixed maintenance CLI with selected broker
credential variables and fixed PATH. The CLI owns the actual journal throughout
checkpoint and any cancellation. Stop requests still cancel at most one working
retained order per tick and never recreate uncertain effects. No missing history
is treated as a fresh account and no quota cleanup discards reservations.

The systemd oneshot uses DynamicUser, a private 0700 StateDirectory, strict
filesystem/home/kernel protections, empty capabilities, core dumps disabled,
128 MiB memory, sixteen tasks and 25% CPU. The worker has a 75-second subprocess
deadline inside a 90-second unit deadline. The timer waits one minute after a
tick becomes inactive rather than trying to catch up during a slow tick.
Credential variables are present only in this private broker process tree; no
model adapter or bridge is invoked. stdout reports selected counts and request
status, never vendor bodies or captured stderr. Cancellation request acceptance
does not prove cancellation or reconciled fills. Memory limits additionally bound
captured subprocess output; the four-MiB response check is after capture.

## Handback

Prepared seven tests and eleven targeted mutations for protected config, command
selection, credential filtering, journal preservation, quotas and response trust.
The mutation harness requires seven executed tests and rejects syntax/import
failures as kills. The Rust test floor rises only to the already observed 2841.
All checks run on GitHub. New unit files are source only: no account reads,
cancellations, service changes or deployments occurred during implementation.

Next inspect this branch and parent CI, then prepare one verified provisioning
path with deliberate journal genesis. Do not install a timer against missing
history or count this recovery-only worker as an autonomous entry system. Entry
generation, execution capacity/cost semantics, effective website handover,
learning/evaluation and eligible real-money activation still require work.
