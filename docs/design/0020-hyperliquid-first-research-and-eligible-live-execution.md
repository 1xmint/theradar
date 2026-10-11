<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0020 — Hyperliquid-first research and eligible live execution

**Status:** owner-requested direction; public-data foundation in progress;
live venue unresolved after the owner confirmed U.S. residence/location.
**Date:** 2026-10-11.
**Build sequence:** [plan 0016](../plans/0016-hyperliquid-paper-foundation.md).

## Recommendation and decision

Josh requested Hyperliquid perpetual futures as the primary starting venue,
retaining Solana as an optional secondary connector. Prefer BTC, ETH and SOL
research to fresh-launch execution. This supersedes Solana-first private-trader
sequencing in [design 0017](0017-a-private-autonomous-trader.md), not existing
public Solana research or historical measurements.

Recommend Hyperliquid-first **public-data research and paper evaluation**, then
testnet execution where permitted. Live execution requires venue/account
eligibility. Josh confirmed U.S. residence/location during the discussion, so
Hyperliquid is not a confirmed suitable live venue for him. Do not infer permission
from a reachable API or use VPN/proxy circumvention. The [official terms page](https://app.hyperliquid.xyz/terms)
did not expose readable text to the research tool; a [SEC-filed disclosure](https://www.sec.gov/Archives/edgar/data/2088139/000121390026056692/ea0290714-424b3_bitwise.htm)
describes the interface's U.S. restriction. Resolve an eligible live venue before
provisioning trading authority. Public observations do not establish eligibility.

## Rationale and its limits

Preserve the owner's motivation: cleaner learning data, major-pair liquidity,
small experimental capital, lower infrastructure friction and measurable net
expectancy. The [official API](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api)
provides REST, WebSocket and an official Python SDK. [Subscriptions](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions)
cover books, trades, contexts, orders, fills, accounts and funding. The public
book stream is bounded-depth snapshots, not every resting order. [Rate limits](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/rate-limits-and-user-limits)
include weighted IP and volume-linked address limits. This removes initial Solana
RPC/priority-fee/transaction-landing work, not competition or adverse selection.

There is no verified universal #1 ranking or measured positive Radar perps
expectancy. The supplied approximately 6% Solana profitability figure is not
validated here. Do not promise $10–50 live feasibility before measuring minimum
orders, collateral, fees, margin and useful sizing. [Funding](https://hyperliquid.gitbook.io/hyperliquid-docs/trading/funding)
is paid hourly. A naked funding short remains directional; hedges have basis,
fees and leg risk. Funding receipts alone are not profitable total PnL.

Momentum, mean reversion, funding and pairs are experiment candidates, not shipped
edges. Begin with no-trade and simple frozen baselines before ML/RL. Evaluate
forward/out of sample after costs and realistic fills. Testnet fills do not prove
mainnet expectancy. Research may propose a version; it cannot mutate live policy.

## Implementation boundaries

Keep Rust and existing storage until a concrete caller requires another runtime
or Postgres. Do not rewrite working infrastructure for an SDK. Use the official
SDK as a wire/signing reference if a reviewed execution service later needs it;
do not invent cryptography. Preserve existing Solana modules as explicit secondary
tools; the first slice does not disable their server jobs or migrate production.

The first actual caller is `radar perps-read`: fixed public `/info` endpoints,
testnet by default, BTC/ETH/SOL metadata/context and individual book reads.
Network selection changes observations only. Decimal strings remain raw; the
report makes no freshness, atomicity, price-quality or trading-authority claim.
There is no exchange request, credential, signing or order submission.

Next build WebSocket recording with source/receipt times, duplicate/gap retention,
reconnect/resync and bounded storage. Add open orders, fills, positions, margin,
ledger and hourly funding reconciliation. Query the funding account, never an
API-wallet address as a balance source: [API-wallet documentation](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/nonces-and-api-wallets)
warns this can return empty data. Unique signer wallets and durable monotonic
nonces must survive restarts/rotation without reviving old signed actions.

Solana mint/lamport/slot authorization cannot represent perpetual orders. Introduce
explicit venue/account/instrument, side, decimal price/size, reduce-only, client
order ID, expiry, fees and margin bounds. Preserve inert model proposals,
deterministic authorization, isolated signing, durable intent-before-effect and
reconciliation of unknown outcomes. Admit only reviewed trading/cancel actions;
owner withdrawals, transfers and delegation stay outside model authority. Reduced
API-wallet permissions do not bound trading losses.

Keep adaptive risk, optional daily caps, Agent decides checkboxes, research chat
and direct owner pause/revoke. Enforce notional/concentration, leverage,
liquidation distance, stressed maintenance margin, collateral/funding reserves,
slippage, stale-data and reconciliation circuit breakers. No default 3x/5x is
justified by examples. Entry pauses must permit independently bounded reductions.
Paper execution needs spread, queue uncertainty, latency, partial fills, fees,
funding and liquidation. Only eligible reconciled canaries can validate live fills.

## Weakest points and alternatives

Eligibility constrains the live recommendation; expectancy remains unproved.
Hyperliquid research with a U.S.-available execution venue introduces market/basis
differences requiring venue-specific validation. Paper-only learning cannot prove
real fills. Solana-first reuses more code but retains the owner's identified
infrastructure burden. Prove a declared admitted scope; universal venue support
is not an activation gate. Solana trading expansion follows positive live results.
