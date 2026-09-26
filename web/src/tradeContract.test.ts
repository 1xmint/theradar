// SPDX-License-Identifier: Apache-2.0
/**
 * The swap contract, read from both sides.
 *
 * `api.ts`'s `Quote` and `SwapResponse` were written on 2026-09-24 against
 * the doc comment at the top of `crates/radar-serve/src/trade.rs`, and plan
 * 0013 recorded them as "a contract, not an observation": nothing had ever
 * pushed a real quote through the screen. This file is the observation, done
 * the way `routes.test.ts` does it for `access.rs`: read the Rust source and
 * compare text with text. A field renamed on one side fails here, in CI,
 * before the review card can show a number the transaction does not contain.
 *
 * Three things are checked, because three things can drift apart:
 *
 * - the keys `render_quote` emits are exactly `Quote`'s fields;
 * - the keys the swap route emits are exactly `SwapResponse`'s fields, and
 *   its nested `quote` is the same `render_quote` output -- the review step
 *   shows the numbers the transaction was built from, not a second quote;
 * - every refusal `reason` the trade routes can send has a sentence in
 *   `swapRefusalMessage`. The default branch shows the server's own
 *   sentence, which is honest but bare; a reason added on the server and
 *   forgotten here should be a failing test, not a visitor reading a code.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { swapRefusalMessage } from "./honesty";

const TRADE_RS = readFileSync(
  resolve(__dirname, "../../crates/radar-serve/src/trade.rs"),
  "utf8",
);
const API_TS = readFileSync(resolve(__dirname, "./api.ts"), "utf8");

/** The first capture group of every match, typed as the strings they are. */
function captures(text: string, pattern: RegExp): string[] {
  return [...text.matchAll(pattern)].flatMap((m) => (m[1] === undefined ? [] : [m[1]]));
}

/** Keys of the first `json!({ ... })` literal after `marker` in trade.rs. */
function jsonKeysAfter(marker: string): string[] {
  const at = TRADE_RS.indexOf(marker);
  expect(at, `\`${marker}\` not found in trade.rs`).toBeGreaterThan(-1);
  const open = TRADE_RS.indexOf("json!({", at);
  expect(open, `no json! literal after \`${marker}\``).toBeGreaterThan(at);
  const close = TRADE_RS.indexOf("})", open);
  expect(close, "the json! literal is not closed").toBeGreaterThan(open);
  const body = TRADE_RS.slice(open, close);
  return captures(body, /"([a-z_]+)":/g);
}

/**
 * Every key any `json!({ ... })` literal inside `fn <name>(` emits, unioned
 * across every branch -- `render_tx_status` returns a different literal per
 * match arm (`failed`/`landed`/`pending`/`expired`), unlike `render_quote`'s
 * single literal, so this counts distinct keys across the whole function
 * body rather than assuming there is only one `json!({...})` to read.
 */
function jsonKeysUnionIn(fnMarker: string): string[] {
  const at = TRADE_RS.indexOf(fnMarker);
  expect(at, `\`${fnMarker}\` not found in trade.rs`).toBeGreaterThan(-1);
  const end = TRADE_RS.indexOf("\n}\n", at);
  expect(end, `${fnMarker} is not closed`).toBeGreaterThan(at);
  const body = TRADE_RS.slice(at, end);
  return [...new Set(captures(body, /"([a-z_]+)":/g))];
}

/** Field names of `export interface <name> { ... }` in api.ts. */
function interfaceFields(name: string): string[] {
  const at = API_TS.indexOf(`export interface ${name} {`);
  expect(at, `\`interface ${name}\` not found in api.ts`).toBeGreaterThan(-1);
  const end = API_TS.indexOf("\n}", at);
  expect(end, `interface ${name} is not closed`).toBeGreaterThan(at);
  const body = API_TS.slice(at, end);
  // A field line: indentation, the name, an optional `?`, a colon. Comment
  // lines inside the interface start with `/**` or `*` and never match.
  return captures(body, /^\s+([a-z_]+)\??:/gm);
}

describe("the swap contract agrees on both sides", () => {
  it("render_quote emits exactly the fields Quote declares", () => {
    const server = jsonKeysAfter("fn render_quote(");
    const web = interfaceFields("Quote");
    expect(server.length, "render_quote emits nothing?").toBeGreaterThan(5);
    expect([...server].sort()).toEqual([...web].sort());
  });

  it("the swap route emits exactly the fields SwapResponse declares", () => {
    const server = jsonKeysAfter("async fn swap_inner(");
    const web = interfaceFields("SwapResponse");
    expect([...server].sort()).toEqual([...web].sort());
  });

  it("the swap response's nested quote is render_quote's own output", () => {
    const at = TRADE_RS.indexOf("async fn swap_inner(");
    const body = TRADE_RS.slice(at);
    // The variable the JSON literal nests must be the one `render_quote`
    // filled. If a second quote is ever fetched for the review card, this is
    // where the two would diverge.
    expect(body).toMatch(/let quote_json = render_quote\(/);
    expect(body).toMatch(/"quote": quote_json,/);
  });

  it("render_tx_status emits exactly the keys TxStatus declares, across every state", () => {
    const server = jsonKeysUnionIn("fn render_tx_status(");
    const web = interfaceFields("TxStatus");
    expect(server.length, "render_tx_status emits nothing?").toBeGreaterThan(1);
    expect([...server].sort()).toEqual([...web].sort());
  });

  it("every refusal reason the trade routes send has a sentence on the screen", () => {
    const reasons = [
      ...new Set(captures(TRADE_RS, /refusal\(\s*StatusCode::[A-Z_]+,\s*"([a-z_]+)"/g)),
    ];
    expect(reasons.length, "no refusal( calls found in trade.rs").toBeGreaterThan(4);
    const detail = "the server's own sentence";
    for (const reason of reasons) {
      expect(
        swapRefusalMessage(reason, detail),
        `\`${reason}\` falls through to the server's bare sentence`,
      ).not.toBe(detail);
    }
  });
});
