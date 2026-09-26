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
    // The server sends `{"error": "...", "message": "..."}` for every failure
    // it authors -- `error` is a stable code (`"not_collected"`), `message`
    // is the human sentence explaining *which* not-collected this is (e.g.
    // an empty launch index versus a snapshot not yet built). `message` wins
    // when both are present: two different `Degradation::NotCollected`
    // responses share the same `error` code by design (`market/mod.rs`), so a
    // caller that needs to tell them apart -- `launchesEmptyMessage` in
    // `honesty.ts` does -- needs the sentence, not the code.
    let detail = response.statusText;
    try {
      const body = (await response.json()) as { error?: string; message?: string };
      if (body.message) detail = body.message;
      else if (body.error) detail = body.error;
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
 * `/health`'s shape, narrowed to the one field the interface reads:
 * whether trading is switched on. ADR 0024's second dark switch -- the first
 * is `legal.ts`'s `TERMS_APPROVED` -- and `TradePanel` renders only when
 * both agree. Everything else `/health` reports is operational detail this
 * customer-facing bundle has no reason to parse.
 */
export interface Health {
  trading: boolean;
}

/** Public, unauthenticated, and read on every load of a token page -- the
 *  panel's second dark switch (see [`Health`]). A failed read is treated as
 *  "not trading" by every caller, never as "assume it's on": the safe
 *  direction to be wrong in is the one that keeps the button hidden. */
export const health = (signal?: AbortSignal) => get<Health>("/health", signal);

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
  launches: (query: LaunchesQuery = {}, signal?: AbortSignal) =>
    get<Launches>(`/v1/market/launches${launchesSearch(query)}`, signal),
  // Public and identity-free like the rest of this object: the wallet is a
  // query parameter, not a session token. The tape is public chain data, and
  // asking about an address proves nothing about owning it -- which is why
  // this sends no bearer token and the server reads no customer store.
  history: (mint: string, query: HistoryQuery, signal?: AbortSignal) =>
    get<OwnTrades>(`/v1/market/history/${encodeURIComponent(mint)}${historySearch(query)}`, signal),
  /** Public and rate-limited per-IP (ADR 0024: "courtesy only, not a
   *  security boundary") -- a quote costs the caller nothing to ask for and
   *  commits nobody to anything. */
  quote: (query: QuoteQuery, signal?: AbortSignal) => quoteRequest(query, signal),
};

/**
 * A refusal from `/v1/market/quote` or `/v1/customer/swap` -- one class for
 * both, unlike `WatchlistError` / `PositionsError`'s separate classes,
 * because the two routes share the same refusal vocabulary: `busy`,
 * `trading_off`, `no_route`, `unreadable_route`, `bad_request` and
 * `slippage_too_wide` can come from either, and `swapRefusalMessage` in
 * `honesty.ts` reads the same `reason` code regardless of which route sent
 * it. Only `/v1/customer/swap` can additionally send a wallet-session
 * refusal (`no_session` and friends) -- `isWalletSessionRefusal` already
 * handles those without needing to know which route asked -- or the
 * swap-only `sanctioned` (the session wallet is on OFAC's SOL sanctions
 * list; quotes are unaffected).
 */
export class SwapError extends Error {
  constructor(
    readonly status: number,
    readonly reason: string,
    readonly detail: string,
  ) {
    super(`${status} ${reason}: ${detail}`);
    this.name = "SwapError";
  }
}

async function quoteRequest(query: QuoteQuery, signal?: AbortSignal): Promise<Quote> {
  const response = await fetch(`/v1/market/quote${quoteSearch(query)}`, {
    signal: signal ?? null,
    headers: { accept: "application/json" },
  });
  let body: { error?: string; reason?: string } | null = null;
  try {
    body = (await response.json()) as { error?: string; reason?: string };
  } catch {
    // A non-JSON body is itself informative: something upstream answered.
  }
  if (!response.ok) {
    throw new SwapError(response.status, body?.reason ?? "unknown", body?.error ?? response.statusText);
  }
  if (body === null) {
    throw new SwapError(response.status, "could_not_read", "the response body was not JSON");
  }
  return body as unknown as Quote;
}

