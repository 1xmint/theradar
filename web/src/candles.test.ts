// SPDX-License-Identifier: Apache-2.0
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Candle } from "./api";
import {
  findCandleAtTime,
  loadChartType,
  loadLogScale,
  saveLogScale,
  saveChartType,
  solPriceFormat,
  toCandlestickData,
  toLineData,
  toVolumeData,
} from "./candles";

function candle(overrides: Partial<Candle> = {}): Candle {
  return {
    time: 1_000,
    bucket_start: "2026-09-29 00:00:00",
    open: 1,
    high: 2,
    low: 0.5,
    close: 1.5,
    quote_volume: 100,
    token_volume: 200,
    trade_count: 3,
    ...overrides,
  };
}

describe("toCandlestickData", () => {
  it("carries only OHLC, keyed by time", () => {
    expect(toCandlestickData([candle()])).toEqual([
      { time: 1_000, open: 1, high: 2, low: 0.5, close: 1.5 },
    ]);
  });
});

describe("toLineData", () => {
  it("is the closing price only", () => {
    expect(toLineData([candle()])).toEqual([{ time: 1_000, value: 1.5 }]);
  });
});

describe("toVolumeData", () => {
  it("reads quote_volume, not a volume field that does not exist on the wire", () => {
    const up = toVolumeData([candle({ close: 2, open: 1, quote_volume: 42 })], "up", "down");
    expect(up).toEqual([{ time: 1_000, value: 42, color: "up" }]);
  });

  it("colours a bar that closed down with the down colour", () => {
    const down = toVolumeData([candle({ close: 1, open: 2 })], "up", "down");
    expect(down[0]?.color).toBe("down");
  });
});

describe("findCandleAtTime", () => {
  it("finds the bucket a chart time falls in", () => {
    const candles = [candle({ time: 1_000 }), candle({ time: 1_060, close: 3 })];
    expect(findCandleAtTime(candles, 1_060)?.close).toBe(3);
  });

  it("is null for a time no bucket in this response covers", () => {
    expect(findCandleAtTime([candle({ time: 1_000 })], 5_000)).toBeNull();
  });
});

describe("solPriceFormat", () => {
  it("formats a pump.fun-magnitude price with real precision, not '0.00'", () => {
    const fmt = solPriceFormat();
    expect(fmt.formatter(2.2e-7)).not.toBe("0.00");
    expect(fmt.formatter(2.2e-7)).toContain("2.2");
  });
});

describe("chart-type preference", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it("defaults to candles when nothing is stored", () => {
    expect(loadChartType()).toBe("candles");
  });

  it("round-trips a saved preference", () => {
    saveChartType("line");
    expect(loadChartType()).toBe("line");
    saveChartType("candles");
    expect(loadChartType()).toBe("candles");
  });

  it("defaults to candles when localStorage throws on read", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    });
    expect(loadChartType()).toBe("candles");
    expect(() => saveChartType("line")).not.toThrow();
  });
});

describe("log-scale preference", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it("is off when nothing is stored", () => {
    expect(loadLogScale()).toBe(false);
  });

  it("round-trips a saved preference", () => {
    saveLogScale(true);
    expect(loadLogScale()).toBe(true);
    saveLogScale(false);
    expect(loadLogScale()).toBe(false);
  });

  it("is off, and does not throw, when localStorage throws", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    });
    expect(loadLogScale()).toBe(false);
    expect(() => saveLogScale(true)).not.toThrow();
  });
});
