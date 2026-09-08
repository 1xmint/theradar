<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0016 — Free to use, metered intelligence

**Date:** 2026-09-08
**Status:** proposed, and **written too early — see the correction below before
reading the rest.** Nothing here is implemented and nothing here changes
`Policy::CLOSED`.
**Asks:** should Radar be free to use, with a paywall only on AI credits?

## Correction, recorded the same day

**This document was filed an hour after the idea was raised, before a single
objection to it had been answered.** It is one side of a conversation that had
not happened yet, and it was given the repository's authority anyway. The
[`AGENTS.md`](../../AGENTS.md) §3 rule that produced it has been changed in the
same commit: the conversation comes first, and the document records where it
*ended*.

One argument below is also wrong in a way worth naming, because it is a mistake
this repository is otherwise careful about. The section headed "Why the name is
wrong" reasons from a measured selection edge of 0 bps to a conclusion about
**whether the product is worth building**. A measured zero does not support that.
It is a fact about an instrument that has never been pointed at the thing being
proposed — §1's "zero is a measurement about your instrument", used backwards.

**What the 0 bps figure does constrain is narrow and still holds: what may be
advertised.** Radar cannot market returns nobody has demonstrated. That is a
claim about copy, not about the roadmap, and the two were run together below.
Read every "so this is not a product" sentence as "so this is not yet a sentence
we can put on a pricing page".

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

## What can be advertised today, and what cannot

*(Originally headed "Why the name is wrong, and this is the part that matters".
It is not the part that matters, and the correction above says why. It is a
constraint on public copy, and it is a real one.)*

[`GOAL.md`](../../GOAL.md) states three measured facts:

- Measured selection edge: **0 bps**
  ([research 0017](../research/0017-a-control-that-could-have-been-traded.md)).
- The bar before one trade is worth making: **~456 bps** of expected edge, and
  ~850 before a position over about $59 makes sense
  ([research 0022](../research/0022-capacity-was-a-budget-not-a-ceiling.md)).
- **The shipped policy refuses everything, and nothing has ever traded.**

`/health` on the production box returns `"policyClosed": true` today.

So a page today reading "advanced trading AI" would advertise a capability the
instrument has not yet found. **That is a marketing nuance and nothing more** —
the first draft of this section said it was "not a marketing nuance", which was
the error. Building toward that capability, and designing the measurement that
would demonstrate it, is ordinary work and is wanted. What GOAL.md forbids is
selling the feeling of it before the measurement exists:

> A product that claimed an edge it could not show would be the ordinary thing to
> build. […] **Radar's entire pitch is that it will tell you the truth about its
> own performance**, and that only means something if it does so while the truth
> is unflattering.

There is also a plain legal edge. Selling an AI marketed as producing trading
returns is closer to an advisory product than to a data product, and the operator
is a Tennessee entity with worldwide reach. That is a question for counsel, and
it is a much smaller question if the thing sold is analysis.

**What can be sold on day one**, because Radar can already show it: launch-block
structure, creator history, the coordination bands from
[research 0024](../research/0024-the-spike-became-a-hump-and-the-signal-moved.md),
dated population comparisons, and a refusal log that says what was rejected and
why. That is worth money to somebody about to buy a launch, and none of it needs
an edge to exist first.

**Recommended launch framing:** credits buy *research* — "ask Radar about this
launch". **That is a starting position, not a ceiling.** If the assistant is
built to the point where an edge is measurable and measured, the copy changes
with the evidence. The rule is that the claim follows the measurement, not that
the ambition is capped.

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
