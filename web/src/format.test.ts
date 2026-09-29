// SPDX-License-Identifier: Apache-2.0
import { describe, expect, it } from "vitest";
import { formatSolPrice } from "./format";

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
