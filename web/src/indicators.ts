// SPDX-License-Identifier: Apache-2.0
//! Chart indicators: moving averages over price and over volume.
//!
//! Pulled out of `CandleChart.tsx` for the same reason `honesty.ts` is its own
//! file -- this is the part with a wrong version that looks right. A moving
//! average seeded with zeros for the bars it does not have yet renders a line
//! from the first candle, sloping gently up to the truth as the window fills;
//! a reader has no way to tell that opening slope from a real trend. The only
//! honest picture is no line at all until there is enough history to mean
//! what a moving average is supposed to mean, which is why every function
//! here returns `null` -- never `0`, never an average of fewer bars than it
//! was asked for -- until its window is full.
//!
//! # Absent, not truncated, not zero-seeded
//!
//! Plan 0013 phase E's rubric, in full: "an indicator whose lookback exceeds
//! the available bars is absent, not truncated and not zero-seeded". Both
//! wrong versions produce a number, and a number is a claim: a truncated
//! average over 7 bars when 20 were asked for is presented as though it were
//! the 20-bar average, and a zero-seeded one is worse, since its first real
//! bars are diluted by fabricated zeros that were never a price or a volume.
//! `sma` and `ema` below have no branch that does either -- an index before
//! the window fills is `null` by construction, not by a check that could be
//! forgotten.

/** A moving average's method. */
export type IndicatorMethod = "sma" | "ema";

/** What a moving average is computed over. */
export type IndicatorTarget = "price" | "volume";

/** Lookbacks offered for a price moving average -- the common ones a reader
 *  of any candlestick chart already expects. */
export const SMA_LOOKBACKS: readonly number[] = [20, 50, 200];
export const EMA_LOOKBACKS: readonly number[] = [12, 26, 50];

/** The one volume moving average offered. Radar's `Candle` carries `volume`
 *  already (drawn as the histogram beneath the candles); this is a second,
 *  smoothed read of the same number, not a new data source. */
export const VOLUME_SMA_LOOKBACKS: readonly number[] = [20];

/**
 * The simple moving average of `values`, aligned to the same indices.
 *
 * `out[i]` is the average of `values[i - lookback + 1 ..= i]` once that whole
 * window exists, and `null` at every index before it does. If `values` never
 * reaches `lookback` entries the result is `null` all the way through --
 * there is no index at which a partial window is substituted for the real
 * one.
 */
export function sma(values: readonly number[], lookback: number): (number | null)[] {
  if (!Number.isFinite(lookback) || lookback <= 0) {
    throw new Error(`lookback must be a positive number, got ${lookback}`);
  }
  const out: (number | null)[] = new Array(values.length).fill(null);
  let sum = 0;
  for (let i = 0; i < values.length; i++) {
    sum += values[i] ?? 0;
    if (i >= lookback) sum -= values[i - lookback] ?? 0;
    // Only once the window has `lookback` terms in it -- not before, and
    // never with fewer than `lookback` added in.
    if (i >= lookback - 1) out[i] = sum / lookback;
  }
  return out;
}

/**
 * The exponential moving average of `values`, aligned to the same indices.
 *
 * Seeded the conventional way -- the simple average of the first `lookback`
 * values, placed at index `lookback - 1` -- and recursed forward from there.
 * That seed is not an arbitrary choice: an EMA seeded from a single price (or
 * from zero, as the wrong version this rubric guards against would do) is a
 * different number that happens to converge toward the real one after enough
 * bars, and a reader has no way to know how many "enough" is. Before the seed
 * exists, and whenever `values` is shorter than `lookback` altogether, every
 * entry is `null`.
 */
export function ema(values: readonly number[], lookback: number): (number | null)[] {
  if (!Number.isFinite(lookback) || lookback <= 0) {
    throw new Error(`lookback must be a positive number, got ${lookback}`);
  }
  const out: (number | null)[] = new Array(values.length).fill(null);
  if (values.length < lookback) return out;

  const k = 2 / (lookback + 1);
  let seed = 0;
  for (let i = 0; i < lookback; i++) seed += values[i] ?? 0;
  seed /= lookback;
  out[lookback - 1] = seed;

  let prev = seed;
  for (let i = lookback; i < values.length; i++) {
    const value = (values[i] ?? 0) * k + prev * (1 - k);
    out[i] = value;
    prev = value;
  }
  return out;
}

/** Runs [`sma`] or [`ema`] by name, so a caller holding a method as data
 *  (the visitor's stored choice) does not need its own switch. */
export function computeSeries(
  method: IndicatorMethod,
  values: readonly number[],
  lookback: number,
): (number | null)[] {
  return method === "sma" ? sma(values, lookback) : ema(values, lookback);
}

/** One indicator the visitor has turned on. Persisted verbatim -- see
 *  [`loadIndicatorChoices`] -- so this shape is also the storage schema. */
