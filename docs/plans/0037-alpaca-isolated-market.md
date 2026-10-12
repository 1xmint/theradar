<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0037 — Actual IEX capture and isolated replay

**Status:** implementation awaiting GitHub verification and actual market capture.

Branch: feat/alpaca-isolated-market, based on 799b85c. Previous goal turn made
verified progress: owner supplied successful isolated paper account output.
Parent run 38265093637 passed all 14 jobs, inspected with gh run view --json jobs.

## Checklist

- [x] Use existing verified CLI capture/replay commands rather than duplicating
  provider parsing or raw HTTP authentication in deployment glue.
- [x] Add explicit --market helper mode with fixed SPY/QQQ IEX command and fresh
  capture filenames. StateDirectory is persistent, private and systemd-managed.
- [x] Require successful capture metadata and integer u64 availability before
  replaying. Reject unknown modes, injected paths, wrong feed/authority and
  missing/malformed availability. Replay uses the original host watermark.
- [x] Replay in a separate transient unit: remove credential EnvironmentFile,
  explicitly unset broker credential variables and isolate its network.
- [x] Add actual caller and capture-to-replay tests, and expand targeted Python
  mutation checks to exercise the new boundary behavior.
- [ ] Inspect GitHub results; upload exact verified helper with matching hash.
- [ ] Owner runs protected market observation; inspect actual IEX coverage,
  prices, timestamps, conditions and displayed sizes as observations.

## Handback

Code verification pending. No workstation jobs. Existing verified reader release
from 38263691902 can be reused: no Rust change in this branch. The helper retains
all previous protected account mode behavior, including root-only credential
checks and exact executable hash staging. The owner command adds --market.

Capture and replay are separate credential-bearing and offline processes.
Metadata validation connects actual capture output to the replay watermark.
Replay reports partial IEX coverage and execution_authority=false; quote units,
conditions, executable capacity and realistic costs remain unproven until actual
evidence supports them. A replay at historical availability is not a fresh
execution quote. Capture errors stop before replay; completed files are retained
even if subsequent replay fails. No automatic order retry or permission.

Systemd 255's [official execution documentation](https://github.com/systemd/systemd/blob/v255/man/systemd.exec.xml)
specifies that StateDirectory persists across unit shutdown, is protected under
/var/lib/private with DynamicUser, and has ownership adjusted for the service.
The two transient units run sequentially; this is an administrator observation,
not a concurrent continuous collector. The existing CLI syncs the capture inode
before publishing; this helper adds no claim of directory power-loss durability.

Next: continuous recording with storage/resource bounds and coverage diagnostics,
time-gated research/forward evaluation, protected execution supervisor, website
policy/handover/revoke and cancellation/recovery. Trading stays disabled.
