<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0021 — Profit-first venue selection for a U.S. account

**Status:** researched recommendation; profit objective clarified by owner;
Alpaca-first sequencing recommended, not yet accepted as a new venue decision.
**Date:** 2026-10-11.
**Preceding discussion:** [design 0020](0020-hyperliquid-first-research-and-eligible-live-execution.md).

## What the owner wants

Josh clarified that the goal is mainly success on profit probabilities when
learned intelligence is applied. Do not optimize for a favorite venue, leverage,
trading frequency, API elegance or an impressive model. Preserve adaptive risk,
optional daily loss caps, Agent decides choices, research chat and direct
pause/revoke. The owner has confirmed U.S. residence/location.

The supplied external ranking gives CME 44%, Alpaca 36%, regulated crypto 31%,
Solana 18%, forex 12% and prop challenges 8%. No source, strategy, horizon,
denominator, capital assumption or success definition accompanied those numbers.
They are unverified claims, not calibrated probabilities, and must not enter
configuration, performance claims or capital-allocation decisions. Market depth
and clean data do not establish a particular bot's positive expectancy.

## Recommendation

Start a narrow U.S. stocks/ETFs experiment through Alpaca paper trading, initially
without leverage or a dependency on shorting. This is an implementation and
experimental-access recommendation, not evidence of the highest profit probability.
Alpaca offers a [paper environment](https://docs.alpaca.markets/us/docs/paper-trading)
and [fractional orders](https://docs.alpaca.markets/us/docs/fractional-trading).
Fractional short sales are unsupported; do not paper-test a fractional pairs
strategy that the intended live account cannot execute. Account eligibility,
restrictions, order capabilities and settled/buying-power constraints still need
verification before live use. Equity sessions, holidays, corporate actions and
dividends must be represented rather than copying a 24/7 crypto clock.

Alpaca's paper simulator omits impact, latency slippage, queue position, regulatory
fees and dividends; paper-only accounts have IEX data. Its [data plans](https://docs.alpaca.markets/us/docs/about-market-data-api)
distinguish single-exchange real-time coverage from consolidated coverage. Label
the feed and its coverage; do not treat IEX volume as total market volume.
Include data/model/hosting costs in project-level net results, separately from
trade PnL. Small capital does not make recurring operating costs disappear.

CME micro futures remain a candidate when the account can support useful sizing,
margin and stress buffers. The micro symbols are **MES/MNQ**, while ES/NQ are
larger E-mini contracts. [MES specifications](https://www.cmegroup.com/markets/equities/sp/micro-e-mini-sandp-500.contractSpecs.html)
and [MNQ specifications](https://www.cmegroup.com/markets/equities/nasdaq/micro-e-mini-nasdaq-100.contractSpecs.html)
define actual contract units. A clean professional order book does not establish
easier profits; minimum contract granularity, costs, leverage and gap exposure
must fit the mandate. Low advertised intraday margin is not a loss budget.

Regulated crypto derivatives may be closer to the original 24/7 experiment, but
verify the specific U.S. product, broker/API, account eligibility, contract units,
fees and liquidity before ranking them. Keep Hyperliquid public observations and
Solana as optional research modules. Do not build several live connectors before
the first experiment can demonstrate useful evidence.

## How Radar should earn a probability estimate

Define success over a stated horizon and capital/risk mandate: sustained positive
net return with acceptable drawdown, compared with cash/no-trade and simple
tradable baselines matched to the experiment. Report absolute PnL, costs, risk,
exposure, turnover and uncertainty together. High win rate can conceal occasional
large losses; positive dollars alone need not outperform a simpler alternative.

Freeze strategy/model versions, training windows and promotion criteria before
evaluating held-out and forward periods. Prevent future information, selection
bias and repeated testing from manufacturing a winner. A ChatGPT narrative is
not a statistical forecast, and adding RL does not remove these requirements.
Test whether each research/model addition improves outcomes over its baseline.
Live adaptation may reduce risk or abstain; deployment of a changed strategy
requires a separately evaluated version, not self-editing signing permissions.

Model conservative execution, fees, borrow/funding where applicable, partial fills,
cancel races and outages. Paper results establish integration and a hypothesis;
eligible small live canaries establish execution behavior. Promote only from
reconciled results with costs included. Unknown evidence means no probability
claim. Retaining the ability to wait is part of the product, not a failure to trade.

## What remains undecided

The owner has clarified the profit-first objective, not approved the percentages
or a new Alpaca account/deposit/live mandate. The existing Hyperliquid source PR
can finish GitHub verification, but further venue implementation pauses during
this direction discussion. Recommend Alpaca as the next primary experiment;
record acceptance before changing the implementation sequence again.
