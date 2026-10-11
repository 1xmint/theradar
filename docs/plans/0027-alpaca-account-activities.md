<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0027 — Collect paper fills and nontrade cash/fee events for reconciliation

**Status:** source implemented; GitHub verification pending.

Branch: feat/alpaca-account-activities, based on f9553a8, fully verified by CI
38183693452. Independent of intent-binding repair CI 38184222577: no matching or
journal changes. Keep the source base fixed until child checks finish.
GitHub deleted the merged feat/alpaca-paper-integration branch and retargeted
PR 382 to feat/integrated-autonomous-controls. This child targets that parent at
3a4ba61, whose tree was verified identical to f9553a8. Parent CI 38184135535
is still running; do not advance that parent while either child is checked.
Previous goal turn made progress by merging verified foundation PR 380 and
repairing the intent-binding lint after inspecting the complete run.

## Caller and checklist

Actual caller: radar alpaca-activities-read --account-id <paper-id> --max-pages <N>.
This supplies the missing fill/nontrade event input for plan 0024; it is not an
economic reconciliation verdict or an autonomous executor.

- [x] Fixed paper host and existing redirect-disabled authenticated GET transport.
- [x] Pin account identity before/after, retain changes in selected fields.
- [x] Ascending 100-item pages, encode last ID as cursor, deduplicate across pages.
- [x] Refuse missing/ambiguous identities, oversized pages and resource-limit hits
  without a partial report. Explicit activity account IDs must match when present.
- [x] Preserve selected fill/cash/fee strings and missing values; omit unknown
  fields/descriptions. Unknown activity types stay visible rather than disappear.
- [x] Four unit regressions plus one command regression; floor 2342 (2337 + 5).
  Combining with the five intent-binding tests requires 2347.
- [ ] Read full GitHub CI and exact mutation diagnostics; repair failures.
- [ ] Integrate only verified source, update combined floor and rerun checks.

## Handback

No credentials, orders, delegation, deployment or activation. max-pages is an
explicit unsigned 16-bit positive request bound, not a financial risk setting.
The page size follows the documented endpoint maximum, verified before coding.
Pagination exhaustion is not economic finality: the endpoint is not an atomic
snapshot and later corrections or fees can arrive. Alpaca documents that
nontrade fee creation is typically the following day in UTC. No absent fee or
amount is treated as zero. No reservation is released, even after a short page.

Next: bind activity fills to retained broker/order identity, reconcile cumulative
quantity, exact gross amounts, fees and account cash without double counting or
releasing uncertainty; then complete the pure risk verdict, isolated executor,
supervisor, website handover and measured evaluation under plan 0024. The full
autonomous trading objective is still incomplete. No owner paper account yet.
