// SPDX-License-Identifier: Apache-2.0
//! The token header's name and symbol: shown when the server recorded a
//! launch, and the shortened mint -- never the word "unknown" -- when it did
//! not. `CoinImage` fetches nothing here because these tokens carry no `uri`.

import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { MarketToken } from "./api";
import type { Load } from "./useApi";
import { TokenHeader } from "./TokenHeader";

const MINT = "5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump";
const WALLET = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";

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

function signIn(): void {
  localStorage.setItem(
    "radar.wallet.session",
    JSON.stringify({ token: "a-token", address: WALLET, expiresInSeconds: 3600 }),
  );
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function watchlistBody(coins: string[], limit = 100): unknown {
  return { wallet: WALLET, coins, limit };
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

//! The watchlist star and panel: one `useWatchlist` read shared by both, so
//! toggling the star is what the panel below it is checked against too.

describe("TokenHeader watchlist", () => {
  afterEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("invites a stranger to connect a wallet, in the panel and on the star, rather than a silent no-op", () => {
    render(<TokenHeader load={ready(token())} />);
    expect(screen.getByText(/connect a wallet to keep a watchlist/i)).toBeTruthy();
    const star = screen.getByRole("button", { name: /connect a wallet to keep a watchlist/i });
    expect((star as HTMLButtonElement).disabled).toBe(true);
  });

  it("says the watchlist is empty for a signed-in wallet with nothing saved", async () => {
    signIn();
    vi.stubGlobal("fetch", vi.fn(async () => jsonResponse(watchlistBody([]))));
    render(<TokenHeader load={ready(token())} />);

    expect(await screen.findByText("Your watchlist is empty")).toBeTruthy();
  });

  it("says something other than 'empty' when the read fails, and the two sentences differ", async () => {
    signIn();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new TypeError("network down");
      }),
    );
    render(<TokenHeader load={ready(token())} />);

    const failed = await screen.findByText(/could not read your watchlist/i);
    expect(failed.textContent).not.toBe("Your watchlist is empty");
  });

  it("fills the star when the server's list already carries this mint", async () => {
    signIn();
    vi.stubGlobal("fetch", vi.fn(async () => jsonResponse(watchlistBody([MINT]))));
    render(<TokenHeader load={ready(token())} />);

    const star = await screen.findByRole("button", { name: /remove from your watchlist/i });
    expect(star.textContent).toBe("★");
  });

  it("toggles by calling PUT then DELETE against the mint's own path with the bearer token", async () => {
    signIn();
    const fetcher = vi.fn(async (_url: string, init?: RequestInit) => {
      if (init?.method === "PUT") return jsonResponse(watchlistBody([MINT]));
      if (init?.method === "DELETE") return jsonResponse(watchlistBody([]));
      return jsonResponse(watchlistBody([]));
    });
    vi.stubGlobal("fetch", fetcher);
    render(<TokenHeader load={ready(token())} />);

    const addStar = await screen.findByRole("button", { name: /add to your watchlist/i });
    fireEvent.click(addStar);
    await screen.findByRole("button", { name: /remove from your watchlist/i });

    const removeStar = screen.getByRole("button", { name: /remove from your watchlist/i });
    fireEvent.click(removeStar);
    await screen.findByRole("button", { name: /add to your watchlist/i });

    const calls = fetcher.mock.calls as [string, RequestInit | undefined][];
    const put = calls.find(([, init]) => init?.method === "PUT");
    const del = calls.find(([, init]) => init?.method === "DELETE");

    expect(put).toBeTruthy();
    expect(String(put?.[0])).toBe(`/v1/customer/watchlist/${MINT}`);
    expect((put?.[1]?.headers as Record<string, string>).authorization).toBe("Bearer a-token");

    expect(del).toBeTruthy();
    expect(String(del?.[0])).toBe(`/v1/customer/watchlist/${MINT}`);
    expect((del?.[1]?.headers as Record<string, string>).authorization).toBe("Bearer a-token");
  });

  it("says the list is full when the server refuses a 409, beside the star", async () => {
    signIn();
    const fetcher = vi.fn(async (_url: string, init?: RequestInit) => {
      if (init?.method === "PUT") {
        return jsonResponse(
          { error: "a watchlist holds at most 100 coins; remove one first", reason: "full" },
          409,
        );
      }
      return jsonResponse(watchlistBody([]));
    });
    vi.stubGlobal("fetch", fetcher);
    render(<TokenHeader load={ready(token())} />);

    const addStar = await screen.findByRole("button", { name: /add to your watchlist/i });
    fireEvent.click(addStar);

    expect(await screen.findByText(/remove one before adding another/i)).toBeTruthy();
  });

  it("never sends a query string to a watchlist route", async () => {
    signIn();
    const fetcher = vi.fn(async (_url: string, init?: RequestInit) => {
      if (init?.method === "PUT") return jsonResponse(watchlistBody([MINT]));
      return jsonResponse(watchlistBody([]));
    });
    vi.stubGlobal("fetch", fetcher);
    render(<TokenHeader load={ready(token())} />);

    const addStar = await screen.findByRole("button", { name: /add to your watchlist/i });
    fireEvent.click(addStar);
    await screen.findByRole("button", { name: /remove from your watchlist/i });

    for (const [url] of fetcher.mock.calls as [string, RequestInit | undefined][]) {
      expect(String(url)).not.toContain("?");
    }
  });
});
