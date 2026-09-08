<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0016 — Free to use, metered intelligence

**Date:** 2026-09-08
**Status:** proposed. This is a recommendation on a question the owner asked, not
a decision he has made. Nothing here is implemented and nothing here changes
`Policy::CLOSED`.
**Decides nothing on its own.** It exists because a business-model question
arrived in chat and [`AGENTS.md`](../../AGENTS.md) §3 says a chat answer to a
direction question is a draft.
**Asks:** should Radar be free to use, with a paywall only on AI credits?

## The question, as the owner put it

> Users of Radar can use Radar for free — they just need an account and to
> connect their wallet for the trading and using it part. The only thing behind a
> paywall would be *AI credits*, which fund the model API. Similar to Replit AI:
> it is good, it needs credits, and running out does not stop you working
> manually. So Radar is dual — free to use, with optional credits for an advanced
> trading AI.

## The recommendation in one line

**Adopt the structure. Do not ship the words "advanced trading AI".**

## Why the structure is right

Metering the model and nothing else is the correct place to put the only
paywall, for a reason that is specific to this product rather than a preference:

- **Model spend is the only materially variable cost per user.** The store, the
  reads and the interface are near-fixed. A subscription would charge a light
  user for a heavy user's tokens; credits do not.
- **It degrades honestly.** A customer out of credits keeps the measurements,
  the decision log and the evidence — everything Radar computes deterministically
  and can stand behind. They lose the thing that genuinely costs money to run.
- **The metering already exists.** `chat`'s budget and ledger, and the per-day
  `RADAR_CHAT_PER_CUSTOMER_DAILY` ceiling, are the mechanism this model needs.
  This is a pricing decision on top of built machinery, not a new subsystem.
- **Rule 8 already points the right way.** A meter with no budget refuses
  everything, so an unfunded account fails closed rather than free.

## Why the name is wrong, and this is the part that matters

[`GOAL.md`](../../GOAL.md) states three measured facts:

- Measured selection edge: **0 bps**
  ([research 0017](../research/0017-a-control-that-could-have-been-traded.md)).
- The bar before one trade is worth making: **~456 bps** of expected edge, and
  ~850 before a position over about $59 makes sense
  ([research 0022](../research/0022-capacity-was-a-budget-not-a-ceiling.md)).
- **The shipped policy refuses everything, and nothing has ever traded.**

`/health` on the production box returns `"policyClosed": true` today.

So "advanced trading AI, worth buying" would sell the one capability this
repository has measured and found **absent**. That is not a marketing nuance. It
is the exact product GOAL.md says Radar exists not to be:

> A product that claimed an edge it could not show would be the ordinary thing to
> build. […] **Radar's entire pitch is that it will tell you the truth about its
> own performance**, and that only means something if it does so while the truth
> is unflattering.

There is also a plain legal edge. Selling an AI marketed as producing trading
returns is closer to an advisory product than to a data product, and the operator
is a Tennessee entity with worldwide reach. That is a question for counsel, and
it is a much smaller question if the thing sold is analysis.

**The owner's instinct is not wrong — the naming is.** What is being sold has
real value and Radar can already show it: launch-block structure, creator
history, the coordination bands from
[research 0024](../research/0024-the-spike-became-a-hump-and-the-signal-moved.md), dated
population comparisons, and a refusal log that says what was rejected and why.
That is worth money to somebody about to buy a launch. It is not a trading AI.

**Recommended framing:** credits buy *research*. "Ask Radar about this launch,"
not "let Radar trade it."

## Three things the sketch assumes that are not true today

1. **"Connect their wallet for the trading part."** There is no trading part.
   `Policy::CLOSED` ships, nothing has ever traded, and
   [ADR 0005](../adr/0005-customers-keep-custody-and-grant-radar-a-bounded-signer.md)
   makes a connected wallet *authentication, not authority*. A pricing page that
   implies otherwise would be describing a product that does not exist. Manual
   trading "without the AI" is not a fallback the product currently has either.
2. **A third auth system.** Radar already has two: SIWS wallet sessions and
   Privy. Adding Clerk makes three login paths, three refusal surfaces and three
   places for an admission bug. Pick one before adding users. Privy already
   carries the email login the owner asked for.
3. **Credit margin is not automatic.** A credit sold at a fixed price against a
   model bill that varies per question can go underwater on exactly the customers
   who use it most. Credits must be denominated in *metered spend*, not in
   questions, and the ledger has to be the source of truth. The parts exist; the
   denomination is a decision.

## Where this recommendation is weakest

- **"Free to use" may be worth less than it sounds.** If the deterministic
  surface is what is free and the model is what is paid, and the model is the
  part people actually want, then "free" is a trial rather than a product. That
  is fine, but it should be called what it is.
- **No demand evidence.** Nobody has paid Radar anything, and no potential
  customer has been asked. This document argues from cost structure and from what
  can be honestly claimed — not from a measured willingness to pay.
- **The competitive question is untouched.** Whether a research assistant over
  launch data is defensible against a free alternative is not analysed here.
- **It does not answer the owner's actual hard question**, which is what an
  *advanced* assistant would be. That is research, and it is scoped separately.

## What this does not decide

- **Prices, credit denomination, or free-tier limits.** Numbers need the demand
  evidence this document says is missing.
- **Anything about `Policy::CLOSED`.** Trading remains off, and turning it on is
  a decision about real money that no pricing model authorises.
- **Whether to build it at all**, or in what order relative to
  [plan 0010](../plans/0010-radar-actualization.md).
