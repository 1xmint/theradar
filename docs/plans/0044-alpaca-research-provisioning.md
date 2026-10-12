<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0044 — Provision the verified research worker separately

**Status:** GitHub-verified setup staged; awaiting owner sudo execution. No installation verified.

Branch feat/alpaca-research-setup from e748281, stacked on PR 404. Previous
goal turn made concrete progress: campaign worker verified, release built and
staged. Parent documentation run 38286203010 passed all 14 jobs, inspected;
its branch was not moved while that run was live.
The fully autonomous trading goal remains active; this is research provisioning.

## Checklist

- [x] Separate research binary/worker from the running hash-pinned recorder.
- [x] Verify staged binary bytes and exact installer bundle before effects.
- [x] Refuse unsafe directories, credentials and existing installation files.
- [x] Use initial service probe before enabling timer; retain partial installs.
- [x] Add GitHub tests and eight exact installer mutation reapplications.
- [x] Inspect GitHub results and prepare owner-terminal setup command:
  4c733c5, run 38286704736.
- [ ] Independently verify installed hashes, timer and real research attempts.

## Handback

Code proof: 4c733c56efbad892beaa163bd7f8533a18944ecf, CI 38286704736
passed all 14 jobs. Tests job 114936691467 passed 2817 Rust and 41 Python tests,
including five new setup tests. Eight installer, twelve campaign and 32 previous
Python mutations caught; both exact subscription regression checks passed. Both
unit pairs verified. All four Rust shard logs explicitly report no changed Rust
source; no mutation catches inferred from those skipped workloads.

Packaged exact committed deploy bytes (git show) in a plain four-member tar:
/tmp/radar-research-bundle.tar SHA-256
0cc7a5db48615b63873dea622292255cc6f27b481d4bab2e18a7e54373b0ffff.
Staged installer /tmp/radar-alpaca-research-setup.py SHA-256
dbe1203f80cc88e756ac90711309e440ea4287f971831676251003cb872b410a.
Independent SSH hashes match both and the previously staged release CLI. Old
recorder timer remains active. New research library, config and units were absent
when inspected; existing paper credentials stat root:root 0600, contents unread.

Owner-terminal command (already requested; private sudo password required):
`sudo python3 /tmp/radar-alpaca-research-setup.py 0cc7a5db48615b63873dea622292255cc6f27b481d4bab2e18a7e54373b0ffff`.
Await its output, then independently inspect installed hashes, state/config
ownership, timer and actual per-slot records. No installation, real model call,
broker order or performance result is claimed yet. Draft PR 405 is attached,
stacked on PR 404. This handback changes documentation only; later documentation
CI is separate from the code proof. This turn made concrete progress; the owner
sudo step is newly pending, not a repeated three-turn blocker. While pending,
complete cohort outcome collection/scoring remains available independent work.

Installer pins the CLI from release run 38285575372, code 99085c7, SHA-256
12efd7e4fd958138840dc0056f84fe6ba5690cb7de01ce02065131e6cb2112d0.
It reads staged bytes once with no symlink following and installs those verified
bytes exclusively under /usr/local/lib/radar-research. The worker and service
now use that separate location. Existing market recorder/helper hashes remain
valid. No production website restart or broker order authority changes.

The reviewed example becomes twelve hypothetical research slots, starting two
minutes after setup, at ten-minute intervals. This uses the owner's existing
unlimited subscription authorization, not a paper account capital mandate.
Initial probe normally freezes future slots; a slow setup/clock jump may reach
a due slot. All attempts remain subject to worker/CLI quote validation. A probe
exit is not proof of real model success or profitability. The timer polls every
fifteen seconds after completion; the bridge keeps its existing authentication.

Setup accepts the exact bundle hash, verifies members and the existing root-only
paper credential file without reading its contents, and checks the bridge socket
is active. The new config is root-owned readable public research inputs. Files
are synced and linked exclusively; reruns refuse existing targets. Failed or
partial installs require explicit inspection, never blind overwrite or cleanup.
No credentials/model output are printed. The owner must execute sudo privately;
admin authorization is already present, but the agent cannot supply their password.
Outcome collection/scoring, execution evidence and supervision, and site handover
remain unfinished. No live trading or positive expectancy claim.
