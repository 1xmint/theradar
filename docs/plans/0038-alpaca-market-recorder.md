<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0038 — Bounded continuous IEX research recording

**Status:** GitHub verified and uploaded; administrator installation pending.

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
- [x] Inspect all GitHub jobs and mutation results; upload verified bundle.
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
The byte cap measures file lengths, not filesystem allocation or metadata;
physical disk use can be higher. The independent file cap bounds small-file
growth. This is an application quota, not a dedicated filesystem partition.

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

Reference check for the next execution-input work: Alpaca's current
[market-data source guide](https://github.com/alpacahq/alpaca-skills/blob/main/skills/broker-api/market-data/SKILL.md)
states sizes are shares after 2025-11-03, while its
[stream schema](https://docs.alpaca.markets/us/docs/real-time-stock-pricing-data)
still labels them round lots. The
[dated change notice](https://docs.alpaca.markets/us/v1.1/changelog/marketdata-bid-and-ask-size-display-change)
explicitly covers CTA/UTP. This strengthens the shares interpretation but does
not independently verify this IEX REST capture's normalization or prove routed
order capacity. Preserve raw fields and resolve that distinction before using
them as a risk input. No multiplying by 100 from the older schema by default.

Systemd's [timer source documentation](https://github.com/systemd/systemd/blob/v255/man/systemd.timer.xml)
defines OnUnitInactiveSec relative to last deactivation and permits a past
OnBootSec to fire immediately when enabled. The timer can therefore capture
again soon after the installer's probe; each invocation uses a fresh filename.

Verification: `gh run view 38268824054 --json jobs` at
c99bb62321a7b7d75ab2dd04d0707bf36178d1fb passed all 14 jobs. Read tests job
114877715245: 2,804 Rust tests, 27 Python tests, existing 17 plus new 15 targeted
Python mutations all caught by executed tests, and systemd-analyze verify passed.
No targeted survivors or import/compiler failures. All four Rust shards reported
no changed Rust source; their success is not Python mutation coverage.

Packaged committed bytes using checkout-index with core.autocrlf=false and tar,
without running local jobs. Archive lists exactly the three expected top-level
files. Uploaded bundle and installer, then compared local Get-FileHash and
remote sha256sum. Bundle SHA256:
6f1840d571261aacd4c0bf03e04f1773792e5045a0a1fa75128f7b052c63a3d3.
Installer SHA256:
f8e1fa5ec9c21fd880787a386ea6af6cb5f96a2511a16d72e268c582f5b58d84.
Direct SSH stat confirmed installed reader root 755 and credentials root 600;
timer was absent. No credential contents read. Owner received the exact sudo
installer command with the bundle hash. Installation and continuous capture
evidence remain pending. This handback update is documentation only; code proof
is c99bb62. Draft PR 399 is stacked on PR 398 and attached to this chat.
