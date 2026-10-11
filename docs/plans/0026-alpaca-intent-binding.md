<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0026 — Bind paper recovery payloads to retained order intents

**Status:** source implemented; GitHub verification pending.

Branch: feat/alpaca-intent-binding, based on verified combined 61f82b2
(full CI 38183043973). Independent of directory-sync integration CI 38183693452;
no changes to journal persistence. Keep its base fixed while this CI runs.
Previous goal turn made progress: verified directory-sync source, integrated it
and dispatched combined CI. This turn advances the payload gate needed by recovery.

## Caller and checklist

Actual caller: radar alpaca-order-read with optional --journal <private-file>.
Existing unbound lookup behavior stays available and never claims payload binding.

- [x] Refuse missing logs, wrong accounts and absent retained IDs before keys/IO.
- [x] Hold cooperative journal ownership through the bracketed broker reads.
- [x] Pure exact payload matching using the existing decimal parser: identity,
  ticker, side, nanoshares, limit; simple equity/day/nonextended order shape.
- [x] Refuse missing/changed/complex payloads; absent broker orders stay unresolved.
- [x] Preserve all claims and uncertainty; no writes, retries or claim release.
- [x] Add four unit regressions and one actual-command regression. Independent
  test floor 2340 (2335 + 5); combined with directory sync it will be 2342.
- [ ] Read full GitHub CI and exact mutation diagnostics; fix failures.
- [ ] Integrate verified source and rerun combined checks before parent advancement.

## Handback

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
