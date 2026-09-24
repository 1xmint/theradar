// SPDX-License-Identifier: Apache-2.0
//! The token header's name and symbol: shown when the server recorded a
//! launch, and the shortened mint -- never the word "unknown" -- when it did
//! not. `CoinImage` fetches nothing here because these tokens carry no `uri`.

import { act, fireEvent, render, screen } from "@testing-library/react";
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

/**
 * An empty, fully-shaped `/v1/customer/positions` body.
 *
 * `TokenHeader` now reads both the watchlist and positions on sign-in, so
 * every fetch stub below must answer both routes. This is the "nothing
 * held" answer -- a stub that instead reused `watchlistBody`'s shape for a
 * positions request would hand `PositionsPanel` a body with no `tokens`
 * array and crash it, which is exactly what caught this the first time.
 */
function positionsBody(): unknown {
  return {
    wallet: WALLET,
    slot: 1,
    read_at: 0,
    age_seconds: 0,
    sol: {
      lamports: 0,
      ui_amount: "0.000000000",
      price: null,
      quote: null,
      value: null,
      priced: false,
      price_reason: null,
    },
    tokens: [],
  };
}

/** Routes a stubbed fetch by URL: positions gets its own empty body, and
 *  everything else (the watchlist) falls through to `otherwise`. */
