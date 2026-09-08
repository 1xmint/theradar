<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0018 — The shell is public, and the gate moves to the data

**Date:** 2026-09-08
**Status:** accepted, and **implemented in the same change**. This is Josh's
decision, recorded. One thing it needs is not code and is named at the bottom.
**Decides:** what `radar.heyvera.org` shows a stranger.
**Amends:** the audience classification in
[`crates/radar-serve/src/access.rs`](../../crates/radar-serve/src/access.rs).
The module's argument for verifying the Cloudflare assertion rather than reading
the header is untouched and still the reason that file exists.

## Context

`radar.heyvera.org` showed `small-art-43c3.cloudflareaccess.com` and *"Log in to
Radar — Sign in with:"*. That is Cloudflare Access, and it was working exactly as
designed: `/` and `/assets/` were `Audience::Customer`, no customer session
existed on a first visit, and the guard fell through to the operator check.

The interface has had its own sign-in since ADR 0011's amendment.
[`Wallet`](../../web/src/Wallet.tsx) sits in the header and renders five states,
one of which is *signed out — a button*. `siws.rs` verifies what comes back, the
guard accepts the session, and none of it was ever reachable: **a visitor gated
at the front door never sees the sign-in the product already has.** The only
login on offer was the operator's identity provider, which is not something to
hand a stranger.

## Decision

**The shell is public. The data is not.**

Public: `/`, `/assets/`, and the client routes that serve the same HTML —
`/decisions`, `/evidence`, `/wallet`, `/ask`, `/token/…`.

Unchanged: every read those pages make. `/v1/funnel`, `/v1/scoreboard`,
`/v1/decisions`, `/v1/evidence/…`, `/v1/tokens/…`, `/v1/customer/wallet`,
`/v1/customer/events` and `/v1/chat` are `Customer` — a wallet session, or the
operator. `/v1/store`, `/v1/events`, `/mcp`, `/ops`, `/instance` and `/analyst`
are `Operator`, and the fallback for anything unclassified is still `Operator`.

So a stranger gets a panel with nothing in it and a button that fills it. That is
the ordinary shape of a product, and it is the shape this interface was built
for.

## The argument this overrules, because it was a good one

`access_guard.rs` said, in a comment written deliberately:

> An interface whose HTML is behind a login and whose JavaScript bundle is not
> still leaks the shape of the product, and the bundle is where the API paths
> are written down.

That is right **about a private product**, and Radar was one. It stops being the
argument the moment the panel is something strangers are meant to open: the
shape of the product is then the marketing, and the secret is the data. Every
path the bundle names is still classified, and a reader who learns that
`/v1/store` exists learns a path that refuses them.

The comment is not deleted. It is answered where it was made.

## What this costs

**The interface can no longer tell who is reading it.** `AUDIENCE` in
[`App.tsx`](../../web/src/App.tsx) was the constant `"operator"` — honestly so,
when Access gated everything. It is the constant `"customer"` now, for the same
reason: a wallet session proves a customer and never an operator, and the only
thing that would prove an operator is an operator-gated read succeeding. Probing
for one would put a 403 on every stranger's first page load to decide the
contents of a menu.

So the operator reaches `/instance` and `/analyst` by typing them. They are the
one person who knows those pages exist, the server admits them, and the pages
render. **Hiding a link is a courtesy and never a control** — the server refuses
an operator route to a customer token whatever the client believes.

**The bundle is world-readable.** It always was to anyone who got past Access;
now it is to everyone. It contains no secret — it is client code — and this ADR
is the place that says so out loud rather than leaving it implied.

## What is still required, and is not code

**`RADAR_CUSTOMER_ACCESS` is `Closed` unless set**, and closed means nobody is
admitted however well their wallet signs.
[`admission.rs`](../../crates/radar-serve/src/admission.rs) is explicit that
going public is `RADAR_CUSTOMER_ACCESS=open`, typed by a person, precisely so a
dropped variable cannot silently open a private product.

Until that is set on the box, this change gets a visitor to the panel and to the
connect button, and every sign-in is then refused. **That is the deploy step,
and it is deliberate that a code change cannot perform it.**

Setting it means anyone with a Solana wallet can read the decisions, the
evidence, the scoreboard and the assistant. The assistant costs model spend per
question; it is metered (`chat`'s budget and ledger), and the size of that
budget is a number to look at before the switch rather than after.

## What this does not decide

- **Whether the panel is a trading surface.** It is not. `Policy::CLOSED` ships,
  nothing has ever traded, and nothing here changes that. It is a research
  panel, and calling it anything else on a public page would be the kind of
  claim this repository exists to refuse.
- **Whether Access stays on the operator surface.** It does, and it should: the
  operator's screens are the reason that verifier is written the way it is.
- **Anything about `cabalhunter.org`**, which is a separate static site with its
  own origin.
