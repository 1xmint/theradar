// SPDX-License-Identifier: Apache-2.0
//! The chart's "Log" switch: it flips `aria-pressed`, sets the right price
//! scale to the logarithmic mode (and back), and a chart mounted afterwards --
//! a new coin or interval -- opens in the saved mode. `lightweight-charts`
//! draws to a canvas jsdom does not have, so it is replaced by a recorder.

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { PriceScaleMode } from "lightweight-charts";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const applyOptions = vi.fn();
const priceScale = vi.fn(() => ({ applyOptions }));

vi.mock("lightweight-charts", async (importActual) => {
  const actual = await importActual<typeof import("lightweight-charts")>();
  const series = () => ({
    applyOptions: vi.fn(),
    setData: vi.fn(),
    priceScale: () => ({ applyOptions: vi.fn() }),
    coordinateToPrice: vi.fn(),
  });
  return {
    ...actual,
    createChart: () => ({
      addSeries: series,
      priceScale,
      applyOptions: vi.fn(),
      subscribeCrosshairMove: vi.fn(),
      subscribeClick: vi.fn(),
      timeScale: () => ({ fitContent: vi.fn() }),
      remove: vi.fn(),
    }),
  };
});

vi.mock("./api", async (importActual) => {
  const actual = await importActual<typeof import("./api")>();
  return {
    ...actual,
    market: {
      ...actual.market,
      candles: vi.fn(async () => ({
        mint: "M",
        interval: "15m",
        covered: { from: "2026-09-01 00:00:00", to: "2026-09-01 01:00:00", complete: true },
        requested: { from: "2026-09-01 00:00:00", to: "2026-09-01 01:00:00" },
        quote_mint: null,
        candles: [
          {
            time: 1_000,
            bucket_start: "2026-09-01 00:00:00",
            open: 8e-7,
            high: 8e-7,
            low: 1e-11,
            close: 1e-11,
            quote_volume: 1,
            token_volume: 1,
            trade_count: 1,
          },
        ],
      })),
    },
  };
});

import { CandleChart } from "./CandleChart";

class FakeResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

beforeEach(() => {
  globalThis.ResizeObserver = FakeResizeObserver;
  applyOptions.mockClear();
  priceScale.mockClear();
});

afterEach(() => {
  localStorage.clear();
});

async function logButton(): Promise<HTMLElement> {
  return screen.findByRole("button", { name: "Log" });
}

describe("the chart's Log switch", () => {
  it("starts linear, goes logarithmic on click and back on the second click", async () => {
    render(<CandleChart mint="M" />);
    const button = await logButton();
    expect(button.getAttribute("aria-pressed")).toBe("false");
    expect(applyOptions).toHaveBeenLastCalledWith({ mode: PriceScaleMode.Normal });

    fireEvent.click(button);
    expect(button.getAttribute("aria-pressed")).toBe("true");
    expect(priceScale).toHaveBeenLastCalledWith("right");
    expect(applyOptions).toHaveBeenLastCalledWith({ mode: PriceScaleMode.Logarithmic });
    expect(localStorage.getItem("radar.logScale")).toBe("1");

    fireEvent.click(button);
    expect(button.getAttribute("aria-pressed")).toBe("false");
    expect(applyOptions).toHaveBeenLastCalledWith({ mode: PriceScaleMode.Normal });
  });

  it("opens logarithmic when that was the saved choice", async () => {
    localStorage.setItem("radar.logScale", "1");
    render(<CandleChart mint="M" />);
    const button = await logButton();
    expect(button.getAttribute("aria-pressed")).toBe("true");
    await waitFor(() =>
      expect(applyOptions).toHaveBeenLastCalledWith({ mode: PriceScaleMode.Logarithmic }),
    );
  });

  it("keeps the mode across a switch between Candles and Line", async () => {
    render(<CandleChart mint="M" />);
    fireEvent.click(await logButton());
    fireEvent.click(screen.getByRole("button", { name: "line" }));
    expect(screen.getByRole("button", { name: "Log" }).getAttribute("aria-pressed")).toBe("true");
    expect(applyOptions).toHaveBeenLastCalledWith({ mode: PriceScaleMode.Logarithmic });
  });
});