/** Which side of a swap: paying SOL for the token, or paying the token for
 *  SOL. Its own type rather than reusing [`TradeSide`] -- that type's third
 *  member, `"unknown"`, describes a trade Radar could not attribute, which
 *  has no meaning for a swap the visitor is about to build. */
export type SwapSide = "buy" | "sell";

/**
 * `/v1/market/quote`'s answer, and the `quote` field `/v1/customer/swap`
 * echoes back inside its own response -- one shape, because the transaction
 * `/v1/customer/swap` builds is built from exactly this quote, and showing
 * the review step anything but these same numbers would show numbers the
 * transaction does not actually contain.
 *
 * Amounts are u64 base units as decimal strings, never floats, matching
 * every other on-chain amount this file carries (`PositionsToken.amount`).
 * `in_decimals` / `out_decimals` are nullable: a mint Radar's route has no
 * decimals for renders as raw base units with a label, never a guessed
 * conversion (`TradePanel.tsx`).
 */
export interface Quote {
  mint: string;
  side: SwapSide;
  in_mint: string;
  out_mint: string;
  in_amount: string;
  out_amount: string;
  /** The floor: what the trade receives if it fills at the worst price the
   *  slippage tolerance allows. The number this screen makes prominent --
   *  `out_amount` is an estimate, this is the guarantee. */
  worst_out: string;
  in_decimals: number | null;
  out_decimals: number | null;
  slippage_bps: number;
  /** Null when no venue in the route reports one -- not zero impact. */
  impact_bps: number | null;
  venues: string[];
  quoted_at: number;
}

export interface QuoteQuery {
  mint: string;
  side: SwapSide;
  amount: string;
  slippage_bps?: number;
}

function quoteSearch(query: QuoteQuery): string {
  const params = new URLSearchParams({
    mint: query.mint,
    side: query.side,
    amount: query.amount,
  });
  if (query.slippage_bps !== undefined) {
    params.set("slippage_bps", String(query.slippage_bps));
  }
  return `?${params.toString()}`;
}

/**
 * A refusal from `/v1/customer/watchlist*`.
 *
 * `tenant::refusal` sends `{"error": <sentence>, "reason": <code>}` -- the
 * mirror image of the `{error, message}` shape `get` above reads, and the
 * `reason` code is load-bearing here in a way `get`'s callers never needed:
 * the star button and the watchlist panel each switch on *which* refusal
 * this is (`full` beside the star, a session reason everywhere), not just on
 * whether one happened. Carrying the code as its own field, rather than
 * parsing it back out of a sentence, is the same reasoning `OwnTrade` gives
 * for not re-deriving `matched_by` from prose.
 */
export class WatchlistError extends Error {
  constructor(
    readonly status: number,
    readonly reason: string,
    readonly detail: string,
  ) {
    super(`${status} ${reason}: ${detail}`);
    this.name = "WatchlistError";
  }
}

/** `/v1/customer/watchlist`'s shape, exactly: the wallet it belongs to, its
 *  coins, and the ceiling `tenant::WATCHLIST_LIMIT` enforces. */
export interface Watchlist {
  wallet: string;
  coins: string[];
  limit: number;
}

async function watchlistRequest(
  method: "GET" | "PUT" | "DELETE",
  path: string,
  token: string,
  signal?: AbortSignal,
): Promise<Watchlist> {
  const response = await fetch(path, {
    method,
    signal: signal ?? null,
    headers: { authorization: `Bearer ${token}`, accept: "application/json" },
  });
  let body: { error?: string; reason?: string } | null = null;
  try {
    body = (await response.json()) as { error?: string; reason?: string };
  } catch {
    // A non-JSON body is itself informative: something upstream answered.
  }
  if (!response.ok) {
    throw new WatchlistError(
      response.status,
      body?.reason ?? "unknown",
      body?.error ?? response.statusText,
    );
  }
  return body as unknown as Watchlist;
}

