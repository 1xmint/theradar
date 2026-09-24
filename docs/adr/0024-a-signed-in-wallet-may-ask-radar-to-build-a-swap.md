<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0024 — A signed-in wallet may ask Radar to build a swap; Radar never signs, sends, or holds a fee

**Date:** 2026-09-24
**Status:** accepted, and implemented in the same change.
**Decides:** what the server half of [plan 0013 Phase D](../plans/0013-the-terminal-find-look-track-trade.md#phase-d--trade-visitor-signed-swap)
builds, and what it is never allowed to become.
**Amends:** [`crates/radar-serve/src/trade.rs`](../../crates/radar-serve/src/trade.rs),
which calls [`crates/radar-exec/src/route.rs`](../../crates/radar-exec/src/route.rs)
(the Router client [ADR 0019](0019-radar-prices-through-jupiter-and-no-longer-asks-it-for-a-transaction.md)
put in place) and the new
[`crates/radar-exec/src/assemble.rs`](../../crates/radar-exec/src/assemble.rs).
**Does not amend** [ADR 0005](0005-customers-keep-custody-and-grant-radar-a-bounded-signer.md)
or [ADR 0009](0009-radar-builds-its-own-pump-fun-swaps.md): those describe the
private trader's own signer and Radar's own pump.fun venue, both untouched
here. This ADR is the plan's item D.1, written as the plan requires — before
the routes it governs.

## Context

ADR 0019 made `radar-exec` price through Jupiter's Router (`/build`) and stop
there: `Routing::build_buy` refuses in writing, because a raw instruction blob
with address lookup tables is not something Radar's own signer can read, and
nothing assembled a transaction from it. That left the customer-facing half of
Phase D open: a signed-in visitor with their own wallet wants a price and a
transaction to sign, and neither requires Radar's signer at all if the
transaction never needs Radar's signature.

Plan 0013 names Phase D "visitor-signed swap" for exactly this reason, and its
first line is explicit: *"Radar builds, the visitor's wallet signs and sends.
No custody, no fee, no server-side key."* That sentence is the whole decision;
the rest of this document is what it forces in the code.

## Decision

**One. Radar assembles bytes; it never holds a key that can move them.** The
unsigned transaction `POST /v1/customer/swap` returns names the *session
wallet* as fee payer (`radar-exec/src/assemble.rs`, `AssembledTransaction`,
built from Jupiter's own `/build` instructions and lookup tables), and nothing
in `radar-serve` or `radar-exec` holds a private key capable of signing it.
This is a stronger guarantee than "Radar doesn't sign this transaction" — it
is "Radar could not sign this transaction if it wanted to," because the module
that builds it has no signing material in scope at all. Signing and
broadcasting happen entirely client-side, with the wallet the visitor already
controls. `crate::customer_signing` and `radar-signer` — the bounded signer
ADR 0005 describes for the *private* trader — are not imported by `trade.rs`
and are not on this path.

**Two. No fee.** The transaction Jupiter's Router returns and Radar assembles
carries no platform fee instruction, and none is added. `RouteError::Unverifiable`
is still what `Router::build_buy`'s legacy path returns per ADR 0019; the new
path used here is `Router::build`, which returns the Router's raw instructions
for `assemble.rs` to compile as-is. A `tipInstruction` present in a captured
response is treated as a refusal to assemble, not as an instruction to include
or strip silently — `assemble.rs`'s own tests cover this — because adding it
would be Radar choosing to spend the visitor's SOL on Radar's behalf, and
stripping it silently would build a transaction that does not match the
lookup tables' account list.

**Three. Nothing is persisted, and no wallet address is logged.** `Trading`
holds only an in-memory decimals cache (public information — a mint's decimal
count) and in-memory rate-limit windows keyed on a visitor string or an
`Address` that is discarded a minute after last use. Neither route writes to
`radar-store` or any per-customer file. A routing failure logs its error
*kind* (`route_error_response`) and nothing else, because Jupiter error bodies
have echoed request parameters back in captures taken during this work, and a
log line is not the place a wallet address or a trade size belongs. This
mirrors [ADR 0006](0006-radar-records-only-what-it-cannot-recover.md)'s
reasoning, applied to a route that has nothing worth recovering: there is no
committed order, so there is nothing to reconstruct later.

**Four. Two routes, one Jupiter budget, three caps.** `GET /v1/market/quote`
is public (no wallet, no session) because a price is not identity-bearing
information and gating it would make the terminal's basic "what would this
cost" question require signing in first. `POST /v1/customer/swap` sits behind
[`Tenant`](../../crates/radar-serve/src/tenant.rs) because building a
wallet-specific unsigned transaction — naming that wallet as fee payer — is
meaningless without knowing which wallet, and the session is how Radar knows
that without trusting a caller-supplied address. Both draw from one Jupiter
API key with one real rate limit, so a global cap (30 calls/minute) protects
the key regardless of which route spent it; a per-visitor cap (6/minute,
keyed on `CF-Connecting-IP` or the peer address) rations the identity-free
route; a per-wallet cap (6/minute) rations the identity-bearing one. A caller
over any applicable cap is refused `busy` before Jupiter is asked anything —
proven by tests that count the upstream double's own received requests, not
just the HTTP status Radar returns.

**Five. A slippage cap, never a clamp.** Default 100 bps, hard ceiling 500
bps. A caller asking for more than the ceiling is refused `slippage_too_wide`
rather than silently capped, for the reason [ADR 0019](0019-radar-prices-through-jupiter-and-no-longer-asks-it-for-a-transaction.md)
already gives for keying Jupiter access on an explicit variable: a request
answered as if it had asked for something narrower than it did is a quieter
failure than one that says what it refused and why. A wide tolerance is what
makes a swap worth sandwiching; the ceiling exists so Radar cannot be made to
ask Jupiter for one on a visitor's behalf without that visitor being told no
first.

**Six. Off is the shipped state, and a half-configured "on" refuses to start.**
`RADAR_TRADE` follows the same shape [ADR 0019](0019-radar-prices-through-jupiter-and-no-longer-asks-it-for-a-transaction.md)'s
"no key, no quote" rule and `access::Mode::from_vars` both already use: an
operator names the capability they want, in a variable named for it, or it
does not exist. `RADAR_TRADE=on` with no `RADAR_JUPITER_API_KEY` refuses to
start the process at all, rather than starting and answering `trading_off` for
every request for a reason that is actually a misconfiguration — a fault that
would otherwise surface as "trading doesn't work" days after the operator
forgot the second variable, instead of at the moment they set the first.

## Consequences

- **A visitor can price and build a swap without Radar ever being able to move
  their funds.** The strongest thing Radar can do wrong on this path is
  refuse, price incorrectly, or leak a wallet's rate-limit standing to another
  visitor sharing a NAT — not spend anything, because it never holds anything
  spendable.
- **Radar cannot execute what it builds.** There is no follow-up endpoint that
  takes a signature and submits; `radar_exec::submit` (used by the private
  trader) is not reachable from `trade.rs`. A visitor who wants their swap
  landed must sign and send it themselves, which is the point, not a
  limitation to be lifted later.
- **The keyless tier still is not a fallback.** Exactly as ADR 0019 decided:
  `RADAR_TRADE=on` without a key is a startup refusal, not a slower success.
- **A future fee, if the product ever wants one, is a new decision, not a
  parameter.** Nothing in `assemble.rs` or `trade.rs` has a slot for one; a fee
  would mean adding an instruction and a wallet to receive it, and that wallet
  is custody-adjacent in exactly the way this ADR says Radar avoids. Reopening
  it means reopening this document, not passing a flag.

## What this does not decide

Whether Radar ever executes a trade on its own authority — the private,
autonomous trader plan 0013 explicitly excludes ("Not in this plan") and
designs 0017/0018 describe — is untouched. That trader's custody model is ADR
0005's, decided separately, and nothing here widens or narrows it.
