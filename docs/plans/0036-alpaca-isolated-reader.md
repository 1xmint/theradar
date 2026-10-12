<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0036 — Isolated paper account observation

**Status:** GitHub verified and uploaded; owner reports successful isolated account observation.

Branch: feat/alpaca-isolated-reader, based on verified 2a25e15. Owner confirmed
private paper credential provisioning; values never entered model context.

## Checklist

- [x] Inspect VPS runtime: systemd 255, protected /usr/local/lib root-owned 755,
  no Radar CLI installed. Noninteractive sudo requires an administrator password.
- [x] Start GitHub release build 38263691902 at verified 2a25e15; no server build.
- [x] Add administrator staging of exact SHA256-verified executable bytes into
  root-owned /usr/local/lib/radar-paper/radar. Reject artifact symlinks/nonfiles,
  mismatched hashes, unsafe directories and destination symlinks.
- [x] Add one-shot DynamicUser reader invocation with fixed alpaca-read command,
  root-only credential file loaded by systemd, clean installer environment,
  restricted filesystem/privileges, bounded time/memory and disabled core dumps.
  No timer, listener, sudo rule, service enablement or trade submission.
- [x] Add actual caller, hash/permission/symlink tests and targeted Python
  boundary mutation checks to existing GitHub private-setup recipe.
- [x] Inspect CI and release jobs; upload verified reader/script, match hashes.
- [x] Owner runs private administrator command; inspect selected account report.

## Handback

No credentials read by this agent. Setup and the selected Radar runtime report
are owner-provided evidence, detailed below. A temporary distinct
reader user gets broker environment variables because the existing CLI uses
them; website/model users do not. This does not claim a compromised reader
cannot misuse broker credentials: command isolation is the boundary here, and
this is an operator-initiated reader, not an authenticated trading supervisor.

`deploy/read-alpaca-paper.py <verified-reader-sha256>` requires sudo. It validates
credential-file type/ownership/mode without reading its contents, stages verified
bytes from /tmp/radar-paper-reader.new, then starts one transient systemd unit.
Only selected account/position/clock fields reach stdout. No broker account
number or credential echo. CLI evidence follows plan 0034's verified code.

Targeted Python mutations alter hash equality,
directory ownership/write checks, symlink refusal, credential permission/type,
DynamicUser and fixed command selection. Mutation failures must come from actual
executed test cases, not import/compiler failures. Rust shards may have no
changed source and are not claimed as Python coverage. No workstation jobs.

Further work remains: actual market data capacity/conditions/cost/session facts,
authenticated execution supervisor, owner policy/handover/revoke, cancellation
and recovery, website integration and forward evaluation. Paper observation
does not enable trading or demonstrate an economic edge.

Verification: `gh run view 38264321375 --json jobs` passed all 14 jobs at
f85b673cb9d5ddb164b629c74ed5e878d7b17767. Inspected tests job 114862511138:
2,804 Rust tests, 16 Python tests (all four reader tests), and eight caught
targeted Python mutations. No targeted survivors or import/compiler failures.
All Rust shards reported no changed Rust source, not Python mutation coverage.

Release run 38263691902 passed every step at verified parent
2a25e15f1d839246e9cb3023f87797be8251bc2a. Downloaded radar-linux-x86_64,
inspected BUILD-INFO.txt and matched its reader digest against local Get-FileHash
and remote sha256sum after upload. Reader SHA256:
1ac8285579e39d474aa1eaaccba2a0b480306d089a45eb6b43cf737a98fe8dc0.
Uploaded reader helper from verified f85b673; local and remote SHA256:
919d5a1683bbca426ace51f2dbaae87db9d1d3dafbcf16323aecd805cab6e0bf.

Owner received the exact sudo command. Noninteractive sudo refused because a
password is required; no privilege bypass or credential access attempted.
Owner subsequently supplied the successful reader output: active USD paper
account, all three restriction flags false, cash/equity 100000, buying power
400000 and no positions. Broker clock reported open at
2026-10-12T10:42:32.207923767-04:00; host read interval 1791816151..1791816152
Unix seconds. The report explicitly states atomic_snapshot=false and
execution_authority=false. systemd reported successful exit 0, runtime 393ms,
CPU 50ms and peak memory 1.1M. This is owner-provided runtime output, not an
independent attestation of every sandbox property. Do not treat buying power
as an owner capital budget or authorize leverage from it. Credential values
and the account identifier are not recorded in this plan.

No production web restart, orders or activation. Next work is actual market
recording and selected quote evidence with explicit coverage/availability.
This evidence update is documentation only; code evidence refers to f85b673.
