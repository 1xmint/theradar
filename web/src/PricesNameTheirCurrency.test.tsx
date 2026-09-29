// SPDX-License-Identifier: Apache-2.0
//! No market price Radar serves is in dollars: each is `quote_amount /
//! token_amount` in whatever the trade paid in. A "$" on one is a claim about
//! money the reader is about to act on, so no price panel may print it, and each
//! names its unit when the record carries a quote mint.

import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { MarketCoin, MarketToken } from "./api";
import { CoinList } from "./CoinList";
import { TokenHeader } from "./TokenHeader";
import { TradeTape } from "./TradeTape";
import { YourTradesPanel } from "./YourTradesPanel";

const MINT = "5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump";
const WALLET = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";
const WSOL = "So11111111111111111111111111111111111111112";
const PRICE = 0.000179;

afterEach(() => {
  localStorage.clear();
  vi.unstubAllGlobals();
});

function json(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

const aCoin: MarketCoin = {
  mint: MINT,
  tx_count: 3,
  token_volume: 10,
  quote_mint: WSOL,
  quote_volume: 1,
  price: PRICE,
  change_pct: null,
};

const aToken: MarketToken = {
  mint: MINT,
  name: null,
  symbol: null,
  creator: null,
  uri: null,
  published_at: null,
  metadata_reason: "none",
  price: PRICE,
  quote_mint: WSOL,
  price_reason: null,
  decimals_reason: "per-trade",
  market_cap: null,
  market_cap_reason: "not computed",
  liquidity: null,
  liquidity_reason: "not computed",
};

describe("price panels", () => {
  it("never print a dollar sign on a price, and name the unit beside it", async () => {
    const list = render(
      <CoinList
        load={{
          state: "ready",
          value: { coins: [aCoin], window: { from: "a", to: "b" } },
        }}
        sort={{ key: "volume", dir: "desc" }}
        onSortChange={() => {}}
        selectedMint={null}
        onSelect={() => {}}
      />,
    );
    expect(list.container.textContent).toContain("0.000179 SOL");
    expect(list.container.textContent).not.toContain("$");
    list.unmount();

    const header = render(<TokenHeader load={{ state: "ready", value: aToken }} />);
    expect(header.container.textContent).toContain("0.000179 SOL");
    expect(header.container.textContent).not.toContain("$");
    header.unmount();

    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        json({
          mint: MINT,
          trades: [
            {
              ts: "2026-09-29 00:00:00",
              signature: "sig1",
              side: "buy",
              token_amount: 10,
              quote_amount: 1,
              quote_mint: WSOL,
              price: PRICE,
              trader: WALLET,
            },
          ],
        }),
      ),
    );
    const tape = render(<TradeTape mint={MINT} />);
    await screen.findByText("0.000179 SOL");
    expect(tape.container.textContent).not.toContain("$");
    tape.unmount();

    localStorage.setItem(
      "radar.wallet.session",
      JSON.stringify({ token: "t", address: WALLET, expiresInSeconds: 3600 }),
    );
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        json({
          mint: MINT,
          wallet: WALLET,
          fold: {
            fact: "wallet_trades_in_window",
            complete: false,
            truncated: false,
            unattributable_trades: 0,
            trades: [
              {
                ts: "2026-09-29 00:00:00",
                slot: 1,
                signature: "sig2",
                side: "buy",
                token_amount: 10,
                quote_amount: 1,
                price: PRICE,
                quote_mint: WSOL,
                matched_by: "trader",
              },
            ],
          },
          caveat: "c",
        }),
      ),
    );
    const own = render(<YourTradesPanel mint={MINT} />);
    await screen.findByText("0.000179 SOL");
    expect(own.container.textContent).not.toContain("$");
  });

  it("show no unit rather than assume SOL when the quote asset is unknown", () => {
    const header = render(
      <TokenHeader load={{ state: "ready", value: { ...aToken, quote_mint: null } }} />,
    );
    expect(header.container.textContent).toContain("0.000179");
    expect(header.container.textContent).not.toContain("0.000179 ");
  });
});
