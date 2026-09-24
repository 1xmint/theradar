// SPDX-License-Identifier: Apache-2.0
//! Tests for `TradePanel.tsx`: the buy/sell panel itself, rendered directly
//! with a `positions` prop rather than through `TokenHeader`. Whether the
//! panel mounts at all -- `legal.ts`'s `TERMS_APPROVED` and
//! `useHealth.ts`'s `useTrading()` -- is `TokenHeader.tsx`'s job, per this
//! component's own doc comment ("This file does not re-check them"), so it
//! is not re-tested here. What is covered here is everything the panel does
//! once mounted: amount and slippage validation, the debounced live quote,
//! the review step built from `/v1/customer/swap`'s own response (never the
//! live quote), 60-second staleness, a wallet decline, every documented
//! refusal code, and the post-send notice.
//!
//! `./sign` is mocked wholesale rather than fed a real base64
//! `VersionedTransaction`: `signAndSend`'s own byte-level behaviour is
//! `sign.test.ts`'s job, and this file only needs to control whether it
//! resolves, declines, or fails.

import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Positions, PositionsToken, Quote, SwapResponse } from "./api";
import type { PositionsLoad } from "./usePositions";
import { TradePanel } from "./TradePanel";

vi.mock("./sign", () => ({ signAndSend: vi.fn() }));
import { signAndSend } from "./sign";

const MINT = "MintAAA1111111111111111111111111111111111";

function signIn() {
  localStorage.setItem(
    "radar.wallet.session",
    JSON.stringify({ token: "tok-1", address: "WalletAddr111111111111111111111111111111", expiresInSeconds: 3600 }),
  );
  // A signed-in visitor has a wallet extension; without one on `window`,
  // `detect()` finds nothing and approving stops at "No wallet extension
  // found" before `signAndSend` (mocked above) is ever reached.
  vi.stubGlobal("solana", {});
}

function jsonResponse(body: unknown, status = 200): Response {
  return { ok: status >= 200 && status < 300, status, json: async () => body } as Response;
}

function mockFetch(handler: (url: string, init?: RequestInit) => Response | Promise<Response>) {
  vi.stubGlobal("fetch", vi.fn(async (url: string, init?: RequestInit) => handler(url, init)));
}

function quoteBody(overrides: Partial<Quote> = {}): Quote {
  return {
    mint: MINT,
    side: "buy",
    in_mint: "So11111111111111111111111111111111111111112",
    out_mint: MINT,
    in_amount: "1000000000",
    out_amount: "5000000",
    worst_out: "4900000",
    in_decimals: 9,
    out_decimals: 6,
    slippage_bps: 100,
    impact_bps: 50,
    venues: ["Jupiter"],
    quoted_at: 1_700_000_000,
    ...overrides,
  };
}

function swapBody(quoteOverrides: Partial<Quote> = {}): SwapResponse {
  return {
    transaction: "ZmFrZQ==",
    last_valid_block_height: 123,
    quote: quoteBody(quoteOverrides),
  };
}

function heldToken(overrides: Partial<PositionsToken> = {}): PositionsToken {
  return {
    mint: MINT,
    program: "token",
    amount: "5000000",
    decimals: 6,
    ui_amount: "5",
    price: null,
    quote: null,
    value: null,
    priced: false,
    ...overrides,
  };
}

function positionsReady(tokens: PositionsToken[] = []): PositionsLoad {
  const positions: Positions = {
    wallet: "WalletAddr111111111111111111111111111111",
    slot: 1,
    read_at: 1_700_000_000,
    age_seconds: 0,
    sol: {
      lamports: 0,
      ui_amount: "0",
      price: null,
      quote: null,
      value: null,
      priced: false,
      price_reason: "no wSOL trade found",
    },
    tokens,
  };
  return { state: "ready", value: positions };
}

function typeAmount(value: string) {
  fireEvent.change(screen.getByPlaceholderText("0.0"), { target: { value } });
}

async function buildReview() {
  const reviewButton = (await screen.findByRole("button", {
    name: "Review trade",
  })) as HTMLButtonElement;
  await waitFor(() => expect(reviewButton.disabled).toBe(false));
  fireEvent.click(reviewButton);
}

