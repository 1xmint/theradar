<!-- SPDX-License-Identifier: Apache-2.0 -->
# Radar

Autonomous trading research, with **Alpaca paper stocks/ETFs as the primary next
experiment**. Hyperliquid public-data research and Solana remain secondary;
neither is the starting live venue for this U.S. owner. See the
[accepted direction](docs/design/0022-alpaca-paper-first-experiment.md).

The existing Solana infrastructure has an x402/MCP agent surface built
in and, on the live instance, sitting behind Cloudflare Access rather than open
to any outside agent (issue #186) — see [What is live](#what-is-live) below.

`radar alpaca-read` reads a paper account, open positions and the US equity market
clock. It requires private environment variables `RADAR_ALPACA_PAPER_KEY_ID` and
`RADAR_ALPACA_PAPER_SECRET_KEY`; it always uses Alpaca's paper host and never
places orders. See [setup and remaining work](docs/plans/0017-alpaca-paper-foundation.md).
The command is not yet wired to the deployed site. Existing measurements below
describe the Solana experiment, not Alpaca performance.

`radar alpaca-cash-checkpoint --journal <private-file> --account-id <paper-id>
--max-pages <positive-u16>` retains selected broker observations durably beside
order claims. Initialize before reserving any order; subsequent calls reconcile
against the retained baseline. Rejected reads leave the baseline unchanged.
Checkpoints also retain share quantities and require subsequent changes to match
new bound fills, with account/position reads bracketing collection. Legacy cash
baselines without share quantities require explicit reconciliation before use.
`radar alpaca-order-preflight` joins these observations and local liabilities to
the pure equity risk kernel. It performs paper-host reads and reports refusals;
unverified capacity/cost inputs remain unknown. It cannot submit an order. See
[equity risk work](docs/plans/0033-alpaca-equity-risk.md).
The private `radar alpaca-paper-submit` command uses the same collected state,
checks deterministic risk and the serialized request, and durably records
uncertainty before a fixed paper-only POST. It reloads owner policy/time before
the effect and refuses existing client IDs. Actual capacity/cost inputs remain
unknown, so this path currently refuses before reserving or sending. Service
isolation, site handover and unattended recovery remain required; see
[paper submission work](docs/plans/0034-alpaca-paper-submit.md).

`radar alpaca-paper-cancel --journal <private-file> --account-id <paper-id> --client-order-id <retained-id>`
binds a working simple order to Radar's retained intent before requesting its
cancellation on the fixed paper host. It records cancellation uncertainty before
DELETE, refuses repeated requests, and retains cash/share claims even after HTTP
204 acceptance. Reconcile fills and terminal observations separately; acceptance
does not establish that filling stopped. This is a private operator command,
not an installed unattended supervisor. See
[paper cancellation](docs/plans/0046-alpaca-paper-cancellation.md).

`radar alpaca-paper-maintain --journal <private-file> --account-id <paper-id> --max-pages <positive-u16> [--cancel-working]`
performs one recovery tick under the existing journal owner. It requires an
already retained cash checkpoint before reading credentials or broker facts,
then refreshes cash/share/order state, reports working, terminal and unresolved orders,
and optionally requests cancellation of at most one working order. Pending
cancellation uncertainty is observed rather than retried. Checkpoint failure
stops the tick; cancellation acceptance retains claims. This is the maintenance
phase for a future isolated supervisor, with no new-order authority or installed
timer. See [paper maintenance](docs/plans/0051-alpaca-paper-maintenance.md).
An existing empty file or reservation-only history is not a fresh account;
initialization is a separate operator action. See
[maintenance genesis guard](docs/plans/0053-alpaca-maintenance-genesis.md).

The separate maintenance worker and systemd pair now have source under deploy.
Its root-owned configuration defaults to observation; optional cancel_working
requests one retained-order cancellation per tick. It refuses a missing private
journal and keeps brokerage credentials out of model processes. Provisioning,
verified journal genesis and timer activation remain pending. A separate
single-use initializer and pinned fresh-install setup now have source; neither
enables unattended entries. See [scheduled maintenance](docs/plans/0052-alpaca-maintenance-worker.md)
and [initialization/provisioning](docs/plans/0054-alpaca-execution-initialization.md).

Use protected storage in an existing durably provisioned directory. This does
not establish available capital, profit or trading authority. See
[durable cash work](docs/plans/0030-alpaca-durable-cash.md).

Cash checkpoints now retain complete bound execution progress and derive the
remaining order liabilities: unfilled buy limit cost, unfilled sell shares, and
the full fee reserve. Confirmed terminal orders have no unfilled quantity claim.
Original intents and submission uncertainty remain; retries are still refused.
These observations are not available capital or trading authority. See
[economic claim work](docs/plans/0031-alpaca-economic-claims.md).

`radar alpaca-market-read --symbols SPY,QQQ --output ./data/alpaca/capture-001.json`
captures the explicitly requested IEX feed using the same private paper keys.
Create the output directory first and use a new filename for each observation;
existing captures are never replaced. Replay without credentials using
`radar alpaca-market-replay --input ./data/alpaca/capture-001.json --as-of-unix-ms <N>`.
The replay gate uses host availability time, preserves missing symbols as null,
and does not certify exchange timestamps, quote freshness or consolidated coverage.
The commands are operator tools. A separate bounded recorder schedules research
captures; autonomous broker execution remains pending.

`radar alpaca-quote --input ./data/alpaca/capture-001.json --symbol SPY --as-of-unix-ms <N> --max-age-ms 1000 --max-spread-bps 10`
checks a captured quote against explicit age/spread bounds, the capture's requested
symbols and availability, and its nanosecond UTC exchange timestamp. Quote
provenance must match the supported IEX pattern: both exchanges V,
exactly condition R and tape A/B/C. Missing or other metadata is refused;
matching it does not establish market hours or tradability. Prices and
displayed sizes are parsed as exact integers at six decimal places; size units
remain unverified and cannot yet bound executable share quantity. Unsupported
precision and exponent notation are refused. This produces quote inputs only,
not an account mandate, asset-eligibility verdict or permission to place orders.

`radar alpaca-paper-score --entry <capture> --entry-as-of-unix-ms <N> --exit <capture> --exit-as-of-unix-ms <N> --symbol SPY --quantity-nanoshares <N> --assumptions <json>`
scores one offline long trade at entry ask and later exit bid. Each capture must
have been available at its own watermark and pass the quote checks above. Required
assumptions include model_id, displayed_size_unit (shares), participation_bps,
slippage_bps_per_side, fee_per_side_microusd, max_quote_age_ms and max_spread_bps.
There are no defaults. Fees and adverse slippage are charged on both sides;
spread is already included in ask-to-bid prices. The report retains both quotes
and all assumptions. Capacity is explicitly assumed, broker P&L is not claimed,
and nothing from this scorer enters execution risk state. It is an outcome
measurement tool; it does not establish held-out performance or choose trades.

`radar alpaca-paper-decision --capture <capture> --symbol SPY --proposal <json> --assumptions <json> --output <new-file>`
records a local research choice using the host clock and a capture available at
that cutoff. Proposal fields are strategy_id (include version), action (enter or
wait), quantity_nanoshares, horizon_ms and reason. Quantity also fixes the
same-size always-enter baseline; cash is the second baseline. Cost assumptions
use the scoring schema above. All are copied into the new record, including raw
entry data; existing files refuse publication. No model call or order is made.
`radar alpaca-paper-decision-score --decision <record> --exit <capture> --exit-as-of-unix-ms <N>`
requires exactly the frozen horizon watermark. Unavailable or invalid exit data
and excess assumed participation produce an explicit unscoreable outcome, with
unknown comparison values, rather than dropping the case or assigning zero.
Records are mutable local research files, not authenticated strategy provenance.
This does not yet prove cohort completeness, a learned strategy or forward gains.

`radar alpaca-paper-propose --capture <capture> --symbol SPY --experiment <json> --assumptions <json> --output <new-file>`
calls the configured linked subscription provider for a strict Enter/Wait reply.
The fixed experiment requires strategy_id, quantity_nanoshares and horizon_ms;
the model controls only action and reason. Observations use the existing untrusted
fences. The quote is revalidated when the answer returns, and that completion
cutoff starts the evaluation horizon; there are no pre-answer simulated fills.
The caller requires explicit unlimited subscription allowance with an adequate
per-call bound. Finite daily allowances need shared durable metering first; paid
API providers cannot use this path. No credentials are handled by this caller.
Malformed/late/failed answers refuse without creating a decision. The separate
research worker retains scheduled attempts, including failures, under a frozen
campaign; see [research provisioning](docs/plans/0044-alpaca-research-provisioning.md).
There is no verified model edge or execution authority from this command.

`python3 deploy/report-alpaca-cohort.py <research-store> <capture-tape> <as-of-unix-ms>`
reports every planned slot, including failed, missed, interrupted and unrecorded
attempts. Completed decisions are compared at their exact frozen horizons using
the latest capture available by that horizon. Ties or absent/invalid exit quotes
remain unscoreable. Scoring uses the separately installed research CLI and receives
no brokerage credentials. This is a manual read-only report, not an installed
scoring service. Overlapping hypothetical outcomes are not added into portfolio
profit; aggregate profit remains unknown. Local records do not authenticate model
provenance. See [cohort evaluation](docs/plans/0045-alpaca-cohort-report.md).

`radar alpaca-reconcile --account-id <expected-paper-account-id>` pins the account
identity and brackets positions/market-clock reads with account and open-order
reads. It preserves pending orders and nested legs, reports changes in selected
fields, and refuses full 500-order responses. Stable fields do not prove an atomic
snapshot or complete economic accounting. It uses the same private paper keys
and cannot submit orders. See [reconciliation work](docs/plans/0018-alpaca-paper-reconciliation.md).

The same report includes conservative exposure bounds for supported simple equity
limit orders and long positions. It reserves original pending quantities, rounds
funds down and exposure up, and reports unsupported or changing states with null
bounds. Fees, durable local claims, freshness and complete economic reconciliation
remain unresolved; these bounds are not available capital or trading authority.
See [exposure accounting](docs/plans/0020-alpaca-paper-exposure.md).

`radar alpaca-activities-read --account-id <paper-id> --max-pages 10` retrieves
paginated fill and nontrade cash/fee observations between pinned account reads.
The page limit bounds requests, not capital; hitting it refuses partial history.
Exhausting pagination does not prove economic completeness or final fees, and
never releases reservations. See [activity reconciliation inputs](docs/plans/0027-alpaca-account-activities.md).

`radar alpaca-order-read --account-id <expected-paper-account-id> --client-order-id <retained-id>`
queries a specific paper order between pinned account reads. It preserves selected
fill/terminal observations without authorizing a retry or releasing claims.
HTTP 404 means not visible, not proof that submission failed; other errors refuse
without partial output. See [recovery reads](docs/plans/0021-alpaca-order-recovery-read.md).
Add `--journal <private-file>` to compare the broker payload with a retained
intent under journal ownership. Only exact simple day equity limit payloads with
extended hours disabled can match. Missing orders stay unresolved; matching does
not reconcile fills/fees, free a claim or authorize execution. See
[intent binding](docs/plans/0026-alpaca-intent-binding.md).

`radar alpaca-order-reconcile --journal <private-file> --account-id <paper-id> --client-order-id <retained-id> --max-pages 10`
connects retained intent, matched order reads and paginated activities under journal
ownership. It requires unchanged selected order/account observations, matches
execution quantity intervals and computes gross USD bounds with exact arithmetic.
It retains the entire original cash/share claim. Gross execution consistency does
not establish final fees, account cash, complete history or permission to trade.
See [execution reconciliation](docs/plans/0028-alpaca-fill-reconciliation.md).

`radar alpaca-cash-reconcile --journal <private-file> --account-id <paper-id> --baseline <prior-activities-report.json> --max-pages 10`
compares a prior private `alpaca-activities-read` JSON report with current broker
observations, binding every retained order around the activity reads. It refuses
changed/disappeared baseline records, unbound new fills and unsupported new cash
activity types. Exact trade cash flow, deposits/withdrawals and observed USD fees
are reported separately. A matching cash balance is not measured profit or final
economic reconciliation; all claims remain held. Baseline input is bounded to
16 MiB and must remain in protected operator storage. See
[cash accounting](docs/plans/0029-alpaca-cash-accounting.md).

`radar alpaca-order-reserve --journal <private-file> --account-id <paper-id> --intent <typed-intent>`
records an immutable paper claim. `radar alpaca-order-journal --journal <private-file> --account-id <paper-id>`
reopens and verifies it under exclusive cooperative ownership. These operator
commands need no broker keys and cannot submit, retry or release an order.
See [durable paper claims](docs/plans/0022-alpaca-order-journal.md).

`radar alpaca-assets-read --account-id <paper-id> --symbols SPY,QQQ` reads
identity-bound equity assets between pinned paper account observations. Missing
tradability and fractional-share flags remain unknown. This does not authorize
orders. See [asset checks](docs/plans/0023-alpaca-asset-observations.md).

**Status: recording, and refusing to trade.** The pipeline runs end to end on a
live instance — recording launches, measuring what became of them, and running
the full decision lane over what it has recorded. 479,564 launches, 1,326,862 outcome measurements and 7,543
replayable decisions as of 2026-09-03.

**Nothing has ever traded, and no edge has been found.** The measured selection
edge is **0 bps** ([0017](docs/research/0017-a-control-that-could-have-been-traded.md))
against a bar of roughly **456 bps** — the expected edge a strategy must clear
before a single trade is worth making
([0022](docs/research/0022-capacity-was-a-budget-not-a-ceiling.md)). The shipped
`Policy::CLOSED` refuses every proposal, and on that arithmetic it is the correct
position rather than a placeholder.

The trading lane is **composed and shut**, and an earlier version of this section
said it was *"built and tested end to end"* with *"nothing missing"*. That
sentence is exactly what [LEARNINGS](LEARNINGS.md) 10 retracts, so here is the
distinction it was hiding:

- **Composed** — `radar-exec`'s composition tests run strategy → kernel →
  signer → executor, and `Policy::CLOSED` refuses what a permissive policy
  authorises. [docs/STATE.md](docs/STATE.md) owns the account of what those
  tests do and do not establish, for the local lane and the customer one, and
  names them.
- **Not exercised** — nothing has been signed by a wallet, sent, or filled.
  Two production crates depend on `radar-exec`: `radar-cli`, which reaches
  `radar_exec::route` for `radar route` and prints unsigned bytes; and
  `radar-serve`, whose `/v1/market/quote` and `/v1/customer/swap` routes
  (Plan 0013 Phase D, ADR 0024) reach the same router to price and assemble
  an unsigned swap transaction for a signed-in wallet to sign itself. Neither
  reaches past the router: nothing in production reaches `pipeline::execute`,
  the signer client, the submitter or `customer_signing`, so **there is no
  production caller for the trading path**, and writing one is a decision
  about money rather than a wiring task. `repo-conformance`'s
  `the_documented_dependency_claims_are_true` pins all three statements; the
  earlier claim that *nothing* depended on `radar-exec` was false from
  2026-08-31 and went uncaught for three days (LEARNINGS 29).
- **`Policy::CLOSED` has never refused a real proposal, because it has never been
  handed one.** A live run over 41,254 candidates raised zero proposals — the
  cause was a hardcoded probe size that made a proposal arithmetically
  impossible, not a market offering nothing (LEARNINGS 10).

**The trading lane is frozen** for the duration of the current work. It unfreezes
on one condition: a measured edge at or above that 456 bps bar, in a stratum
Radar can size into, on data that was not used to find it.

`radar consider` prints exactly what the system would have done and why it did
not do it. On the first live run it considered 3,924 recorded launches, spent
nothing on paid analysis because no paid call could have changed an answer, and
proposed nothing — see
[docs/research/0005](docs/research/0005-first-end-to-end-decision-pass.md).

## What this is for

Radar is built to answer one question honestly: **does a measurable edge exist in
this market, and if so, where?**

That framing is deliberate, because the base rate is unforgiving. In April 2026 —
one of the strongest months in two years — roughly 96% of pump.fun wallets either
lost money or made under $500, and only 5.4% cleared $1,000. Meanwhile the
lowest-latency public data path shut down, and the firms still winning on speed
are buying private fiber. Latency is not an edge available here.

What is available is unglamorous and measurable:

- **Avoidance** — not buying the traps. Cheap, deterministic, compounding.
- **Exit-first sizing** — most losses are inability to exit, not bad entries.
- **Cost discipline** — fees, tips, slippage and failed transactions are a large
  share of retail P&L.
- **Composition** — creator history × funding graph × social/on-chain divergence
  is hard to compute, which is why it may not be arbitraged away yet.

So the first thing Radar builds is not a strategy. It is a **recorder with a
point-in-time guarantee**, because every signal has to be validated against
outcomes, and the dataset only accumulates forward.

## The idea that shapes the architecture

Radar buys its data per call. That turns every design decision into arithmetic,
and two answers fall straight out:

**Decode locally.** Parsed transactions cost $0.05 each; a launch analysis touches
50–200 of them. But `getBlock` returns *every* transaction in a slot for $0.001 —
including the create, the dev buy, and every same-slot coordinated buy. Decoding
in Rust rather than buying it parsed is worth roughly **100–300×** on the bill,
and it is the real justification for the Rust layer.
See [ADR 0001](docs/adr/0001-decode-locally-never-buy-parsed-transactions.md).

**Never pay twice for the same fact.** Every fact carries a *mutability class*.
A token's structural facts are fixed within seconds of launch and consulted on
every re-evaluation, so they are fetched once per mint and never again. Authority
revocations are one-way latches: watched until they close, then never asked about
again. This needs nothing from any vendor and it is the largest saving available.

## Layout

| Crate | What it does |
|---|---|
| `radar-types` | Domain vocabulary. Slots as the only clock, integer money, mutability classes, provenance and trust tiers. |
| `radar-asof` | Point-in-time correctness. A watermark applied at every boundary function that reads the store — four `admits` call sites plus one hand-rolled gate, not a compile-time guarantee. LEARNINGS 9 retracts the stronger claim this row used to make. |
| `radar-provider` | Spend metering, and now only that. Pure policy — no HTTP, no clock, no async. `Meter`/`Ledger`/`Budget`/`Commitment` run: `radar-agent` reserves before every model call and `radar-serve` persists the ledger across restarts. Its cache, breaker and planner had no caller anywhere and were **deleted on 2026-09-04**, taking the crate from 1,897 lines to 484. |
| `radar-agent` | The boundary a reasoning layer sits behind. Pure policy: the model gets read-only tools, observed text is fenced as data, and its answer is text nobody parses into an action. |
| `radar-model` | Reaching a model provider: the vendor CLI as a subprocess that owns its own credential, or a metered API key. The impure edge `radar-agent` deliberately does not have. |
| `radar-decode` | Solana program decoders. Matches Anchor discriminator bytes, never logged instruction names; an unrecognised discriminator is a recorded value, never a guess. |
| `radar-store` | Append-only, slot-partitioned Parquet event log. Nulls mean "not recoverable", never zero; the watermark reaches onto disk. |
| `radar-stream` | The live market feed: a Yellowstone gRPC subscription read as balance changes, so one fold covers every venue, held in a bounded in-memory tape the market routes read. Off unless `RADAR_STREAM_ENDPOINT` is set. |
| `radar-backfill` | Bulk historical extraction from CryptoHouse, decoded by the same decoder the live recorder uses. |
| `radar-instruments` | The instrument registry. One declaration; internal, HTTP, x402 and MCP surfaces derived from it, and every invocation recorded. |
| `radar-serve` | Ops page, JSON API, and stateless MCP (2026-07-28). The public analyst's site moved to realorrug-serve with the bot (ADR-0024); this crate's public surface is now `/v1/market/*` only. |
| `radar-sim` | Exit analysis. Structural disqualification from the mint account, then a measured sell curve — never a single liquidity number. |
| `radar-risk` | The risk kernel. A pure function from a proposal to a verdict — the only thing that can authorise capital. |
| `radar-strategy` | Deterministic strategies. They emit proposals, which are inert data, and assemble candidates in one place so look-ahead is prevented once rather than per strategy. |
| `radar-graph` | Coordination detection. Scores what a launch block contained. Its thresholds derive from [0008](docs/research/0008-the-launch-block-gives-the-bundle-away.md), which [0024](docs/research/0024-the-spike-became-a-hump-and-the-signal-moved.md) superseded on 17,497 launches: exactly six recipients is **25.1%** of instant graduations, not 68%, and the strongest band has moved to ten-to-thirteen. The rule has not been re-derived — but **the count is now recorded** alongside the verdict (`Decision.launch_recipients`), which is [ADR 0012](docs/adr/0012-the-launch-block-count-is-recorded-not-the-threshold-retuned.md)'s first commitment and what makes the next re-derivation a query over the store rather than another chain scan. |
| `radar-research` | Replay. Re-runs a recorded decision at its original watermark and separates a store that gained history from a strategy that is not a pure function of its inputs. Also builds and reads the creator index and the base-rate snapshot — moved here from `radar-roast` when the bot that read them left for its own repository (ADR-0024). |
| `radar-signer` | A separate process holding the key. Re-decodes every transaction and trusts nothing the caller said about it. |
| `radar-exec` | Route, gate, sign, submit, reconcile. The last stage, and the one holding the least authority. |
| `radar-pda` | Pure Solana program-derived address calculation shared by transaction builders and the independent signer. |
| `radar-pumpfun` | Builds pump.fun instructions and prices them off the bonding curve. Pure: no clock, no network, no key. The curve is `x·y=k` over published reserves, so a fill is computed rather than quoted — see ADR 0009. |
| `radar-customer` | The customer model. Pure: a bounded grant derived from the kernel's authorisation, and a signature meter. No account table — see ADR 0006. |
| `radar-onchain` | The on-demand chain read behind `radar dossier`. Rebuilds a token's launch block and prices its curve from RPC in a bounded number of calls, for a mint that may be forty seconds old — which the store cannot answer, being minutes behind by design and holding the launch-block verdict rather than the number. Also reads mint/curve/fees in one RPC context for the hypothetical `radar curve-exit` command. Read-only: no key, no signer in its tree. |
| `radar-journal` | What the autonomous surface decided, written **before** it acted. One JSON object per line, hash-chained: each event carries the id of the one before it, so an alteration is visible from a later event — relative to a trusted checkpoint, which is what a chain buys and all it buys. Reading one back keeps three answers apart that an operator must not confuse: nothing was ever written, the last write was interrupted, and something wrote to this file that was not the journal. A write that fails blocks the effect it was recording. ADR 0017. |
| `radar-cli` | The operator command line. Reads live state and computes nothing it does not have. |

Every crate in that table exists and is a workspace member; `repo-conformance`
fails the build if the table and the workspace disagree. (This paragraph
previously listed graph, sim, strategy, exec, signer and research as *planned*,
five lines after the table listed them as built.)

## The safety invariant

Borrowed from GitLocus and applied to money: **model judgement never authorises
capital.**

```
strategy or model  --emits-->  Proposal       inert data, zero authority
risk kernel        --emits-->  Authorization  pure fn; nonce, expiry, hard bounds
signer process     --emits-->  Signature      re-derives the tx, trusts nothing
```

`radar_risk::evaluate` has no clock, no network and no ambient state — the
current slot is an argument and so is the portfolio. So every past decision can
be replayed, every refusal is reproducible from a recording, and any decision can
be re-judged under a tighter policy without ever having run it that way. The
default policy refuses everything.

## What it does today

Recording continuously from the chain, measuring each token at roughly one hour,
six hours and a day after launch, and serving the result:

```
$ radar creators --store ./data/store -n 3
distinct creators: 1148  (418 launched more than once)

LAUNCHES  CREATOR
     88  6LdWMVxj6R7683M9ioAcaFNRfUhcr9v9K2xNjYd9Fnbx
     44  EH7aHeLEz8wd9wqeMuT364ntXM93j6Mo5cbmjQFhKY9S
     41  bwamJzztZsepfkteWRChggmXuiiCQvpLqPietdNfSXa
```

That first address, through `creator_track_record`: 88 launches, 39 of them old
enough to have been measured, **100% stillborn**, median survival **0 slots**.
The population rate across all measured tokens is ~35%.

The instrument reports `measured=39` rather than 88 because the other 49 have not
yet reached their first checkpoint. It says what it knows and no more, which is
the same reason it will not state a rate below five measured launches.

Whether creator history *predicts returns* has since been measured, and the
answer is not the encouraging one. A creator with a prior organic graduation
graduates again at about **1.69× the base rate**
([0007](docs/research/0007-does-creator-history-predict-anything.md)) — but
graduation predicts **volatility, not profit**: organic graduations end at a
median **−3,228 bps** against −853 for tokens that never graduated
([0011](docs/research/0011-graduation-predicts-volatility-not-profit.md)). A
creator's graduation history is not a good sign, and nothing built on this data
may imply that it is.

## Trying it

```bash
cargo run -p radar-backfill -- --from '2026-08-21 06:00:00' --to '2026-08-21 06:30:00' --store ./data/store
cargo run -p radar-cli -- creators --store ./data/store
RADAR_STORE=./data/store cargo run -p radar-serve
```

The last one serves an ops page at `/`, the instrument catalogue at
`/v1/instruments`, and a stateless MCP endpoint at `/mcp`. Locally, with
`RADAR_ACCESS=off`, these are open. The x402-priced public surface appears only
when `RADAR_X402_PAY_TO` and `RADAR_X402_FACILITATOR` are set — it is never
served free as a fallback.

### What is live

On the deployed instance at `radar.heyvera.org`, `/v1/instruments` and `/mcp`
sit behind Cloudflare Access — `curl` against either gets a 302 to Cloudflare's
login, not a response, so no outside agent reaches them today (issue #186).
What an outside agent or browser actually gets for free is `/v1/market/*`
(coins, launches, candles, quotes — the public market-data tier) and the
terminal UI at `radar.heyvera.org` itself. `/x402/*` classifies as public in
`crates/radar-serve/src/access.rs`, but the deployment does not set
`RADAR_X402_PAY_TO`/`RADAR_X402_FACILITATOR`, so a request there falls through
to the same application shell rather than pricing anything. This section
describes the wall as it stands, not a plan to move it — see
[docs/STATE.md](docs/STATE.md) for the fuller, dated account.

## Measured, not assumed

`scripts/probe/` holds the scripts that produced every number in
[docs/research/](docs/research/), because a figure without the thing that
produced it is a claim rather than a measurement. Running them before writing
decoders overturned three assumptions while they were still cheap to change —
see [0004-measured-launch-base-rates.md](docs/research/0004-measured-launch-base-rates.md).

## Building

```bash
just check
```

Requires the stable Rust toolchain and [`just`](https://just.systems). On Windows
under Git Bash, `export RADAR_CARGO="cargo +stable-x86_64-pc-windows-gnullvm"`
first — [AGENTS.md](AGENTS.md) says why.

## Documentation

- [AGENTS.md](AGENTS.md) — how to work here, and the invariants that are not
  negotiable.
- [docs/adr/](docs/adr/) — decisions, and what each one costs.
- [docs/research/](docs/research/) — the data-sourcing landscape, the
  freshness/caching design, and the vendor feature requests that would change
  the architecture if granted.

## Licence

Apache-2.0. See [LICENSE](LICENSE).
