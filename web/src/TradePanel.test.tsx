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
      if (u.includes("/v1/customer/tx/SIG123")) return jsonResponse({ state: "landed", slot: 1 });
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
    // The built response's own slippage tolerance, not whatever the live
    // fields say -- both default to 100 bps here, but the card must be
    // reading `review.response.quote.slippage_bps`, not the input field.
    expect(within(card).getByText("1.00%")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Approve in wallet" }));
    expect(await screen.findByText("Sent to your wallet.")).toBeTruthy();
    expect(screen.getByRole("link", { name: "View on Solscan" }).getAttribute("href")).toContain("SIG123");
    // `onTraded` fires once the poll below sees the trade landed -- not
    // merely because it was sent (see the dedicated failed/expired tests,
    // which show it is never called for those outcomes at all).
    expect(await screen.findByText("Landed")).toBeTruthy();
    expect(onTraded).toHaveBeenCalledTimes(1);
  });

  it("throws a built transaction away when the slippage changes, so it cannot be approved at the old one", async () => {
    vi.mocked(signAndSend).mockClear();
    signIn();
    mockFetch((url) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody());
      if (u.includes("/v1/customer/swap")) return jsonResponse(swapBody());
      return jsonResponse({ error: "unexpected" }, 404);
    });
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={vi.fn()} />);
    typeAmount("1");
    await buildReview();
    expect(await screen.findByRole("button", { name: "Approve in wallet" })).toBeTruthy();
    fireEvent.change(screen.getByDisplayValue("100"), { target: { value: "50" } });
    await waitFor(() => expect(screen.queryByRole("button", { name: "Approve in wallet" })).toBeNull());
    expect(screen.queryByRole("region", { name: "Trade review" })).toBeNull();
    expect(vi.mocked(signAndSend)).not.toHaveBeenCalled();
  });

  it("does not let a build that finishes after the slippage changed become approvable", async () => {
    // Regression test for a race: `startReview` used to always call
    // `setReview({kind: "built"})` once its `/v1/customer/swap` call
    // resolved, even if the slippage (or amount, or side, or mint) had since
    // changed underneath it -- so a transaction built for 500 bps could come
    // back and show "Approve in wallet" after the field said 50 bps.
    vi.mocked(signAndSend).mockClear();
    signIn();
    let resolveSwap: ((value: Response) => void) | undefined;
    const swapPromise = new Promise<Response>((resolve) => {
      resolveSwap = resolve;
    });
    mockFetch((url, init) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody());
      if (u.includes("/v1/customer/swap") && init?.method === "POST") return swapPromise;
      return jsonResponse({ error: "unexpected" }, 404);
    });
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={vi.fn()} />);
    typeAmount("1");
    fireEvent.change(screen.getByDisplayValue("100"), { target: { value: "500" } });
    await buildReview();
    await screen.findByText("Building the transaction…");
    // The slippage changes mid-build, away from the 500 bps the in-flight
    // `/v1/customer/swap` call was made for.
    fireEvent.change(screen.getByDisplayValue("500"), { target: { value: "50" } });
    await act(async () => {
      resolveSwap?.(jsonResponse(swapBody({ slippage_bps: 500 })));
    });
    await waitFor(() => expect(screen.queryByRole("button", { name: "Approve in wallet" })).toBeNull());
    expect(screen.queryByRole("region", { name: "Trade review" })).toBeNull();
    expect(vi.mocked(signAndSend)).not.toHaveBeenCalled();
  });

  it("does not let a build that finishes after the amount changed become approvable", async () => {
    vi.mocked(signAndSend).mockClear();
    signIn();
    let resolveSwap: ((value: Response) => void) | undefined;
    const swapPromise = new Promise<Response>((resolve) => {
      resolveSwap = resolve;
    });
    mockFetch((url, init) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody());
      if (u.includes("/v1/customer/swap") && init?.method === "POST") return swapPromise;
      return jsonResponse({ error: "unexpected" }, 404);
    });
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={vi.fn()} />);
    typeAmount("1");
    await buildReview();
    await screen.findByText("Building the transaction…");
    // The amount changes mid-build, away from the "1" the in-flight
    // `/v1/customer/swap` call was made for.
    typeAmount("2");
    await act(async () => {
      resolveSwap?.(jsonResponse(swapBody()));
    });
    await waitFor(() => expect(screen.queryByRole("button", { name: "Approve in wallet" })).toBeNull());
    expect(screen.queryByRole("region", { name: "Trade review" })).toBeNull();
    expect(vi.mocked(signAndSend)).not.toHaveBeenCalled();
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

