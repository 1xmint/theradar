<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0044 — Provision the verified research worker separately

**Status:** Implementation awaiting GitHub; no installation or activation.

Branch feat/alpaca-research-setup from e748281, stacked on PR 404. Previous
goal turn made concrete progress: campaign worker verified, release built and
staged. Parent documentation run 38286203010 must finish before moving its branch.
The fully autonomous trading goal remains active; this is research provisioning.

## Checklist

- [x] Separate research binary/worker from the running hash-pinned recorder.
- [x] Verify staged binary bytes and exact installer bundle before effects.
- [x] Refuse unsafe directories, credentials and existing installation files.
- [x] Use initial service probe before enabling timer; retain partial installs.
- [x] Add GitHub tests and eight exact installer mutation reapplications.
- [ ] Inspect GitHub results and prepare owner-terminal setup command.
- [ ] Independently verify installed hashes, timer and real research attempts.

## Handback

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
