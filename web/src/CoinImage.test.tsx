// SPDX-License-Identifier: Apache-2.0
//! `CoinImage` reads a creator-supplied, untrusted `uri` -- AGENTS.md rule 4 --
//! so every one of these tests is about what it refuses, not what it renders
//! on the happy path: a non-https image is dropped, a broken fetch degrades to
//! a placeholder, and a row never goes blank because a JSON document three
//! hops away was malformed.

import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { CoinImage } from "./CoinImage";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

describe("CoinImage", () => {
  it("renders the fetched image once the metadata document resolves", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse({ image: "https://cdn.example.test/coin.png" })),
    );
    render(<CoinImage uri="https://example.test/meta-1.json" symbol="RADAR" />);

    const img = await screen.findByRole("img");
    expect(img.getAttribute("src")).toBe("https://cdn.example.test/coin.png");
    expect(img.getAttribute("loading")).toBe("lazy");
    expect(img.getAttribute("referrerpolicy")).toBe("no-referrer");
  });

  it("rewrites an ipfs:// image to an https gateway URL", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse({ image: "ipfs://bafybeigdyrz/coin.png" })),
    );
    render(<CoinImage uri="https://example.test/meta-2.json" symbol="RADAR" />);

    const img = await screen.findByRole("img");
    expect(img.getAttribute("src")).toBe("https://ipfs.io/ipfs/bafybeigdyrz/coin.png");
  });

  it("refuses a non-https image URL and shows the placeholder instead", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse({ image: "javascript:alert(1)" })),
    );
    render(<CoinImage uri="https://example.test/meta-3.json" symbol="RADAR" />);

    await waitFor(() => {
      expect(screen.queryByRole("img")).toBeNull();
    });
    expect(screen.getByText("R")).toBeTruthy();
  });

  it("shows the symbol's first letter as a placeholder when the fetch fails", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new Error("network down");
      }),
    );
    render(<CoinImage uri="https://example.test/meta-4.json" symbol="radar" />);

    await waitFor(() => {
      expect(screen.getByText("R")).toBeTruthy();
    });
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("shows a neutral dot when there is no symbol and no uri at all", () => {
    render(<CoinImage uri={null} symbol={null} />);
    expect(screen.getByText("•")).toBeTruthy();
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("falls back to the placeholder when the loaded image itself fails to render", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse({ image: "https://cdn.example.test/coin.png" })),
    );
    render(<CoinImage uri="https://example.test/meta-5.json" symbol="RADAR" />);

    const img = await screen.findByRole("img");
    img.dispatchEvent(new Event("error"));

    await waitFor(() => {
      expect(screen.queryByRole("img")).toBeNull();
    });
    expect(screen.getByText("R")).toBeTruthy();
  });
});