describe("TradePanel: did the trade land?", () => {
  // Shared setup for every test below: sign in, build a review, approve it,
  // and stub `/v1/customer/tx/SIG123` with whatever `txStatus` returns.
  // Every test supplies its own `txStatus` handler and reads the resulting
  // landing text off the "Sent to your wallet." card.
  async function sendAndApprove(
    txStatus: (url: string, init?: RequestInit) => Response | Promise<Response>,
    onTraded: ReturnType<typeof vi.fn<() => void>> = vi.fn(),
  ) {
    signIn();
    mockFetch((url, init) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody());
      if (u.includes("/v1/customer/swap") && init?.method === "POST") return jsonResponse(swapBody());
      if (u.includes("/v1/customer/tx/")) return txStatus(u, init);
      return jsonResponse({ error: "unexpected" }, 404);
    });
    vi.mocked(signAndSend).mockResolvedValue({ ok: true, signature: "SIG123" });
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={onTraded} />);
    typeAmount("1");
    await buildReview();
    fireEvent.click(await screen.findByRole("button", { name: "Approve in wallet" }));
    expect(await screen.findByText("Sent to your wallet.")).toBeTruthy();
    return onTraded;
  }

  it("shows the on-chain reason when the trade failed, and never refreshes positions", async () => {
    const onTraded = await sendAndApprove(() => jsonResponse({ state: "failed", reason: "slippage exceeded" }));
    expect(await screen.findByText("Failed on chain: slippage exceeded")).toBeTruthy();
    expect(onTraded).not.toHaveBeenCalled();
    // The Solscan link stays up in every state, including this one.
    expect(screen.getByRole("link", { name: "View on Solscan" })).toBeTruthy();
  });

  it("shows expired only when the server itself says so, and never refreshes positions", async () => {
    const onTraded = await sendAndApprove(() => jsonResponse({ state: "expired" }));
    expect(await screen.findByText("Expired -- nothing was spent")).toBeTruthy();
    expect(onTraded).not.toHaveBeenCalled();
    expect(screen.getByRole("link", { name: "View on Solscan" })).toBeTruthy();
  });

  it("ends the poll immediately on a 400/401 -- a request or session problem that asking again cannot fix", async () => {
    const onTraded = await sendAndApprove(() =>
      jsonResponse({ error: "bad signature", reason: "bad_request" }, 400),
    );
    expect(await screen.findByText("Unknown -- check Solscan")).toBeTruthy();
    expect(screen.queryByText("Expired -- nothing was spent")).toBeNull();
    expect(onTraded).not.toHaveBeenCalled();
    expect(screen.getByRole("link", { name: "View on Solscan" })).toBeTruthy();
  });

  it("keeps polling through a rate-limited (busy) read rather than giving up on the first one", async () => {
    let calls = 0;
    const onTraded = await sendAndApprove(() => {
      calls += 1;
      return jsonResponse({ error: "rate limited", reason: "busy" }, 503);
    });
    // Still polling, never settled to "unknown", after the first transient
    // refusal -- a `busy` read is not evidence the trade cannot be checked,
    // only that this one attempt was refused. The next attempt is a full
    // `POLL_INTERVAL_MS` (1.5s) later, so this needs a longer-than-default
    // wait.
    await waitFor(() => expect(calls).toBeGreaterThan(1), { timeout: 4_000 });
    expect(screen.queryByText("Unknown -- check Solscan")).toBeNull();
    expect(onTraded).not.toHaveBeenCalled();
  }, 10_000);

  it("recovers from a transient chain_unreadable read and still shows Landed", async () => {
    let calls = 0;
    const onTraded = await sendAndApprove(() => {
      calls += 1;
      if (calls === 1) {
        return jsonResponse({ error: "could not read the chain", reason: "chain_unreadable" }, 502);
      }
      return jsonResponse({ state: "landed", slot: 1 });
    });
    // The retry after the transient error is a full `POLL_INTERVAL_MS`
    // (1.5s) later, so this needs a longer-than-default wait too.
    expect(await screen.findByText("Landed", {}, { timeout: 4_000 })).toBeTruthy();
    expect(onTraded).toHaveBeenCalledTimes(1);
  }, 10_000);

  it("shows unknown, never expired, once the ~90s cap is reached with no answer", async () => {
    signIn();
    // The clock jumps forward *inside* the mocked read, past the cap, before
    // the poll's own elapsed-time check runs -- so this needs only the one
    // read the effect makes right after "sent", not 60 real 1.5s intervals.
    let clock = 1_700_000_000_000;
    vi.spyOn(Date, "now").mockImplementation(() => clock);
    const onTraded = vi.fn();
    mockFetch((url, init) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody());
      if (u.includes("/v1/customer/swap") && init?.method === "POST") return jsonResponse(swapBody());
      if (u.includes("/v1/customer/tx/")) {
        clock += 91_000;
        return jsonResponse({ state: "pending" });
      }
      return jsonResponse({ error: "unexpected" }, 404);
    });
    vi.mocked(signAndSend).mockResolvedValue({ ok: true, signature: "SIG123" });
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={onTraded} />);
    typeAmount("1");
    await buildReview();
    fireEvent.click(await screen.findByRole("button", { name: "Approve in wallet" }));
    expect(await screen.findByText("Unknown -- check Solscan")).toBeTruthy();
    expect(screen.queryByText("Expired -- nothing was spent")).toBeNull();
    expect(onTraded).not.toHaveBeenCalled();
  });

  it("stops polling once the component unmounts", async () => {
    let calls = 0;
    signIn();
    mockFetch((url, init) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody());
      if (u.includes("/v1/customer/swap") && init?.method === "POST") return jsonResponse(swapBody());
      if (u.includes("/v1/customer/tx/")) {
        calls += 1;
        return jsonResponse({ state: "pending" });
      }
      return jsonResponse({ error: "unexpected" }, 404);
    });
    vi.mocked(signAndSend).mockResolvedValue({ ok: true, signature: "SIG123" });
    const { unmount } = render(
      <TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={vi.fn()} />,
    );
    typeAmount("1");
    await buildReview();
    fireEvent.click(await screen.findByRole("button", { name: "Approve in wallet" }));
    await screen.findByText("Sent to your wallet.");
    await waitFor(() => expect(calls).toBeGreaterThan(0));
    const callsAtUnmount = calls;
    unmount();
    await new Promise((resolve) => setTimeout(resolve, 1600));
    expect(calls).toBe(callsAtUnmount);
  });
});

