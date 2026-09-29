// SPDX-License-Identifier: Apache-2.0
/**
 * `api.ts`'s `Candle` against `radar_backfill::market::fold::Candle`, the
 * struct both candle handlers (`radar-serve/src/market/mod.rs` and
 * `live.rs::roll_up`) serialise. The web `Candle` once declared a `volume`
 * field that does not exist on the wire -- `quote_volume` does -- so the
 * volume histogram and its SMA silently read `undefined` from every response.
 * Modelled on `tradeContract.test.ts`, which does the same check for the swap
 * routes: read the Rust source and compare text with text, so a field
 * renamed on one side fails here rather than in a chart that draws nothing.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const FOLD_RS = readFileSync(
  resolve(__dirname, "../../crates/radar-backfill/src/market/fold.rs"),
  "utf8",
);
const API_TS = readFileSync(resolve(__dirname, "./api.ts"), "utf8");

/** Field names of `pub struct Candle { ... }` in fold.rs. */
function rustCandleFields(): string[] {
  const at = FOLD_RS.indexOf("pub struct Candle {");
  expect(at, "`pub struct Candle` not found in fold.rs").toBeGreaterThan(-1);
  const end = FOLD_RS.indexOf("\n}", at);
  expect(end, "struct Candle is not closed").toBeGreaterThan(at);
  const body = FOLD_RS.slice(at, end);
  return [...body.matchAll(/pub ([a-z_]+):/g)].map((m) => m[1]!);
}

/** Field names of `export interface Candle { ... }` in api.ts. */
function webCandleFields(): string[] {
  const at = API_TS.indexOf("export interface Candle {");
  expect(at, "`interface Candle` not found in api.ts").toBeGreaterThan(-1);
  const end = API_TS.indexOf("\n}", at);
  expect(end, "interface Candle is not closed").toBeGreaterThan(at);
  const body = API_TS.slice(at, end);
  return [...body.matchAll(/^\s+([a-z_]+):/gm)].map((m) => m[1]!);
}

describe("the candle contract agrees on both sides", () => {
  it("api.ts's Candle has exactly fold.rs's Candle fields", () => {
    const server = rustCandleFields();
    const web = webCandleFields();
    expect(server.length, "found no fields on the Rust struct?").toBeGreaterThan(5);
    expect([...web].sort()).toEqual([...server].sort());
  });
});
