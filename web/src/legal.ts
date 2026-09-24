// SPDX-License-Identifier: Apache-2.0
//! Whether the trade panel is allowed to exist, and the words it shows.
//!
//! `docs/legal/terms-and-trade-notice-draft.md` is a draft written by an AI
//! assistant, not reviewed by a lawyer, and not approved by the owner. ADR
//! 0024's "Before the button is switched on" section requires the owner to
//! approve this text *and* the server to switch trading on -- two separate
//! gates, because a text approval with no server change ships nothing, and a
//! server switch with no approved text would ship the placeholder draft.
//! `TradePanel.tsx` reads `TERMS_APPROVED` for the first gate and `/health`'s
//! `trading` field for the second; neither alone is enough.
//!
//! # Why this constant lives in code, not a config file
//!
//! A config flag can be flipped by anyone who can edit deploy config, without
//! reading what it turns on. This constant sits directly above the text it
//! gates, in the file whose diff the owner has to look at to ship the
//! feature at all -- flipping it means looking at the notice one more time.

/**
 * Whether the owner has approved the notice and terms text below for
 * showing to visitors. `false` until that approval happens, independent of
 * whether the text still contains a placeholder.
 *
 * This is the whole feature's dark switch on the client side. See
 * `legal.test.ts` for the invariant that keeps it honest: this can never be
 * `true` while [`TERMS_TEXT`] or [`NOTICE_TEXT`] still holds a placeholder
 * bracket, because flipping this without reading the draft is exactly the
 * failure mode the comment above describes.
 */
export const TERMS_APPROVED = false;

/**
 * Part 1 of the draft: the short notice shown beside the buy/sell button.
 *
 * Copied verbatim from `docs/legal/terms-and-trade-notice-draft.md`. This
 * file does not paraphrase or summarise it -- a second rendering of the same
 * notice could drift from the one the owner actually approved, the same
 * reason `siws.ts` never reassembles the server's sign-in challenge text.
 */
export const NOTICE_TEXT =
  "You sign every trade. Radar never holds your money or your keys. " +
  "Radar builds the swap from Jupiter's public route; your wallet shows it " +
  "to you, and nothing happens unless you approve it there. Memecoins can " +
  "lose all their value in minutes, and a swap can fill at the worst price " +
  "shown. Nothing on this site is financial advice.";

/**
 * Part 2 of the draft: the terms page at `/terms`. Copied verbatim,
 * including its unresolved `[PLACEHOLDER]` brackets -- the owner has not
 * filled them in yet, and inventing a value here (a jurisdiction, a contact
 * address) would put words in the owner's mouth that were never approved.
 */
export const TERMS_TEXT = `## Terms of use

*Last updated: [DATE]*

These terms cover your use of Radar at radar.heyvera.org, operated by
[OPERATOR] ("we"). By using the site you agree to them. If you do not agree,
do not use it.

**1. What Radar is.** Radar shows market data about Solana tokens. It shows
coin lists, prices, trades, holders, and, when you sign in with your wallet, your
own watchlist and holdings. It can also build a swap transaction for you to
review and sign in your own wallet.

**2. What Radar is not.** Radar is not a broker, an exchange, a bank, a
custodian or an investment adviser. We never hold your funds, your private
keys or your seed phrase, and we cannot move anything from your wallet. We do
not execute, send or retry trades: your wallet does, and only when you approve
it. We charge no fee on trades.

**3. No advice.** Nothing on the site is financial, investment, legal or tax
advice, or a recommendation to buy or sell anything. Data can be late,
incomplete or wrong. Radar says when it cannot see something, but it cannot
promise it always knows.

**4. The risk is yours.** Tokens shown on Radar are highly speculative. Most
lose most or all of their value, often within hours. They can be scams. A
swap can fill at any price down to the worst case you approved, and
transactions on Solana cannot be reversed. Network fees are charged by
Solana, not by us, and are paid even when a swap fails. Only trade what you
can afford to lose entirely.

**5. Your wallet is yours to protect.** You are responsible for your wallet,
its security, and everything you approve in it. Read what your wallet shows
you before approving. We will never ask for your seed phrase or private key.
Anyone who does is not us.

**6. Third parties.** Swap routes come from Jupiter, and trades settle on
Solana and on the venues Jupiter routes through. We do not control them and
are not responsible for what they do, including outages, failed routes and
price changes between quote and fill.

**7. Where you can use it.** You may not use Radar where doing so is against
the law that applies to you, or if you are subject to sanctions.
[RESTRICTED REGIONS: if any, list them here and say that the swap feature is
not offered there.]

**8. What we keep.** When you sign in with your wallet we keep your public
address and your watchlist, so we can show them back to you. Nobody else can
read them through Radar, including us through the site. We do not keep a
record of the swaps you build or sign. [Hosting and network providers, such as
Cloudflare, may log requests; name them in a privacy notice.]

**9. No warranty.** The site is provided as it is, with no promise that it
works, is available, or is accurate. To the fullest extent the law allows,
we are not liable for any loss from using it, including trading losses,
failed or mistaken transactions, or data that was late or wrong.

**10. Changes.** We may change the site or these terms. The date at the top
says when they last changed. We may switch off the swap feature at any time.

**11. Law.** These terms are governed by the laws of [JURISDICTION].

**12. Contact.** [CONTACT]`;

/**
 * Whether `text` still contains an unresolved `[PLACEHOLDER]`-style bracket.
 *
 * Matches the draft's own convention (`docs/legal/terms-and-trade-notice-draft.md`:
 * "Placeholders are in `[BRACKETS]`") rather than a fixed list of known
 * placeholders -- a placeholder the owner adds later, spelled differently
 * from `[OPERATOR]` or `[DATE]`, still needs to be caught.
 */
export function hasPlaceholder(text: string): boolean {
  return /\[[^\]]*\]/.test(text);
}

/**
 * Whether the text above is, as written, safe to approve -- i.e. free of the
 * placeholders the draft's own header warns about.
 *
 * `legal.test.ts` asserts `TERMS_APPROVED` can never be `true` while this is
 * `false`. Today it is `false` (the draft still has five placeholders), so
 * that test is currently exercising the real, live text, not a hypothetical.
 */
export const TERMS_SAFE_TO_APPROVE =
  !hasPlaceholder(NOTICE_TEXT) && !hasPlaceholder(TERMS_TEXT);
