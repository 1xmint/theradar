<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0022 — Durable paper intent and submission ordering

**Status:** source implemented; full GitHub verification pending.

Branch: feat/alpaca-order-journal. Independent base: integration 6ae333d, identical
tree to source 02a3d3d verified in complete CI 38179746811. Keep the parent fixed
while other child checks run. Tests/builds/lint/mutations run only on GitHub.

## Objective

An autonomous broker loop must retain the same order identity and claim through
restarts and lost responses. Add typed broker history with actual operator CLI
callers; reuse existing hash-chain storage and exclusive ownership without
reinterpreting Solana token quantities or slots as equity units/time.

## Checklist

- [x] Typed paper intent with explicit nanoshares, micro-USD price/fees and side.
- [x] Immutable client-ID binding and idempotent reservation across restarts.
- [x] Both Reserved and SubmissionUnknown retain the entire claim; no release API.
- [x] Persist SubmissionUnknown before invoking an effect with the stored intent.
- [x] Refuse another submit, including after a failed effect or restart.
- [x] Poison an owner after uncertain persistence; require fresh verified replay.
- [x] Refuse broken/torn/mixed domain or illegal but correctly hashed history.
- [x] Reuse cooperative file ownership; preserve historical hashes when broker
  data is absent. Keep broker and Solana domain replay separate.
- [x] Add seven library and two CLI process regressions; independent floor 2318.
- [ ] Read complete GitHub CI and exact mutation diagnostics; fix actual failures.
- [ ] Integrate only after all relevant parent/child checks finish.

## Operator intent format

The private input is strict JSON, with integer units and no credentials:

```json
{"account_id":"paper-id","client_order_id":"radar_1","symbol":"SPY","side":"buy","quantity_nanoshares":10000000,"limit_microusd_per_share":100000000,"fee_bound_microusd":1000}
```

This records an inert claim, not a capital mandate. The website remains the
intended place for manual values or Agent decides and adaptive risk settings.
The CLI does not grant authority or replace those controls.

## Handback

Initial GitHub CI 38182002434 found a needless owned path parameter and a
surviving mutation at operation.rs:483:49 (domain-marker OR changed to AND).
Borrow the path and extend the domain regression with correctly hashed histories
carrying either marker alone; neither can appear as empty Solana history. Keep
the initial run intact until every job finishes; replacement verification is
pending. No local jobs or mutation exclusions.

The same run's shard 0 found size-bound survivors at alpaca_journal.rs:15:25
(+ to - and *) and :18:20 (> to == and >=). The process regression now accepts
strict valid JSON ending exactly at 64 KiB and refuses valid JSON with trailing
whitespace beyond it for the size-specific reason. This distinguishes a bounded
read hiding excess bytes from genuinely invalid JSON. Test count is unchanged.

Replacement CI 38182243726 lint reached the CLI process fixture and requires
numeric separators on its three long Rust literals. Apply that source-format
correction and wait for the run to finish before publishing it. Runtime JSON
values stay unchanged. Further verification remains on GitHub.

Source only, no credentials, network orders, delegation, deposits or deployment.
The submit ordering API has regression callers but no production executor yet.
It deliberately has no release/terminal reconciliation route: even an HTTP 404 or
canceled/filled order observation cannot currently free the durable claim.
The actual CLI can reserve/read only. Missing journal reads refuse rather than
claim zero reservations. Exact intent repeats do not append or reset unknown state.

Protected stable directories, journal and sidecar paths remain mandatory.
Cooperative locks and hash chains do not prevent administrator replacement,
rollback to a valid prefix, or another writer bypassing ownership. File sync
orders the record before the effect; directory-entry/power-loss guarantees still
need verification before a production executor relies on newly created logs.
This is not economic reconciliation or capital authorization.

Next: bind retained intents to broker lookup payloads, fills and cash movements;
derive fresh complete risk state and deterministic mandate/quote/asset verdicts;
connect isolated paper execution, supervisor, website handover and research chat.
Validate held-out and forward net outcomes before separately eligible live setup.
