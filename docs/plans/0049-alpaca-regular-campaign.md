<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0049 — Keep contextual research inside regular market hours

**Status:** Source prepared; GitHub verification pending. No deployment.

The context change at e5ecb06 passed all 14 jobs in 38309801068: 2835 Rust,
47 Python tests, 39 caught mutations, nine compiler-unviable, no survivors.
All four diagnostic artifacts and exact regression outputs were inspected.
The previous turn pushed that repair and started release 38309896074. CI
completed before this branch was created; release completed during this turn.
Goal remains fully
autonomous trading, including supervision, effective website handover and
cost-adjusted forward outcomes; this campaign is one remaining component.

## Work

- [x] Add an actual read-only broker-clock CLI caller using the existing session validator.
- [x] Require a fresh open session with room for capture, model and full outcome horizon.
- [x] Call that guard before capture/model for snapshot-context-v1 research attempts.
- [x] Provide fixed separate context-campaign reader, config and state paths.
- [x] Preserve legacy quote-only campaign behavior and its immutable history.
- [ ] Verify new tests, all GitHub jobs and retained mutation diagnostics.
- [ ] Provision a verified new artifact and future regular-session campaign.

`radar alpaca-research-session --horizon-ms <positive-ms>` reads only the fixed
paper host's /v2/clock with the paper credential reader. Its report gives no
trading authority. The same strict UTC/offset parser validates clock freshness
within five seconds and an open session. The guarded outcome must be strictly
before the broker close after allowing the worker's 20-second capture and
100-second model deadlines. Holidays/early closes come from broker evidence.

The worker checks this report for context experiments before market capture or
model calls. Failure is retained as an ordinary failed scheduled slot; there is
no retry or silent rescheduling. The model environment still excludes broker
credentials. Existing quote-only experiments retain their two-command flow.
After model completion, the retained decision's outcome watermark is checked
against the guarded close again, so process/setup overhead cannot silently push
an accepted outcome past close. A late/unknown horizon keeps its partial files
and failed slot, without adopting the decision as valid campaign output.

The fixed --context-campaign worker option selects a separate installation,
config and state store. It does not accept arbitrary reader/config/store paths.
Separate service/timer source preserves existing hardening and the earlier
campaign. Installation and an explicit future schedule still require reviewed
provisioning; no source-only unit is represented as running.

## Handback

Prepared one Rust actual-collector test for timing/closure/failure boundaries,
plus four Python real-dispatch tests for guard order, invalid reports, credential
separation and fixed entry paths. Run tests/build/lint/mutations on GitHub only.
Do not deploy the parent e5ecb06 release for this worker: it lacks the session
guard command. Next: inspect this branch's CI, produce its release, then stage
the separate root provisioning and frozen campaign. No broker orders enabled.

Initial run 38310848091 was still live when two runner-reported formatting
hunks were repaired. Review also added a retained-completion horizon check and
boundary tests. Wait for this run to finish before pushing these repairs.
The Python mutation harness's fixed executed-test count also needs to rise from
nine to thirteen with the new dispatch tests. Nine new targeted mutations cover
guard omission, report fields and the post-completion close boundary. Rust
mutation shards inspected: two caught, three compiler-unviable (main/run dead
code and the research stub's unused arguments); shard 3 selected zero mutants
and therefore produced no artifact. These are not skipped mutation diagnostics.
The run then completed: Rust 2836 and Python 50 passed, but the mutation harness
refused its old nine-test count after actually running twelve tests and catching
the first injected bug. Repair count is thirteen, including the new completion
test. All remaining initial jobs passed except the two formatting hunks. No
workstation jobs, production changes or orders.
