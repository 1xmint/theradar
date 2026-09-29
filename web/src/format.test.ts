// SPDX-License-Identifier: Apache-2.0
import { describe, expect, it } from "vitest";
import { formatChartPrice, formatSolPrice } from "./format";

describe("formatSolPrice", () => {
  it("never prefixes with '$' -- a SOL-quoted number is not a dollar figure", () => {
    expect(formatSolPrice(1.2345)).not.toContain("$");
    expect(formatSolPrice(2.2e-7)).not.toContain("$");
  });

  it("keeps real precision at pump.fun magnitudes, unlike a fixed four decimals", () => {
    // 0.0000 is what `formatSolAmount`'s fixed four decimals would print here
    // -- indistinguishable from every other sub-cent price.
    expect(formatSolPrice(2.2e-7)).not.toBe("0.0000");
    expect(formatSolPrice(2.2e-7)).toBe((2.2e-7).toPrecision(3));
  });

  it("is exactly zero for zero, not an empty precision string", () => {
    expect(formatSolPrice(0)).toBe("0.00");
  });
});

describe("formatChartPrice", () => {
  it("tells apart two axis labels a quiet coin's candles sit between", () => {
    // Both read "0.000181" at three figures -- the repeated axis labels seen
    // in production on 2026-09-29.
    expect(formatChartPrice(0.00018053)).toBe("0.00018053");
    expect(formatChartPrice(0.0001810)).toBe("0.00018100");
    expect(formatChartPrice(0.00018053)).not.toBe(formatChartPrice(0.000181));
  });

  it("defers to formatSolPrice at zero and above a cent", () => {
    expect(formatChartPrice(0)).toBe("0.00");
    expect(formatChartPrice(1.2953886)).toBe("1.2954");
  });
});
