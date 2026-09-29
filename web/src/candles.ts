// SPDX-License-Identifier: Apache-2.0
//! Pure helpers for `CandleChart.tsx` -- turning a `Candle[]` into the shapes
//! `lightweight-charts` wants, the candles/line chart-type preference, and the
//! price axis format. Pulled out on their own so they can be tested without a
//! chart, a DOM, or a fetch.

import type { UTCTimestamp } from "lightweight-charts";
import type { Candle } from "./api";
import { formatChartPrice } from "./format";

/** "Candles" is the informative default; "Line" trades detail for a
 *  cleaner read of the close-price trend. */
export type ChartType = "candles" | "line";

const CHART_TYPE_KEY = "radar.chartType";

/** Reads the visitor's chart-type preference. `localStorage` access is
 *  wrapped in `try`/`catch` -- private browsing and a full quota both throw
 *  on read in some browsers, and a preference toggle is not worth blanking
 *  the chart over. Anything but the literal `"line"` is `"candles"`, so a
 *  corrupted or pre-migration value never leaves the chart with no series. */
export function loadChartType(): ChartType {
  try {
    return localStorage.getItem(CHART_TYPE_KEY) === "line" ? "line" : "candles";
  } catch {
    return "candles";
  }
}

/** Persists the visitor's chart-type preference. Swallows a `localStorage`
 *  write failure the same way `loadChartType` swallows a read failure: the
 *  toggle still works for the rest of this session, it just will not survive
 *  a reload. */
export function saveChartType(type: ChartType): void {
  try {
    localStorage.setItem(CHART_TYPE_KEY, type);
  } catch {
    // Not persisted this time; see the doc comment above.
  }
}

/** `Candle[]` as `lightweight-charts`' `CandlestickSeries` wants it. */
export function toCandlestickData(candles: readonly Candle[]) {
  return candles.map((c) => ({
    time: c.time as UTCTimestamp,
    open: c.open,
    high: c.high,
    low: c.low,
    close: c.close,
  }));
}

/** `Candle[]` as a `LineSeries` of closing prices. */
export function toLineData(candles: readonly Candle[]) {
  return candles.map((c) => ({ time: c.time as UTCTimestamp, value: c.close }));
}

/** `Candle[]` as the volume histogram, coloured by each bar's own direction.
 *  Reads `quote_volume` -- the field the server actually sends. The chart
 *  briefly read a `volume` field that did not exist on the wire, which left
 *  `undefined` reaching a chart library that draws nothing for it: no error,
 *  no bars. */
export function toVolumeData(candles: readonly Candle[], upColor: string, downColor: string) {
  return candles.map((c) => ({
    time: c.time as UTCTimestamp,
    value: c.quote_volume,
    color: c.close >= c.open ? upColor : downColor,
  }));
}

/** The candle whose bucket a given chart time falls in, for the OHLC readout
 *  in line mode -- a line series only reports `value` from the crosshair, not
 *  the open/high/low `formatChartPrice` needs to show. `null` when the time
 *  does not name a bucket this response carries (e.g. the crosshair left the
 *  data entirely). */
export function findCandleAtTime(candles: readonly Candle[], time: number): Candle | null {
  return candles.find((c) => c.time === time) ?? null;
}

/** The price axis format for a SOL-quoted series. `lightweight-charts`'
 *  default is two fixed decimals, which renders every pump.fun-magnitude
 *  price (~2e-7 SOL) as "0.00" -- indistinguishable candles on an axis that
 *  looks precise. `formatChartPrice` supplies the same precision the OHLC
 *  readout uses; `minMove` is set small enough that the axis does not
 *  itself round a sub-cent move away. */
export function solPriceFormat() {
  return {
    type: "custom" as const,
    formatter: (price: number) => formatChartPrice(price),
    // 1e-9 was only ~3.5% of a 2.8e-8 price, so a quiet coin's axis got one
    // or two labels. Well below any real tick, so it never rounds a move away.
    minMove: 1e-12,
  };
}