describe("TradePanel: rebuilding a stale trade before it is sent", () => {
  it("rebuilds instead of sending a build older than 60s, and shows the fresh numbers", async () => {
    vi.mocked(signAndSend).mockClear();
    signIn();
    let clock = 1_700_000_000_000;
    vi.spyOn(Date, "now").mockImplementation(() => clock);
    let swapCalls = 0;
    mockFetch((url, init) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody());
      if (u.includes("/v1/customer/swap") && init?.method === "POST") {
        swapCalls += 1;
        // The rebuild returns different numbers than the original build, so
        // the test can tell the fresh response is what is shown.
        return jsonResponse(swapBody({ worst_out: swapCalls === 1 ? "4900000" : "4111111" }));
      }
      return jsonResponse({ error: "unexpected" }, 404);
    });
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={vi.fn()} />);
    typeAmount("1");
    await buildReview();
    const approveButton = await screen.findByRole("button", { name: "Approve in wallet" });
    expect(swapCalls).toBe(1);
    // The build goes stale (61s pass) without the panel's own 1s ticker
    // having re-rendered yet -- the button on screen still reads "Approve in
    // wallet". Clicking it now must rebuild rather than send the stale
    // transaction.
    clock += 61_000;
    fireEvent.click(approveButton);
    await waitFor(() => expect(swapCalls).toBe(2));
    expect(await screen.findByText("Review before you approve")).toBeTruthy();
    const card = screen.getByRole("region", { name: "Trade review" });
    expect(within(card).getByText("4.111111")).toBeTruthy();
    expect(vi.mocked(signAndSend)).not.toHaveBeenCalled();
  });

  it("sends a fresh build directly, with no rebuild", async () => {
    vi.mocked(signAndSend).mockClear();
    signIn();
    let swapCalls = 0;
    mockFetch((url, init) => {
      const u = String(url);
      if (u.includes("/v1/market/quote")) return jsonResponse(quoteBody());
      if (u.includes("/v1/customer/swap") && init?.method === "POST") {
        swapCalls += 1;
        return jsonResponse(swapBody());
      }
      if (u.includes("/v1/customer/tx/")) return jsonResponse({ state: "pending" });
      return jsonResponse({ error: "unexpected" }, 404);
    });
    vi.mocked(signAndSend).mockResolvedValue({ ok: true, signature: "SIG123" });
    render(<TradePanel mint={MINT} symbol="FOO" positions={positionsReady()} onTraded={vi.fn()} />);
    typeAmount("1");
    await buildReview();
    fireEvent.click(await screen.findByRole("button", { name: "Approve in wallet" }));
    expect(await screen.findByText("Sent to your wallet.")).toBeTruthy();
    expect(swapCalls).toBe(1);
    expect(vi.mocked(signAndSend)).toHaveBeenCalledTimes(1);
  });
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
