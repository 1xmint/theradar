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
- [ ] 2. Repair observable research inputs and coverage.
- [ ] 3. Repair cohort/label protocol and provenance.
- [ ] 4. Implement durable audit, offline replay, and effect recovery.
- [ ] 5. Implement admitted launch evidence and permanent receipts.
- [ ] 6. Replace mixed scoring with explicit whole-week fallback modes.
- [ ] 7. Implement autonomous evidence relay, selection, and claim integration.
- [ ] 8. Build and visually verify the evidence-newsroom site, including public winner payout addresses, copy/explorer links, and verified transaction evidence in History.
- [ ] 9. Implement distribution and operating measurements.
- [ ] 10. Run available bounded measurements and prepare launch-readiness evidence.

## Handback

**Stopped at:** item 1 recorded on branch `docs/radar-next-phase` against `4d14032f493d01ef76d27744769efda786956fc7`. The four documents are committed and three ADRs are written. No Radar implementation code, production settings, posts, token, or payment was changed.

**Owner decisions:** fully autonomous selection, no human judge, farmed participation accepted, one winner whenever valid entries exist, deterministic and disclosed engagement-data fallbacks, complete developer logs and replay, evidence-newsroom art direction, Tennessee operator with worldwide reach. X written approval is settled by the owner and must not be reopened.

**Next action:** item 2, `fix/observable-research-inputs` — known/unknown event fields, legacy sentinel handling on read, coverage manifests, and bounded partition reads, with the regressions design 0015 §3.1 names. Follow its PR ordering and `AGENTS.md` verification requirements. No production research result exists from this handoff; do not invent one.

**Completion condition:** implemented behavior and rendered site pass the stated acceptance cases, the result and residual limits are documented, and each outstanding operational/legal prerequisite has a precise owner and deliverable. A missing engagement measurement cannot become a hidden no-winner veto, and a selected winner cannot bypass the existing payment authorization checks.
