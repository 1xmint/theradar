<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0050 — Provision the separately scheduled contextual campaign

**Status:** Source prepared; GitHub verification pending. Not installed.

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
- [ ] Inspect GitHub checks and actual targeted mutation results.
- [ ] Stage independently verified files/bundle and a concrete future start.
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

Eight installer tests cover real temporary-path installation, hashes, metadata,
exact bundle members, overwrite refusal and schedule/type/strategy boundaries.
Fourteen targeted mutations must fail executed tests on GitHub. No local test,
lint, build or mutation jobs. Root setup is not requested until files and its
command are concrete and verified; the owner must enter sudo privately in the
VPS terminal. Continue toward execution supervision, effective website handover
and measured outcomes; fully autonomous trading remains incomplete.