function respond(
  url: string | URL | Request,
  otherwise: () => Response | Promise<Response>,
): Response | Promise<Response> {
  return String(url).includes("/customer/positions") ? jsonResponse(positionsBody()) : otherwise();
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
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => respond(url, () => jsonResponse(watchlistBody([])))),
    );
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
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => respond(url, () => jsonResponse(watchlistBody([MINT])))),
    );
    render(<TokenHeader load={ready(token())} />);

    const star = await screen.findByRole("button", { name: /remove from your watchlist/i });
    expect(star.textContent).toBe("★");
  });

  it("toggles by calling PUT then DELETE against the mint's own path with the bearer token", async () => {
    signIn();
    const fetcher = vi.fn(async (url: string, init?: RequestInit) => {
      return respond(url, () => {
        if (init?.method === "PUT") return jsonResponse(watchlistBody([MINT]));
        if (init?.method === "DELETE") return jsonResponse(watchlistBody([]));
        return jsonResponse(watchlistBody([]));
      });
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
    const fetcher = vi.fn(async (url: string, init?: RequestInit) => {
      return respond(url, () => {
        if (init?.method === "PUT") {
          return jsonResponse(
            { error: "a watchlist holds at most 100 coins; remove one first", reason: "full" },
            409,
          );
        }
        return jsonResponse(watchlistBody([]));
      });
    });
    vi.stubGlobal("fetch", fetcher);
    render(<TokenHeader load={ready(token())} />);

    const addStar = await screen.findByRole("button", { name: /add to your watchlist/i });
    fireEvent.click(addStar);

    expect(await screen.findByText(/remove one before adding another/i)).toBeTruthy();
  });

  it("sends one change for a double click, not the same change twice", async () => {
    signIn();
    let answer: (response: Response) => void = () => {};
    const fetcher = vi.fn(async (url: string, init?: RequestInit) => {
      return respond(url, () => {
        if (init?.method === "PUT") return new Promise<Response>((resolve) => (answer = resolve));
        return jsonResponse(watchlistBody([]));
      });
    });
    vi.stubGlobal("fetch", fetcher);
    render(<TokenHeader load={ready(token())} />);

    const addStar = await screen.findByRole("button", { name: /add to your watchlist/i });
    fireEvent.click(addStar);
    fireEvent.click(addStar);
    answer(jsonResponse(watchlistBody([MINT])));
    await screen.findByRole("button", { name: /remove from your watchlist/i });

    const changes = (fetcher.mock.calls as [string, RequestInit | undefined][]).filter(
      ([, init]) => init?.method === "PUT" || init?.method === "DELETE",
    );
    expect(changes).toHaveLength(1);
  });

  it("never sends a query string to a watchlist route", async () => {
    signIn();
    const fetcher = vi.fn(async (url: string, init?: RequestInit) => {
      return respond(url, () => {
        if (init?.method === "PUT") return jsonResponse(watchlistBody([MINT]));
        return jsonResponse(watchlistBody([]));
      });
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

//! The positions panel: the signed-in wallet's own on-chain holdings, read
//! and priced by the server -- see `positions.rs` and `usePositions.ts`.

describe("TokenHeader positions", () => {
  afterEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  /** Stubs `fetch` so `/v1/customer/positions` answers with `body`/`status`
   *  and everything else (the watchlist, read alongside it) answers empty. */
  function stubPositions(body: unknown, status = 200): void {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) =>
        String(url).includes("/customer/positions")
          ? jsonResponse(body, status)
          : jsonResponse(watchlistBody([])),
      ),
    );
  }

  it("invites a stranger to connect a wallet before showing any holdings", () => {
    render(<TokenHeader load={ready(token())} />);
    expect(
      screen.getByText(/connect a wallet to see what it holds/i),
    ).toBeTruthy();
  });

  it("says a wallet with nothing on chain holds no tokens", async () => {
    signIn();
    stubPositions({
      wallet: WALLET,
      slot: 1,
      read_at: 0,
      age_seconds: 0,
      sol: {
        lamports: 0,
        ui_amount: "0.000000000",
        price: null,
        quote: null,
        value: null,
        priced: false,
        price_reason: null,
      },
      tokens: [],
    });
    render(<TokenHeader load={ready(token())} />);

    expect(await screen.findByText("This wallet holds no tokens")).toBeTruthy();
  });

  it("adds the SOL balance to the empty sentence rather than calling a wallet with SOL 'empty'", async () => {
    signIn();
    stubPositions({
      wallet: WALLET,
      slot: 1,
      read_at: 0,
      age_seconds: 0,
      sol: {
        lamports: 1_500_000_000,
        ui_amount: "1.500000000",
        price: null,
        quote: null,
        value: null,
        priced: false,
        price_reason: "no SOL/USDC or SOL/USDT trade in the tape",
      },
      tokens: [],
    });
    render(<TokenHeader load={ready(token())} />);

    expect(await screen.findByText(/holds no tokens.*1\.500000000 SOL/i)).toBeTruthy();
  });

  it("marks an untracked mint as unpriced rather than $0, and links the tracked one", async () => {
    signIn();
    stubPositions({
      wallet: WALLET,
      slot: 7,
      read_at: 0,
      age_seconds: 3,
      sol: {
        lamports: 0,
        ui_amount: "0.000000000",
        price: null,
        quote: null,
        value: null,
        priced: false,
        price_reason: null,
      },
      tokens: [
        {
          mint: MINT,
          program: "token",
          amount: "1000000",
          decimals: 6,
          ui_amount: "1",
          price: null,
          quote: null,
          value: null,
          priced: false,
        },
      ],
    });
    render(<TokenHeader load={ready(token())} />);

    expect(await screen.findByText("Radar does not price this coin")).toBeTruthy();
    const shortMint = `${MINT.slice(0, 4)}…${MINT.slice(-4)}`;
    expect(screen.getByRole("link", { name: shortMint }).getAttribute("href")).toBe(`/token/${MINT}`);
  });

  it("shows a SOL row priced in SOL terms, never as a dollar figure, alongside a priced USDC token", async () => {
    signIn();
    stubPositions({
      wallet: WALLET,
      slot: 9,
      read_at: 0,
      age_seconds: 1,
      sol: {
        lamports: 2_000_000_000,
        ui_amount: "2.000000000",
        price: 150.5,
        quote: "USDC",
        value: 301,
        priced: true,
        price_reason: null,
      },
      tokens: [
        {
          mint: MINT,
          program: "token",
          amount: "1000000",
          decimals: 6,
          ui_amount: "1",
          price: 0.5,
          quote: "USDC",
          value: 0.5,
          priced: true,
        },
      ],
    });
    render(<TokenHeader load={ready(token())} />);

    // Item 11: SOL gets its own row whenever lamports > 0, even though there
    // are other tokens too.
    expect(await screen.findByText("SOL")).toBeTruthy();
    expect(screen.getByText("2.000000000")).toBeTruthy();
    // A USDC-quoted value is shown as a dollar figure.
    expect(screen.getByText("$301.00")).toBeTruthy();
  });

  it("shows a SOL-quoted value as SOL, never with a dollar sign", async () => {
    signIn();
    stubPositions({
      wallet: WALLET,
      slot: 11,
      read_at: 0,
      age_seconds: 1,
      sol: {
        lamports: 1_000_000_000,
        ui_amount: "1.000000000",
        price: null,
        quote: null,
        value: null,
        priced: false,
        price_reason: "no route to price SOL itself in this tape",
      },
      tokens: [
        {
          mint: MINT,
          program: "token",
          amount: "2000000",
          decimals: 6,
          ui_amount: "2",
          price: 0.25,
          quote: "SOL",
          value: 0.5,
          priced: true,
        },
      ],
    });
    render(<TokenHeader load={ready(token())} />);

    expect(await screen.findByText("0.5000 SOL")).toBeTruthy();
    // Unpriced SOL shows its own reason, not a silent absence.
    expect(screen.getByText("no route to price SOL itself in this tape")).toBeTruthy();
  });

  it("tells a session refusal apart from a chain read Radar could not complete", async () => {
    signIn();
    stubPositions({ error: "session expired", reason: "session_expired" }, 403);
    render(<TokenHeader load={ready(token())} />);
    expect(
      await screen.findByText(/sign in with your wallet again to see your holdings/i),
    ).toBeTruthy();

    vi.unstubAllGlobals();
    stubPositions({ error: "the chain could not be read", reason: "unreadable_chain" }, 502);
    render(<TokenHeader load={ready(token())} />);
    expect(
      await screen.findByText(/says nothing about what the wallet holds/i),
    ).toBeTruthy();
  });

  it("says Radar is rate-limiting reads on a 503 busy, not that the read failed outright", async () => {
    signIn();
    stubPositions({ error: "rate limited", reason: "busy" }, 503);
    render(<TokenHeader load={ready(token())} />);

    expect(await screen.findByText(/rate-limiting balance reads/i)).toBeTruthy();
  });

  it("keeps ticking the displayed age instead of freezing it at the server's own count (item 5)", async () => {
    signIn();
    vi.useFakeTimers();
    try {
      stubPositions({
        wallet: WALLET,
        slot: 1,
        read_at: 0,
        age_seconds: 10,
        sol: {
          lamports: 0,
          ui_amount: "0.000000000",
          price: null,
          quote: null,
          value: null,
          priced: false,
          price_reason: null,
        },
        tokens: [],
      });
      render(<TokenHeader load={ready(token())} />);

      // Flush the initial positions fetch without letting wall-clock time pass.
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(screen.getByText("as of 10s ago")).toBeTruthy();

      // 15s pass (three 5s ticks) with no new read. The label must grow past
      // the server's original count -- a label frozen at "10s ago" forever
      // would be the bug this test exists to catch.
      await act(async () => {
        await vi.advanceTimersByTimeAsync(15_000);
      });
      expect(screen.getByText("as of 25s ago")).toBeTruthy();
    } finally {
      vi.useRealTimers();
    }
  });
});
