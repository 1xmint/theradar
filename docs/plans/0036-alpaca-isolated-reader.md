<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0036 — Isolated paper account observation

**Status:** implementation awaiting GitHub verification and owner-initiated runtime read.

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
- [ ] Inspect CI and release jobs; upload verified reader/script, match hashes.
- [ ] Owner runs private administrator command; inspect selected account report.

## Handback

No credentials read by this agent. Setup success is owner-reported evidence;
actual Radar account/positions/clock read remains pending. A temporary distinct
reader user gets broker environment variables because the existing CLI uses
them; website/model users do not. This does not claim a compromised reader
cannot misuse broker credentials: command isolation is the boundary here, and
this is an operator-initiated reader, not an authenticated trading supervisor.

`deploy/read-alpaca-paper.py <verified-reader-sha256>` requires sudo. It validates
credential-file type/ownership/mode without reading its contents, stages verified
bytes from /tmp/radar-paper-reader.new, then starts one transient systemd unit.
Only selected account/position/clock fields reach stdout. No broker account
number or credential echo. CLI evidence follows plan 0034's verified code.

GitHub tests/release pending. Targeted Python mutations alter hash equality,
directory ownership/write checks, symlink refusal, credential permission/type,
DynamicUser and fixed command selection. Mutation failures must come from actual
executed test cases, not import/compiler failures. Rust shards may have no
changed source and are not claimed as Python coverage. No workstation jobs.

Further work remains: actual market data capacity/conditions/cost/session facts,
authenticated execution supervisor, owner policy/handover/revoke, cancellation
and recovery, website integration and forward evaluation. Paper observation
does not enable trading or demonstrate an economic edge.
