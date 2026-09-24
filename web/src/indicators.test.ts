// SPDX-License-Identifier: Apache-2.0
//! Plan 0013 phase E's rubric: "an indicator whose lookback exceeds the
//! available bars is absent, not truncated and not zero-seeded". Every
//! `sma`/`ema` test below is written so that a reimplementation with either
//! wrong behaviour fails it -- see the comments on each one for exactly which
//! wrong version it catches and why.

import { describe, expect, it, vi } from "vitest";

import {
  absenceMessage,
  ema,
  indicatorChoiceKey,
  indicatorLabel,
  loadIndicatorChoices,
  saveIndicatorChoices,
  sma,
  type IndicatorChoice,
} from "./indicators";

/** A storage that can be made to misbehave, matching `Wallet.test.tsx`'s
 *  fixture for `storedSession`. */
function storage(initial?: string, throws = false) {
  const data = new Map<string, string>();
  if (initial !== undefined) data.set("radar.chart.indicators", initial);
  return {
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

describe("sma", () => {
  it("computes the exact average once its window fills", () => {
    // 3-bar window over [1..6]: the first two bars have no full window, then
    // each average is the mean of exactly the last 3 closes.
    expect(sma([1, 2, 3, 4, 5, 6], 3)).toEqual([null, null, 2, 3, 4, 5]);
  });

  it("is absent -- not a partial-window average -- before the window fills", () => {
    // A wrong implementation that averages "however many bars exist so far"
    // would put a number at every index, e.g. index 0 -> 1, index 1 -> 1.5.
    // The real average needs all 3 bars, so both must be null.
    const result = sma([1, 2, 3, 4, 5, 6], 3);
    expect(result[0]).toBeNull();
    expect(result[1]).toBeNull();
  });

  it("is absent in full, not truncated, when the lookback exceeds every bar there is", () => {
    // Only 3 values exist for a 5-bar lookback. A truncated implementation
    // would fall back to averaging the 3 it has (here, 2) at the last index
    // instead of reporting that no 5-bar average exists yet; a zero-seeded
    // one would pad the missing 2 slots with 0 and report (0+0+1+2+3)/5 = 1.2.
    // Both are numbers; the honest answer is that there is no 5-bar average
    // over 3 bars, so every entry must be null.
    expect(sma([1, 2, 3], 5)).toEqual([null, null, null]);
  });

  it("throws rather than silently accepting a non-positive lookback", () => {
    expect(() => sma([1, 2, 3], 0)).toThrow();
    expect(() => sma([1, 2, 3], -1)).toThrow();
  });
});

describe("ema", () => {
  it("computes the exact seed-then-recurse values once its window fills", () => {
    // lookback 3 over [1,2,3,4,5]: k = 2/4 = 0.5.
    // Seed at index 2 is the plain average of [1,2,3] = 2.
    // Index 3: 4*0.5 + 2*0.5 = 3. Index 4: 5*0.5 + 3*0.5 = 4.
    expect(ema([1, 2, 3, 4, 5], 3)).toEqual([null, null, 2, 3, 4]);
  });

  it("is absent -- not zero-seeded -- before its seed window fills", () => {
    // A zero-seeded wrong version would start producing numbers from index 0
    // (e.g. value*k + 0*(1-k)). The real EMA has nothing to report until the
    // seed window -- the same `lookback` bars `sma` needs -- exists.
    const result = ema([1, 2, 3, 4, 5], 3);
    expect(result[0]).toBeNull();
    expect(result[1]).toBeNull();
  });

  it("is absent in full when the lookback exceeds every bar there is", () => {
    expect(ema([1, 2], 5)).toEqual([null, null]);
  });
});

describe("absenceMessage", () => {
  it("names the exact lookback and what this interval actually has", () => {
    expect(absenceMessage(20, 7)).toBe(
      "20-bar average needs 20 bars; this interval has 7.",
    );
  });
});

describe("indicatorLabel", () => {
  it("labels a price SMA, a price EMA, and a volume SMA distinctly", () => {
    const labels = [
      indicatorLabel({ method: "sma", lookback: 20, target: "price" }),
      indicatorLabel({ method: "ema", lookback: 20, target: "price" }),
      indicatorLabel({ method: "sma", lookback: 20, target: "volume" }),
    ];
    expect(new Set(labels).size).toBe(labels.length);
    expect(labels[2]).toContain("volume");
  });
});

describe("indicatorChoiceKey", () => {
  it("gives the same key for equal choices and different keys otherwise", () => {
    const a: IndicatorChoice = { method: "sma", lookback: 20, target: "price" };
    const b: IndicatorChoice = { method: "sma", lookback: 20, target: "price" };
    const c: IndicatorChoice = { method: "ema", lookback: 20, target: "price" };
    expect(indicatorChoiceKey(a)).toBe(indicatorChoiceKey(b));
    expect(indicatorChoiceKey(a)).not.toBe(indicatorChoiceKey(c));
  });
});

describe("loadIndicatorChoices / saveIndicatorChoices", () => {
  it("round-trips a saved list of choices", () => {
    const store = storage();
    const choices: IndicatorChoice[] = [
      { method: "sma", lookback: 20, target: "price" },
      { method: "ema", lookback: 12, target: "price" },
    ];
    saveIndicatorChoices(choices, store);
    expect(loadIndicatorChoices(store)).toEqual(choices);
  });

  it("returns an empty list when nothing was stored", () => {
    expect(loadIndicatorChoices(storage())).toEqual([]);
  });

  it("discards a corrupt entry rather than keeping the valid-looking part of it", () => {
    const bad = JSON.stringify([
      { method: "sma", lookback: 20, target: "price" },
      { method: "not-a-method", lookback: 20, target: "price" },
    ]);
    // A filtering ("repairing") implementation would keep the first choice.
    // Discarding the whole list keeps the promise that what is shown is
    // exactly what the visitor chose, not a best-effort remainder.
    expect(loadIndicatorChoices(storage(bad))).toEqual([]);
  });

  it("removes the corrupt entry so it cannot be read again", () => {
    const store = storage("not json");
    loadIndicatorChoices(store);
    expect(store.removeItem).toHaveBeenCalledWith("radar.chart.indicators");
  });

  it("survives storage that throws outright", () => {
    expect(loadIndicatorChoices(storage(undefined, true))).toEqual([]);
  });

  it("does not throw when storage cannot be written", () => {
    expect(() =>
      saveIndicatorChoices([{ method: "sma", lookback: 20, target: "price" }], storage(undefined, true)),
    ).not.toThrow();
  });
});
