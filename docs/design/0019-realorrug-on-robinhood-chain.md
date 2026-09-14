<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0019 — realorrug on Robinhood Chain

**Status:** **converged with Josh, 2026-09-13**; the decisions are recorded in
[ADR 0023](../adr/0023-realorrug-lives-on-robinhood-chain-and-the-bot-moves-with-it.md).
This document is the reasoning: the four questions, the options, the
recommendation and where it is weakest. Nothing in it has been built, bought or
launched.
**Date:** 2026-09-13.
**Reopens:** [design 0010](0010-close-the-remainder-then-raise-the-ceiling.md)
V7 and §9.3, "no second chain", at Josh's request.
**Facts from:** [research 0035](../research/0035-robinhood-chain-read-from-its-own-pages.md).
Every price and chain fact below is cited there with its source and date; this
document does not repeat the links.
**Scope:** the community token and what it needs from the bot and the repo.
Not the trading lane.

## 1. What Josh asked

The token is renamed from cabalhunter to **realorrug**. He wants it launched as
soon as possible, on Robinhood Chain rather than pump.fun, "because robinhood is
designed for agentic usages", and cheaply: good data, not the firehose Radar's
trading side wants. He asked whether the bot should live in its own repository.

Four questions follow from that.

## 2. What data the token and its community need

**The token needs very little, and none of it is fast.**

| need | what it is | how | chain |
|---|---|---|---|
| the launch block | proof that the curve is the only recipient (ADR 0013 constraint 1) | one transaction read, once | token's |
| trades and volume | the pool's swap events | log query on a timer, or a webhook | token's |
| holders | the token's transfer events | same | token's |
| fee accrual | what the prize is this week | balance or event read, hourly is plenty | token's |
| payout proof | the sent transaction read back: recipient and amount | one read a week | token's |
| the bot's dossiers | facts about a token someone asks about | as today, per summoned reply | Solana, until the bot reads Robinhood Chain |
| base rates and history | population facts the sheet cites | as today, from CryptoHouse | Solana |

**Near-real-time here means minutes, and a webhook delivers it.** No item above
is decided in under a second, so no item needs a stream.

**What it costs, at our volume:**

| | Solana | Robinhood Chain |
|---|---|---|
| free tier | Helius: 1M credits a month, 10 requests a second, webhooks | Alchemy: 30M compute units a month, 25 requests a second, 5 webhooks |
| what the token uses | a few thousand reads a month | one log query a minute is 43,200 calls a month; a call would have to cost about 700 units to exhaust the free tier |
| the bot's dossiers | 35–150 credits each, so 6,000–28,000 a month free (design 0007 J6) | not built |
| history | CryptoHouse, free | Dune indexes the chain; **price not read** |
| first paid step | $49 a month | $0.525 per million units |
| **total now** | **$0** | **$0** |

**Data cost does not decide the chain.** What decides it is §4.

### 2.1 PR #248, the Yellowstone stream

**Not bought on the token's or the bot's account.** Neither consumes a stream:
everything in the table above is minutes-scale and fits a free tier. The PR
serves the trading terminal's market pages; whether that is worth buying is the
terminal's own decision and is not made here.

## 3. Costs side by side, beyond data

| | pump.fun (Solana) | ETH-fee curve on Robinhood Chain |
|---|---|---|
| launch | network fee (not re-read this session) | 0.0005 ETH launcher fee (unverified) plus gas |
| weekly payout transaction | fractions of a cent | cents |
| creator fee to the prize | 30 bps on the curve, in SOL | **unknown until captured** (Bankr's is 66.5 bps, but in the token and WETH) |
| code before week 1's prize | exists | payout, fee reader, own-token reader: new |
| a claim program later (ADR 0013 defers it) | a custom Solana program, audited from scratch | standard audited EVM building blocks, still audited |

**Cash is negligible on both.** Josh's rule was "if the cost is negligible,
Robinhood": that is what was applied. **The real cost of Robinhood Chain is
build time before launch**, not money.

## 4. Robinhood Chain instead of Solana

### 4.1 The reason given, checked

"Robinhood is designed for agentic usages" does not survive a read of
Robinhood's own pages. Its agent features are a Trading MCP and Agentic Accounts
for eligible US brokerage customers, and the chain docs mention no agent
tooling. Research 0035 §2.

**The reason that does hold is an EVM reason plus an audience reason.**

- **Rewards and the flywheel.** A claim, split or time-lock contract on EVM is
  assembled from audited parts; on Solana it is a new program.
- **Gas sponsorship.** Alchemy's Gas Manager is live on the chain, so a winner
  or an agent can act without holding ETH.
- **Agent wallets are not a difference.** Privy, already in use
  ([ADR 0007](../adr/0007-the-privy-authorization-key-lives-in-the-signer-process.md)),
  serves both.
- **Robinhood's audience and brand** are specific to this chain rather than to
  Base or Arbitrum, and for a meme that has to become famous, that counts.

### 4.2 For

- A less crowded launch surface than pump.fun's ~41,000 launches a day
  ([ADR 0002](../adr/0002-historical-data-comes-from-cryptohouse-not-a-vendor-archive.md)).
- DEX volume of the same order as Solana's ($1.50B against $1.74B, 24h,
  2026-09-13), though not split by asset.
