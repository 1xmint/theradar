// SPDX-License-Identifier: Apache-2.0
//! The candlestick chart. `lightweight-charts` draws it; this component only
//! owns the fetch, the timeframe selector, the indicator and drawing
//! controls, and the honesty caption underneath.
//!
//! `PricePath.tsx` drew the token's price history by hand for the
//! decision-record pages, and it does not survive contact with this
//! requirement -- see its removal note in this session's commit. It plotted
//! `last_price` from outcome measurements taken roughly hourly; a trading
//! terminal needs real OHLCV at a chosen interval with a crosshair, which is
//! a different kind of chart from a different kind of data, not a bigger
//! version of the same one.
//!
//! # Indicators and drawings
//!
//! The math -- moving averages, absent rather than zero-seeded or truncated
//! when there is not enough history -- lives in `indicators.ts`, tested on
//! its own. This file only turns a chosen indicator into a `lightweight-charts`
//! line series, or into nothing plus a stated reason when it is absent.
//!
//! Drawings (horizontal lines, trend lines) are the visitor's own: placed with
//! clicks on this chart, kept in `localStorage` per mint (`drawings.ts`), and
//! never sent to Radar. There is no route that accepts one and no `fetch` call
//! in `drawings.ts` -- a line a visitor draws on their own copy of the chart
//! is not data Radar lost track of, it is data Radar never had.

import {useCallback, useEffect, useMemo, useRef, useState} from "react";
import {
  CandlestickSeries,
  ColorType,
  HistogramSeries,
  LineSeries,
  LineStyle,
  PriceScaleMode,
  createChart,
  type IChartApi,
  type IPriceLine,
  type ISeriesApi,
  type MouseEventParams,
  type Time,
  type UTCTimestamp,
} from "lightweight-charts";
import { CANDLE_INTERVALS, market, type Candle, type CandleInterval } from "./api";
import {
  findCandleAtTime,
  loadChartType,
  loadLogScale,
  saveChartType,
  saveLogScale,
  solPriceFormat,
  toCandlestickData,
  toLineData,
  toVolumeData,
  type ChartType,
} from "./candles";
import {
  clearDrawings,
  loadDrawings,
  newDrawingId,
  saveDrawings,
  type Drawing,
} from "./drawings";
import {
  EMA_LOOKBACKS,
  SMA_LOOKBACKS,
  VOLUME_SMA_LOOKBACKS,
  computeIndicator,
  indicatorChoiceKey,
  indicatorLabel,
  loadIndicatorChoices,
  saveIndicatorChoices,
  type IndicatorChoice,
} from "./indicators";

import {formatChartPrice, formatStamp, quoteLabel} from "./format";
import { useApi } from "./useApi";

/** How often the mounted chart refetches candles for the live feed, in
 *  milliseconds. Both server paths read a local tape or store on the request
 *  path (see the PR body) -- there is no CryptoHouse query to rate-limit
 *  against, so this only needs to be fast enough to feel live. */
const CANDLE_REFRESH_MS = 15_000;

/**
 * The chart's colours, as sRGB rather than as the palette's `oklch()`.
 *
 * `lightweight-charts` parses its colour strings itself and its parser predates
 * `oklch`: handed one it throws `Failed to parse color`, and because that throw
 * happens inside the chart's own render it is uncaught and blanks the entire
 * page — not just the chart. Observed 2026-09-11; the terminal rendered as an
 * empty black rectangle with the error only visible in the console.
 *
 * So these are the palette's values converted once, here, rather than read from
 * CSS custom properties at runtime. **They must be kept in step with
 * `index.css` by hand**, which is a real cost and the reason it is written down:
 * the alternative is reading the computed value and converting `oklch` to sRGB
 * in this file, which is a colour-space conversion nobody should hand-roll to
 * style a chart.
 */
const CHART_COLORS = {
  /** `--color-dim`, the axis labels. */
  dim: "#9aa0ab",
  /** `--color-line`, the grid and the scale borders. */
  line: "#3a3f47",
  /** A fainter line still, for the volume histogram's baseline. */
  faint: "#5c626b",
  /** `--color-good`, a candle that closed up. */
  up: "#5fd39a",
  /** `--color-bad`, a candle that closed down. */
  down: "#f08a5d",
  /** The same two at half opacity, for volume bars under the candles. */
  upSoft: "rgba(95, 211, 154, 0.5)",
  downSoft: "rgba(240, 138, 93, 0.5)",
  /** The visitor's own drawings -- distinct from every indicator colour below
   *  so a horizontal line is never mistaken for a moving average. */
  drawing: "#e8c14b",
} as const;

