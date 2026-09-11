// SPDX-License-Identifier: Apache-2.0
//! The shapes `radar-serve` returns.
//!
//! Hand-written rather than generated, and deliberately narrow: only the fields
//! the interface actually reads. A type that claimed to mirror the whole server
//! response would drift silently, and the drift would be invisible until a
//! field nobody rendered turned out to matter.
//!
//! This file used to also carry the decision-record shapes -- `Funnel`,
//! `DecisionRecord`, `TokenEvidence`, `Capacity`, `Returns`, `Activity`,
//! `Scoreboard`, `Health`, and the `api`/`research`/`operator` groups of
//! fetchers built on them. They went with the pages that were their only
//! callers (`Decisions.tsx`, `Scoreboard.tsx`, `Token.tsx`, `Analyst.tsx`,
//! `Health.tsx`): the owner's correction was explicit that the client should
//! stop referring to those routes, a separate task is making them
//! operator-only server-side, and a fetcher with no caller is not a seam kept
//! open, it is dead code that happens to compile. What is left is what the
//! terminal actually calls: the agent chat, and the market surface below.

/**
 * A failed fetch, carrying enough to say what went wrong.
 *
 * A page that renders "error" and nothing else sends someone to the logs. The
 * status is usually the whole answer: 503 means the store is empty, which is a
 * normal state for a fresh instance rather than a fault.
 */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly detail: string,
  ) {
    super(`${status}: ${detail}`);
    this.name = "ApiError";
  }
}

async function get<T>(path: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(path, {
    signal: signal ?? null,
    headers: { accept: "application/json" },
  });
  if (!response.ok) {
    // The server sends `{"error": "..."}` for every failure it authors, so this
    // usually says something useful. When it does not, the status still does.
    let detail = response.statusText;
    try {
      const body = (await response.json()) as { error?: string };
      if (body.error) detail = body.error;
    } catch {
      // A non-JSON body is itself informative: something upstream answered.
    }
    throw new ApiError(response.status, detail);
  }
  return (await response.json()) as T;
}

/** How the credential-linking flow is going. */
export type Progress =
  | {
      state: "waiting";
      verification_url: string;
      user_code: string;
      seconds_elapsed: number;
    }
  | { state: "linked" }
  | { state: "failed"; status: string }
  | { state: "idle" };

/** What the model said, and what it was shown. */
export interface Answered {
  text: string;
  citations: string[];
  uncited: boolean;
}

async function send<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(path, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json" },
    // `null` rather than `undefined`: strict optional properties treat an
    // explicitly-undefined field as a type error, and `null` is what fetch
    // documents for "no body".
    body: body === undefined ? null : JSON.stringify(body),
  });
  if (!response.ok) {
    let detail = response.statusText;
    try {
      const parsed = (await response.json()) as { error?: string };
      if (parsed.error) detail = parsed.error;
    } catch {
      // A non-JSON body is itself informative: something upstream answered.
    }
    throw new ApiError(response.status, detail);
  }
  return (await response.json()) as T;
}

export const agent = {
  /** Starts a device-authorisation flow, or returns the one already open. */
  link: () => send<Progress>("/v1/link"),
  /** Where the current flow has got to. */
  linkStatus: (signal?: AbortSignal) => get<Progress>("/v1/link", signal),
  /** Asks a question. */
  ask: (question: string) => send<Answered>("/v1/chat", { question }),
};

/**
 * The market surface: `/v1/market/*`.
 *
 * Answers "what is this token doing right now" from a venue read — price,
 * candles, the tape, holders — and none of it passes through a Radar
 * decision. That distinction is the whole reason the terminal's right rail
 * reserves a named, empty space for Radar's own signals rather than folding
 * them into this object: a market read and a Radar judgement are different
 * kinds of fact, and this file should not make them look like one kind by
 * routing them through the same group of fetchers.
 *
 * # This shape is not yet confirmed against a live server
 *
 * These routes did not exist in `radar-serve` when this was written; another
 * session was building them in parallel. The field names below are this
 * session's best-effort mirror of the contract it was given, not a read of
 * real JSON. Two conventions are assumptions, recorded so the next person
 * does not mistake them for verified fact:
 *
 * - **A nullable fact carries its reason in a sibling `<field>_reason`
 *   field**, e.g. `price` / `price_reason`. This is a plain nullable field
 *   plus a sibling, not a wrapped `{value, reason}` object, matching this
 *   file's existing convention of hand-written flat DTOs — and the
 *   contract's own words — "fields that could not be computed arrive null
 *   **with a reason**" — need a field for the reason to live in.
 * - **No route reports a row-cap flag.** Rather than assume one, the tape and
 *   holders panels infer a possible cap by comparing the number of rows
 *   returned against the number requested (`capCaption` in `honesty.ts`) —
 *   true regardless of what the server ends up calling the field, at the
 *   cost of a false "may be capped" on the rare exact-match page.
 *
 * If the live shape differs, only this block and the components that read it
 * need to change — nothing downstream assumes more than what is documented
 * here.
 */
export const market = {
  coins: (query: MarketCoinsQuery = {}, signal?: AbortSignal) =>
    get<MarketCoins>(`/v1/market/coins${marketCoinsSearch(query)}`, signal),
  token: (mint: string, signal?: AbortSignal) =>
    get<MarketToken>(`/v1/market/token/${encodeURIComponent(mint)}`, signal),
  candles: (
    mint: string,
    query: CandlesQuery,
    signal?: AbortSignal,
  ) => get<Candles>(`/v1/market/candles/${encodeURIComponent(mint)}${candlesSearch(query)}`, signal),
  trades: (mint: string, query: TradesQuery = {}, signal?: AbortSignal) =>
    get<Trades>(`/v1/market/trades/${encodeURIComponent(mint)}${tradesSearch(query)}`, signal),
  holders: (mint: string, query: HoldersQuery = {}, signal?: AbortSignal) =>
    get<Holders>(`/v1/market/holders/${encodeURIComponent(mint)}${holdersSearch(query)}`, signal),
};

