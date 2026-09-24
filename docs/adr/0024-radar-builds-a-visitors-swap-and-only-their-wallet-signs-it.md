<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0024 — Radar builds a visitor's swap, and only their wallet signs it

**Date:** 2026-09-24
**Status:** accepted; nothing built yet. Plan 0013 Phase D builds it.
**Decides:** who builds, signs and sends a trade started from the terminal's
buy/sell button; which coins it reaches; what Radar holds while it does; and
what has to be true before the button is switched on.
**Decided by:** the owner, 2026-09-18 (the terminal only, a buy/sell button
signed by the visitor's own wallet, no custody, no fee) and 2026-09-24 (every
coin through Jupiter, not only coins still on pump.fun; the button stays off
until the owner has approved terms and a notice).
**Amends:** [plan 0013](../plans/0013-the-terminal-find-look-track-trade.md)
Phase D item 3, which said the browser asks Jupiter for the unsigned
transaction. It cannot; see Context.
**Does not amend** [ADR 0019](0019-radar-prices-through-jupiter-and-no-longer-asks-it-for-a-transaction.md)
or [ADR 0003](0003-legacy-transactions-because-the-signer-must-be-able-to-read-them.md):
`radar-signer` is not involved, and neither is `radar-exec`'s trader.

## Context

Plan 0013 Phase D said two things that cannot both hold: "Radar builds, the
visitor's wallet signs and sends" (item 1), and "the browser asks Jupiter for
the unsigned transaction" (item 3). Item 3 is blocked three ways, each
already on record:

1. **The fee.** Jupiter's only product that hands back a finished transaction
   is the Meta-Aggregator (`/order`, `/execute`), which lands it for a
   platform fee (ADR 0019, Context). The owner decided no fee.
2. **The key.** `api.jup.ag` is keyed (`x-api-key`,
   `crates/radar-exec/src/route.rs`). A browser call either ships
   `RADAR_JUPITER_API_KEY` in a bundle anyone can read, or uses the keyless
   tier ADR 0019 stopped using.
3. **The page cannot reach it.** The site's Content-Security-Policy is
   `connect-src 'self'` (`deploy/radar.heyvera.org.caddy`), and the public
   Solana node refuses any request carrying a browser `Origin` (measured
   2026-09-24, plan 0013 Phase C item 2).

The Router (`/build`) does not have these problems. It is the endpoint
`radar-exec` already calls with the owner's key, charges no fee, and returns
instructions plus the address lookup tables and a recent blockhash
(ADR 0019, fact 1) -- everything needed to assemble a transaction without a
further network call.

## Decision

**Radar's server builds the unsigned transaction. The visitor's wallet signs
it and sends it. Nothing else signs, and Radar sends nothing.**

- **Built for the signed-in wallet only.** The build route sits behind
  `Tenant` like the watchlist and positions: the fee payer and `taker` are the
  verified wallet, never an address from the request, so Radar is not a free
  transaction builder for arbitrary wallets.
- **Every coin Jupiter can route**, pump.fun bonding curves and graduated
  pools alike. The transaction is a versioned (v0) message using the lookup
  tables `/build` returned. That is fine for a visitor's wallet; ADR 0003's
  legacy-only rule binds `radar-signer`, which never sees these transactions.
- **No fee.** No platform-fee account, no referral account, no tip to Radar.
  A test asserts that no instruction pays an account Radar controls.
- **Slippage is bounded by Radar, shown to the visitor, and never widened on
  a retry.** A default with a hard cap; the screen shows what they pay, what
  they expect, and the worst case they accept, before the wallet is asked.
- **The quote is public, the build is not.** `GET /v1/market/quote` answers
  anyone, under one cap across every visitor (it spends the owner's Jupiter
  allowance, one request a second on the free tier). A per-visitor limit sits
  inside it keyed on `CF-Connecting-IP`, which is only a courtesy: the origin
  is behind Cloudflare, and a spoofed header dodges the per-visitor limit
  but never the global one.
- **Nothing is kept.** Radar does not store the built transaction, the quote,
  or the visitor's intent. Positions (Phase C) shows the result afterwards by
  reading the chain.

## What Radar will never do

- Hold, derive, generate or be sent a visitor's private key or seed phrase.
- Sign a visitor's transaction, or ask them to sign one for another wallet.
- Send a visitor's transaction to the network, retry it, or re-send it.
- Take a fee, a spread, a referral cut or a tip on a visitor's trade.
- Route a visitor's trade through `radar-signer`, `radar-exec`'s trader, or
  any wallet Radar controls.
- Widen slippage without the visitor seeing the new worst case first.
- Keep a record of what a visitor tried to trade.

## Before the button is switched on

The button ships **off**, behind a server switch the owner turns on in
`/etc/radar/radar.env`. It stays off until:

1. **The owner approves the terms and the notice.** ADR 0005 left "no terms of
   service, no privacy policy and no 'not financial advice' text anywhere"
   open as a precondition for anything like this. Phase D drafts a terms page
   and a notice beside the button; the owner reads and approves them, ideally
   with a lawyer. The draft is not legal advice.
2. **Independent review passes**, because it touches other people's money.
3. **The owner rehearses it** with a throwaway wallet and a few cents: a buy
   and a sell from the screen, with transaction ids, and the Phase C position
   changing after each. The owner signs; nobody else can.

## What this does not decide

- Whether to buy a paid Jupiter tier. The free tier's one request a second is
  the ceiling until visitors show it is not enough.
- Any chain but Solana, any order type but an immediate swap, any fee.
- The live site's missing security headers: the Content-Security-Policy in
  `deploy/radar.heyvera.org.caddy` is not what the live site serves (checked
  2026-09-24, no `Content-Security-Policy` header on `/`). That is the
  owner's deploy fix, and it matters more once a signing button exists.
