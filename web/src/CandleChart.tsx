// SPDX-License-Identifier: Apache-2.0
//! The candlestick chart. `lightweight-charts` draws it; this component only
//! owns the fetch, the timeframe selector, and the honesty caption underneath.
//!
//! `PricePath.tsx` drew the token's price history by hand for the
//! decision-record pages, and it does not survive contact with this
//! requirement -- see its removal note in this session's commit. It plotted
//! `last_price` from outcome measurements taken roughly hourly; a trading
//! terminal needs real OHLCV at a chosen interval with a crosshair, which is
//! a different kind of chart from a different kind of data, not a bigger
//! version of the same one.

import { useEffect, useMemo, useRef, useState } from "react";
import {
  CandlestickSeries,
  ColorType,
  HistogramSeries,
  createChart,
  type IChartApi,
  type ISeriesApi,
  type MouseEventParams,
  type Time,
  type UTCTimestamp,
} from "lightweight-charts";
import { CANDLE_INTERVALS, market, type Candle, type CandleInterval } from "./api";
import { isNarrowerThanRequested } from "./honesty";
import { formatPrice } from "./format";
import { useApi } from "./useApi";

const INTERVAL_LABEL: Record<CandleInterval, string> = {
  "1m": "1m",
  "5m": "5m",
  "15m": "15m",
  "1h": "1h",
  "4h": "4h",
  "1d": "1D",
};

export function CandleChart({ mint }: { mint: string }) {
  const [interval, setInterval] = useState<CandleInterval>("15m");
  const load = useApi(
    (signal) => market.candles(mint, { interval }, signal),
    [mint, interval],
  );

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between gap-2 border-b border-[var(--color-line)] px-3 py-1.5">
        <div className="flex gap-1">
          {CANDLE_INTERVALS.map((i) => (
            <button
              key={i}
              type="button"
              onClick={() => setInterval(i)}
              aria-pressed={i === interval}
              className={`rounded px-2 py-0.5 text-xs ${
                i === interval
                  ? "bg-[var(--color-ink)] text-[var(--color-text)]"
                  : "text-[var(--color-dim)] hover:text-[var(--color-text)]"
              }`}
            >
              {INTERVAL_LABEL[i]}
            </button>
          ))}
        </div>
      </div>

      <div className="min-h-0 flex-1">
        {load.state === "loading" && (
          <Placeholder text="Reading candles…" />
        )}
        {load.state === "failed" && (
          <Placeholder
            text={`Could not read the candle feed: ${load.detail}. This is not a statement about the token's price.`}
            warn
          />
        )}
        {load.state === "ready" && load.value.candles.length === 0 && (
          <Placeholder text="No candles recorded for this interval yet." />
        )}
        {load.state === "ready" && load.value.candles.length > 0 && (
          <Chart candles={load.value.candles} interval={load.value.interval} from={load.value.from} to={load.value.to} />
        )}
      </div>
    </div>
  );
}

function Placeholder({ text, warn = false }: { text: string; warn?: boolean }) {
  return (
    <div className="flex h-full items-center justify-center p-6 text-center text-sm">
      <p className={warn ? "text-[var(--color-warn)]" : "text-[var(--color-dim)]"}>{text}</p>
    </div>
  );
}

/** Renders once real candles exist, so the chart library never has to handle
 *  an empty series -- that state is `Placeholder`'s job above. */