/**
 * The signed-in wallet's own watchlist: `/v1/customer/watchlist`.
 *
 * **Never a query string.** `tenant::with_store` refuses one on sight rather
 * than reading it as a wallet address to look up -- the wallet these
 * functions read is always the bearer token, never a parameter, and adding
 * one here would ask for a refusal these functions have no way to recover
 * from silently.
 */
/**
 * A refusal from `/v1/customer/positions` -- the same `{error, reason}` shape
 * [`WatchlistError`] reads, from the same [`crate::tenant::refusal`] helper
 * on the server, so this is a second class rather than a shared one only
 * because the two routes' reason codes do not entirely overlap (`busy` and
 * `unreadable_chain` are this route's own, `full` is the watchlist's).
 */
export class PositionsError extends Error {
  constructor(
    readonly status: number,
    readonly reason: string,
    readonly detail: string,
  ) {
    super(`${status} ${reason}: ${detail}`);
    this.name = "PositionsError";
  }
}

/** One held mint, exactly as `/v1/customer/positions` sends it under `tokens`.
 *  `amount` is the raw integer, as a string -- never passed through a float
 *  on the wire -- and `ui_amount` is the `decimals`-adjusted string the
 *  server already computed. */
export interface PositionsToken {
  mint: string;
  /** `"token"` or `"token-2022"`, whichever program the account belongs to. */
  program: string;
  amount: string;
  decimals: number;
  ui_amount: string;
  /** In `quote`'s asset, never dollars -- `MarketTrade.price` is
   *  `quote_amount / token_amount`, and the quote asset is wSOL, USDC or
   *  USDT, never USD. Null exactly when `priced` is false -- a mint Radar
   *  does not track, never a price of zero. */
  price: number | null;
  /** Which asset `price` and `value` are denominated in. Null exactly when
   *  `price` is. */
  quote: "SOL" | "USDC" | "USDT" | null;
  value: number | null;
  priced: boolean;
}

/** The wallet's native SOL balance, in the same shape as one [`PositionsToken`]. */
export interface PositionsSol {
  lamports: number;
  ui_amount: string;
  price: number | null;
  quote: "SOL" | "USDC" | "USDT" | null;
  value: number | null;
  priced: boolean;
  /** Set exactly when `priced` is false: SOL is priced only off a wSOL trade
   *  quoted in USDC or USDT, and this is why none was found. */
  price_reason: string | null;
}

/**
 * `/v1/customer/positions`'s shape, exactly: the wallet it belongs to, the
 * slot its balances were read at, when that read happened, and how long ago
 * that was. `slot` and `read_at` stay fixed while an answer is served from
 * the server's own 30-second cache -- `age_seconds` is the field that grows.
 */
export interface Positions {
  wallet: string;
  slot: number;
  read_at: number;
  age_seconds: number;
  sol: PositionsSol;
  tokens: PositionsToken[];
}

async function positionsRequest(token: string, signal?: AbortSignal): Promise<Positions> {
  const response = await fetch("/v1/customer/positions", {
    signal: signal ?? null,
    headers: { authorization: `Bearer ${token}`, accept: "application/json" },
  });
  let body: { error?: string; reason?: string } | null = null;
  try {
    body = (await response.json()) as { error?: string; reason?: string };
  } catch {
    // A non-JSON body is itself informative: something upstream answered.
  }
  if (!response.ok) {
    throw new PositionsError(
      response.status,
      body?.reason ?? "unknown",
      body?.error ?? response.statusText,
    );
  }
  if (body === null) {
    // A 2xx with a body that did not parse as JSON is not an empty-but-valid
    // answer -- it is a read that failed silently. Casting `null` to
    // `Positions` would hand every caller a wallet with no `tokens` array,
    // indistinguishable from "this wallet holds nothing," which is exactly
    // the wrong answer to a read that did not actually happen.
    throw new PositionsError(response.status, "could_not_read", "the response body was not JSON");
  }
  return body as unknown as Positions;
}

export interface SwapRequest {
  mint: string;
  side: SwapSide;
  amount: string;
  slippage_bps?: number;
}

