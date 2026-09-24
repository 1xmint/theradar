// SPDX-License-Identifier: Apache-2.0
//! Drawings persist per mint, in this browser only. Every test here backs one
//! sentence in plan 0012 P6: "Drawings persist per mint in the browser,
//! labelled as the visitor's own -- not in the tenant store".

import { describe, expect, it, vi } from "vitest";

import {
  clearDrawings,
  loadDrawings,
  newDrawingId,
  saveDrawings,
  type Drawing,
} from "./drawings";

const MINT_A = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM";
const MINT_B = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

/** A storage that can be made to misbehave, matching `Wallet.test.tsx`'s
 *  fixture for `storedSession`. A real `Map`-backed store, so per-mint keys
 *  can be checked to be genuinely independent rather than just not throwing. */
function storage(throws = false) {
  const data = new Map<string, string>();
  return {
    data,
    getItem: vi.fn((key: string) => {
      if (throws) throw new Error("blocked");
      return data.get(key) ?? null;
    }),
    setItem: vi.fn((key: string, value: string) => {
      if (throws) throw new Error("blocked");
      data.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      if (throws) throw new Error("blocked");
      data.delete(key);
    }),
  };
}

const HORIZONTAL: Drawing = { id: "h1", kind: "horizontal", price: 0.0042 };
const TREND: Drawing = {
  id: "t1",
  kind: "trend",
  from: { time: 1_700_000_000, price: 0.001 },
  to: { time: 1_700_003_600, price: 0.0015 },
};

describe("loadDrawings / saveDrawings", () => {
  it("returns no drawings when nothing was stored", () => {
    expect(loadDrawings(MINT_A, storage())).toEqual([]);
  });

  it("round-trips a horizontal line and a trend line", () => {
    const store = storage();
    saveDrawings(MINT_A, [HORIZONTAL, TREND], store);
    expect(loadDrawings(MINT_A, store)).toEqual([HORIZONTAL, TREND]);
  });

  it("keeps one mint's drawings separate from another's", () => {
    const store = storage();
    saveDrawings(MINT_A, [HORIZONTAL], store);
    saveDrawings(MINT_B, [TREND], store);
    expect(loadDrawings(MINT_A, store)).toEqual([HORIZONTAL]);
    expect(loadDrawings(MINT_B, store)).toEqual([TREND]);
  });

  it("clearing one mint's drawings leaves another mint's untouched", () => {
    const store = storage();
    saveDrawings(MINT_A, [HORIZONTAL], store);
    saveDrawings(MINT_B, [TREND], store);
    clearDrawings(MINT_A, store);
    expect(loadDrawings(MINT_A, store)).toEqual([]);
    expect(loadDrawings(MINT_B, store)).toEqual([TREND]);
  });

  it("discards a corrupt list in full rather than keeping its valid entries", () => {
    const store = storage();
    store.data.set(
      `radar.chart.drawings.${MINT_A}`,
      JSON.stringify([HORIZONTAL, { id: "bad", kind: "horizontal", price: "not a number" }]),
    );
    // A filtering ("repairing") read would keep HORIZONTAL. Discarding the
    // whole entry keeps the promise that every line on screen is one the
    // visitor actually drew, not a best-effort remainder of one.
    expect(loadDrawings(MINT_A, store)).toEqual([]);
  });

  it("removes the corrupt entry so it cannot be read again", () => {
    const store = storage();
    store.data.set(`radar.chart.drawings.${MINT_A}`, "not json");
    loadDrawings(MINT_A, store);
    expect(store.removeItem).toHaveBeenCalledWith(`radar.chart.drawings.${MINT_A}`);
  });

  it("survives storage that throws outright", () => {
    expect(loadDrawings(MINT_A, storage(true))).toEqual([]);
  });

  it("does not throw when storage cannot be written", () => {
    expect(() => saveDrawings(MINT_A, [HORIZONTAL], storage(true))).not.toThrow();
  });
});

describe("clearDrawings", () => {
  it("does not throw when storage cannot be written", () => {
    expect(() => clearDrawings(MINT_A, storage(true))).not.toThrow();
  });
});

describe("newDrawingId", () => {
  it("gives two calls distinct ids", () => {
    expect(newDrawingId()).not.toBe(newDrawingId());
  });
});
