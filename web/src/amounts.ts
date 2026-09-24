// SPDX-License-Identifier: Apache-2.0
//! Turning what someone typed into the exact integer the server's wire
//! contract wants, without ever putting it through a float.
//!
//! Every amount `/v1/customer/swap` and `/v1/market/quote` read or write is a
//! u64 base-unit count, sent as a JSON decimal string -- `"1500000000"`, never
//! `1.5`. A `number` cannot hold every u64 exactly (`Number.MAX_SAFE_INTEGER`
//! is about 9 * 10^15, and a token with 9 decimals overflows that at 9,007,199
//! tokens), and `parseFloat` followed by multiplying by `10 ** decimals`
//! reintroduces the float it was supposed to avoid -- `0.1 * 1e9` is
//! `100000000.00000001` in IEEE 754, not `100000000`. This module works on the
//! decimal string's digits directly and only ever hands the result to
//! `BigInt`, which is exact.

/** Why a typed amount could not become a base-unit count. */
export type AmountError =
  | { kind: "empty" }
  | { kind: "not-a-number" }
  | { kind: "negative" }
  | { kind: "zero" }
  | { kind: "too-many-decimals"; max: number }
  | { kind: "over-max" };

export type AmountResult =
  | { ok: true; baseUnits: string }
  | { ok: false; error: AmountError };

/**
 * Converts a decimal amount someone typed (e.g. `"1.5"`) into the base-unit
 * string the wire contract wants (e.g. `"1500000000"` at 9 decimals).
 *
 * `maxBaseUnits`, when given, is compared as an integer via `BigInt` -- never
 * by converting either side to a decimal `number` first, which is exactly the
 * precision loss this module exists to avoid. The caller passes the wallet's
 * own holding (`PositionsToken.amount`, already a base-unit string) for the
 * sell side's "you cannot sell more than you hold" rule.
 *
 * Too many fractional digits is **rejected, not truncated or rounded** --
 * silently dropping the extra digits would send the server a smaller amount
 * than the person typed, which for a sell is "sell less than I asked" and for
 * a buy is "spend less than I asked", both wrong in a way nothing would ever
 * surface.
 */
export function toBaseUnits(
  input: string,
  decimals: number,
  maxBaseUnits?: string,
): AmountResult {
  const trimmed = input.trim();
  if (trimmed === "") return { ok: false, error: { kind: "empty" } };
  if (trimmed.startsWith("-")) return { ok: false, error: { kind: "negative" } };

  // Whole digits, optionally a dot, optionally fraction digits. Anything else
  // -- a second dot, a sign in the middle, a letter, scientific notation --
  // is "not a number" rather than something this tries to interpret.
  const match = /^([0-9]*)(?:\.([0-9]*))?$/.exec(trimmed);
  if (!match) return { ok: false, error: { kind: "not-a-number" } };
  const wholeRaw = match[1] ?? "";
  const fracRaw = match[2] ?? "";
  if (wholeRaw === "" && fracRaw === "") {
    // Matched by the regex (both groups can be empty) but means nothing --
    // e.g. the input was just ".".
    return { ok: false, error: { kind: "not-a-number" } };
  }
  if (fracRaw.length > decimals) {
    return { ok: false, error: { kind: "too-many-decimals", max: decimals } };
  }

  const whole = wholeRaw === "" ? "0" : wholeRaw;
  const fracPadded = fracRaw.padEnd(decimals, "0");
  const value = BigInt(`${whole}${fracPadded}`);

  if (value === 0n) return { ok: false, error: { kind: "zero" } };
  if (maxBaseUnits !== undefined && value > BigInt(maxBaseUnits)) {
    return { ok: false, error: { kind: "over-max" } };
  }
  return { ok: true, baseUnits: value.toString() };
}

/** The sentence for an [`AmountError`], for the field it was typed into. */
export function amountErrorMessage(error: AmountError, symbol: string): string {
  switch (error.kind) {
    case "empty":
      return "Enter an amount.";
    case "not-a-number":
      return "That is not a number.";
    case "negative":
      return "Enter a positive amount.";
    case "zero":
      return "Enter an amount greater than zero.";
    case "too-many-decimals":
      return `${symbol} has ${error.max} decimal place${error.max === 1 ? "" : "s"}; that is more than it can hold.`;
    case "over-max":
      return `You do not have that much ${symbol}.`;
  }
}