/** One row of the live coin list. */
export interface MarketCoin {
  mint: string;
  symbol: string | null;
  name: string | null;
  price: number | null;
  price_reason: string | null;
  /** Percentage, not basis points -- this is a market figure, not a return. */
  change_pct: number | null;
  change_reason: string | null;
  /** Quote volume over the list's own window, whatever the server states it as. */
  volume: number | null;
  volume_reason: string | null;
  age_seconds: number | null;
  tx_count: number | null;
  tx_count_reason: string | null;
}

/** What sorts the coin list may be asked for. Purely a request hint: the
 *  list is re-sorted client-side regardless, so a value the server does not
 *  recognise degrades to "however it was returned" rather than an error. */
export type MarketSort = "volume" | "change" | "age" | "price" | "txns";

export interface MarketCoinsQuery {
  limit?: number | undefined;
  sort?: MarketSort | undefined;
}

function marketCoinsSearch(query: MarketCoinsQuery): string {
  const params = new URLSearchParams();
  if (query.limit !== undefined) params.set("limit", String(query.limit));
  if (query.sort !== undefined) params.set("sort", query.sort);
  const search = params.toString();
  return search ? `?${search}` : "";
}

export interface MarketCoins {
  as_of: number;
  coins: MarketCoin[];
  /** The `limit` actually applied, for the same row-cap reasoning as trades
   *  and holders below: a full page is not the same claim as a complete list. */
  limit: number;
}

/** Whether a mint's mint authority has been given up. `null` when Radar could
 *  not read the mint account, never a guess. See AGENTS.md rule 5: this latch
 *  only closes, so `"active"` here means "not observed as revoked", not "safe". */
export type MintAuthorityState = "revoked" | "active" | null;

/** The selected token's header: name, symbol, price, market cap, liquidity,
 *  age, creator -- and the info panel's contract-authority facts. */
export interface MarketToken {
  mint: string;
  name: string | null;
  symbol: string | null;
  decimals: number | null;
  price: number | null;
  price_reason: string | null;
  market_cap: number | null;
  market_cap_reason: string | null;
  liquidity: number | null;
  liquidity_reason: string | null;
  age_seconds: number | null;
  age_reason: string | null;
  creator: string | null;
  mint_authority: MintAuthorityState;
  mint_authority_reason: string | null;
}

export type CandleInterval = "1m" | "5m" | "15m" | "1h" | "4h" | "1d";

export const CANDLE_INTERVALS: readonly CandleInterval[] = [
  "1m",
  "5m",
  "15m",
  "1h",
  "4h",
  "1d",
];

/** One bar. `time` is Unix seconds, which is what `lightweight-charts` wants. */
export interface Candle {
  time: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export interface CandlesQuery {
  interval: CandleInterval;
  from?: number | undefined;
  to?: number | undefined;
}

function candlesSearch(query: CandlesQuery): string {
  const params = new URLSearchParams({ interval: query.interval });
  if (query.from !== undefined) params.set("from", String(query.from));
  if (query.to !== undefined) params.set("to", String(query.to));
  return `?${params.toString()}`;
}

/** OHLCV for one mint, one interval. */
export interface Candles {
  mint: string;
  interval: CandleInterval;
  /** The range actually covered -- which the contract says may be narrower
   *  than what was asked for, e.g. a token younger than the requested window. */
  from: number;
  to: number;
  candles: Candle[];
}

export type TradeSide = "buy" | "sell" | "unknown";

/** One row of the tape. */
export interface Trade {
  ts: number;
  signature: string;
  side: TradeSide;
  token_amount: number;
  quote_amount: number;
  quote_mint: string;
  price: number | null;
  trader: string;
}

export interface TradesQuery {
  limit?: number | undefined;
  before?: number | undefined;
}

function tradesSearch(query: TradesQuery): string {
  const params = new URLSearchParams();
  if (query.limit !== undefined) params.set("limit", String(query.limit));
  if (query.before !== undefined) params.set("before", String(query.before));
  const search = params.toString();
  return search ? `?${search}` : "";
}

/** The tape, newest first (the server's stated order; the tape does not
 *  re-sort, so if that order changes so does the screen). */
export interface Trades {
  mint: string;
  trades: Trade[];
}

export interface HoldersQuery {
  limit?: number | undefined;
}

function holdersSearch(query: HoldersQuery): string {
  const params = new URLSearchParams();
  if (query.limit !== undefined) params.set("limit", String(query.limit));
  const search = params.toString();
  return search ? `?${search}` : "";
}

/** One ranked holder. */
export interface Holder {
  address: string;
  amount: number;
  pct_of_supply: number | null;
}

/** The holders list, and -- load-bearing -- what kind of fact it is. */
export interface Holders {
  mint: string;
  holders: Holder[];
  /**
   * What the list was built from, in the server's own words -- e.g. "folded
   * from transfer history over the last 30 days", never a claim to have read
   * current account balances. Required precisely because it is the field
   * `honesty.ts`'s rules would let a component quietly drop for being ugly;
   * `holdersBasisCaption` refuses to drop it, and substitutes a visible
   * warning on the one response shape that omits it.
   */
  basis: string;
}
