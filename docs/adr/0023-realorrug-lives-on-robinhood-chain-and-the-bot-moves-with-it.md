<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0023 — realorrug lives on Robinhood Chain, and the bot moves with it

**Date:** 2026-09-13
**Status:** accepted. **These are Josh's decisions, recorded**, after a
discussion session; where he chose against my recommendation, the row says so.
Nothing is built, bought or launched by this ADR.
**Amends:** [ADR 0013](0013-a-community-token-exists-and-radar-holds-none-of-it.md):
the venue, the fee currency, the payout, and its "When" section.
Its six constraints stand.
**Supersedes:** [design 0010](../design/0010-close-the-remainder-then-raise-the-ceiling.md)
row V7's "no second chain", for the token and the bot only. The trading lane
is untouched.
**Reasoning:** [design 0019](../design/0019-realorrug-on-robinhood-chain.md).
**Facts:** [research 0035](../research/0035-robinhood-chain-read-from-its-own-pages.md).

## Context

ADR 0013 launched the community token on pump.fun, with the creator fee in SOL
paid out as a weekly prize. Design 0010 V7 kept Radar on one chain. Josh
reopened both: he wants the token, renamed realorrug, on Robinhood Chain, cheap
to run, and the bot in its own repository.

The discussion changed the reason, not the direction. Robinhood's agent
features are brokerage features, not chain features (research 0035 §2). What
holds is that EVM makes the rewards contract ADR 0013 deferred cheaper to build
safely, that gas can be sponsored, and that Robinhood's audience belongs to this
chain. Data costs $0 on either chain at our volume, so it decided nothing.

## Decision

| # | decision | whose |
|---|---|---|
| 1 | The token is named **realorrug**. The meme is the brand. The bot still never calls a specific project a rug: [`realorrug:crates/realorrug-roast/src/forbidden.rs`](https://github.com/1xmint/realorrug/blob/main/crates/realorrug-roast/src/forbidden.rs) keeps "rug", and gains an own-name mask | Josh (name); the rule is unchanged |
| 2 | **Robinhood Chain is the token's permanent home.** It is bridged to Solana later as the same token, never launched twice | Josh, applying "if cost is negligible, Robinhood"; my recommendation agreed |
| 3 | It launches through a **full-supply bonding curve whose creator fee is paid in ETH**, so ADR 0013 constraints 1 and 2 hold. **If no launcher on the chain does this, the home reverts to pump.fun** | my recommendation, agreed |
| 4 | ADR 0013's **30-day demand gate is dropped** | Josh |
| 5 | The **legal review runs now, in parallel, and does not block launch** | Josh |
| 6 | A **new `realorrug` repository; the bot moves there now, before launch** | Josh, choosing over my recommendation to move it later |
| 7 | Data: free tiers and webhooks on both chains; **no stream is bought for the token or the bot**, PR #248 included | agreed |

## Consequences

- **The launch waits on two things:** the bot's move (decision 6) and a capture
  of a real launch on the chosen launcher confirming the fee rate, the ETH fee
  currency and a curve-only launch block (decision 3). Research 0035 §6.
- **ADR 0013 constraint 2's sentence changes currency, not substance.** The
  operator's only flow is still the creator fee, now in ETH; it is still paid out
  in full.
- **The payout is new code.** `radar-payout` is pump.fun-only, and the contest
  ledger's claim and amount types are Solana-shaped (design 0019 §4.4). The
  payout key's rules from ADR 0013 carry over: not the trading signer, never
  customer funds, a blast radius of one week of fees.
- **Constraint 6 is unmet until the bot can read its own token.** A narrow
  own-token reader comes before general Robinhood Chain roasting.
- **Until a lawyer answers, the exposure is Josh's personally.** Stated once.
- **Radar's website, research and backfill import the bot today.** The move
  has to cut those links (design 0019 §5.2–5.3); how `radar-serve` deploys
  after the split is open.

## Not decided

- Which launcher: Pons v2 is the candidate, unverified.
- When the bridge to Solana happens, and over which bridge.
- The rewards or claim contract, which stays gated on an audit, as in ADR 0013.
- Anything about the trading lane, `Policy::CLOSED`, or Radar's venues for trading.
