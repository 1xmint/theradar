<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0038 — Bounded continuous IEX research recording

**Status:** Implemented; GitHub verification and administrator installation pending.

Branch: feat/alpaca-market-recorder, based on e160788. Parent run 38266721764
passed all 14 jobs. Owner supplied successful actual SPY/QQQ capture and replay;
details are in plan 0037. That proves feed access, not executable capacity.

## Checklist

- [x] Reuse the verified reader for fixed SPY/QQQ IEX snapshots.
- [x] Separate private persistent dataset, one worker at a time, no deletion.
- [x] Reserve 1 MiB before each read; pause at 256 MiB or 65,536 files.
- [x] Label absent symbols/quotes unknown and preserve raw response and watermark.
- [x] Root installer verifies bundle bytes, reader hash, credential permissions
  and installation directories; refuses overwrites and probes before timer enable.
- [x] Add fake-provider tests and targeted behavioral mutations on GitHub.
- [ ] Inspect all GitHub jobs and mutation results; upload verified bundle.
- [ ] Owner installs; inspect actual capture diagnostics and timer state.

## Handback

No workstation tests or builds. GitHub private-setup runs recorder tests,
targeted quota/provenance/installer mutations and systemd unit verification.
The unchanged CLI release from run 38263691902 remains pinned to SHA256
1ac8285579e39d474aa1eaaccba2a0b480306d089a45eb6b43cf737a98fe8dc0.

The timer samples roughly once a minute, including outside market hours. It is
a research snapshot series, not a tick feed. Missing symbols/quotes are unknown;
present quote objects are not automatically fresh, valid or executable. IEX
is partial coverage; raw size units, conditions and execution costs still need
evidence. Original availability gates later replay, not exchange time alone.

The private StateDirectory is separate from manual observation captures. A
DynamicUser worker inherits paper credentials from systemd, has no order caller,
and reports execution_authority=false. The credentials themselves are not
read by the installer or supplied to the website. The worker has 128 MiB memory,
16 tasks, 25% CPU quota and a 45-second service timeout. Its CLI call times out
after 20 seconds. Completed captures and unknown files count toward the quota;
no automatic deletion or reset. Quota pause returns a diagnostic without a
provider call, and the timer continues inexpensive quota checks.

Installer first starts the worker and requires successful exit before enabling
the timer. A successful pass can report storage_limit on an existing full
dataset; inspect diagnostics before claiming data collection. A failed probe
leaves files installed and timer disabled. It does not roll back files; resolve
the cause and start the installed worker explicitly before enabling the timer.

Next: time-gated research and forward evaluation, validated execution inputs,
protected broker supervisor, and website policy/handover/pause/revoke. Owner
chooses manual values or Agent decides per option; adaptive risk remains the
default direction with an optional daily loss stop. No autonomous activation,
orders or profitability claim follows from a recorder deployment.
