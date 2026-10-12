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

The optional --journal recovery path holds a verified BrokerLog owner across the
reads and loads the requested immutable intent before broker credentials. A found
order must match account/client identity, ticker, side, exact nanoshares and exact
micro-USD limit. The initial supported payload is a simple quantity-based US
equity limit order, day time-in-force, extended_hours explicitly false; notional,
stop, replacement and nonempty nested-leg fields refuse. Missing/null required
fields never match. A changed account refuses binding; HTTP 404 remains unresolved.
The report's intent_payload_verified covers these order fields only. The local
fee bound has no broker order-field equivalent; fee adequacy, fills, cash,
freshness, eligibility and authority remain separate. A matching order never
releases a reservation or permits retry. See the broker's
[order semantics](https://docs.alpaca.markets/us/docs/orders-at-alpaca).

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
even after an error/restart. The private paper submission caller is described
below; actual unknown risk inputs still prevent it from releasing an effect.
Uncertain writes poison the current owner; torn, mixed-domain and invalid replay
refuse. Existing ownership/hash-chain storage is reused, with no new broker bytes
in historical hashes when absent. These controls establish ordering and identity,
not capital permission, complete reconciliation or resistance to host rollback.
There is no release API until intent/fill/cash reconciliation is established.

The actual alpaca-order-reconcile caller now holds that journal owner while it
collects a bound order, paginated activities, then another bound order. All six
selected account observations and both selected order observations must match.
This rejects visible races but does not establish an atomic snapshot or detect
changes outside selected fields. Missing orders and any failed read refuse
without a partial report. Activities use strict GET behavior; HTTP 404 is only
an unresolved observation for the order lookup.

For this order's FILL events, execution identity, ticker, side and fill type must
be present. Missing fill order IDs cannot be assumed unrelated; identified other
orders and nontrade events are preserved without contributing to this order's
gross amount. Each cumulative quantity defines an interval ending at cum_qty and
starting at cum_qty minus qty. Sorted intervals must cover zero through the order's
observed filled_qty without gaps or overlaps; leaves_qty must equal original
quantity minus cumulative quantity. Execution prices must respect the retained
limit. Unsupported status/decimal shapes refuse. Filled status requires the full
original quantity; other accepted statuses are observations, not finality claims.

Products of exact nanoshares and nano-USD prices are summed before conversion to
floor/ceiling micro-USD gross bounds. Broker average-price strings are preserved
but are not used to infer execution amounts. The execution_quantities_matched
flag covers this bounded consistency check only. Fees, cash/baseline accounting,
late events/corrections, finality and durable claim release remain unresolved;
fills_reconciled and all authority/retry/release flags remain false. Even a fully
filled observed order retains its entire original cash/share reservation.

The alpaca-cash-reconcile caller compares a prior private activity report with
current account-wide observations under journal ownership. It binds every retained
order before and after paginated activity collection, checks unchanged selected
current account/order fields, and reuses execution interval validation. All prior
activity IDs must still exist with equal selected fields; new fills must belong
to bound retained orders. New CSD deposits, CSW withdrawals and USD FEE entries
are supported; unknown new activity types refuse instead of becoming zero. Fees
may have either sign (including credits); deposits cannot be negative and
withdrawals cannot be positive. This classification follows the official
[activity schemas](https://docs.alpaca.markets/us/docs/account-activities).

Cash checks require explicit USD currency and exact nonnegative account balances.
Fill products are accumulated in signed integers at 18 decimal places; nontrade
cash strings support nine. Expected cash and trade cash flow round outward to
nano-USD only after aggregation. Current reported cash must lie within the exact
floor/ceiling; no arbitrary discrepancy tolerance or unverified broker rounding
rule is added. Actual broker cash precision/booking still needs capture validation.
The prior report's account pin, stable selected fields, page exhaustion and host
times are rechecked; it cannot overlap/follow the current read. A baseline file
larger than 16 MiB refuses before network access.

The baseline file is operator-provided evidence, not authenticated provenance or
a durable execution checkpoint. Keep it in protected storage. Matching selected
cash deltas does not prove complete or atomic history, final fees, fee-reserve
adequacy, settlement, available capital or profit. Trade cash flow includes the
cost of opening positions and is not P&L. External funding is separately labelled.
All original claims and authority refusals remain. Durable economic transitions
and complete risk state are still required before any claim release or execution.

`radar alpaca-cash-checkpoint` connects the same verified provider reads to a
durable baseline in the owned order journal. Genesis must precede retained orders;
later calls revalidate the retained selected observations, bind all retained
orders and verify cash deltas before advancing the baseline. Rejected corrections,
missing records, unknown cash effects or failed reads preserve it for recovery.
File and directory sync precede an updated baseline; uncertain persistence
poisons the owner until integrity-checked replay. This provides local restart
continuity, not broker atomicity, complete economics, rollback resistance or
claim release. Generic storage trusts the protected caller's normalization.

Cash checkpoints also retain the private caller's complete selected progress for
every retained order, atomically with the cash baseline. Residual buy claims ceil
the unfilled limit notional; residual sell claims hold unfilled shares. Every
order keeps its full fee reserve. Filled/canceled/expired/rejected observations
end unfilled quantity claims; pending_cancel and done_for_day do not, following
[Alpaca's lifecycle](https://docs.alpaca.markets/us/docs/orders-at-alpaca).
Progress cannot change broker identity, decrease fills, reopen a terminal order,
forget prior progress or cover an unsubmitted reservation. Original payloads and
submission uncertainty persist. Unknown historical progress holds original claims.
This is local economic accounting, not profit, fee finality, settlement or an
available-capital verdict; pure equity risk and execution integration remain.

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

## Position quantities in durable checkpoints

The existing cash checkpoint path now also brackets its selected cash/fill reads
with pinned account and position-quantity reads. It retains long US equity asset
IDs and exact nanoshares in the same durable observation. On subsequent calls,
opening shares plus new bound fill quantities must equal current shares. Missing
positions, unexplained additions, changed asset IDs, duplicates, unsupported
positions and visible account/quantity races refuse without advancing the log.
Host observation times cover the outer brackets. Price-dependent market values
are not used to infer share quantities. This follows the broker's
[positions endpoint](https://docs.alpaca.markets/us/docs/working-with-positions)
and [fill activity schema](https://docs.alpaca.markets/us/docs/account-activities).
Genesis establishes selected opening quantities, not historical cost or profit.
Older checkpoints without quantities remain readable but refuse automatic
baseline replacement. Corporate actions/transfers need explicit reconciliation.
See [position accounting](../plans/0032-alpaca-position-accounting.md).

## Equity risk boundary

`radar-risk::equity` judges inert simple day limit proposals using Unix
milliseconds, nanoshares and micro-USD. Its effective owner policy defaults
closed. It checks account identity, pause, freshness/availability, session,
marketable limit distance, spread, capacity, cost/fee evidence, available cash
and owned shares, and order/symbol/portfolio ceilings. Optional loss stops gate
entries; independent reduction checks remain. Authorization has private fields,
binds the exact order and time interval, and cannot be deserialized from model
output. The separate authenticated executor is still required.

Its actual caller is `radar alpaca-order-preflight --journal <private-file>
--intent <typed-intent> --policy <private-policy> --max-pages <positive-u16>`.
It requires an existing protected journal and reads provider facts directly on
the fixed paper and IEX hosts; there is no operator/model evidence-file shortcut.
It joins the existing cash/share checkpoint with bracketed pending orders,
positions, asset observations, quote checks and a final pinned account. Retained
cash claims are subtracted from cash before independently bounding by buying
power/equity, and retained sells subtract owned shares. Unknown/unbound pending
orders or changed execution progress refuse. Current selected account/quantity
matches detect visible races, not atomicity. Selected asset exchange values are
from the [official SDK enums](https://github.com/alpacahq/alpaca-py/blob/master/alpaca/trading/enums.py);
unknown exchanges and asset restrictions refuse.

Preflight does not reserve or submit the proposed order and reports no execution
authority. It may durably update a valid observation before a later refusal.
Actual feed capacity units/conditions and fee/cost adequacy remain unknown, so
their kernel inputs are absent and orders remain refused. Session timestamps,
price/quantity increments, actual-response verification, effective site handover,
revocation and isolated execution are still required before activation. See
[equity risk work](../plans/0033-alpaca-equity-risk.md).

`radar alpaca-paper-submit` uses the same collector and protected journal owner
as preflight. It checks deterministic risk, re-decodes the serialized request
against the opaque authorization, durably reserves the intent, and records
submission uncertainty before any network effect. It reloads owner policy/time
after those writes, checks the original permission and re-evaluates the aged
provider facts under the latest policy before the fixed paper-only POST. A
matching response is neither reconciled fills nor permission to retry/release.
Actual capacity/cost inputs remain unknown and refuse before reservation. This
private CLI process is not yet an isolated authenticated execution service or
unattended supervisor; site handover, cancellation/recovery and forward evaluation
remain required. See [paper submission](../plans/0034-alpaca-paper-submit.md).