/**
 * `/v1/customer/swap`'s answer: an unsigned, base64-encoded v0
 * `VersionedTransaction` whose fee payer is the signed-in wallet, built from
 * exactly the nested `quote` -- never a promise that it will land, and never
 * signed or sent by anything on this server (ADR 0024). `sign.ts` is the one
 * place that turns `transaction` into bytes and hands them to a wallet.
 */
export interface SwapResponse {
  transaction: string;
  last_valid_block_height: number;
  quote: Quote;
}

async function swapRequest(token: string, body: SwapRequest): Promise<SwapResponse> {
  const response = await fetch("/v1/customer/swap", {
    method: "POST",
    headers: {
      authorization: `Bearer ${token}`,
      "content-type": "application/json",
      accept: "application/json",
    },
    body: JSON.stringify(body),
  });
  let parsed: { error?: string; reason?: string } | null = null;
  try {
    parsed = (await response.json()) as { error?: string; reason?: string };
  } catch {
    // A non-JSON body is itself informative: something upstream answered.
  }
  if (!response.ok) {
    throw new SwapError(
      response.status,
      parsed?.reason ?? "unknown",
      parsed?.error ?? response.statusText,
    );
  }
  if (parsed === null) {
    // Mirrors `positionsRequest`'s defensive branch: a 2xx with a body that
    // did not parse as JSON is a read that failed silently, not a swap with
    // no transaction in it.
    throw new SwapError(response.status, "could_not_read", "the response body was not JSON");
  }
  return parsed as unknown as SwapResponse;
}

/**
 * `GET /v1/customer/tx/{signature}`'s answer: whether a transaction the
 * signed-in wallet already sent and signed itself has landed. This route
 * never signs or sends anything (ADR 0024, same as `swap`) -- it only reads
 * what the chain now says about a signature the wallet already produced.
 *
 * `"pending"` is also this shape's answer to "not sure yet" -- a caller
 * treats every state that is not `"landed"` as "do not tell the customer
 * this succeeded" (`TradePanel.tsx`).
 */
export interface TxStatus {
  state: "pending" | "landed" | "failed" | "expired";
  /** Set only when `state` is `"failed"` -- a short, plain rendering of the
   *  on-chain error, never the raw JSON `TransactionError` shape. */
  reason?: string;
  /** Set only when `state` is `"landed"`. */
  slot?: number;
}

async function txStatusRequest(
  token: string,
  signature: string,
  lastValidBlockHeight: number,
  signal?: AbortSignal,
): Promise<TxStatus> {
  const params = new URLSearchParams({ last_valid_block_height: String(lastValidBlockHeight) });
  const response = await fetch(
    `/v1/customer/tx/${encodeURIComponent(signature)}?${params.toString()}`,
    {
      signal: signal ?? null,
      headers: { authorization: `Bearer ${token}`, accept: "application/json" },
    },
  );
  let parsed: { error?: string; reason?: string; state?: string } | null = null;
  try {
    parsed = (await response.json()) as { error?: string; reason?: string; state?: string };
  } catch {
    // A non-JSON body is itself informative: something upstream answered.
  }
  if (!response.ok) {
    throw new SwapError(
      response.status,
      parsed?.reason ?? "unknown",
      parsed?.error ?? response.statusText,
    );
  }
  if (parsed === null || parsed.state === undefined) {
    // Mirrors `swapRequest`'s defensive branch: a 2xx with no readable
    // `state` is a read that failed silently, never treated as landed.
    throw new SwapError(response.status, "could_not_read", "the response body was not JSON");
  }
  return parsed as unknown as TxStatus;
}

