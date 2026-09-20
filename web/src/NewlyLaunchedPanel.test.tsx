// SPDX-License-Identifier: Apache-2.0
//! `NewlyLaunchedPanel` reads `/v1/market/launches` -- coins Radar's own
//! collector recorded, not every launch on Solana -- and rule 9 requires an
//! empty index to read differently from a snapshot Radar could not read.

import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { NewlyLaunchedPanel } from "./NewlyLaunchedPanel";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

const MINT_A = "5NfV2sy8DqXamLvYEE4LcTWzGqZc5Emv4bqqhVDWpump";
const MINT_B = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";

describe("NewlyLaunchedPanel", () => {
  it("lists the recorded launches the server sent, newest slot first as given", () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        jsonResponse({
          launches: [
            { mint: MINT_A, name: "Ay", symbol: "AY", uri: "ipfs://ay", slot: 500 },
            { mint: MINT_B, name: "Bee", symbol: "BEE", uri: "ipfs://bee", slot: 400 },
          ],
          window: { from_slot: 100, to_slot: 500 },
          complete: false,
        }),
      ),
    );
    render(<NewlyLaunchedPanel />);

    expect(await screen.findByText("AY")).toBeTruthy();
    expect(screen.getByText("BEE")).toBeTruthy();
  });

  it("states plainly that this is only what Radar recorded, not every launch", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        jsonResponse({
          launches: [{ mint: MINT_A, name: "Ay", symbol: "AY", uri: "ipfs://ay", slot: 500 }],
          window: { from_slot: 100, to_slot: 500 },
          complete: false,
        }),
      ),
    );
    render(<NewlyLaunchedPanel />);

    const caption = await screen.findByText(/coins radar recorded the launch of/i);
    expect(caption.textContent?.toLowerCase()).toContain("not every coin");
  });

  it("reads a quiet launch window differently from a snapshot Radar could not build", async () => {
    // Both are a 503 `not_collected` with an empty body otherwise -- only the
    // server's `message` differs, and a panel that ignored it (or always
    // showed one sentence) would pass this with either fixture but not both.
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        jsonResponse(
          { error: "not_collected", message: "Radar has recorded no launches in its window" },
          503,
        ),
      ),
    );
    render(<NewlyLaunchedPanel />);

    const quiet = await screen.findByText(/not recorded a launch in its window/i);
    expect(quiet.textContent?.toLowerCase()).not.toContain("could not look");
  });

  it("reads a snapshot Radar could not build differently from a quiet window", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        jsonResponse(
          {
            error: "not_collected",
            message: "the market snapshot has not been built yet; check back shortly",
          },
          503,
        ),
      ),
    );
    render(<NewlyLaunchedPanel />);

    const unreachable = await screen.findByText(/could not look/i);
    expect(unreachable.textContent?.toLowerCase()).not.toContain("not recorded a launch");
  });
});
