<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0017 — Alpaca paper account foundation

**Status:** in progress; owner-approved Alpaca-first experiment.

Base: verified integration 64230845eb5c9452f7ff16c55c40138dd3f1bbbe.
Branch: feat/alpaca-paper-account. Independent of the unfinished Hyperliquid
public-data branch; neither child advances the parent while the other CI runs.

## Checklist

- [x] Record accepted profit-first, Alpaca-paper-first direction in GOAL and design 0022.
- [x] Implement actual CLI paper-only account/positions/clock reader.
- [x] Add four unit regressions plus an operator-process refusal regression;
  test floor 2293. Verification pending GitHub, no local tests/builds/lint/jobs.
- [ ] Read complete GitHub CI, including mutation diagnostics, and fix failures.
- [ ] Owner creates paper account; connect paper credentials privately.
- [ ] Verify actual account response; no assumption that fixtures prove access.
- [ ] Record market data with coverage and as-of boundaries.
- [ ] Build durable paper orders, fills, reservations and restart reconciliation.
- [ ] Connect adaptive risk, research chat and site's existing handover controls.
- [ ] Forward validation net costs versus tradable baselines, then separately
  configure eligible live execution and revocable owner authority.

## Handback

Account creation remains with Josh; no keys requested in chat. Source implements
observations only, not trades. No production deployment, deposit, delegation,
paid data or live orders. Existing private deployment remains unchanged.
GitHub verification must establish the new source before integration. Historical
Solana edge thresholds are not Alpaca thresholds. Do not claim live expectancy
or activation readiness from paper results or an account read.

Initial source 83ee6ca is in draft PR 373. GitHub CI 38177814576 reports a
lint failure at alpaca_read.rs:20: the Account struct's account_blocked field
triggers struct_field_names. Rename the internal type BrokerAccount; preserve
Alpaca's JSON schema and all report semantics. No waiver or new dependency.
Wait for all jobs to finish before pushing and require fresh complete CI.
Hyperliquid public-reader correction 05a5e17 passed complete CI 38177609139;
that branch remains secondary and unmerged. No production changes.

### IEX capture and replay increment

CI 38178028897 at correction 07f8c1c completed: build, tests, MSRV and every
mutation shard passed. Lint now reached the process test and requires assert_eq
for its empty byte vector (alpaca_read_process.rs:19). Apply that assertion form
without changing refusal behavior; no waiver. The next source adds actual CLI callers for IEX
snapshots and offline replay. Fixed data host and feed, paper credentials only,
exact response bytes retained as a JSON string, host availability rounded up
to the next millisecond, immutable filenames and synced-file publication.
Replay admits captures only at/after their host availability watermark. Missing
symbols stay null; exchange timestamps, freshness and tradability stay unverified.
This is an operator capture tool, not an unattended data service or order loop.

Five market unit regressions and one additional process regression bring the
test floor to 2299. They cover query binding, raw precision, watermark equality
and future refusal, clock reversal, null/malformed data, no-overwrite publication,
format and exact size boundaries, and the actual replay command without keys.
Require complete GitHub CI and mutation results. No local jobs, production
changes, broker account access, orders or paid data. Follow with validated quote
inputs and durable paper execution/reconciliation; the full goal remains open.

Source 132fd3c is in PR 373 under CI 38178443684. The completed lint job
requires replacing the replay process test's nonempty stderr assertion at
alpaca_read_process.rs:60. Assert the specific future-watermark/invalid-watermark
diagnostic instead; this improves the regression and avoids a lint waiver.
Also scope GOAL's historical 456 bps bar explicitly to the original Solana
experiment, consistent with the accepted venue-specific profit objective.
Do not push these corrections until the current run is terminal and all
failure/mutation diagnostics have been read. No production or account access.

### Quote inputs for paper execution

After reading the terminal CI 38178443684 failure (lint only, every build/test/
mutation job green), publish correction 2389c36 and require a fresh complete run.
The next increment adds the actual alpaca-quote CLI consumer: fixed-point price
and displayed size parsing from original JSON digits, nanosecond UTC date checks,
capture universe/availability binding, positive two-sided quote, no crossed book,
and explicit inclusive age/spread bounds. Unsupported precision/exponents are
refused. This is not order authorization or a consolidated-market quote.

Use serde_json's existing raw_value feature only; no new package or lockfile
resolution. Four unit regressions and one actual process regression bring the
floor to 2304. Verify exact decimal/u64 boundaries, impossible dates/nanoseconds,
zero/unknown/crossed inputs, equality and exceeded bounds, sub-millisecond future
quotes, and a raw response containing an unrequested symbol. All verification
remains on GitHub. No account creation, private keys, live orders, service
deployment or paid data. Complete verification before integrating; next are
paper policy/account reconciliation, reservations and durable order submission.

The capture/replay correction 2389c36 passed complete GitHub CI 38178691796.
Before publishing quote inputs, primary-source review found unresolved displayed
size units: stream docs still say round lots, the November 2025 CTA/UTP changelog
says shares, and neither explicitly establishes IEX normalization. Keep displayed
size millionths with null units, not microshares or executable capacity. Verify
the actual feed/instrument units before paper execution sizes use this field.
Prices retain explicit micro-USD per share. Record this limitation rather than
manufacturing a conversion; all other quote checks remain necessary.
