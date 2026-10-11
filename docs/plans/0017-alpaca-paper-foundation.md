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