afterEach(() => {
  localStorage.clear();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("TradePanel amount and slippage validation", () => {
  it("shows an amount error for non-numeric input", async () => {
    mockFetch(() => jsonResponse({ error: "not found" }, 404));
    render(<TradePanel mint={MINT} symbol="FOO" positions={{ state: "signed-out" }} onTraded={vi.fn()} />);
    typeAmount("abc");
    expect(await screen.findByText("That is not a number.")).toBeTruthy();
  });

  it("shows a slippage error for zero", async () => {
    mockFetch(() => jsonResponse({ error: "not found" }, 404));
    render(<TradePanel mint={MINT} symbol="FOO" positions={{ state: "signed-out" }} onTraded={vi.fn()} />);
    fireEvent.change(screen.getByDisplayValue("100"), { target: { value: "0" } });
    expect(await screen.findByText("Enter a slippage tolerance greater than zero.")).toBeTruthy();
  });

  it("rejects a slippage above the 500 bps cap, never silently widening it", async () => {
    mockFetch(() => jsonResponse({ error: "not found" }, 404));
    render(<TradePanel mint={MINT} symbol="FOO" positions={{ state: "signed-out" }} onTraded={vi.fn()} />);
    fireEvent.change(screen.getByDisplayValue("100"), { target: { value: "600" } });
    expect(
      await screen.findByText("That slippage tolerance is wider than Radar allows (max 5%)."),
    ).toBeTruthy();
  });
});

describe("TradePanel sell side", () => {
  it("tells a signed-out visitor to connect a wallet to sell", async () => {
    mockFetch(() => jsonResponse({ error: "not found" }, 404));
    render(<TradePanel mint={MINT} symbol="FOO" positions={{ state: "signed-out" }} onTraded={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "sell" }));
    expect(
      await screen.findByText(
        "Connect a wallet to sell. Radar can only sell from a wallet it can see the balance of.",
      ),
    ).toBeTruthy();
  });

  it("says it is still reading holdings while positions load", async () => {
    signIn();
    mockFetch(() => jsonResponse({ error: "not found" }, 404));
    render(<TradePanel mint={MINT} symbol="FOO" positions={{ state: "loading" }} onTraded={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "sell" }));
    expect(
      await screen.findByText("Reading this wallet's holdings before it can be sold from."),
    ).toBeTruthy();
  });

  it("says the wallet does not hold this token when positions has no match", async () => {
    signIn();
    mockFetch(() => jsonResponse({ error: "not found" }, 404));
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady([])} onTraded={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "sell" }));
    expect(
      await screen.findByText("This wallet does not hold FOO, so there is nothing to sell."),
    ).toBeTruthy();
  });

  it("rejects a sell amount over the wallet's own holding", async () => {
    signIn();
    mockFetch(() => jsonResponse({ error: "not found" }, 404));
    render(
      <TradePanel
        mint={MINT}
        symbol="FOO"
        positions={positionsReady([heldToken({ amount: "1000000", decimals: 6 })])}
        onTraded={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "sell" }));
    typeAmount("5");
    expect(await screen.findByText("You do not have that much FOO.")).toBeTruthy();
  });
});

describe("TradePanel live quote", () => {
  it("shows pay, receive, worst case, impact, venues and the round-trip caption", async () => {
    mockFetch((url) => {
      if (String(url).includes("/v1/market/quote")) return jsonResponse(quoteBody());
      return jsonResponse({ error: "unexpected" }, 404);
    });
    render(<TradePanel mint={MINT} symbol="FOO" positions={{ state: "signed-out" }} onTraded={vi.fn()} />);
    typeAmount("1");
    expect(await screen.findByText("0.50%")).toBeTruthy();
    expect(screen.getByText("Jupiter")).toBeTruthy();
    expect(screen.getByText("4.9")).toBeTruthy();
    expect(
      screen.getByText(
        "Buying and selling straight back would cost about 1.0% in price impact alone, before Solana's own network fee.",
      ),
    ).toBeTruthy();
  });

  it("warns rather than silently accepting a quote priced at a different slippage than requested", async () => {
    mockFetch((url) => {
      if (String(url).includes("/v1/market/quote")) return jsonResponse(quoteBody({ slippage_bps: 150 }));
      return jsonResponse({ error: "unexpected" }, 404);
    });
    render(<TradePanel mint={MINT} symbol="FOO" positions={{ state: "signed-out" }} onTraded={vi.fn()} />);
    typeAmount("1");
    expect(
      await screen.findByText(
        "Note: this quote reflects a 1.50% slippage tolerance, not the 1.00% set above.",
      ),
    ).toBeTruthy();
  });

  it("shows a refusal message when the live quote itself fails", async () => {
    mockFetch(() => jsonResponse({ error: "No route", reason: "no_route" }, 502));
    render(<TradePanel mint={MINT} symbol="FOO" positions={{ state: "signed-out" }} onTraded={vi.fn()} />);
    typeAmount("1");
    expect(
      await screen.findByText(
        "No route exists for this trade right now. Try a smaller amount or a different token.",
      ),
    ).toBeTruthy();
  });
});