export const customer = {
  watchlist: {
    list: (token: string, signal?: AbortSignal) =>
      watchlistRequest("GET", "/v1/customer/watchlist", token, signal),
    watch: (token: string, mint: string) =>
      watchlistRequest("PUT", `/v1/customer/watchlist/${encodeURIComponent(mint)}`, token),
    unwatch: (token: string, mint: string) =>
      watchlistRequest("DELETE", `/v1/customer/watchlist/${encodeURIComponent(mint)}`, token),
  },
  positions: {
    /** **Never a query string** -- see `TokenHeader.tsx`'s `PositionsPanel` /
     *  `positions.rs`'s own note: the wallet this reads is always the bearer
     *  token's, and the server refuses `unscoped` rather than reading one as
     *  a parameter. */
    get: (token: string, signal?: AbortSignal) => positionsRequest(token, signal),
  },
  /** Builds an unsigned swap transaction for the signed-in wallet to review
   *  and sign itself. Never a query string, for the same reason as
   *  `positions.get` -- the wallet is always the bearer token's. */
  swap: (token: string, body: SwapRequest) => swapRequest(token, body),
  /** Whether a transaction the signed-in wallet already signed and sent has
   *  landed. Polled by `TradePanel.tsx` after `sign.ts` returns a
   *  signature. */
  txStatus: (
    token: string,
    signature: string,
    lastValidBlockHeight: number,
    signal?: AbortSignal,
  ) => txStatusRequest(token, signature, lastValidBlockHeight, signal),
};

/**
 * One row of the live coin list, exactly as `/v1/market/coins` sends it.
 *
 * **Rewritten 2026-09-11 to match the server rather than a guess at it.** The
 * previous declaration was written in parallel with the endpoint and against a
 * prose description of it, and named eight fields the server does not send --
 * `symbol`, `name`, `age_seconds`, `volume`, and a `_reason` for four of them --
 * while ignoring the two it does, `quote_volume` and `quote_mint`. Every one of
 * those read back `undefined`, went through a number formatter, and rendered as
 * `NaN` in a column header that promised a figure. A confident `NaN` is the
 * failure this codebase exists to prevent, and TypeScript could not catch it:
 * the type was internally consistent and simply described a different server.
 *
 * What is genuinely absent stays absent. The coins endpoint does not fetch
 * metadata -- that is one query per mint against an endpoint with a hundred and
 * twenty queries an hour -- so a row has no name or symbol, and the list shows
 * the mint. It does not compute age either. Neither gets an invented field here.
 */
export interface MarketCoin {
  mint: string;
  /** Distinct transactions moving this mint in the window. Never null. */
  tx_count: number;
  /** Summed mint amount, `decimals`-adjusted. Null when the server could not
   *  adjust it, never zero. */
  token_volume: number | null;
  /** Which quote asset the price is denominated in. Null exactly when
   *  `quote_volume` and `price` are. */
  quote_mint: string | null;
  /** Summed quote amount over the window. */
  quote_volume: number | null;
  /** Last priced fill in the window, in `quote_mint`. Null when no trade in
   *  the window carried both legs. */
  price: number | null;
  /** Percentage, not basis points -- this is a market figure, not a return.
   *  Null when the window held fewer than two priced fills. */
  change_pct: number | null;
  /** Creator-supplied, untrusted. Sent only when Radar recorded this mint's
   *  own pump.fun launch; null otherwise, never a guess. */
  name?: string | null;
  /** Creator-supplied, untrusted. Same rules as `name`. */
  symbol?: string | null;
  /** Creator-supplied, untrusted. An off-chain metadata document the browser
   *  may fetch for an image -- never the server. Same rules as `name`. */
  uri?: string | null;
}

/** What sorts the coin list may be asked for. Purely a request hint: the
 *  list is re-sorted client-side regardless, so a value the server does not
 *  recognise degrades to "however it was returned" rather than an error. */
export type MarketSort = "volume" | "change" | "price" | "txns";

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

/** The window a market answer covers, in the server's own `YYYY-MM-DD HH:MM:SS`. */
export interface MarketWindow {
  from: string;
  to: string;
}

export interface MarketCoins {
  coins: MarketCoin[];
  /**
   * The window the list ranks activity over.
   *
   * Load-bearing, not decoration: "the busiest coins" is meaningless without
   * saying busiest over what, and the server states it rather than leaving the
   * screen to imply one.
   */
  window: MarketWindow;
}