/** Cycled across whichever indicators are active, so two moving averages on
 *  screen at once are never the same colour. Kept short on purpose: this is a
 *  candlestick chart first, and a reader who turns on five averages at once
 *  has already left the part of this feature that was designed for. */
const INDICATOR_COLORS: readonly string[] = [
  "#7ea1ff",
  "#c792ea",
  "#5fd3d3",
  "#f0c85f",
  "#ff8fa3",
  "#9adb6c",
];

const INTERVAL_LABEL: Record<CandleInterval, string> = {
  "1m": "1m",
  "5m": "5m",
  "15m": "15m",
  "1h": "1h",
  "4h": "4h",
  "1d": "1D",
};

/** Every indicator on offer. `SMA_LOOKBACKS`/`EMA_LOOKBACKS`/
 *  `VOLUME_SMA_LOOKBACKS` in `indicators.ts` are the source of truth for which
 *  lookbacks exist; this only turns them into toggleable choices. */
const INDICATOR_OPTIONS: readonly IndicatorChoice[] = [
  ...SMA_LOOKBACKS.map((lookback): IndicatorChoice => ({ method: "sma", lookback, target: "price" })),
  ...EMA_LOOKBACKS.map((lookback): IndicatorChoice => ({ method: "ema", lookback, target: "price" })),
  ...VOLUME_SMA_LOOKBACKS.map(
    (lookback): IndicatorChoice => ({ method: "sma", lookback, target: "volume" }),
  ),
];

