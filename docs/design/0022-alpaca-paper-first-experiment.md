<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0022 — Alpaca paper first, measured profit as the objective

**Status:** owner-approved direction; account/market/quote foundation verified on GitHub; reconciliation increment in progress.

On 2026-10-11 Josh accepted the recommendation to start with Alpaca paper
stocks/ETFs. The objective is durable profit after costs when learned intelligence
is applied, not loyalty to a venue. The owner needs to create the paper account.

## Choice and alternatives

Alpaca is the primary experiment because simulated brokerage execution lets us
develop account reconciliation and evaluate liquid stocks/ETFs before committing
capital. Start without leverage or dependence on shorting. This is a practical
engineering recommendation, not a proven ranking of strategy profitability.
The supplied venue percentages have no established empirical basis in Radar.

Hyperliquid has attractive public APIs, order-book information, funding data and
an official SDK. Those are useful research properties; they establish neither
profit nor this U.S. owner's eligibility for live access. Its research remains
secondary and must not become an access-restriction workaround. Solana's existing
collectors and bounded signer remain available as secondary infrastructure.
Regulated futures or other eligible venues can compete later on measured results,
capital requirements and execution costs. There is no mandatory venue ladder.

## Where the recommendation is weakest

[Alpaca paper trading](https://docs.alpaca.markets/us/docs/paper-trading) omits
important live effects including market impact, queue position and latency
slippage. Paper-only accounts use IEX data. Data coverage must be labelled;
an IEX observation is not a consolidated-market observation. Default simulated
capital is not the owner's intended capital: reset it to the chosen realistic
experiment size before evaluation. No paid data purchase is authorised here.

Use held-out evaluation, forward paper trading, all execution/data costs,
drawdown and executable baselines to decide whether intelligence adds value.
Keep forecasts separate from realised outcomes and prevent future information
from entering replay. Paper gains alone never establish positive live expectancy.

## First implementation boundary

The actual CLI caller is `radar alpaca-read`. It uses only GET on the fixed paper
host for account, open positions and the US equity market clock. Endpoint and
schema references are the [official SDK](https://github.com/alpacahq/alpaca-py)
and [authentication guide](https://docs.alpaca.markets/us/docs/authentication).
Custom credential headers must not follow redirects. Read errors and malformed
JSON produce no partial report and do not echo response bodies or credentials.

Selected broker decimal strings and clock strings are observations, not validated
accounting or a freshness verdict. Optional absent values remain null. The reads
are sequential, not an atomic snapshot; open positions omit pending orders and
closed-position history. Buying power is not a capital mandate. The reader has
no order path, withdrawal path, eligibility decision or execution authority.

The IEX market-data caller is `radar alpaca-market-read`, using the fixed
data host and explicit `feed=iex`. Each capture retains the exact response JSON
as a string, requested symbols and host read/availability times. Publication
syncs the file before linking a new final filename; existing captures are never
overwritten. Offline `radar alpaca-market-replay` rejects captures unavailable
at the specified millisecond watermark and keeps missing symbols null. This is
a host-availability gate, not exchange-time validation or a fresh-quote verdict.
It does not treat Solana slots as equity timestamps. The operator tools are not
yet an unattended collector, historical database or execution-data service.

`radar alpaca-quote` consumes that recorded capture through the same availability
gate and only for a symbol originally requested. It uses serde_json's raw_value
feature to parse original price/size digits without f64, producing micro-USD per
share and millionths of displayed size. Size units remain null: the
[stream schema](https://docs.alpaca.markets/us/docs/real-time-stock-pricing-data)
says round lots, whereas the
[CTA/UTP changelog](https://docs.alpaca.markets/us/v1.1/changelog/marketdata-bid-and-ask-size-display-change)
announces shares from November 2025 without explicitly settling IEX normalization.
Resolve this against the actual feed before deriving executable share capacity.
Six decimal places are supported; more precision,
exponents, nonpositive prices/sizes, crossed quotes, excessive spread/age and
future exchange timestamps are refused. UTC timestamps retain nanoseconds;
canonical date round-trip validation rejects impossible dates and leap seconds.
Age and spread bounds are explicit operator inputs, not autonomous capital
mandates. Quote conditions, tradability, market hours, complete account state,
costs, reservations and execution permissions still need their own checks.

`radar alpaca-reconcile` collects account, open orders, positions, clock, open
orders again and account again, with the configured account ID required on both
account reads. Selected fields are compared after sorting orders by identity;
differences remain visible, not silently combined into a stable state. Pending
cancellation, partial fills and nested legs are retained. A 500-order response
is refused because [the API limit](https://docs.alpaca.markets/us/reference/getallorders-1)
cannot prove another order was not omitted. Smaller responses and stable selected
fields still do not establish atomicity, economic completeness or capital
authority. Future risk-state production must reconcile local durable reservations,
broker fills and unsupported account/order states before releasing any capital.

The separate broker journal records typed paper intents with nanoshares and
micro-USD price/fee units. Its actual operator callers are alpaca-order-reserve
and alpaca-order-journal. Reserved and SubmissionUnknown both retain their full
claim. Client IDs bind immutable intents; a repeated reservation never creates
another claim or resets uncertainty. The library persists uncertainty before an
effect and invokes it with the retained intent, then refuses another submission
even after an error/restart. No production order-submission caller exists yet.
Uncertain writes poison the current owner; torn, mixed-domain and invalid replay
refuse. Existing ownership/hash-chain storage is reused, with no new broker bytes
in historical hashes when absent. These controls establish ordering and identity,
not capital permission, complete reconciliation or resistance to host rollback.
There is no release API until intent/fill/cash reconciliation is established.

Preserve the smart panel: manual inputs with per-option Agent decides checkboxes,
adaptive risk by default, optional daily loss cap, research chat, visible reasons
and direct pause/revoke. The model may recommend a budget from observed funds;
it cannot issue authority or change signer protections. Do not ask for numeric
limits in chat instead of implementing the agreed website controls.

## Next evidence

Connect private paper credentials, verify real account observations, then build
market-data recording with coverage labels and time gates. Add isolated paper
execution with deterministic sizing, durable client-order IDs, fills and restart
reconciliation. Link the supervisor, research chat and website handover. Only
after forward validation and explicit live setup can real-money activation be
considered. The existing Solana risk types are not silently reused for shares.