describe("TradePanel without a connected wallet", () => {
  it("prompts to connect a wallet and disables review", async () => {
    mockFetch(() => jsonResponse(quoteBody()));
    render(<TradePanel mint={MINT} symbol="FOO" positions={{ state: "signed-out" }} onTraded={vi.fn()} />);
    typeAmount("1");
    expect(await screen.findByText("Connect a wallet to trade.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Review trade" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("TradePanel review and send flow", () => {
  it("reviews from the swap response's own quote, not the live one, and completes a send", async () => {
    signIn();
    const onTraded = vi.fn();
    mockFetch((url, init) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody({ worst_out: "4900000" }));
      if (u.includes("/v1/customer/swap") && init?.method === "POST") {
        return jsonResponse(swapBody({ worst_out: "4750000" }));
      }
      return jsonResponse({ error: "unexpected" }, 404);
    });
    vi.mocked(signAndSend).mockResolvedValue({ ok: true, signature: "SIG123" });
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={onTraded} />);
    typeAmount("1");
    await buildReview();
    expect(await screen.findByText("Review before you approve")).toBeTruthy();
    // The live quote's worst case (4900000 -> "4.9") never appears in the
    // review card; only the swap response's own quote (4750000 -> "4.75")
    // does -- the review step must show what the transaction actually
    // contains, not whatever the debounced live quote has ticked to since.
    const card = screen.getByRole("region", { name: "Trade review" });
    expect(within(card).getByText("4.75")).toBeTruthy();
    expect(within(card).queryByText("4.9")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Approve in wallet" }));
    expect(await screen.findByText("Sent to your wallet.")).toBeTruthy();
    expect(screen.getByRole("link", { name: "View on Solscan" }).getAttribute("href")).toContain("SIG123");
    expect(onTraded).toHaveBeenCalledTimes(1);
  });

  it("treats a wallet decline as cancelled, not an error, and keeps the built transaction", async () => {
    signIn();
    const onTraded = vi.fn();
    mockFetch((url) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody());
      if (u.includes("/v1/customer/swap")) return jsonResponse(swapBody());
      return jsonResponse({ error: "unexpected" }, 404);
    });
    vi.mocked(signAndSend).mockResolvedValue({ ok: false, error: { kind: "declined" } });
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={onTraded} />);
    typeAmount("1");
    await buildReview();
    fireEvent.click(await screen.findByRole("button", { name: "Approve in wallet" }));
    expect(await screen.findByText("Cancelled. Nothing was sent.")).toBeTruthy();
    // Still offers to approve the exact same built transaction, rather than
    // forcing a rebuild -- closing the wallet popup is not evidence the
    // transaction went stale.
    expect(screen.getByRole("button", { name: "Approve in wallet" })).toBeTruthy();
    expect(onTraded).not.toHaveBeenCalled();
  });

  it("shows a plain failure message when the wallet extension cannot be reached", async () => {
    signIn();
    mockFetch((url) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody());
      if (u.includes("/v1/customer/swap")) return jsonResponse(swapBody());
      return jsonResponse({ error: "unexpected" }, 404);
    });
    vi.mocked(signAndSend).mockResolvedValue({
      ok: false,
      error: { kind: "failed", detail: "the transaction could not be parsed" },
    });
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={vi.fn()} />);
    typeAmount("1");
    await buildReview();
    fireEvent.click(await screen.findByRole("button", { name: "Approve in wallet" }));
    expect(await screen.findByText("the transaction could not be parsed")).toBeTruthy();
  });

  it("marks a built transaction stale after 60 seconds and offers a rebuild instead of approve", async () => {
    signIn();
    let clock = 1_700_000_000_000;
    vi.spyOn(Date, "now").mockImplementation(() => clock);
    mockFetch((url) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody());
      if (u.includes("/v1/customer/swap")) return jsonResponse(swapBody());
      return jsonResponse({ error: "unexpected" }, 404);
    });
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={vi.fn()} />);
    typeAmount("1");
    await buildReview();
    await screen.findByRole("button", { name: "Approve in wallet" });
    // The clock jumps 61s ahead; the panel's own 1s ticker (a real interval,
    // not faked) is what notices on its next real-time tick.
    clock += 61_000;
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 1100));
    });
    expect(
      await screen.findByText("This quote is more than a minute old. Rebuild it before approving."),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Rebuild" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Approve in wallet" })).toBeNull();
  }, 10_000);
});

describe("TradePanel swap refusal messages", () => {
  it.each([
    ["busy", "", "Radar is rate-limiting trade requests; try again shortly."],
    ["trading_off", "", "Trading is turned off right now."],
    [
      "no_route",
      "",
      "No route exists for this trade right now. Try a smaller amount or a different token.",
    ],
    [
      "unreadable_route",
      "",
      "Radar could not read a usable route for this trade. This says nothing about the token -- Radar could not look.",
    ],
    ["slippage_too_wide", "", "That slippage tolerance is wider than Radar allows."],
    ["unscoped", "", "Radar could not tell which wallet this request was for."],
    ["bad_request", "amount must be positive", "Radar rejected that request: amount must be positive"],
    [
      "session_expired",
      "",
      "Your wallet session is no longer valid. Sign in with your wallet again.",
    ],
    ["something_new_the_client_has_never_seen", "an unfamiliar detail sentence", "an unfamiliar detail sentence"],
  ])("shows the right sentence for reason %s", async (reason, detail, expected) => {
    signIn();
    mockFetch((url) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody());
      if (u.includes("/v1/customer/swap")) {
        return jsonResponse({ error: detail || "refused", reason }, 503);
      }
      return jsonResponse({ error: "unexpected" }, 404);
    });
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={vi.fn()} />);
    typeAmount("1");
    await buildReview();
    expect(await screen.findByText(expected)).toBeTruthy();
  });
});
