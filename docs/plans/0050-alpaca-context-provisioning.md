<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0050 — Provision the separately scheduled contextual campaign

**Status:** GitHub verified and files staged; owner sudo setup pending. Not installed.

The previous turn repaired the session worker and its mutation harness. Parent
source 7b13b49 passed CI 38311232531 (all 14 jobs, 2836 Rust/51 Python tests,
21 campaign mutations and all prior exact regressions) during this turn.
Release 38311227624 and independently hashed downloaded reader match that
source. Parent remains stable while this separate installer is verified.

## Work

- [x] Preserve the old installer and all existing recorder/campaign paths.
- [x] Pin the context installer to the verified session-guard reader artifact.
- [x] Require an exact-member/hash bundle and explicit future campaign start.
- [x] Refuse overwrites, unsafe directories and non-private credential metadata.
- [x] Add installer/schedule tests and targeted regression reapplications.
- [x] Inspect GitHub checks and actual targeted mutation results.
- [x] Stage independently verified files/bundle and a concrete future start.
- [ ] Run owner sudo setup, then inspect installed hashes, timer and frozen plan.

The actual provisioning caller is deploy/setup-alpaca-context.py. Like the older
installer, it uses bounded no-follow regular-file reads, exact-member tar/hash
validation and synced exclusive installation. It deliberately keeps its own
fixed target map and binary hash so upgrading the older setup cannot silently
retarget a root provisioning command. No credential contents are read.

It consumes /tmp/radar-context-research.new and
/tmp/radar-context-research-bundle.tar, installing only the new reader/worker,
context service/timer and /etc/radar/alpaca-context-research.json. The bundle's
example start must be replaced by an explicitly selected epoch-millisecond
start more than two minutes and no more than seven days ahead. Twelve slots,
ten-minute intervals/horizons and snapshot-context-v1 are required. The
service itself checks actual broker session eligibility at each attempt.
The initial tick freezes the future plan before enabling the timer.

Any failed setup can leave partial installed files; inspect them rather than
overwriting or deleting. This installer never stops the older timer or changes
the website, signer, credentials, capital policy or order authority.

## Handback

Source cf136c7ee46122f312dd634a0dad9be2453b5428, GitHub 38312040629:
all 14 jobs passed, 2836 Rust/59 Python tests, all fourteen new setup mutations,
21 campaign mutations, prior exact regressions and all three systemd pairs.
Rust shard logs report no Rust source changes; no mutation artifacts are
expected for this Python-only implementation. Existing older timer remains
active and the context installation was absent at staging inspection.

Selected October 13, 2026 10:00 Eastern (1791900000000 ms), twelve ten-minute
slots. The [NYSE schedule](https://www.nyse.com/trade/hours-calendars) lists no
October 13 holiday and regular core hours 09:30–16:00 Eastern; the broker-clock
guard remains authoritative at each attempt. Package files and independent
VPS hashes match:

- /tmp/radar-alpaca-context-setup.py: 9bf34989c2c69557b11ea6a36ae317133bc842e8b5a60df9314b947f1f051fc4
- /tmp/radar-context-research-bundle.tar: 3aa9d340884e41b5adf1b3e2edbfd1419f12d2c035fcec56d481914fd417c1f7
- /tmp/radar-context-research.new: 712a872adba5400b8744e7019af62817446a651a54bb40a57271ffeeaad0929b

Owner was asked to run the staged setup with the bundle hash in the existing
VPS terminal because sudo requires their private password. Await actual output;
do not infer activation from staging or CI. The explicit future start expires;
if setup is delayed past the required margin, prepare a new reviewed schedule
and bundle rather than retrying stale bytes. Next inspect installed hashes,
timer and frozen future denominator, then continue the order/supervision path.

Eight installer tests cover real temporary-path installation, hashes, metadata,
exact bundle members, overwrite refusal and schedule/type/strategy boundaries.
Fourteen targeted mutations must fail executed tests on GitHub. No local test,
lint, build or mutation jobs. Root setup is not requested until files and its
command are concrete and verified; the owner must enter sudo privately in the
VPS terminal. Continue toward execution supervision, effective website handover
and measured outcomes; fully autonomous trading remains incomplete.
