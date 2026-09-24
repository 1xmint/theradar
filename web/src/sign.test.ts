// SPDX-License-Identifier: Apache-2.0
import { Keypair, MessageV0, SystemProgram, VersionedTransaction } from "@solana/web3.js";
import { describe, expect, it } from "vitest";
import { signAndSend, type SigningProvider } from "./sign";

/** Base64, from bytes -- the same hand-rolled encoding `siws.ts` uses, so
 *  this test exercises the same round trip `sign.ts`'s `fromBase64` decodes. */
function toBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

/** A real, well-formed v0 transaction, base64-encoded, standing in for what
 *  `/v1/customer/swap` would return. */
function fakeTransactionBase64(): string {
  const payer = Keypair.generate();
  const message = MessageV0.compile({
    payerKey: payer.publicKey,
    recentBlockhash: SystemProgram.programId.toBase58(),
    instructions: [
      SystemProgram.transfer({
        fromPubkey: payer.publicKey,
        toPubkey: payer.publicKey,
        lamports: 1,
      }),
    ],
  });
  const transaction = new VersionedTransaction(message);
  return toBase64(transaction.serialize());
}

describe("signAndSend", () => {
  it("deserialises the server's bytes and hands the wallet a real VersionedTransaction", async () => {
    let received: VersionedTransaction | null = null;
    const provider: SigningProvider = {
      signAndSendTransaction: async (transaction) => {
        received = transaction;
        return { signature: "5" + "1".repeat(87) };
      },
    };
    const result = await signAndSend(provider, fakeTransactionBase64());
    expect(result).toEqual({ ok: true, signature: "5" + "1".repeat(87) });
    expect(received).toBeInstanceOf(VersionedTransaction);
  });

  it("reads as cancelled, not an error, when the wallet rejects the popup", async () => {
    const provider: SigningProvider = {
      signAndSendTransaction: async () => {
        throw Object.assign(new Error("User rejected the request"), { code: 4001 });
      },
    };
    const result = await signAndSend(provider, fakeTransactionBase64());
    expect(result).toEqual({ ok: false, error: { kind: "declined" } });
  });

  it("reads as a failure, not a cancel, when the wallet approved but could not send", async () => {
    // The dangerous confusion: someone told "cancelled" after approving may
    // approve again and trade twice.
    const provider: SigningProvider = {
      signAndSendTransaction: async () => {
        throw Object.assign(new Error("Blockhash not found"), { code: -32003 });
      },
    };
    const result = await signAndSend(provider, fakeTransactionBase64());
    expect(result).toEqual({
      ok: false,
      error: { kind: "failed", detail: "Your wallet could not send this trade: Blockhash not found" },
    });
  });

  it("keeps a non-Error rejection's message", async () => {
    const provider: SigningProvider = {
      signAndSendTransaction: async () => {
        throw { code: 500, message: "internal" };
      },
    };
    const result = await signAndSend(provider, fakeTransactionBase64());
    expect(result).toEqual({
      ok: false,
      error: { kind: "failed", detail: "Your wallet could not send this trade: internal" },
    });
  });

  it("does not call the wallet at all when the bytes are not a real transaction", async () => {
    let called = false;
    const provider: SigningProvider = {
      signAndSendTransaction: async () => {
        called = true;
        return { signature: "should-not-happen" };
      },
    };
    const result = await signAndSend(provider, "not-valid-base64-transaction-bytes");
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.kind).toBe("failed");
    expect(called).toBe(false);
  });

  it("tells a parse failure apart from a decline -- one is a bug, the other is a choice", async () => {
    const provider: SigningProvider = {
      signAndSendTransaction: async () => ({ signature: "irrelevant" }),
    };
    const parseFailure = await signAndSend(provider, "%%%not base64%%%");
    expect(parseFailure.ok).toBe(false);
    if (!parseFailure.ok) expect(parseFailure.error.kind).toBe("failed");
  });
});
