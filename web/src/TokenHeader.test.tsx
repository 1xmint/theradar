// SPDX-License-Identifier: Apache-2.0
//! The token header's name and symbol: shown when the server recorded a
//! launch, and the shortened mint -- never the word "unknown" -- when it did
//! not. `CoinImage` fetches nothing here because these tokens carry no `uri`.

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { MarketToken } from "./api";
import type { Load } from "./useApi";
import { TokenHeader } from "./TokenHeader";

const MINT = "5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump";

function token(overrides: Partial<MarketToken> = {}): MarketToken {
  return {
    mint: MINT,
    name: null,
    symbol: null,
    creator: null,
    uri: null,
    published_at: null,
    metadata_reason: "no pump.fun launch was recorded for this mint",
    price: null,
    price_reason: "no route priced",
    decimals_reason: "decimals travel per-trade",
    market_cap: null,
    market_cap_reason: "not computed",
    liquidity: null,
    liquidity_reason: "not computed",
    ...overrides,
  };
}

function ready(value: MarketToken): Load<MarketToken> {
  return { state: "ready", value };
}

describe("TokenHeader", () => {
  it("shows the recorded name and symbol when the server sent them", () => {
    render(
      <TokenHeader
        load={ready(token({ name: "Radar Coin", symbol: "RADAR" }))}
      />,
    );
    expect(screen.getByText("RADAR")).toBeTruthy();
    expect(screen.getByText("Radar Coin")).toBeTruthy();
  });

  it("falls back to the shortened mint, never the word unknown, when name and symbol are null", () => {
    render(<TokenHeader load={ready(token())} />);
    const shortMint = `${MINT.slice(0, 4)}…${MINT.slice(-4)}`;
    // Both the heading (symbol) and the subtitle (name) fall back to the same
    // shortened mint -- two occurrences, not one.
    expect(screen.getAllByText(shortMint)).toHaveLength(2);
    // "first seen unknown" is a real, unrelated fact this header states when
    // `published_at` is null -- it must not be confused with a name or symbol
    // rendering the word "unknown", which this test would otherwise miss.
    expect(screen.getByRole("heading", { level: 1 }).textContent).not.toMatch(/unknown/i);
  });
});