/** The selected token's header: name, symbol, price, market cap, liquidity,
 *  age, creator -- and the info panel's contract-authority facts. */
/**
 * One coin's header, exactly as `/v1/market/token/{mint}` sends it.
 *
 * Matched to the server on 2026-09-11, for the reason on [`MarketCoin`]: the
 * previous declaration named `decimals`, `age_seconds`, `age_reason`,
 * `mint_authority` and `mint_authority_reason`, none of which the endpoint
 * sends, and missed `published_at`, `decimals_reason` and `metadata_reason`,
 * which it does. `age_seconds` rendering as `NaNd old` in the header was that
 * mismatch, visible.
 *
 * The `_reason` fields are not optional decoration. Every one of them pairs
 * with a value that is `null` **for a stated cause** — this endpoint performs
 * no live account read and no unbounded scan, so supply and pool reserves are
 * genuinely out of reach, and saying which is the difference between an
 * unmeasured figure and a missing one.
 */
export interface MarketToken {
  mint: string;
  name: string | null;
  symbol: string | null;
  creator: string | null;
  /** Creator-supplied, untrusted. An off-chain metadata document the browser
   *  may fetch for an image -- never the server. Null with `metadata_reason`
   *  when Radar recorded no launch for this mint. */
  uri: string | null;
  /** When `solana.tokens` first carried this mint. Null with a
   *  `metadata_reason` for a token too young or too obscure to be indexed. */
  published_at: string | null;
  /** Why `name`, `symbol`, `uri`, `creator` and `published_at` are all null,
   *  when they are. Null when metadata was found. */
  metadata_reason: string | null;
  price: number | null;
  price_reason: string | null;
  /** Why the header carries no decimals. Always present: decimals travel
   *  per-trade on the tape, not on the header. */
  decimals_reason: string | null;
  /** Always null today, with `market_cap_reason` saying why. */
  market_cap: number | null;
  market_cap_reason: string | null;
  /** Always null today, with `liquidity_reason` saying why. */
  liquidity: number | null;
  liquidity_reason: string | null;
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
  /**
   * The range actually covered, as the server's `YYYY-MM-DD HH:MM:SS` UTC
   * stamps -- **text, not epoch seconds**. It may be narrower than what was
   * asked for: a coin younger than the window, or one the collector has not
   * reached. Parse with `format.parseStamp`, never `new Date(x * 1000)`,
   * which produced the literal words "Invalid Date" under a TIME header until
   * 2026-09-12.
   */
  covered: MarketWindow & { complete: boolean };
  /** The range the caller asked for, echoed back. */
  requested: MarketWindow;
  candles: Candle[];
}

export type TradeSide = "buy" | "sell" | "unknown";

