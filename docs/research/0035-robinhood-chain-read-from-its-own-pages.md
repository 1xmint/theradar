<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0035 — Robinhood Chain, read from its own pages

**Date:** 2026-09-13
**Status:** read, not captured. Every fact here comes from a page, read on the
date above, and **no transaction on Robinhood Chain has been read by this
repository yet**. AGENTS.md §1 applies: a reference proposes, a capture
disposes. The launcher facts in §3 are the ones a capture must settle before
anything is launched. Prices go stale monthly.
**Feeds:** [design 0019](../design/0019-realorrug-on-robinhood-chain.md) and
[ADR 0023](../adr/0023-realorrug-lives-on-robinhood-chain-and-the-bot-moves-with-it.md).
**Corrects:** [design 0010](../design/0010-close-the-remainder-then-raise-the-ceiling.md)
§1.4 carried Robinhood Chain facts from trade press, unverified. §1 confirms
them from first-party pages, except "four memecoin launch routes", which no
first-party page states.

## 1. The chain

| fact | value | source |
|---|---|---|
| what it is | "an Arbitrum Layer-2 Chain built on Ethereum, using Ethereum blobs for data availability and ETH as the native gas token" | [docs.robinhood.com/chain/connecting](https://docs.robinhood.com/chain/connecting) |
| mainnet | 2026-07-01 | [Robinhood newsroom, Jul 1 2026](https://robinhood.com/us/en/newsroom/robinhood-accelerates-global-expansion-robinhood-chain-mainnet-stock-tokens-agentic-trading/) |
| chain ID | 4663 (testnet 46630) | connecting page |
| public RPC | `https://rpc.mainnet.chain.robinhood.com`, "rate-limited and not recommended for production use" | connecting page |
| explorer | robinhoodchain.blockscout.com | connecting page |
| permissionless | "Anyone can interact with the network, build applications, and deploy smart contracts." | [docs.robinhood.com/chain](https://docs.robinhood.com/chain) |
| RPC providers | Alchemy (recommended), QuickNode, Blockdaemon, dRPC, Validation Cloud | connecting page |
| bridge partner | LayerZero | chain docs |
| data partners | Allium, Entropy Advisors, Zerion; Chainlink Data Streams (price oracle, not chain data) | chain docs; [/chain/data-streams](https://docs.robinhood.com/chain/data-streams) |

## 2. What "agentic" means in Robinhood's own words

The July 1 announcement's agent features are **brokerage features**:

> "Using our Trading MCP, eligible US traders can connect their AI model of
> choice to Robinhood data sources and tools."

"Agentic Accounts for crypto" are to "begin rolling out soon to eligible US
traders". Both attach an AI to a Robinhood account. **The chain documentation
mentions no agent tooling.** The announcement calls the chain "AI-native",
without saying what that means in code.

Per AGENTS.md §2, this settles one thing: **nothing shipped on the chain today
is an agent feature Solana lacks.** It does not settle whether one will ship.
What is real, and is a property of an EVM chain rather than of Robinhood's:
Alchemy's Gas Manager (a sponsor pays a user's gas) is live on Robinhood Chain
([Alchemy blog](https://www.alchemy.com/blog/robinhood-chain-mainnet-is-live-on-alchemy)),
and audited contract building blocks for claims, splits and time-locks are
mature on EVM.

## 3. Launchers, and the fee currency

ADR 0013 constraint 2 turns on **what currency the creator fee is paid in**,
so that is the column that matters.

| launcher | creator fee | paid in | supply | source | verified? |
|---|---|---|---|---|---|
| **Bankr** (Doppler, Uniswap v4) | 0.665% of volume (95% of a 0.7% pool fee); traders pay 1.75% all-in | **"your token and WETH"** | 15% vests to the fee recipient by default; "you can turn vesting off" | [docs.bankr.bot overview](https://docs.bankr.bot/token-launching/overview/), [FAQ](https://docs.bankr.bot/faq/token-launching/) | first-party docs |
| **Pons v2** | not found | "the pairing asset (which is ETH by default)" | "Full supply mints to a bonding curve"; graduates to a Uniswap v4 pool with locked liquidity | [cryptonomist, 2026-07-23](https://en.cryptonomist.ch/2026/07/23/pons-v2-upgrade-eth-bonding-curve/); launch fee 0.0005 ETH per [launchpad.family](https://www.launchpad.family/) ("checked on-chain, 2026-09-08") | **no: trade press and a third party** |
| pump.fun (Solana, for comparison) | 30 bps on the curve; a ladder after graduation | SOL | full supply to the curve | [research 0023](0023-the-fee-is-a-schedule-and-the-published-interface-is-incomplete.md), [0028](0028-the-fee-after-graduation-is-a-ladder.md) | captured |

Bankr also says: the fee recipient "is locked when the token is created and
can't be reassigned"; the swap fee "starts at 80%" at launch and decays over
about ten seconds (**who receives that fee was not found**); and on Robinhood
Chain "retail launches are not gas-sponsored and the launch wallet pays network
gas."

Launchers die. Noxa, a Robinhood Chain launchpad, stopped on 2026-07-13
([cryptoticker](https://cryptoticker.io/en/robinhood-chain-memecoins-explained/),
trade press).

## 4. Activity

DefiLlama's API, read 2026-09-13:

| | DEX volume, 24h | 7d | 30d |
|---|---|---|---|
| Robinhood Chain | $1.50B | $13.06B | $33.89B |
| Solana | $1.74B | $19.19B | $73.76B |
| of which pump.fun / PumpSwap | $54M / $377M | | |

[api.llama.fi/overview/dexs/robinhood](https://api.llama.fi/overview/dexs/robinhood),
[…/solana](https://api.llama.fi/overview/dexs/solana). **The Robinhood figure
is not split by asset**: it almost certainly includes stock tokens, so it is not
a memecoin number. No memecoin share was measured.

## 5. Data prices

| provider | free | first paid step | source |
|---|---|---|---|
| Alchemy (Robinhood Chain) | 30M compute units a month, 25 requests a second, 5 webhooks; Token/Transfers/Portfolio APIs **not** free | pay as you go, $0.525 per million units | [alchemy.com/pricing](https://www.alchemy.com/pricing) |
| Helius (Solana) | 1M credits a month, 10 requests a second, webhooks, DAS at 2/s | Developer $49 a month, 10M credits; LaserStream gRPC from Business, $499 | [helius.dev/pricing](https://www.helius.dev/pricing) |
| Dune (Robinhood Chain history) | raw tables, decoded logs and traces, `erc20_robinhood.evt_*` | **price not read** | [Dune docs](https://docs.dune.com/data-catalog/evm/robinhood/overview) |
| CryptoHouse (Solana history) | free, in use | — | [ADR 0002](../adr/0002-historical-data-comes-from-cryptohouse-not-a-vendor-archive.md) |

A per-method compute-unit table (for example `eth_getLogs` at 60 units) was seen
only on a third-party summary, not on Alchemy's page, so it is not relied on.

## 6. Not established

- Pons v2's creator fee rate, fee currency and zero allocation, from its
  contract or a real launch. **Launch-blocking.**
- Who receives Bankr's launch-window fee.
- Robinhood Chain's memecoin share of volume.
- Dune's price for this use.
- Whether any Robinhood Chain memecoin has reached the Robinhood app. One
  (CASHCAT, 2026-08-06) is reported by trade press only.
