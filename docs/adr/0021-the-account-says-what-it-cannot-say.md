<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0021 — The account says what it cannot say

**Date:** 2026-09-10
**Status:** accepted, and **implemented in the same change**.
**Decides:** how Radar represents what it holds, and what happens when it cannot
read it.
**Amends:** [`crates/radar-types/src/portfolio.rs`](../../crates/radar-types/src/portfolio.rs),
[`crates/radar-types/src/quantity.rs`](../../crates/radar-types/src/quantity.rs),
[`crates/radar-store/src/portfolio.rs`](../../crates/radar-store/src/portfolio.rs)
and [`crates/radar-cli/src/consider.rs`](../../crates/radar-cli/src/consider.rs).
**Does not amend** [`crates/radar-types/src/money.rs`](../../crates/radar-types/src/money.rs)'s
`MicroUsd`, which is unchanged. `SignedMicroUsd` is added beside it.

## Context

Everything Radar knew about what it held was
[`Position`](../../crates/radar-store/src/position.rs): a mint, a creator, two
slots, a notional in micro-dollars and two optional prices.

It has no wallet. No raw token quantity. No decimals. No cash, no wrapped SOL,
no quote balance. No pending reservation. No fee, rent or tip. No partial fill.
No valuation timestamp distinct from the open and close slots.

And the one place that read it —
[`radar consider`](../../crates/radar-cli/src/consider.rs) — ended in
`unwrap_or_default()`. A read that **failed** produced an empty
`Vec<Position>`, the kernel was handed a portfolio with zero deployed, and every
exposure limit was measured against a fact about the disk. The comment above it
said this was safe because nothing writes a position yet. That was true. It was
also a fuse with no date on it: the fix and the first trade were not linked to
each other by anything but a paragraph.

Two measurements widen the problem beyond "add some fields".

1. **[`0033`](../research/0033-the-pumpswap-pool-account-has-eight-lengths.md) captured
   ten PumpSwap pools from mainnet**, and **six of the ten quote in something
   other than SOL** — one of them USDC, the rest in `pump` and other mints. So a
   holding's quote asset may itself have no dollar price without a chain of
   quotes that does not exist here.
2. **[`radar-pumpfun`](../../crates/radar-pumpfun/src/token.rs) refuses six
   Token-2022 extensions by name**, because each can change what a balance is
   worth or whether it can move. "There is a token account" is not enough to
   say what is held.

## Decision

**Absent and zero are separate types, not two readings of one number.**
AGENTS.md rule 9, applied to money.

- A quantity is `TokenQuantity`: raw base units **and** the decimals that give
  them meaning. `Decimals` cannot be built from a number nobody read — the two
  constructors are the SOL protocol constant and `from_mint_account`, and serde
  routes through the same gate. Arithmetic across two units returns `None`
  rather than adding the integers.
- A balance is `Balance::Counted(quantity)` or `Balance::Uncounted(refusal)`.
  There is no zero to fall back to, and the refusal carries the Token-2022
  extension code that stopped the count.
- A dollar figure is `Valuation::Known { value, as_of }` — **with the slot it was
  true at** — or `Valuation::Unknown(reason)`. `Unvaluable::QuoteUnpriced` is
  0033's finding written down. An estimate in that field is a number a limit
  gets measured against as though somebody had measured it.
- Cash, native SOL, wrapped SOL, USDC and every other asset stay apart, keyed by
  the existing [`Asset`](../../crates/radar-types/src/asset.rs). Design 0017 §3:
  *"SOL and USDC amounts must not share an untyped integer or fixed $1
  assumption."*
- Fees, rent and tips are three lamport fields, not one total. Rent is
  recoverable when an account closes and a priority fee is not, so one number
  answers neither question.
- Realised and unrealised are separate answers, and the unrealised one is
  `Unknown` as soon as **any** open holding is. A partial sum reads as a total,
  and the partial one is smaller — which is the direction that gets permission.

**A reservation is released by an outcome and never by a clock.** `reserve`
computes what is free and inserts the claim inside one `&mut self` borrow, so two
requests that together exceed the balance cannot both succeed; a shared portfolio
needs a lock to be reached at all, and this is what makes serialising the callers
sufficient. `settle` takes **no `now` parameter**, so there is nothing to build a
timeout out of. A `PartiallyFilled` outcome leaves the remainder **reserved**:
freeing it would let a second proposal spend against a claim that still exists.

**An unreadable inventory stops the pass.** `consider`'s `unwrap_or_default` is
gone. So is the weaker sibling of the same error: a portfolio that knows it
cannot account for something on record has totals that are **lower bounds**, and
a limit checked against a lower bound binds late — so `Portfolio::incompleteness`
stops the pass too.

**No wallet means nothing can be held or claimed.** `Custody::Unattributed` is
the honest value for an instance with none configured, and it is enforced rather
than described: `hold` and `reserve` both refuse. Rule 8.

## Consequences

**What this reaches.** `radar consider` is the caller. It refuses on a failed
position read and on an inventory it cannot fully account for, and prints what
the account holds before the kernel judges anything.

**What it does not.** Three things, deliberately:

- **Nothing writes a position row yet**, so every portfolio assembled today is
  empty and complete. That is why the refusal is not a check that fires on the
  normal case.
- **There is no durable operation record.** That is the next slice, and until it
  exists [`portfolio_from`](../../crates/radar-store/src/portfolio.rs) records an
  open position as *exposure it cannot quantify* rather than as a holding —
  because the row carries dollars committed, not units received, decimals, or
  which token program the mint is on. Guessing the last of those would file the
  same address under two assets with different transfer semantics.
- **Nothing is reconciled against the chain.**
  [`radar-onchain`](../../crates/radar-onchain) can already read accounts
  atomically at one slot and is not wired in.

**What was considered and rejected.**

- *Rewriting `MicroUsd` to carry an asset.* It is depended on across the
  workspace and every one of those callers is talking about a cost or a limit,
  where an unsigned dollar figure is the right shape. `SignedMicroUsd` sits
  beside it; a replacement is not the smallest change that solves this.
- *A second asset vocabulary.* `Asset` already distinguishes native SOL, wrapped
  SOL, USDC, SPL and Token-2022, and its `tag()` numbers are hashed into
  authorisation nonces. A parallel enum would give the workspace two spellings
  of one trade.
- *An `Option<TokenQuantity>` balance.* `None` and `Some(zero)` are one keystroke
  apart and mean opposite things, and `None` carries no reason. The two-variant
  enum makes the absent case say why.
- *A new store table.* Nothing needs one yet. A table with no writer is the
  shape AGENTS.md §5 names.