/** One row of the tape. */
export interface Trade {
  /** `YYYY-MM-DD HH:MM:SS.ffffff` UTC. Text, not epoch seconds -- see
   *  `Candles.covered`. */
  ts: string;
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

/** One ranked holder, as `/v1/market/holders/{mint}` sends it. */
export interface Holder {
  /** A token account or a wallet, as `Holders.granularity` says. */
  account: string;
  /** The balance, `decimals`-adjusted. */
  balance: number;
  /** Whether this holder acted as a pool in a trade the live feed saw. Only
   *  the live feed sends it. */
  pool?: boolean;
}

/** The holders list, and -- load-bearing -- what kind of fact it is. */
export interface Holders {
  /** The server nests the whole answer under this key. */
  fold: HoldersFold;
}

/** What `/v1/market/holders/{mint}` actually returns under `fold`. */
export interface HoldersFold {
  holders: Holder[];
  /**
   * What the list was built from, in the server's own words -- today
   * `"folded_transfers"`, never a claim to have read current account balances.
   */
  fact: string;
  /**
   * Whether a row is a token account or an owner. `"token_account"` **over-
   * counts holders**, because one person can hold the same mint in several
   * accounts, and the screen must say so rather than print a crowd size it
   * did not measure.
   */
  granularity: string;
  /** The window the fold covers. Not "since launch": a coin older than this
   *  has balances this fold cannot see, and that is a different sentence from
   *  "this coin has no holders". */
  from: string;
  to: string;
  /**
   * How many trades of this mint the fold saw but could not attribute to a
   * wallet -- no identified trader, or an unknown side. Only the free path's
   * `"net_traded_in_window"` fact sends it, since it is the only fold that
   * skips rows rather than reading a balance directly.
   */
  unattributed_trades?: number;
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

export interface LaunchesQuery {
  limit?: number | undefined;
}

function launchesSearch(query: LaunchesQuery): string {
  const params = new URLSearchParams();
  if (query.limit !== undefined) params.set("limit", String(query.limit));
  const search = params.toString();
  return search ? `?${search}` : "";
}

/**
 * One launch, as `/v1/market/launches` sends it -- the coins Radar's own
 * collector recorded the launch of, not every launch on Solana.
 */
export interface MarketLaunch {
  mint: string;
  /** Creator-supplied, untrusted. Capped server-side; never fetched by
   *  either the server or this client. */
  name: string;
  /** Creator-supplied, untrusted. Same rules as `name`. */
  symbol: string;
  /** Creator-supplied, untrusted. An off-chain metadata document the browser
   *  may fetch for an image -- never the server, and this client does not
   *  fetch it either. */
  uri: string;
  /** The slot Radar recorded the launch at. There is no wall-clock timestamp
   *  on a launch record, only a slot -- so ranking and the window below are
   *  both in slots, not the `YYYY-MM-DD HH:MM:SS` window the trade-backed
   *  routes send. */
  slot: number;
}

/** The window `/v1/market/launches` covers, in slots -- see `MarketLaunch.slot`. */
export interface LaunchesWindow {
  from_slot: number;
  to_slot: number;
}

export interface Launches {
  launches: MarketLaunch[];
  window: LaunchesWindow;
  /**
   * Always `false`. The launch index only reaches
   * `LAUNCH_LOOKBACK_SLOTS` (roughly a week) back from the watermark, so this
   * list is never the whole history of a coin's launch -- only ever "what
   * Radar recorded recently".
   */
  complete: boolean;
}

export interface HistoryQuery {
  wallet: string;
  limit?: number | undefined;
}

function historySearch(query: HistoryQuery): string {
  const params = new URLSearchParams();
  params.set("wallet", query.wallet);
  if (query.limit !== undefined) params.set("limit", String(query.limit));
  return `?${params.toString()}`;
}

/** How a stored trade was tied to the wallet that asked for it. */
export type MatchedBy = "trader" | "receiving_account";

/**
 * One of the reader's own trades, as `/v1/market/history/{mint}` sends it.
 *
 * **No `trader` field, deliberately.** The row is already known to be this
 * wallet's, and on four buys in five the tape does not name a trader at all
 * -- `matched_by` says which happened, and a `trader` column here would be
 * empty on most buys for reasons the screen could not explain.
 */
export interface OwnTrade {
  ts: string;
  slot: number;
  signature: string;
  side: TradeSide;
  token_amount: number;
  quote_amount: number | null;
  price: number | null;
  matched_by: MatchedBy;
}

/** What `/v1/market/history/{mint}` returns under `fold`. */
export interface OwnTradesFold {
  fact: "wallet_trades_in_window";
  /** Always `false`. The server states plainly that this can never be all of
   *  a wallet's trades, and the screen repeats it rather than implying
   *  otherwise by staying quiet. */
  complete: boolean;
  /** More of this wallet's trades were found than `limit` returned. */
  truncated: boolean;
  /** Trades of this coin naming neither a wallet nor a receiving account, so
   *  they could belong to anyone -- including this reader. Load-bearing: zero
   *  rows with a non-zero count here is not "you made no trades". */
  unattributable_trades: number;
  trades: OwnTrade[];
}

export interface OwnTrades {
  mint: string;
  wallet: string;
  fold: OwnTradesFold;
  /** The server's own sentence about what this list cannot include. Rendered,
   *  not summarised. */
  caveat: string;
}
