<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0019 — Align private setup with the accepted primary venue

**Status:** source updated; GitHub verification pending.

Branch: feat/alpaca-primary-setup. Base: integration 9fa709d.
Independent of the reconciliation branch; neither change advances the parent
while the other's GitHub checks run. No workstation tests/builds/jobs.

## Objective

The actual PrivateSetup page still advertised Privy as the primary route to
autonomous execution after the owner chose Alpaca paper stocks/ETFs. Show the
accepted venue and account-creation next step without claiming a broker connection
exists. Retain Solana as optional and keep its existing draft controls separate.

## Checklist

- [x] Put Alpaca paper onboarding before the optional Solana module.
- [x] Link the dashboard; explain simulated funds, realistic capital, no leverage
  or shorting at the start, private paper keys and paper/live performance limits.
- [x] Clearly state broker connection is not yet available through this page.
- [x] Preserve ChatGPT linking and all existing Privy behavior.
- [ ] Read full GitHub CI before integration. Existing web tests exercise setup
  linking and configuration failures; this copy-only change adds no mirror tests.

## Handback

Source changes are not deployed. No broker credentials or account status are
invented, no order or authority route added, and no preferences moved between
venues. Account creation is still the owner's step. Next implementation is
complete broker risk state, isolated paper execution/recovery and supervisor/site
handover with the agreed Agent decides controls. Public onboarding reference:
[Alpaca paper trading](https://docs.alpaca.markets/us/docs/paper-trading).
