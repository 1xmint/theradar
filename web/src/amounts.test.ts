// SPDX-License-Identifier: Apache-2.0
import { describe, expect, it } from "vitest";
import { amountErrorMessage, toBaseUnits } from "./amounts";

describe("toBaseUnits", () => {
  it("converts a whole number at 9 decimals", () => {
    expect(toBaseUnits("2", 9)).toEqual({ ok: true, baseUnits: "2000000000" });
  });

  it("converts a fractional amount without float drift", () => {
    // 0.1 * 1e9 is 100000000.00000001 in IEEE 754 -- this must not be that.
    expect(toBaseUnits("0.1", 9)).toEqual({ ok: true, baseUnits: "100000000" });
  });

  it("converts an amount with the maximum number of decimal places", () => {
    expect(toBaseUnits("1.123456789", 9)).toEqual({ ok: true, baseUnits: "1123456789" });
  });

  it("handles a leading-dot amount", () => {
    expect(toBaseUnits(".5", 9)).toEqual({ ok: true, baseUnits: "500000000" });
  });

  it("handles a trailing-dot amount", () => {
    expect(toBaseUnits("5.", 9)).toEqual({ ok: true, baseUnits: "5000000000" });
  });

  it("handles a large amount past Number.MAX_SAFE_INTEGER exactly", () => {
    // 10,000,000 tokens at 9 decimals is well past 2^53.
    expect(toBaseUnits("10000000.000000001", 9)).toEqual({
      ok: true,
      baseUnits: "10000000000000001",
    });
  });

  it("rejects an empty string", () => {
    expect(toBaseUnits("", 9)).toEqual({ ok: false, error: { kind: "empty" } });
  });

  it("rejects whitespace only", () => {
    expect(toBaseUnits("   ", 9)).toEqual({ ok: false, error: { kind: "empty" } });
  });

  it("rejects a negative amount", () => {
    expect(toBaseUnits("-1", 9)).toEqual({ ok: false, error: { kind: "negative" } });
  });

  it("rejects zero", () => {
    expect(toBaseUnits("0", 9)).toEqual({ ok: false, error: { kind: "zero" } });
  });

  it("rejects a zero with trailing decimal zeros", () => {
    expect(toBaseUnits("0.000", 9)).toEqual({ ok: false, error: { kind: "zero" } });
  });

  it("rejects too many decimal places rather than rounding", () => {
    expect(toBaseUnits("1.1234567891", 9)).toEqual({
      ok: false,
      error: { kind: "too-many-decimals", max: 9 },
    });
  });

  it("rejects letters", () => {
    expect(toBaseUnits("abc", 9)).toEqual({ ok: false, error: { kind: "not-a-number" } });
  });

  it("rejects a bare dot", () => {
    expect(toBaseUnits(".", 9)).toEqual({ ok: false, error: { kind: "not-a-number" } });
  });

  it("rejects two dots", () => {
    expect(toBaseUnits("1.2.3", 9)).toEqual({ ok: false, error: { kind: "not-a-number" } });
  });

  it("rejects scientific notation", () => {
    expect(toBaseUnits("1e9", 9)).toEqual({ ok: false, error: { kind: "not-a-number" } });
  });

  it("allows an amount exactly equal to the maximum", () => {
    expect(toBaseUnits("1", 9, "1000000000")).toEqual({ ok: true, baseUnits: "1000000000" });
  });

  it("rejects an amount one base unit over the maximum", () => {
    expect(toBaseUnits("1.000000001", 9, "1000000000")).toEqual({
      ok: false,
      error: { kind: "over-max" },
    });
  });

  it("rejects an amount over the maximum even past safe-integer precision", () => {
    // If this compared via `Number`, both sides would round to the same
    // float and the over-max amount would slip through.
    expect(toBaseUnits("9007199254.740993", 9, "9007199254740992")).toEqual({
      ok: false,
      error: { kind: "over-max" },
    });
  });
});

describe("amountErrorMessage", () => {
  it("renders every kind of error distinctly", () => {
    const texts = [
      amountErrorMessage({ kind: "empty" }, "SOL"),
      amountErrorMessage({ kind: "not-a-number" }, "SOL"),
      amountErrorMessage({ kind: "negative" }, "SOL"),
      amountErrorMessage({ kind: "zero" }, "SOL"),
      amountErrorMessage({ kind: "too-many-decimals", max: 9 }, "SOL"),
      amountErrorMessage({ kind: "over-max" }, "SOL"),
    ];
    expect(new Set(texts).size).toBe(texts.length);
  });

  it("names the token in the over-max message", () => {
    expect(amountErrorMessage({ kind: "over-max" }, "BONK")).toContain("BONK");
  });

  it("refuses an amount past u64 rather than sending one the server can only reject", () => {
    expect(toBaseUnits("18446744073709551615", 0)).toEqual({ ok: true, baseUnits: "18446744073709551615" });
    expect(toBaseUnits("18446744073709551616", 0)).toEqual({ ok: false, error: { kind: "too-large" } });
    expect(toBaseUnits("18446744073.709551616", 9)).toEqual({ ok: false, error: { kind: "too-large" } });
  });
});