export function CandleChart({ mint }: { mint: string }) {
  const [interval, setInterval] = useState<CandleInterval>("15m");
  const load = useApi(
    (signal) => market.candles(mint, { interval }, signal),
    [mint, interval],
    CANDLE_REFRESH_MS,
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
          <Chart
            mint={mint}
            candles={load.value.candles}
            interval={load.value.interval}
            from={load.value.covered.from}
            to={load.value.covered.to}
            complete={load.value.covered.complete}
            quoteMint={load.value.quote_mint}
          />
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

/** What the chart is waiting for the next click to do. `null` means clicks do
 *  nothing but move the crosshair, which is the ordinary state. */
type PlacingMode = "horizontal" | "trend" | null;

/** Renders once real candles exist, so the chart library never has to handle
 *  an empty series -- that state is `Placeholder`'s job above. */
function Chart({
  mint,
  candles,
  interval,
  from,
  to,
  complete,
  quoteMint,
}: {
  mint: string;
  candles: Candle[];
  interval: CandleInterval;
  /** The covered range, as the server's UTC stamps. Text, not epoch. */
  from: string;
  to: string;
  /** The server's own statement about whether it covered what was asked for. */
  complete: boolean;
  quoteMint: string | null;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const candleSeriesRef = useRef<ISeriesApi<"Candlestick"> | null>(null);
  const lineSeriesRef = useRef<ISeriesApi<"Line"> | null>(null);
  const volumeRef = useRef<ISeriesApi<"Histogram"> | null>(null);
  const indicatorSeriesRef = useRef<Map<string, ISeriesApi<"Line">>>(new Map());
  // Each price line remembers the series it was drawn on. After a
  // Candles/Line toggle, `mainSeries()` is the *other* series, and
  // `removePriceLine` on a series that does not own the line silently does
  // nothing -- so "Clear lines" left the old line on screen.
  const priceLineRef = useRef<
    Map<string, { owner: ISeriesApi<"Candlestick"> | ISeriesApi<"Line">; line: IPriceLine }>
  >(new Map());
  const trendSeriesRef = useRef<Map<string, ISeriesApi<"Line">>>(new Map());

  // "Candles" is the informative default; "Line" trades detail for a cleaner
  // read of the close-price trend. Both series exist from mount and only one
  // is ever visible -- swapping which one is *visible* rather than tearing
  // one down and building the other keeps the crosshair/click subscriptions
  // (registered once, below) working without re-subscribing on every toggle.
  // `chartTypeRef` is what those closures actually read.
  const [chartType, setChartType] = useState<ChartType>(() => loadChartType());
  // A logarithmic price axis keeps every candle's shape when one real candle
  // crashes 99.99% (a rug), which on a linear axis flattens all the others.
  // Off by default. It is a scale option, not a series option, so it applies
  // to the candle and line series alike.
  const [logScale, setLogScale] = useState<boolean>(() => loadLogScale());
  const chartTypeRef = useRef<ChartType>(chartType);
  chartTypeRef.current = chartType;

  /** The series the reader is currently looking at -- what the crosshair,
   *  click-to-place and price-line handlers must all act on. */
  const mainSeries = useCallback((): ISeriesApi<"Candlestick"> | ISeriesApi<"Line"> | null => {
    return chartTypeRef.current === "line" ? lineSeriesRef.current : candleSeriesRef.current;
  }, []);

  const [crosshairTime, setCrosshairTime] = useState<number | null>(null);

  const [indicatorChoices, setIndicatorChoices] = useState<IndicatorChoice[]>(() =>
    loadIndicatorChoices(),
  );
  const [indicatorMessages, setIndicatorMessages] = useState<string[]>([]);

  // Drawings are keyed by mint, so switching coins must reload -- not carry
  // the previous coin's lines onto this one's chart.
  const [drawings, setDrawings] = useState<Drawing[]>(() => loadDrawings(mint));
  useEffect(() => {
    setDrawings(loadDrawings(mint));
  }, [mint]);

  const [placing, setPlacing] = useState<PlacingMode>(null);
  const placingRef = useRef<PlacingMode>(null);
  placingRef.current = placing;
  const pendingPointRef = useRef<{ time: number; price: number } | null>(null);

  // The click handler below is registered once, in the chart-lifecycle
  // effect, so it always reads the *current* mint and drawing list through
  // this ref rather than the values closed over at mount.
  const addDrawingRef = useRef<(mint: string, drawing: Drawing) => void>(() => {});
  addDrawingRef.current = useCallback((forMint: string, drawing: Drawing) => {
    setDrawings((prev) => {
      // A stale click from a chart that has since switched mints must not
      // attach its line to the wrong coin's list.
      if (forMint !== mint) return prev;
      const next = [...prev, drawing];
      saveDrawings(forMint, next);
      return next;
    });
  }, [mint]);

  const mintRef = useRef(mint);
  mintRef.current = mint;

  // Chart lifecycle: created once per mount of a container, torn down on
  // unmount. Recreating it on every candle update would drop the reader's
  // zoom and scroll position on every refresh.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const chart = createChart(container, {
      layout: {
        background: { type: ColorType.Solid, color: "transparent" },
        textColor: CHART_COLORS.dim,
        fontSize: 11,
      },
      grid: {
        // Vertical grid lines mostly repeat the time-axis labels; drawn this
        // faint they still give the eye a column to line a candle up against
        // without competing with the candles themselves.
        vertLines: { color: CHART_COLORS.faint, style: LineStyle.SparseDotted },
        horzLines: { color: CHART_COLORS.line },
      },
      rightPriceScale: { borderColor: CHART_COLORS.line },
      timeScale: {
        borderColor: CHART_COLORS.line,
        timeVisible: true,
        // A little daylight past the last candle so the newest bar is not
        // drawn flush against the price-scale border.
        rightOffset: 4,
      },
      crosshair: { mode: 0 },
    });

    const candleSeries = chart.addSeries(CandlestickSeries, {
      upColor: CHART_COLORS.up,
      downColor: CHART_COLORS.down,
      borderVisible: false,
      wickUpColor: CHART_COLORS.up,
      wickDownColor: CHART_COLORS.down,
      priceFormat: solPriceFormat(),
    });

    const lineSeries = chart.addSeries(LineSeries, {
      color: CHART_COLORS.up,
      lineWidth: 2,
      priceFormat: solPriceFormat(),
      visible: false,
    });

    const volume = chart.addSeries(HistogramSeries, {
      color: CHART_COLORS.faint,
      priceFormat: { type: "volume" },
      priceScaleId: "",
    });
    volume.priceScale().applyOptions({ scaleMargins: { top: 0.8, bottom: 0 } });

    chartRef.current = chart;
    candleSeriesRef.current = candleSeries;
    lineSeriesRef.current = lineSeries;
    volumeRef.current = volume;

    chart.subscribeCrosshairMove((param: MouseEventParams<Time>) => {
      setCrosshairTime(param.time === undefined ? null : Number(param.time));
    });

    // Drawing placement: armed by the toolbar below, disarmed after the click
    // (or clicks) that complete a shape. Reads `placingRef` and
    // `addDrawingRef` rather than closed-over state, since this subscription
    // is made once at mount and both change afterwards. `mainSeries()` reads
    // `chartTypeRef` for the same reason: candles/line can toggle long after
    // this subscription was made.
    chart.subscribeClick((param: MouseEventParams<Time>) => {
      const mode = placingRef.current;
      const series = mainSeries();
      if (!mode || !series || !param.point || param.time === undefined) return;
      const price = series.coordinateToPrice(param.point.y);
      if (price === null) return;
      const time = Number(param.time);

      if (mode === "horizontal") {
        addDrawingRef.current(mintRef.current, {
          id: newDrawingId(),
          kind: "horizontal",
          price,
        });
        pendingPointRef.current = null;
        setPlacing(null);
        return;
      }

      // mode === "trend": the first click sets the anchor and waits; the
      // second completes the line.
      const pending = pendingPointRef.current;
      if (!pending) {
        pendingPointRef.current = { time, price };
        return;
      }
      addDrawingRef.current(mintRef.current, {
        id: newDrawingId(),
        kind: "trend",
        from: pending,
        to: { time, price },
      });
      pendingPointRef.current = null;
      setPlacing(null);
    });

    const resize = new ResizeObserver(() => {
      chart.applyOptions({ width: container.clientWidth, height: container.clientHeight });
    });
    resize.observe(container);

    return () => {
      resize.disconnect();
      chart.remove();
      chartRef.current = null;
      candleSeriesRef.current = null;
      lineSeriesRef.current = null;
      volumeRef.current = null;
      indicatorSeriesRef.current.clear();
      priceLineRef.current.clear();
      trendSeriesRef.current.clear();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Toggling chart type swaps which series is *visible* rather than adding or
  // removing one -- see the ref declarations above for why. Persisted so the
  // next visit (or the next mint) opens the way this one was left.
  useEffect(() => {
    candleSeriesRef.current?.applyOptions({ visible: chartType === "candles" });
    lineSeriesRef.current?.applyOptions({ visible: chartType === "line" });
    saveChartType(chartType);
  }, [chartType]);

  // Declared after the chart-creation effect so a freshly mounted chart (new
  // coin or interval) gets the saved mode too.
  useEffect(() => {
    chartRef.current?.priceScale("right").applyOptions({
      mode: logScale ? PriceScaleMode.Logarithmic : PriceScaleMode.Normal,
    });
    saveLogScale(logScale);
  }, [logScale]);

  // `fitContent()` only on the first data a given mint+interval loads. Every
  // 15s refresh after that must leave the reader's zoom and scroll alone --
  // firing it on every update was the bug: a reader who zoomed in got yanked
  // back out on the next poll.
  const fitKeyRef = useRef<string | null>(null);
  useEffect(() => {
    const candle = candleSeriesRef.current;
    const line = lineSeriesRef.current;
    const volume = volumeRef.current;
    if (!candle || !line || !volume) return;
    candle.setData(toCandlestickData(candles));
    line.setData(toLineData(candles));
    volume.setData(toVolumeData(candles, CHART_COLORS.upSoft, CHART_COLORS.downSoft));

    const fitKey = `${mint}:${interval}`;
    if (fitKeyRef.current !== fitKey) {
      fitKeyRef.current = fitKey;
      chartRef.current?.timeScale().fitContent();
    }
  }, [candles, mint, interval]);

  // Indicators: recomputed from *this* interval's candles every time either
  // changes. Switching timeframe therefore recomputes from the new interval's
  // own bars -- it never carries the previous interval's line over and never
  // stretches a short interval's average to look like a full one.
  useEffect(() => {
    const chart = chartRef.current;
    if (!chart) return;

    for (const s of indicatorSeriesRef.current.values()) chart.removeSeries(s);
    indicatorSeriesRef.current.clear();

    const times = candles.map((c) => c.time);
    const closes = candles.map((c) => c.close);
    const volumes = candles.map((c) => c.quote_volume);
    const messages: string[] = [];

    indicatorChoices.forEach((choice, i) => {
      const values = choice.target === "volume" ? volumes : closes;
      const result = computeIndicator(times, values, choice);
      if (result.absent) {
        if (result.message) messages.push(`${indicatorLabel(choice)}: ${result.message}`);
        return;
      }
      const color = INDICATOR_COLORS[i % INDICATOR_COLORS.length] ?? CHART_COLORS.drawing;
      const line = chart.addSeries(LineSeries, {
        color,
        lineWidth: 2,
        priceScaleId: choice.target === "volume" ? "" : "right",
        lastValueVisible: false,
        priceLineVisible: false,
        crosshairMarkerVisible: false,
        title: indicatorLabel(choice),
      });
      line.setData(result.points.map((p) => ({ time: p.time as UTCTimestamp, value: p.value })));
      indicatorSeriesRef.current.set(indicatorChoiceKey(choice), line);
    });

    setIndicatorMessages(messages);
  }, [candles, indicatorChoices]);

  useEffect(() => {
    saveIndicatorChoices(indicatorChoices);
  }, [indicatorChoices]);

  // Drawings: horizontal lines as native price lines on the candle series,
  // trend lines as a two-point line series -- `lightweight-charts` draws a
  // straight segment between exactly two points without needing a plugin.
  useEffect(() => {
    const chart = chartRef.current;
    const series = mainSeries();
    if (!chart || !series) return;

    for (const { owner, line } of priceLineRef.current.values()) owner.removePriceLine(line);
    priceLineRef.current.clear();
    for (const s of trendSeriesRef.current.values()) chart.removeSeries(s);
    trendSeriesRef.current.clear();

    for (const drawing of drawings) {
      if (drawing.kind === "horizontal") {
        const priceLine = series.createPriceLine({
          price: drawing.price,
          color: CHART_COLORS.drawing,
          lineWidth: 1,
          lineStyle: LineStyle.Solid,
          axisLabelVisible: true,
          title: "",
        });
        priceLineRef.current.set(drawing.id, { owner: series, line: priceLine });
      } else {
        const [first, second] = [drawing.from, drawing.to].sort((a, b) => a.time - b.time);
        if (!first || !second) continue;
        const line = chart.addSeries(LineSeries, {
          color: CHART_COLORS.drawing,
          lineWidth: 2,
          lastValueVisible: false,
          priceLineVisible: false,
          crosshairMarkerVisible: false,
        });
        line.setData([
          { time: first.time as UTCTimestamp, value: first.price },
          { time: second.time as UTCTimestamp, value: second.price },
        ]);
        trendSeriesRef.current.set(drawing.id, line);
      }
    }
    // `chartType` is a dependency, not just a read through `mainSeries()`:
    // toggling chart type must reattach a horizontal line to the series that
    // is now visible, or it stays on a hidden one and looks like it vanished.
  }, [drawings, chartType]);

  const toggleIndicator = useCallback((choice: IndicatorChoice) => {
    setIndicatorChoices((prev) => {
      const key = indicatorChoiceKey(choice);
      const active = prev.some((c) => indicatorChoiceKey(c) === key);
      return active ? prev.filter((c) => indicatorChoiceKey(c) !== key) : [...prev, choice];
    });
  }, []);

  const clearAllDrawings = useCallback(() => {
    clearDrawings(mint);
    setDrawings([]);
    setPlacing(null);
    pendingPointRef.current = null;
  }, [mint]);

  const activeKeys = useMemo(
    () => new Set(indicatorChoices.map(indicatorChoiceKey)),
    [indicatorChoices],
  );

  // `crosshairTime` only ever names a bucket this response carries --
  // `findCandleAtTime` looks the OHLC back up rather than the crosshair
  // handler carrying it, since a line series' own crosshair data is just a
  // `value` (the close), not open/high/low.
  const last = candles.at(-1) ?? null;
  const readout = (crosshairTime === null ? null : findCandleAtTime(candles, crosshairTime)) ?? last;

  // The server says whether it covered the range asked for. It used to be
  // inferred here by comparing the returned candles' edges against the
  // window -- a guess standing in for a fact the response already carried,
  // and one that read "complete" for any window whose first and last candle
  // happened to sit at its edges.
  const narrower = !complete;

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-[var(--color-line)] px-3 py-1.5">
        <div className="flex gap-1">
          {(["candles", "line"] as const).map((type) => (
            <button
              key={type}
              type="button"
              onClick={() => setChartType(type)}
              aria-pressed={type === chartType}
              className={`rounded px-2 py-0.5 text-xs capitalize ${
                type === chartType
                  ? "bg-[var(--color-ink)] text-[var(--color-text)]"
                  : "text-[var(--color-dim)] hover:text-[var(--color-text)]"
              }`}
            >
              {type}
            </button>
          ))}
        </div>
        <button
          type="button"
          onClick={() => setLogScale((on) => !on)}
          aria-pressed={logScale}
          title="Logarithmic price axis"
          className={`rounded px-2 py-0.5 text-xs ${
            logScale
              ? "bg-[var(--color-ink)] text-[var(--color-text)]"
              : "text-[var(--color-dim)] hover:text-[var(--color-text)]"
          }`}
        >
          Log
        </button>
        {INDICATOR_OPTIONS.map((choice) => {
          const key = indicatorChoiceKey(choice);
          const active = activeKeys.has(key);
          return (
            <button
              key={key}
              type="button"
              onClick={() => toggleIndicator(choice)}
              aria-pressed={active}
              className={`rounded px-2 py-0.5 text-xs ${
                active
                  ? "bg-[var(--color-ink)] text-[var(--color-text)]"
                  : "text-[var(--color-dim)] hover:text-[var(--color-text)]"
              }`}
            >
              {indicatorLabel(choice)}
            </button>
          );
        })}
        <span className="ml-auto flex items-center gap-2 text-xs">
          <button
            type="button"
            onClick={() => setPlacing((p) => (p === "horizontal" ? null : "horizontal"))}
            aria-pressed={placing === "horizontal"}
            className={`rounded px-2 py-0.5 ${
              placing === "horizontal"
                ? "bg-[var(--color-ink)] text-[var(--color-text)]"
                : "text-[var(--color-dim)] hover:text-[var(--color-text)]"
            }`}
          >
            {placing === "horizontal" ? "Click the chart…" : "+ Line"}
          </button>
          <button
            type="button"
            onClick={() => {
              pendingPointRef.current = null;
              setPlacing((p) => (p === "trend" ? null : "trend"));
            }}
            aria-pressed={placing === "trend"}
            className={`rounded px-2 py-0.5 ${
              placing === "trend"
                ? "bg-[var(--color-ink)] text-[var(--color-text)]"
                : "text-[var(--color-dim)] hover:text-[var(--color-text)]"
            }`}
          >
            {placing === "trend" ? "Click two points…" : "+ Trend"}
          </button>
          {drawings.length > 0 && (
            <button
              type="button"
              onClick={clearAllDrawings}
              className="text-[var(--color-dim)] underline hover:text-[var(--color-text)]"
            >
              Clear lines
            </button>
          )}
        </span>
      </div>

      <div className="flex items-baseline gap-3 px-3 py-1 text-xs tabular-nums text-[var(--color-dim)]">
        {readout ? (
          <>
            {/* Not USD: `Fill.price` in radar-stream/src/decode.rs is
             *  quote_amount / token_amount against whichever quote leg the
             *  pool paired with. This readout
             *  once carried a "$" prefix on a number never in dollars. */}
            <span>O {formatChartPrice(readout.open)}</span>
            <span>H {formatChartPrice(readout.high)}</span>
            <span>L {formatChartPrice(readout.low)}</span>
            <span>C {formatChartPrice(readout.close)}</span>
            {/* The unit comes from the server, never assumed: many pump.fun
             *  coins trade against PUMP or USDC, and a fixed "SOL" here was
             *  as wrong for them as the "$" it replaced. No unit when the
             *  server cannot name one. */}
            {quoteMint && <span>{quoteLabel(quoteMint)}</span>}
          </>
        ) : (
          <span>&nbsp;</span>
        )}
      </div>
      <div ref={containerRef} className="min-h-0 flex-1" />
      <p className="border-t border-[var(--color-line)] px-3 py-1 text-[10px] text-[var(--color-dim)]">
        {interval} candles, {formatStamp(from)} – {formatStamp(to)}
        {narrower ? " — Radar may not have every trade in this range, so a gap can be missing data rather than quiet trading." : ""}
        {drawings.length > 0 ? " — your lines, kept in this browser." : ""}
      </p>
      {indicatorMessages.length > 0 && (
        <p className="px-3 pb-1 text-[10px] text-[var(--color-dim)]">
          {indicatorMessages.join(" ")}
        </p>
      )}
    </div>
  );
}
