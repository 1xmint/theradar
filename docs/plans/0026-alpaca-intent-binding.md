<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0026 — Bind paper recovery payloads to retained order intents

**Status:** source verified on GitHub; combined reconciliation integration pending.

Branch: feat/alpaca-intent-binding, initially based on verified combined 61f82b2
(full CI 38183043973). Incorporated directory-sync integration f9553a8 so the PR
has a clean merge base and additive test floor. Its CI 38183693452 was still
running at incorporation; both constituent sources had passed full CI.
No payload changes to journal persistence. Keep the base fixed while this CI runs.
Previous goal turn made progress: verified directory-sync source, integrated it
and dispatched combined CI. This turn advances the payload gate needed by recovery.

## Caller and checklist

Actual caller: radar alpaca-order-read with optional --journal <private-file>.
Existing unbound lookup behavior stays available and never claims payload binding.

- [x] Refuse missing logs, wrong accounts and absent retained IDs before keys/broker IO.
- [x] Hold cooperative journal ownership through the bracketed broker reads.
- [x] Pure exact payload matching using the existing decimal parser: identity,
  ticker, side, nanoshares, limit; simple equity/day/nonextended order shape.
- [x] Refuse missing/changed/complex payloads; absent broker orders stay unresolved.
- [x] Preserve all claims and uncertainty; no writes, retries or claim release.
- [x] Add four unit regressions and one actual-command regression. Independent
  test floor 2342 (2337 including directory sync + 5).
- [x] Repair d3f7975 passed full CI 38184222577, including all mutation shards.
- [ ] Integrate verified source and rerun combined checks before parent advancement.

## Handback

CI 38184000795 at 52fa7af completed: only the new process test's assert_is_empty
lint failed. Repaired with the existing assert_eq empty-vector convention.
All other jobs passed, including tests and all mutation shards: 32 caught,
zero missed/timeouts, six unviable. Exact diagnostics show unused/dead-code
deny-warnings errors for three no-op replacements, missing BrokerIntent::Default,
an illegal || let-chain mutation, and type inference after deleting the sole
Some(path) argument arm. Compile refusals are not behavioral mutation kills.
No exclusions added. Full repair d3f7975 then passed CI 38184222577.

No orders, credentials, deployment or activation. Matching confirms selected
broker payload fields only, never fill/cash/fee reconciliation, eligibility,
freshness or capital authority. Time-in-force day and nonextended execution are
fixed constraints of the initial lane, not new owner controls or claims that
the broker permits no other order types. Arithmetic regression prices are
synthetic, including sub-tick examples; tick eligibility belongs to the risk gate.
The journal's fee bound is internal and is not compared to a nonexistent broker
order fee field. All reservations remain held, even if status says terminal.

Next: fill/fee/cash reconciliation and the pure equity risk verdict with isolated
paper executor, then unattended supervisor/site controls and measured evaluation
under plan 0024. Account creation remains outstanding; it does not block this code.
