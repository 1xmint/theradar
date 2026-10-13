<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0054 — Deliberate paper initialization and observation provisioning

**Status:** source prepared; GitHub verification pending; not installed.

Branch feat/alpaca-execution-initialization, base 37b56014. Parent CI
38346644746 passed all fourteen jobs, 2842 Rust/66 Python tests and seven
executed maintenance regressions. Release 38346705855 provides the pinned
reader, SHA-256 3f21fc1ae3b54b2333a5b809eda916c296be0948d69dba1f9da618adec9733d2.
Parent proof is recorded here without changing its verified branch head.

## Work

- [x] Separate one-time initialization from recurring maintenance.
- [x] Preserve a durable no-retry fence before any checkpoint attempt.
- [x] Prepare pinned, exclusive fresh-install setup and shared private units.
- [ ] Inspect GitHub tests/build/lint, executed mutation checks and unit syntax.
- [ ] Package and independently verify exact reviewed installation bytes.
- [ ] Owner sudo initialization/probe/timer installation; inspect actual results.

The caller is deploy/setup-alpaca-execution.py. It reads a hash-bound exact
six-member bundle and pinned reader, checks credential metadata without reading
contents, and installs exclusively into protected root-owned directories. It
refuses prior library, both public/private systemd state aliases, existing
targets and dangling symlinks. Configuration is exact, observation-only and
pins the paper account UUID plus bounded pagination. Partial installs remain
for explicit inspection; there is no automatic upgrade/delete/retry path.

The initialization service calls deploy/initialize-alpaca-paper.py with no
arguments. It requires empty private storage and protected observation config,
exclusively creates and syncs a persistent fence, then calls the fixed
alpaca-cash-checkpoint command once with only broker credential variables and
fixed PATH. It validates pinned paper metadata, empty initial local order/claim
maps and this invocation's checkpoint times, requires retained nonempty regular
journal storage and publishes completion exclusively. These empty maps describe
local retained intents, not a broker-wide statement that no orders ever existed.
The underlying verified CLI establishes selected opening cash/share observations;
it does not establish full historical cost or profit. Any failure or interruption
retains its fence and partial history. Never delete them to retry genesis.

Initialization and maintenance share User=radar-paper-execution with DynamicUser,
private 0700 StateDirectory and 0077 umask. This follows the
[systemd v255 execution specification](https://raw.githubusercontent.com/systemd/systemd/v255/man/systemd.exec.xml)
for explicit dynamic names and private state ownership; the VPS reports v255.
Filesystem/kernel protections and CPU/memory/task/deadline bounds remain.
The installer starts initialization, probes maintenance and only then enables
the timer. Either failed probe prevents timer enablement. The installed config
does not request cancellation, and no model or new-order path is invoked.

Six initializer and eight provisioning tests use isolated temporary stores,
fake broker CLI/systemd calls and no credentials. Targeted mutations reapply
thirteen initializer and eleven setup mistakes; their harness requires the
actual six/eight executed test counts and refuses syntax/import failures as
test kills. Existing worker environment tests still cover the extracted shared
minimal environment helper. No local jobs: all verification belongs on GitHub.

## Runtime prerequisite verified

Owner contextual setup succeeded. Independent SSH inspection at October 13
04:56 UTC found the timer active/waiting, successful service and twelve future
slots. Reader/worker hashes match the verified package; config and frozen
campaign share 0acdaa71288bf38664a5eb59ae6e07455b0078cdfd8fe756209822be31e86495.
The campaign begins today October 13 at 10 AM Eastern. Earlier research history
remains retained. This is scheduling proof, not outcomes or execution authority.

## Handback

Prepared deliberate initialization and observation provisioning. Verification,
packaging and actual owner sudo setup remain pending. No paper orders, live
trades, website deployment or service changes occurred from this implementation.
The next concrete owner command must be staged and verified before asking for
their private sudo entry. Keep parent heads stable while child checks run.

Remaining full-goal work: execution capacity/cost semantics currently refuse
all submissions, the supervisor still needs proposal-to-risk-to-submit and
exit/revocation integration, and the site needs effective Alpaca balance and
manual/agent handover controls. Learning/evaluation must measure held-out and
forward outcomes after costs; the scheduled context experiment is not a learned
edge. Eligible real-money setup remains separate. Recovery observation alone
does not complete fully autonomous trading.