export interface IndicatorChoice {
  method: IndicatorMethod;
  lookback: number;
  target: IndicatorTarget;
}

/** A stable key for one choice, used as a React key and to de-duplicate. */
export function indicatorChoiceKey(choice: IndicatorChoice): string {
  return `${choice.target}-${choice.method}-${choice.lookback}`;
}

/** The label shown on the indicator's toggle and beside its line. */
export function indicatorLabel(choice: IndicatorChoice): string {
  const base = choice.method === "sma" ? "SMA" : "EMA";
  return choice.target === "volume"
    ? `${choice.lookback}-bar volume ${base}`
    : `${choice.lookback}-bar ${base}`;
}

/**
 * What to say when an indicator has no line to draw for lack of history.
 *
 * Rule 9 applies to a chart the same as it does to a table: a moving average
 * that cannot be computed must say so in words a reader would use, not
 * disappear silently (which reads as "nothing to show" rather than "not
 * enough bars yet") and not draw anyway (the failure this whole module
 * exists to prevent).
 */
export function absenceMessage(lookback: number, available: number): string {
  return `${lookback}-bar average needs ${lookback} bars; this interval has ${available}.`;
}

/** One indicator's line, plus whether it has one at all. */
export interface IndicatorResult {
  choice: IndicatorChoice;
  /** Only the bars where the average exists -- never a `null` placeholder,
   *  so the chart library is never handed a point to decide whether to draw.
   *  A gap in this array is not a bug to fix; it is the absence the rubric
   *  asks for. */
  points: { time: number; value: number }[];
  /** True when there were not enough bars to draw even one point. */
  absent: boolean;
  /** Set exactly when `absent` is true. */
  message: string | null;
}

/**
 * Builds one indicator's drawable line from candle times and a value series
 * (closes for a price average, volumes for the volume average).
 *
 * `times` and `values` must be the same length and in the same order as the
 * candles they came from -- this function does not know about `Candle` and
 * does not need to; keeping it generic over "a time" and "a number" is what
 * keeps [`sma`] and [`ema`] testable without a fixture that pulls in the API
 * client's types.
 */
export function computeIndicator(
  times: readonly number[],
  values: readonly number[],
  choice: IndicatorChoice,
): IndicatorResult {
  const series = computeSeries(choice.method, values, choice.lookback);
  const points: { time: number; value: number }[] = [];
  for (let i = 0; i < series.length; i++) {
    const value = series[i];
    if (typeof value === "number") points.push({ time: times[i] ?? 0, value });
  }
  return {
    choice,
    points,
    absent: points.length === 0,
    message: points.length === 0 ? absenceMessage(choice.lookback, values.length) : null,
  };
}

// --- persistence: the visitor's own indicator choices, kept in this browser -

const CHOICE_KEY = "radar.chart.indicators";

function isIndicatorChoice(value: unknown): value is IndicatorChoice {
  if (value === null || typeof value !== "object") return false;
  const c = value as Record<string, unknown>;
  return (
    (c.method === "sma" || c.method === "ema") &&
    typeof c.lookback === "number" &&
    Number.isFinite(c.lookback) &&
    c.lookback > 0 &&
    (c.target === "price" || c.target === "volume")
  );
}

/**
 * The visitor's remembered indicator choices, or `[]` if there are none or
 * the stored value is not a list of choices.
 *
 * Anything that fails to parse, or parses to something other than an array of
 * valid choices, is discarded outright and the entry removed -- the same rule
 * `Wallet.tsx`'s `storedSession` applies to the wallet session, for the same
 * reason: a half-repaired list of indicators is a worse experience than an
 * empty one, and silently keeping the valid-looking half of a corrupt entry
 * is still showing the visitor something nobody chose.
 */
export function loadIndicatorChoices(
  store: Pick<Storage, "getItem" | "removeItem"> = localStorage,
): IndicatorChoice[] {
  let raw: string | null;
  try {
    raw = store.getItem(CHOICE_KEY);
  } catch {
    // A private window, or a browser set to block site data.
    return [];
  }
  if (raw === null) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed) && parsed.every(isIndicatorChoice)) {
      return parsed;
    }
  } catch {
    // Not JSON.
  }
  try {
    store.removeItem(CHOICE_KEY);
  } catch {
    // Nothing to do; the read already failed safely.
  }
  return [];
}

/** Remembers the visitor's indicator choices for next time. Never throws --
 *  an unstorable browser still gets a working chart for this page's
 *  lifetime, just not one that remembers itself. */
export function saveIndicatorChoices(
  choices: readonly IndicatorChoice[],
  store: Pick<Storage, "setItem"> = localStorage,
): void {
  try {
    store.setItem(CHOICE_KEY, JSON.stringify(choices));
  } catch {
    // Unstorable; see above.
  }
}
