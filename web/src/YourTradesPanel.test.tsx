// SPDX-License-Identifier: Apache-2.0
//! `YourTradesPanel` has five ways to be empty and rule 9 says they must not
//! read alike: not signed in, none of yours, this coin has no recorded trades
//! at all, Radar could not look, and Radar could not be reached. Four of the
//! five are a fact about Radar rather than about the reader's trading, and a
//! reader who cannot tell them apart will believe the one that is wrong.

import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { YourTradesPanel } from "./YourTradesPanel";

const MINT = "5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump";
const WALLET = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";
const CAVEAT = "Only trades Radar recorded, and only those it can tie to this wallet.";

afterEach(() => {
  localStorage.clear();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

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

function answer(trades: unknown[], unattributable = 0, truncated = false): Response {
  return jsonResponse({
    mint: MINT,
    wallet: WALLET,
    fold: {
      fact: "wallet_trades_in_window",
      complete: false,
      truncated,
      unattributable_trades: unattributable,
      trades,
    },
    caveat: CAVEAT,
  });
}

function aTrade(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    ts: "2026-09-18 12:30:00",
    slot: 500,
    signature: "3fT9zzzz",
    side: "sell",
    token_amount: 1200,
    quote_amount: 3,
    price: 0.0025,
    matched_by: "trader",
    ...over,
  };
}

/** Nothing is fetched when nobody is signed in, so the stub proves it too. */
function refuseToFetch(): ReturnType<typeof vi.fn> {
  const fetcher = vi.fn(async () => {
    throw new Error("the panel must not ask without a wallet");
  });
  vi.stubGlobal("fetch", fetcher);
  return fetcher;
}

describe("YourTradesPanel", () => {
  it("says it has not been told which wallet is the reader's, rather than showing nothing", async () => {
    const fetcher = refuseToFetch();
    render(<YourTradesPanel mint={MINT} />);

    expect(await screen.findByText(/has not been told which wallet is yours/i)).toBeTruthy();
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("lists the reader's own trades and says which were named and which derived", async () => {
    signIn();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        answer([
          aTrade(),
          aTrade({ signature: "4gU0yyyy", side: "buy", matched_by: "receiving_account" }),
        ]),
      ),
    );
    render(<YourTradesPanel mint={MINT} />);

    expect(await screen.findByText(/named/i)).toBeTruthy();
    expect(screen.getByText(/derived/i)).toBeTruthy();
    // The server's own sentence about what the list cannot contain, rendered
    // rather than summarised.
    expect(screen.getByText(new RegExp(CAVEAT.slice(0, 30), "i"))).toBeTruthy();
  });

  it("links a trade to the explorer's transaction page, not its account page", async () => {
    signIn();
    vi.stubGlobal("fetch", vi.fn(async () => answer([aTrade()])));
    render(<YourTradesPanel mint={MINT} />);

    const link = await screen.findByRole("link");
    // A signature handed to `/account/` renders as "not found", which reads as
    // though the trade never happened.
    expect(link.getAttribute("href")).toBe("https://solscan.io/tx/3fT9zzzz");
  });

  it("says plainly that none are the reader's when the tape attributed every trade", async () => {
    signIn();
    vi.stubGlobal("fetch", vi.fn(async () => answer([], 0)));
    render(<YourTradesPanel mint={MINT} />);

    expect(await screen.findByText(/none of the trades radar recorded for this coin are yours/i))
      .toBeTruthy();
  });

  it("does not claim the reader made no trades when some trades name nobody", async () => {
    signIn();
    vi.stubGlobal("fetch", vi.fn(async () => answer([], 4)));
    render(<YourTradesPanel mint={MINT} />);

    // The load-bearing difference: zero rows with unattributable trades is
    // "Radar cannot tell", not "you did not trade this coin".
    const said = await screen.findByText(/4 trades of this coin name nobody at all/i);
    expect(said.textContent).toMatch(/any of those could be yours/i);
    expect(said.textContent).not.toMatch(/^None of the trades Radar recorded for this coin are yours\.$/);
  });

  it("counts a single unattributable trade as one trade, not 1 trades", async () => {
    signIn();
    vi.stubGlobal("fetch", vi.fn(async () => answer([], 1)));
    render(<YourTradesPanel mint={MINT} />);

    expect(await screen.findByText(/one trade of this coin name nobody at all/i)).toBeTruthy();
  });

  it("warns beside a full list that unlisted trades of this coin could also be the reader's", async () => {
    signIn();
    vi.stubGlobal("fetch", vi.fn(async () => answer([aTrade()], 2)));
    render(<YourTradesPanel mint={MINT} />);

    expect(await screen.findByText(/2 trades of this coin name nobody, so any of them could be yours/i))
      .toBeTruthy();
  });

  it("separates 'this coin has no recorded trades' from 'Radar could not look'", async () => {
    signIn();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        jsonResponse(
          {
            error: "not_collected",
            message: "Radar has recorded no trades of this coin in its window",
          },
          503,
        ),
      ),
    );
    render(<YourTradesPanel mint={MINT} />);

    const said = await screen.findByText(/recorded no trades of this coin at all/i);
    expect(said.textContent).toMatch(/nothing of yours to show either/i);
    expect(said.textContent).not.toMatch(/could not look/i);
  });

  it("says Radar could not look when the collector has not run, and that this is not an answer", async () => {
    signIn();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        jsonResponse(
          {
            error: "not_collected",
            message: "the market-tape collector has not produced anything for this store yet",
          },
          503,
        ),
      ),
    );
    render(<YourTradesPanel mint={MINT} />);

    const said = await screen.findByText(/radar could not look/i);
    expect(said.textContent).toMatch(/says nothing about whether you have traded this coin/i);
  });

  it("calls a broken connection a connection problem, not a verdict on the reader's trades", async () => {
    signIn();
    vi.stubGlobal("fetch", vi.fn(async () => { throw new TypeError("network down"); }));
    render(<YourTradesPanel mint={MINT} />);

    const said = await screen.findByText(/could not reach radar/i);
    expect(said.textContent).toMatch(/this is a connection problem, not an answer/i);
  });

  it("asks the wallet-scoped route for the selected coin", async () => {
    signIn();
    const fetcher = vi.fn(async (_url: string) => answer([]));
    vi.stubGlobal("fetch", fetcher);
    render(<YourTradesPanel mint={MINT} />);
    await screen.findByText(/none of the trades/i);

    const url = String(fetcher.mock.calls[0]?.[0]);
    expect(url).toContain(`/v1/market/history/${MINT}`);
    expect(url).toContain(`wallet=${WALLET}`);
  });
});
