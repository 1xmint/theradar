<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0019 — Radar prices through Jupiter, and no longer asks it for a transaction

**Date:** 2026-09-09
**Status:** accepted, and **implemented in the same change**.
**Decides:** which Jupiter endpoint `radar-exec` calls, what it asks for, and
what it does when there is no API key.
**Amends:** [`crates/radar-exec/src/route.rs`](../../crates/radar-exec/src/route.rs)
and [`crates/radar-cli/src/route.rs`](../../crates/radar-cli/src/route.rs).
**Does not amend** [ADR 0003](0003-legacy-transactions-because-the-signer-must-be-able-to-read-them.md).
The signer still refuses address lookup tables, and this change makes that
refusal *cheaper* to discover rather than weaker.

## Context

`radar-exec` called `https://lite-api.jup.ag/swap/v1/quote` and `/swap`,
unauthenticated, asking for `asLegacyTransaction=true` so the signer could read
every account inline. Jupiter has deprecated `lite-api` and the v1 Swap API. The
replacement is the Swap API v2 at `https://api.jup.ag/swap/v2`, which offers two
products: the **Meta-Aggregator** (`/order`, `/execute`), which returns a
finished versioned transaction and lands it for a platform fee, and the
**Router** (`/build`), which returns raw Metis instructions for the caller to
assemble.

[Design 0017 §3](../design/0017-a-private-autonomous-trader.md) chose the
Router, because Radar's signer must read every byte it signs and a finished
transaction handed over by a vendor is the opposite of that. That choice is not
reopened here.

What is new is a measurement. Design 0017 warned: *"Do not silently replace the
endpoint and assume equivalence."* So the endpoint was captured before it was
trusted, on 2026-09-09, and the captures are committed unedited in
[`crates/radar-exec/fixtures/`](../../crates/radar-exec/fixtures/). Three facts
came back that the documentation did not supply:

1. **`/build` has no `asLegacyTransaction` parameter and returns no transaction.**
   It returns `swapInstruction`, `setupInstructions`,
   `computeBudgetInstructions`, `cleanupInstruction`, `otherInstructions`,
   `tipInstruction`, `blockhashWithMetadata` and
   `addressesByLookupTableAddress`. Assembling the transaction is the caller's
   job now.
2. **Every route came back through address lookup tables.** Five tables for
   SOL→USDC; one for USDC→SOL asked with `maxAccounts=20`, the smallest account
   set the API offers. There is no flag that turns this off, because there is no
   longer a transaction for a flag to shape.
3. **Jupiter still answers without an API key**, at a lower rate limit. A
   missing key does not produce an error. It produces a slower success.

Fact 3 is the dangerous one, and it is why the deny-by-default rule below is
stated as a decision rather than left as an implementation detail.

Facts 1 and 2 confirm rather than contradict
[research 0021](../research/0021-the-signer-cannot-read-the-only-venue-that-lists-them.md),
which found Radar could not get a signable transaction out of Jupiter for the
tokens it selects. [ADR 0009](0009-radar-builds-its-own-pump-fun-swaps.md)
answered that by building pump.fun swaps directly, in `radar-pumpfun`. Jupiter's
role was already pricing; this records it.

## Decision

**One. `radar-exec` calls `https://api.jup.ag/swap/v2/build`, and it calls it to
get a price.** Not a transaction. `Router::quote` returns a `Quote`: amounts in,
amounts out, the floor, the impact, the venue labels, and the number of lookup
tables the route uses. The instruction blobs are read for that count and
dropped. They are deliberately unmodelled — a type that parsed them would
suggest something assembles them, and nothing does.

**Two. A quote names an `Asset` on both sides.** `build_buy(mint, wallet,
size_lamports)` could express exactly one thing: SOL in, one named mint out.
`QuoteRequest { input, output, amount, taker }` expresses any admitted pair in
either direction. Native SOL and wrapped SOL both go on the wire as
`So111…112`, because Jupiter has no name for a lamport balance — and the `Asset`
that was asked for is what a `Quote` carries back, so the distinction
`radar-types` keeps is not lost at the boundary.

**Three. No key, no quote.** `Credentials` is constructible only from a lookup
that supplies `RADAR_JUPITER_API_KEY`, `Router` is constructible only from
`Credentials`, and neither has a `Default`. Quoting without a key is a program
that does not compile. There is no fallback to the keyless tier, and that is the
point: the keyless tier *works*, so a fallback would turn a missing credential
into a throttled quote inside a live decision, discovered months later or never.
AGENTS rule 8, and `radar-serve`'s x402 config is the same shape for the same
reason.

The key is redacted in `Debug` rather than derived, because this repository is
public and a derived `Debug` puts a credential into the first panic message that
touches a `Router`.

**Four. `Routing::build_buy` refuses, in writing.** `Router` still implements
the pipeline's routing trait, and the implementation always returns
`RouteError::Unverifiable` carrying the reason: the Router returns instructions
and lookup tables, nothing assembles a legacy transaction from them, and Radar's
own venue is `radar-pumpfun`.

Refusing rather than deleting the implementation is deliberate. Deleting it
would return `pipeline::Routing` to having no production implementation at all,
with nothing in the tree saying so — which is
[LEARNINGS](../../LEARNINGS.md) 10's shape exactly, and what
`the_pipeline_has_real_implementations.rs` exists to prevent. A refusal that
explains itself is visible in a journal line, in a test, and at the routing
stage, where nothing is yet at stake. An absent implementation is visible
nowhere.

It is also not a downgrade in capability. The endpoint that returned a legacy
transaction is deprecated, and research 0021 established that it would not have
returned one for Radar's own candidates anyway.

## Consequences

- **`radar route` prints a price, not a transaction.** It gained
  `--input/--output/--amount/--taker` for the general case and kept
  `--mint/--wallet/--lamports` as the SOL-in shorthand, which is what `radar
  --help` documents.
- **Radar cannot execute a Jupiter route.** It could not before either; now the
  code says so at the point of asking. Building a signer-readable transaction
  from `/build`'s instructions — or reviewing a successor to ADR 0003 that lets
  the signer read lookup tables — is separate work and is not started here.
- **A venue label in a `Quote` is not support.** Radar decodes pump.fun and
  nothing else. Reaching Whirlpool or Raydium through an aggregator's price is
  not the same as being able to price, verify or trade there, and nothing
  downstream should read the label as a claim that it is.
- **The free tier is one request per second.** Nothing here batches or bursts.
  No purchase is proposed.
- **A second unauthenticated caller survives.**
  [`crates/radar-sim/src/jupiter.rs`](../../crates/radar-sim/src/jupiter.rs)
  still points at `lite-api.jup.ag/swap/v1/quote` for its exit-price probe. It
  is outside this change and will stop working when Jupiter finishes the
  phase-out.

## What this does not decide

Whether the signer should learn to read address lookup tables. That needs a
reviewed successor to ADR 0003, and until it exists the answer stays no.
