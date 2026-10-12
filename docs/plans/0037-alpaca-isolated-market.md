<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0037 — Actual IEX capture and isolated replay

**Status:** GitHub verified and uploaded; owner reports successful actual IEX capture/replay.

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
- [x] Inspect GitHub results; upload exact verified helper with matching hash.
- [x] Owner runs protected market observation; inspect actual IEX coverage,
  prices, timestamps, conditions and displayed sizes as observations.

## Handback

No workstation jobs. Existing verified reader release
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

Verification: `gh run view 38266064496 --json jobs` at
d186a69a568679a742165c6bd7afe7994579054b passed all 14 jobs. Inspected tests
job 114868414338: 2,804 Rust tests, 20 Python tests, and all 17 targeted Python
mutations caught by executed tests. No targeted survivors or import/compiler
failures. All four Rust shards reported no changed Rust source; they do not
provide Python coverage. No new release build: verified reader is unchanged.

Direct server inspection confirmed /etc/radar/alpaca-paper.env root-owned,
mode 600 and unreadable by guardian; contents never read. Installed reader and
its directory are root-owned 755. Reader and pending upload both match
1ac8285579e39d474aa1eaaccba2a0b480306d089a45eb6b43cf737a98fe8dc0.
Uploaded this verified helper and matched local Get-FileHash with remote
sha256sum: 8200c584f75cf9bf37ba6a6a099a73059b15cdc78bb719d802c6f3f05b84ab02.
Owner received the exact sudo command with --market and a request for selected
snapshot output only. Actual market report remains pending; no feed quality,
capacity, cost or executable quote claim follows from green helper tests.

This handback update is documentation only; code verification refers to d186a69.

Owner supplied successful offline replay output for both SPY and QQQ. Capture
read_started_unix_ms=1791817622619, available_at_unix_ms=1791817622835;
replay as_of equals availability. SPY bid/ask 778.29/778.33, displayed sizes
240/1560, quote time 2026-10-12T15:07:02.645102517Z. QQQ bid/ask
746.54/746.57, displayed sizes 440/80, quote time
2026-10-12T15:07:02.669247747Z. Both quotes report exchange V and condition R.
These sizes are raw provider fields, not normalized executable quantities.
Trade/minute/daily/prior-daily fields are present too. Execution authority,
consolidated-market coverage and exchange-time validation remain false.
Replay exited 0; runtime 62ms, CPU 51ms, memory peak 1020K. Retained file:
/var/lib/radar-paper-market/capture-9087d1067b8a4207a7b9463dd9b17272.json.
This is owner-provided runtime evidence. Continuous collection is plan 0038.
