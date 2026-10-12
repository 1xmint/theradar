<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0041 — Freeze equity research decisions before outcomes

**Status:** In progress; no forward results or execution authority.

Branch feat/alpaca-forward-decision from 577de97, stacked on PR 401. Previous
goal turn made verified progress: cost-aware scorer passed all GitHub checks and
VPS continuous recorder was independently observed running. Parent docs CI
38279360660 is being watched; parent branch remains untouched.

## Checklist

- [x] Add actual credential-free CLI publishing a new decision record with host
  freeze time, horizon, entry quote, strategy identity, action and cost assumptions.
- [x] Bind a same-size always-enter baseline and cash baseline at decision time.
- [x] Score at exactly the frozen horizon watermark; retain unscoreable cases.
- [x] Add tests of actual commands, publication refusal, chronology and comparison.
- [ ] Inspect GitHub tests, lint, builds and every mutation diagnostic.

## Handback

This is research recording, not evidence that a model has learned an edge or an
authenticated proof of a strategy's information boundary. A host timestamp and
exclusive publication prevent accidental overwrites; an operator can edit local
research files. These records never enter broker risk state or signing authority.
An unattended decision producer, cohort completeness and held-out evaluation
remain required. The whole autonomous-trading objective remains active.
