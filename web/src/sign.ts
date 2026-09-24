// SPDX-License-Identifier: Apache-2.0
//! Handing a server-built transaction to the visitor's wallet to sign and
//! send. Radar never signs and never sends -- ADR 0024's whole point, and
//! the reason `/v1/customer/swap` returns an *unsigned* transaction rather
//! than doing anything with it itself.
//!
//! # Why this needs `@solana/web3.js`, and `siws.ts` does not
//!
//! `siws.ts`'s doc comment explains why sign-in talks to the injected
//! provider directly instead of through `@solana/wallet-adapter`: the
//! interface has a 120 kB gzipped budget for its whole entry bundle, and
//! sign-in needs exactly two method calls on an object the browser already
//! injects (`connect`, `signMessage`). Signing and sending a v0
//! `VersionedTransaction` was researched the same way, looking for a way to
//! avoid a second library, and did not turn up one sound enough to bet money
//! on.
//!
//! Phantom's own documentation
//! (https://docs.phantom.com/solana/sending-a-transaction, fetched
//! 2026-09-24) shows two ways to call the injected provider:
//!
//! 1. `provider.signAndSendTransaction(transaction)` — Phantom's documented,
//!    recommended method. `transaction` is a real `@solana/web3.js`
//!    `Transaction` or `VersionedTransaction` **instance**, and the response
//!    is `{ signature: string }`, base58-encoded.
//! 2. `provider.request({ method: "signAndSendTransaction", params: {
//!    message: bs58.encode(...) } })` — the lower-level form. Its own
//!    example still *builds* that base58 string from a web3.js
//!    `Transaction`'s `.serializeMessage()`, and nothing in Phantom's docs
//!    demonstrates or confirms that this path accepts a hand-built v0
//!    `VersionedTransaction`'s raw bytes (address lookup tables, multiple
//!    signature slots) the way it accepts a legacy `Transaction`'s message.
//!
//! The Wallet Standard's own low-level `signAndSendTransaction` feature
//! (github.com/anza-xyz/wallet-standard,
//! `packages/core/features/src/signAndSendTransaction.ts`) *does* take and
//! return raw `Uint8Array`, no class required. But reaching a wallet through
//! it means adopting a second discovery mechanism
//! (`window.navigator.wallets`, `wallet-standard:register-wallet` events)
//! alongside the injected-provider `detect()` this codebase already uses
//! everywhere else -- for sign-in, and for every session check built on it.
//! That is a bigger and riskier change to the app's shape than adding one
//! well-known class used in exactly one place.
//!
//! This is money-moving code, not a read-only panel. A wrong guess about an
//! undocumented byte layout does not render a wrong number here -- it either
//! throws where a visitor is mid-trade, or, worse, does not throw and sends
//! something malformed. That trade-off is not worth a dependency this small:
//! `@solana/web3.js` is added, used for exactly one call
//! (`VersionedTransaction.deserialize`), and imported nowhere else in this
//! interface. Its bundle-size cost should be checked against the 120 kB
//! budget in review (see the PR body).

import { VersionedTransaction } from "@solana/web3.js";

/**
 * The slice of an injected wallet this module needs, beyond `siws.ts`'s
 * `WalletProvider`. Signing-and-sending is a separate capability from
 * signing in, so it gets its own narrow interface rather than growing
 * `WalletProvider` with a method sign-in never calls.
 */
export interface SigningProvider {
  signAndSendTransaction(
    transaction: VersionedTransaction,
  ): Promise<{ signature: string }>;
}

/** Why sending did not complete. */
export type SendError =
  | { kind: "declined" }
  | { kind: "failed"; detail: string };

/** Base64 to bytes -- the inverse of `siws.ts`'s hand-rolled `base64()`
 *  encoder, kept just as small and for the same reason: this one value does
 *  not justify a library either. */
function fromBase64(encoded: string): Uint8Array {
  const binary = atob(encoded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

/**
 * Deserialises the server's base64 transaction and asks the wallet to sign
 * and send it. The wallet is the only thing that ever sees the private key
 * and the only thing that submits to the network; this function's entire job
 * is translating the server's bytes into the shape the wallet's own method
 * expects.
 *
 * A decline and every other failure are told apart the same way
 * `siws.ts`'s `signMessage` call already does it: there is no error code
 * that distinguishes "the visitor closed the popup" from anything else a
 * wallet can throw, so a rejected promise here always reads as `declined`
 * -- closing the popup is the ordinary case, not a fault, and calling it an
 * error would tell someone who chose not to trade that something went wrong.
 */
export async function signAndSend(
  provider: SigningProvider,
  transactionBase64: string,
): Promise<{ ok: true; signature: string } | { ok: false; error: SendError }> {
  let transaction: VersionedTransaction;
  try {
    transaction = VersionedTransaction.deserialize(fromBase64(transactionBase64));
  } catch (cause) {
    // Not a decline: the server sent something this could not even parse.
    // That is a fact worth showing plainly rather than folding into
    // "cancelled", which would tell a visitor they backed out of something
    // they never got the chance to see.
    return { ok: false, error: { kind: "failed", detail: String(cause) } };
  }

  try {
    const { signature } = await provider.signAndSendTransaction(transaction);
    return { ok: true, signature };
  } catch {
    return { ok: false, error: { kind: "declined" } };
  }
}
