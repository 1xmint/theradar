<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0041 — Freeze equity research decisions before outcomes

**Status:** Implemented and GitHub-verified; no forward results or execution authority.

Branch feat/alpaca-forward-decision from 577de97, stacked on PR 401. Previous
goal turn made verified progress: cost-aware scorer passed all GitHub checks and
VPS continuous recorder was independently observed running. Parent docs CI
38279360660 passed all 14 jobs, inspected before this branch's push; parent
branch remains untouched.

## Checklist

- [x] Add actual credential-free CLI publishing a new decision record with host
  freeze time, horizon, entry quote, strategy identity, action and cost assumptions.
- [x] Bind a same-size always-enter baseline and cash baseline at decision time.
- [x] Score at exactly the frozen horizon watermark; retain unscoreable cases.
- [x] Add tests of actual commands, publication refusal, chronology and comparison.
- [x] Inspect GitHub tests, lint, builds and every mutation diagnostic.

## Handback

This is research recording, not evidence that a model has learned an edge or an
authenticated proof of a strategy's information boundary. A host timestamp and
exclusive publication prevent accidental overwrites; an operator can edit local
research files. These records never enter broker risk state or signing authority.
An unattended decision producer, cohort completeness and held-out evaluation
remain required. The whole autonomous-trading objective remains active.

Initial code 8c51aaf run 38280224921 completed: twelve jobs passed, formatting
and two unreadable_literal lints failed. Applied the exact formatter diff
captured from GitHub job 114915460486, then numeric separators from lint job
114915460377. No local formatter/test/build/lint jobs. Tests job 114915460253
passed 2,813 Rust and 27 Python tests. All four Rust mutation artifacts inspected:
36 caught, twelve compiler-unviable, zero misses/timeouts. Unviable replacements:
main causes dead code; now unused SystemTime/UNIX_EPOCH; publish unused arguments
and dead helper; publish_at unused Write/arguments; load lacks Decision Default
and leaves Read unused; outcome unused arguments; run dead helpers/unused args;
deleted creation/scoring match arms leave dead callers; Assumptions::validate
leaves quantity unused; score leaves json/arguments/helpers unused. These are
compiler refusals, not test kills. All 32 targeted Python mutations remain caught.
Correction is pushed only after the first run is terminal.

The cutoff is exactly frozen_at + horizon, not a selectable later exit. Both
baseline quantity and assumptions are fixed before publication. Publication
syncs the inode and links a new name, but does not fsync the parent directory or
attest a producer's information set. The final pre-link clock check can race the
link; it is not a cryptographic before-outcome guarantee. Unscoreable comparisons
stay unknown, including a Wait choice when its matched baseline cannot be priced.
Next: an unattended producer and complete cohort evaluator; execution inputs,
protected recovery/cancellation and effective website handover remain unfinished.

Code proof: 06a06fd756e83fd5917707414e42afb382bae0e9, GitHub run
38280825246 passed all 14 jobs. Inspected every job and tests job 114917406637:
2,813 Rust tests, 27 Python tests, all 32 targeted Python mutations caught,
systemd unit verification passed. Downloaded all four mutation artifacts and
inspected each unviable compiler log: 36 caught, twelve unviable for the same
reasons above, zero misses/timeouts. No test-only mutation exclusions added.
The process test uses synthetic future exit fixtures to exercise the offline
comparison; it does not claim a real forward result or wait for a market move.
Test setup was split into a helper in the correction; production behavior stayed
the same. Draft PR 402 is attached and stacked on PR 401.

No release, VPS replacement, model call or order in this increment. The continuous
VPS recorder still runs the earlier verified reader. New decision commands are
source/GitHub verified, not deployed or automatically produced. Next work must
connect the existing linked-model boundary to inert equity proposals and record
complete scheduled cohorts, while finishing executable paper risk inputs and
broker supervision/recovery. This handback is documentation only; its CI is
separate from the code proof. Previous goal turn and this turn made concrete
progress; no blocker audit applies. Fully autonomous trading remains incomplete.
