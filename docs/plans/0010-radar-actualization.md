<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0010 — Actualize Radar's intelligence and autonomous distribution

Status: in progress. Item 1 landed 2026-09-07; items 2-10 are not started.

The complete paste-ready specification is [design 0015](../design/0015-radar-implementation-handoff.md). Read that document in full; its owner decisions and ordered PRs control this plan. Supporting inspected evidence is [research 0031](../research/0031-radar-handoff-inspection.md).

## Work ledger

Fill in the commit, named checks, observed outcomes, and deployment state for each item as implemented. Do not mark source-inspected findings as test-proven fixes.

- [x] 1. Record implementation decisions and proposed ADRs. `docs/radar-next-phase`, this
      commit. [ADR 0015](../adr/0015-the-prize-is-an-evidence-relay-and-a-winner-is-always-selected.md)
      (entry, whole-week scoring modes, guaranteed selection),
      [ADR 0016](../adr/0016-the-model-selects-whole-clauses-never-a-number-in-its-own-sentence.md)
      (clause selection), and
      [ADR 0017](../adr/0017-the-journal-records-intent-before-effect-and-replay-proves-only-the-decision.md)
      (journal, outbox, what replay proves). ADR 0013's constraint 4 and design
      0007 §6.2 carry supersession notes rather than edits. `cargo test -p
      repo-conformance`: 33 passed, 0 failed. **No behaviour changed** — every
      ADR states it is not yet implemented.
- [~] 2. Repair observable research inputs and coverage. **Part one done** on
      `fix/observable-event-fields`: `Envelope.success`, `Envelope.tx_index` and
      `Trade.trader` are `Option`, the pre-2026-09-07 sentinels translate on
      read, and every count of successful activity is absent when the window
      holds an unresolved row. Regressions: an old-shape trades file reads back
      as unknown (`older_files_still_read.rs`), and three feature cases
      (`the_feature_table_cannot_see_the_future.rs`). Each was verified by
      re-applying the bug — the redundant filter in `trades_to_t` survived its
      own re-application and was removed rather than tested. `cargo test
      --workspace --all-targets`: 77 suites, 0 failures. **Part two done** on
      `fix/research-coverage-and-bounded-reads`: a `coverage` table records
      which ingestion ranges were run and which finished, read at the watermark;
      `trade_coverage` reads those records instead of inferring coverage from
      partition filenames; `Reader::read_range` bounds the disk read and the
      feature pass uses it. **One piece is deliberately left out and named:**
      nothing in production writes a coverage record yet, because the backfill
      queries by timestamp and the store is keyed by slot. That is item 2c.
- [~] 3. Repair cohort/label protocol and provenance. **Design 0015 §3.2 items 1
      and 3 done** on `fix/research-cohorts-and-labels`: the frozen population
      supplies the fold boundaries before any label is examined, a cut never
      lands inside a group of launches sharing a slot (and a slot wide enough to
      swallow a fold is refused rather than split), and every fold reports
      population / labelled / scored so a verdict over a small fraction of a
      window says so. Regressions: `removing_labels_cannot_move_a_fold_boundary`
      — verified by re-applying the bug, which shifts all five boundaries —
      plus the two `split` cases and the nesting of the cohort counts. §3.2's
      **item 2 followed** on `fix/label-provenance`: every absent label carries
      a typed reason — no entry, no exit, one observation counted twice, a stale
      entry price, a stale exit price, or no price at all — recorded per horizon,
      carried through the file, and reported by `radar edge` as a breakdown of
      the population it could not see. A file written before the column reads
      back as `unrecorded` rather than being assigned a reason. **Items 5 and 6
      are still not done**: descriptive-versus-economic reporting, and
      time-block resampling with a creator-grouped sensitivity analysis. Item 4
      (train-only thresholds) was already held by `grammar`, which takes its
      deciles from the fitting rows.
- [ ] 4. Implement durable audit, offline replay, and effect recovery.
- [ ] 5. Implement admitted launch evidence and permanent receipts.
- [ ] 6. Replace mixed scoring with explicit whole-week fallback modes.
- [ ] 7. Implement autonomous evidence relay, selection, and claim integration.
- [ ] 8. Build and visually verify the evidence-newsroom site, including public winner payout addresses, copy/explorer links, and verified transaction evidence in History.
- [ ] 9. Implement distribution and operating measurements.
- [ ] 10. Run available bounded measurements and prepare launch-readiness evidence.

## Handback

**Stopped at:** 2026-09-08, after item 2. `main` is at the squash of #212.
PR [#211](https://github.com/hey-vera/radar/pull/211) (item 1) and
[#212](https://github.com/hey-vera/radar/pull/212) (item 2 part one) are merged;
[#213](https://github.com/hey-vera/radar/pull/213) (item 2 part two) is open
with auto-merge armed. No production settings, posts, token, or payment was
changed, and nothing was run against a production store.

**Two traps this stretch paid for, both worth knowing before the next PR:**

- CI's mutation shard runs `--in-diff`, so it mutates *changed lines* — including
  a line whose only change was `succeeded` becoming `succeeded()`. Two of the
  three survivors on #212 were pre-existing untested decisions that the diff
  merely touched. The fix that works is to lift the decision out of the function
  that reads a store and prints, so a test can reach it at all.
- A branch stacked on an unmerged PR cannot be rebased onto `main` after that PR
  squash-merges: the replayed commits conflict with their own squashed content.
  `git rebase --onto origin/main <last-already-merged-commit> <branch>` replays
  only the new work and produces the diff the PR should show.

**Owner decisions:** fully autonomous selection, no human judge, farmed participation accepted, one winner whenever valid entries exist, deterministic and disclosed engagement-data fallbacks, complete developer logs and replay, evidence-newsroom art direction, Tennessee operator with worldwide reach. X written approval is settled by the owner and must not be reopened.

**Next action:** item 3, `fix/research-cohorts-and-labels` (design 0015 §3.2) —
frozen cohorts before label filtering, label provenance and censoring, train-only
threshold selection, reproducible reports. It is unblocked and needs no
credential.

**Carried, and named so it is not lost:** *item 2c*, the coverage **producer**.
Nothing in production writes a coverage record, so every trade-derived feature is
absent. That changes nothing today — the production trades directory has never
been written to — but it is a prerequisite for any trade-derived measurement.
The obstacle is real: the backfill queries by `block_timestamp` and the store is
keyed by `block_slot`, and a completed time window that returned no rows attests
no slot range. The workable shape is per-run rather than per-window, bounding the
attested span by the events the run actually saw and under-claiming at the edges.
`crates/radar-store/src/coverage.rs` carries this note too.

No production research result exists from this handoff; do not invent one.

**Completion condition:** implemented behavior and rendered site pass the stated acceptance cases, the result and residual limits are documented, and each outstanding operational/legal prerequisite has a precise owner and deliverable. A missing engagement measurement cannot become a hidden no-winner veto, and a selected winner cannot bypass the existing payment authorization checks.