- The rewards contract ADR 0013 deferred is cheaper to build safely.
- Bridging to Solana later keeps one token and one prize.

### 4.3 Against

- **The bot reads Solana.** Until it reads Robinhood Chain, it cannot state its
  own token's launch block (constraint 1) or roast it (constraint 6).
- **Launchers pay in the wrong currency or are unverified.** Bankr pays in the
  token; Pons v2 is trade press.
- **Launchers die.** Noxa stopped 2026-07-13, and the token outlives its launcher
  only if the pool does.
- **The audience the bot has built is Solana pump.fun traders.**

### 4.4 ADR 0013's six constraints, one by one

| # | constraint | on an ETH-fee curve on Robinhood Chain |
|---|---|---|
| 1 | no dev buy, no allocation | **holds** if the launcher mints full supply to the curve; Bankr's default 15% vesting must be off, so Bankr is not the route as-is. Must be proven from the launch transaction |
| 2 | operator holds zero tokens | **holds only with an ETH-denominated creator fee.** A fee paid in the token puts it in the operator's wallet every week. This is the constraint that picks the launcher |
| 3 | 100% of the creator fee is the weekly prize | **survives in principle, and the code does not.** [`radar-payout`](https://github.com/1xmint/realorrug/blob/main/crates/realorrug-payout/src/lib.rs) is pump.fun's `collect_creator_fee` plus a SOL transfer. [`radar-contest`](https://github.com/1xmint/realorrug/blob/main/crates/realorrug-contest/src/ledger.rs)'s rule has no chain in it, but its ledger does: the claim is "The Solana address, as text" and amounts are `lamports`. **Correction:** during the discussion I called the contest crate chain-agnostic. The rule is; the ledger's money types are not |
| 4 | entry is free, never requires holding | **unchanged**, and ADR 0015's nomination mechanism with it. A winner now claims with an EVM address |
| 5 | never states the token's price | **holds, with one change.** `RADAR_SELF_MINT` is parsed as a Solana address ([`realorrug:crates/realorrug-analyst/src/daemon.rs`](https://github.com/1xmint/realorrug/blob/main/crates/realorrug-analyst/src/daemon.rs)); it has to accept the token's EVM address, and a bridged Solana version's mint as well |
| 6 | roasted like anything else | **not met until the bot can read the token.** A narrow own-token reader first; general Robinhood Chain roasting later |

### 4.5 Recommendation

**Robinhood Chain as the permanent home, launched through a full-supply
bonding curve whose creator fee is paid in ETH, bridged to Solana later as the
same token.** Pons v2 is the candidate. **The launch waits on a capture**: a
real Pons v2 launch read from the chain, confirming the fee rate, the fee
currency and that the curve is the only recipient. If no launcher on the chain
pays the creator in ETH, constraint 2 fails and this recommendation reverts to
pump.fun.

**One token, one home.** Two launches split the holders and the prize, and a
token's home cannot be moved later.

## 5. Its own repository

### 5.1 What Josh chose, and what it costs

**A new `realorrug` repository, and the bot moves there now, before launch.** I
recommended moving the bot later, when it starts reading Robinhood Chain; Josh
chose now. The consequence, stated once: **launch waits for the move.**

### 5.2 The move is not a clean cut

The bot's crates are imported by Radar crates that stay behind. Counted from
`Cargo.toml` on `main`, 2026-09-13:

| crate | imported by |
|---|---|
| `radar-analyst` | `radar-serve`, `radar-cli`, `radar-backfill` |
| `radar-roast` | `radar-serve`, `radar-cli`, `radar-research` |
| `radar-contest` | `radar-serve`, `radar-cli`, `radar-payout`, `radar-analyst` |
| `radar-payout` | `radar-cli` |

What they import:

- `radar-serve` serves the receipts, the leaderboard and the pool page from the
  analyst's log and the contest ledger. **That is the bot's public face.**
- `radar-research` imports `radar_roast::creator` and `radar_roast::BaseRates`:
  research types that happen to live in the roaster.
- `radar-backfill` reads the analyst's log.

### 5.3 Recommended split

| goes to `realorrug` | stays in Radar, consumed by `realorrug` at a tagged git dependency |
|---|---|
| `radar-analyst`, `radar-roast` (minus the two research types), `radar-contest`, the public receipt/leaderboard/pool routes, a new EVM payout | `radar-types`, `radar-onchain`, `radar-model`, `radar-agent`, `radar-provider`, `radar-journal`, `radar-decode` |
| | `radar_roast::creator` and `BaseRates` move **down** into a shared Radar crate first, so `radar-research` stops importing the bot |

- **Preserve history.** Move with `git filter-repo` on the moved paths, not a copy.
- **`radar-payout` is not moved; it is replaced.** Its pump.fun path has no
  caller on Robinhood Chain. It stays in Radar's history, and is deleted in the
  move if nothing else calls it.
- **`radar-backfill`'s read of the analyst log** becomes a read of a file format,
  not an import.

**Where it is weakest:** `radar-serve` is one binary on one box
(AGENTS.md §7, `radar-deploy`). Splitting its routes across two repositories
means two builds and a deploy question this document does not answer. Recorded
as open, not solved.

### 5.4 Things the move must fix

- **The banned-words check refuses the bot's own name.**
  [`realorrug:crates/realorrug-roast/src/forbidden.rs`](https://github.com/1xmint/realorrug/blob/main/crates/realorrug-roast/src/forbidden.rs) line 314 matches
  by substring, so "rug" (line 60) fires on "realorrug", and on "drug" or
  "struggle". Line 311 already masks `OWN_DOMAIN`, which is still
  `cabalhunter.org` (line 289). The own-name mask follows that pattern.
- **The rule stays.** It is Radar's rule, not a chain rule, and it exists
  because calling a named project a "rug" in public accuses identifiable people
  of fraud. The meme and the rule fit together: the brand asks "real or rug?",
  the bot shows the facts, and the crowd gives the verdict. The bot never does.

## 6. Gates

- **The 30-day demand gate is dropped** (ADR 0013 "When"). Josh's decision.
- **The legal review runs now, in parallel, and does not block launch.** Josh's
  decision. ADR 0013 called it "a precondition, not a follow-up"; the
  consequence, stated once, is that until a lawyer answers, the exposure is
  Josh's personally.

## 7. Questions for a lawyer

Written to be handed over as they are.

1. An automated X account publishes measured on-chain facts about specific
   tokens, on request, in a jurisdiction to be named. Is that regulated
   financial promotion or investment advice?
2. The same operator launches a token, holds none of it, and pays 100% of its
   creator fee as a weekly public prize chosen by a published rule. Is the prize
   a lottery, a sweepstake or a promotion under the relevant law, and does free
   entry (ADR 0013 constraint 4) change that?
3. The token and account are named "realorrug". Does a brand built on "real or
   rug" create defamation exposure when the account discusses a specific
   project, given that the account itself never uses the word about one?
4. What entity should launch the token, receive the creator fee and send the
   prize, and what terms must the account and site publish?
5. Robinhood Chain is operated by a US-regulated broker's group, and Robinhood's
   name is not in the token's. Is there any trademark or affiliation exposure in
   describing the token as "on Robinhood Chain"?
6. Does US residence of the operator or of prize winners change any answer above?

## 8. What would change this

- **No ETH-fee launcher survives a capture:** constraint 2 fails and the home is
  pump.fun.
- **Robinhood ships agent tooling on the chain itself:** strengthens §4.1, and
  changes nothing already decided.
- **The move of the bot costs more than a launch window is worth:** the
  recommendation in §5.1 ("move later") is the fallback, and it is Josh's call.
- **A lawyer answers question 3 badly:** the name changes, not the chain.