function Chart({
  candles,
  interval,
  from,
  to,
}: {
  candles: Candle[];
  interval: CandleInterval;
  from: number;
  to: number;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<ISeriesApi<"Candlestick"> | null>(null);
  const volumeRef = useRef<ISeriesApi<"Histogram"> | null>(null);
  const [crosshair, setCrosshair] = useState<Candle | null>(null);

  // Chart lifecycle: created once per mount of a container, torn down on
  // unmount. Recreating it on every candle update would drop the reader's
  // zoom and scroll position on every refresh.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const chart = createChart(container, {
      layout: {
        background: { type: ColorType.Solid, color: "transparent" },
        textColor: "oklch(0.68 0.012 260)",
        fontSize: 11,
      },
      grid: {
        vertLines: { color: "oklch(0.32 0.014 260)" },
        horzLines: { color: "oklch(0.32 0.014 260)" },
      },
      rightPriceScale: { borderColor: "oklch(0.32 0.014 260)" },
      timeScale: { borderColor: "oklch(0.32 0.014 260)", timeVisible: true },
      crosshair: { mode: 0 },
    });

    const series = chart.addSeries(CandlestickSeries, {
      upColor: "oklch(0.8 0.13 155)",
      downColor: "oklch(0.66 0.15 45)",
      borderVisible: false,
      wickUpColor: "oklch(0.8 0.13 155)",
      wickDownColor: "oklch(0.66 0.15 45)",
    });

    const volume = chart.addSeries(HistogramSeries, {
      color: "oklch(0.49 0.014 260)",
      priceFormat: { type: "volume" },
      priceScaleId: "",
    });
    volume.priceScale().applyOptions({ scaleMargins: { top: 0.8, bottom: 0 } });

    chartRef.current = chart;
    seriesRef.current = series;
    volumeRef.current = volume;

    chart.subscribeCrosshairMove((param: MouseEventParams<Time>) => {
      const point = param.seriesData.get(series);
      if (point && "open" in point) {
        setCrosshair({
          time: Number(param.time),
          open: point.open,
          high: point.high,
          low: point.low,
          close: point.close,
          volume: 0,
        });
      } else {
        setCrosshair(null);
      }
    });

    const resize = new ResizeObserver(() => {
      chart.applyOptions({ width: container.clientWidth, height: container.clientHeight });
    });
    resize.observe(container);

    return () => {
      resize.disconnect();
      chart.remove();
      chartRef.current = null;
      seriesRef.current = null;
      volumeRef.current = null;
    };
  }, []);

  useEffect(() => {
    const series = seriesRef.current;
    const volume = volumeRef.current;
    if (!series || !volume) return;
    series.setData(
      candles.map((c) => ({
        time: c.time as UTCTimestamp,
        open: c.open,
        high: c.high,
        low: c.low,
        close: c.close,
      })),
    );
    volume.setData(
      candles.map((c) => ({
        time: c.time as UTCTimestamp,
        value: c.volume,
        color: c.close >= c.open ? "oklch(0.8 0.13 155 / 0.5)" : "oklch(0.66 0.15 45 / 0.5)",
      })),
    );
    chartRef.current?.timeScale().fitContent();
  }, [candles]);

  const last = candles.at(-1) ?? null;
  const readout = crosshair ?? last;

  const narrower = useMemo(() => {
    const first = candles[0];
    const lastCandle = candles.at(-1);
    if (!first || !lastCandle) return false;
    // What was actually asked for is the requested window; absent an explicit
    // one the request is "everything up to now", so the only honest check
    // available without a stored request range is whether the server's own
    // stated `from`/`to` cover the candles it actually returned.
    return isNarrowerThanRequested(from, to, first.time, lastCandle.time);
  }, [candles, from, to]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-baseline gap-3 px-3 py-1 text-xs tabular-nums text-[var(--color-dim)]">
        {readout ? (
          <>
            <span>O {formatPrice(readout.open)}</span>
            <span>H {formatPrice(readout.high)}</span>
            <span>L {formatPrice(readout.low)}</span>
            <span>C {formatPrice(readout.close)}</span>
          </>
        ) : (
          <span>&nbsp;</span>
        )}
      </div>
      <div ref={containerRef} className="min-h-0 flex-1" />
      <p className="border-t border-[var(--color-line)] px-3 py-1 text-[10px] text-[var(--color-dim)]">
        {interval} candles, {new Date(from * 1000).toLocaleString()} –{" "}
        {new Date(to * 1000).toLocaleString()}
        {narrower ? " — narrower than the requested range; this is what Radar has, not the whole history." : ""}
      </p>
    </div>
  );
}
