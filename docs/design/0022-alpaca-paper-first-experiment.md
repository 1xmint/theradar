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

The reconciliation caller also computes pure, conservative exposure bounds when
selected account and pending-order observations are unchanged. The supported
initial lane is active USD accounts with known unrestricted flags, long US equity
positions and simple quantity-based equity limit orders. Notional, market/stop,
multi-leg, replacement, unknown state, malformed decimal and oversold cases have
unknown bounds, never zero. Pending cancellation retains its full commitment.
Reserve original order quantities even after partial fills, since positions and
cash may reflect fills at different instants. This can over-reserve and refuse
otherwise valid actions; it must not be loosened without fill/local-claim evidence.

[Alpaca already deducts open buys from buying power](https://docs.alpaca.markets/us/docs/orders-at-alpaca).
Compute the cash bound as min(cash minus conservative pending buys, broker buying
power, equity), rather than subtracting pending buys from buying power again.
Positions use max(cost basis, market value) and buys add their limit-price bound;
pending sells reserve observed long shares and never create short capacity.
[Nine-place fractional quantities](https://docs.alpaca.markets/us/docs/fractional-trading)
and broker dollar strings use exact integer arithmetic. Cash is floored to
micro-USD; liabilities are ceiled. Unsupported precision/overflow is unknown.
These are observation bounds, not a risk authorization: fees, unsettled funds,
freshness, local claims and complete economic reconciliation remain separate gates.

The paper-only `radar alpaca-order-read` command supplies the broker-side lookup
needed for recovery by a retained client order ID. The
[official SDK](https://github.com/alpacahq/alpaca-py/blob/master/alpaca/trading/client.py)
uses GET /v2/orders:by_client_order_id with the client_order_id query parameter.
Radar permits its own query-safe ASCII letter/digit/hyphen/underscore identifiers,
checks the returned client ID and nonblank broker ID, and pins both surrounding
account reads. Selected fill quantities, average price and lifecycle timestamps
remain exact strings; unknown fields are omitted. Terminal or missing observations
do not release a claim or authorize another submission. Only HTTP 404 is reported
as not_visible; authentication/transport/other HTTP errors refuse. An absent order
is not proof that an ambiguous submission never reached the broker. This lookup
alone supplies neither durable intent history, payload binding nor complete
fill/cash reconciliation.

Account activities are an additional required input for reconciliation, collected
by radar alpaca-activities-read on the fixed paper host. The command reads all
activity types in ascending pages of 100 between pinned account observations;
it percent-encodes the last activity ID as the next cursor and refuses duplicate
IDs, mismatched explicit account IDs, oversized pages and the operator's request
limit. Exact string amounts and nullable fields remain observations, not inferred
zeros. Unknown activity types remain visible. A short page exhausts pagination,
not economic history: pages are not atomic, late events/corrections can appear,
and fees can be created the following day. No claim release or final cash/fee
accounting follows from this command. See the
[activity endpoint](https://docs.alpaca.markets/us/reference/getaccountactivities-2)
and [activity schemas/pagination](https://docs.alpaca.markets/us/docs/account-activities).

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

`radar alpaca-assets-read` reads each requested equity between pinned paper
account reads. It binds symbol, nonempty unique asset ID and us_equity class;
nullable status, exchange, tradability, fractional, margin, short and attribute
observations remain separate from order eligibility. It rejects URL dot segments,
malformed responses and account changes in identity. Stable selected account
fields are still not an atomic snapshot. See the
[asset endpoint](https://docs.alpaca.markets/us/reference/get-v2-assets-symbol_or_asset_id)
and [official asset model](https://github.com/alpacahq/alpaca-py/blob/master/alpaca/trading/models.py).
Actual feed/account verification, price/quantity increments, trading session,
quote conditions, risk state and owner mandate remain required before orders.

Broker record receipts require file sync followed by parent-directory sync on
every append, including after reopening. A directory-sync error can leave an
intact event on disk but returns no receipt; the broker owner remains poisoned
and must reopen. This follows the
[Linux fsync directory-entry requirement](https://raw.githubusercontent.com/mkerrisk/man-pages/master/man2/fsync.2).
Filesystems/platforms that cannot perform this operation refuse broker records.
It assumes a protected, existing, durably provisioned parent directory and a
storage stack honoring sync; it does not claim host-rollback resistance or a
physical power-loss test. Historical nonbroker journal behavior is unchanged.

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
