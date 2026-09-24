// SPDX-License-Identifier: Apache-2.0
//! The right rail: the selected coin's header, the signed-in wallet's
//! watchlist, and the space reserved for Radar's own signals.
//!
//! The signals panel is deliberately empty. The packet is explicit that this
//! is "later, not now" and that the placeholder must say what will go there,
//! never draw a fake chart to fill the space -- a confident wrong number is
//! the one failure this whole product exists to prevent, and an invented
//! signal would be exactly that.
//!
//! The watchlist star (beside the coin's name) and the watchlist list (below
//! the header) share one `useWatchlist` read, so toggling the star updates
//! the list underneath it without a second request.

import type { ReactNode } from "react";
import { Link } from "wouter";
import type { MarketToken } from "./api";
import { CoinImage } from "./CoinImage";
import { MarketFigure } from "./Figures";
import {formatAge, formatCompactUsd, formatPrice, shortenAddress} from "./format";
import { isWalletSessionRefusal, positionsMessage, watchlistMessage, watchlistToggleFailure } from "./honesty";
import { tokenPath } from "./routes";
import type { Load } from "./useApi";
import { usePositions } from "./usePositions";
import { useWatchlist } from "./useWatchlist";

export function TokenHeader({ load }: { load: Load<MarketToken> }) {
  const watchlist = useWatchlist();
  const positions = usePositions();
  return (
    <aside className="flex h-full flex-col border-l border-[var(--color-line)] bg-[var(--color-surface)]">
      <div className="border-b border-[var(--color-line)] p-3">
        <Header load={load} watchlist={watchlist} />
      </div>
      <div className="border-b border-[var(--color-line)] p-3">
        <WatchlistPanel watchlist={watchlist} />
      </div>
      <div className="border-b border-[var(--color-line)] p-3">
        <PositionsPanel positions={positions} />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        <SignalsPlaceholder />
      </div>
    </aside>
  );
}

function Header({
  load,
  watchlist,
}: {
  load: Load<MarketToken>;
  watchlist: ReturnType<typeof useWatchlist>;
}) {
  if (load.state === "loading") {
    return <p className="text-xs text-[var(--color-dim)]">Reading token…</p>;
  }
  if (load.state === "failed") {
    return (
      <p className="text-xs text-[var(--color-warn)]">
        Could not read this token: {load.detail}.
      </p>
    );
  }

  const token = load.value;

  return (
    <div>
      <div className="flex items-baseline gap-2">
        <CoinImage uri={token.uri} symbol={token.symbol} className="h-8 w-8 shrink-0 rounded-full" />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline justify-between gap-2">
            <div className="flex min-w-0 items-baseline gap-1.5">
              <h1 className="truncate text-lg font-semibold">
                {/* The symbol when the server knows it -- Radar's own recorded
                    pump.fun launch -- and the abbreviated mint otherwise, the
                    same fallback the coin list uses, rather than the word
                    "unknown" standing in for a name nobody invented. */}
                {token.symbol ?? shortenAddress(token.mint)}
              </h1>
              <WatchlistStar mint={token.mint} watchlist={watchlist} />
            </div>
            {/* First seen, not age. The endpoint reports when `solana.tokens`
                first carried the mint, which is an indexing date and not a launch
                time -- so the header says "first seen" rather than converting it
                into an age the server never claimed. */}
            <span className="text-xs text-[var(--color-dim)]" title="When this mint first appeared in the chain index. Not necessarily its launch.">
              {token.published_at === null
                ? "first seen unknown"
                : `first seen ${token.published_at.slice(0, 10)}`}
            </span>
          </div>
          <p className="truncate text-xs text-[var(--color-dim)]">
            {token.name ?? shortenAddress(token.mint)}
          </p>
        </div>
      </div>

      <p className="mt-3 text-2xl font-semibold tabular-nums">
        <MarketFigure value={token.price} reason={token.price_reason} format={formatPrice} />
      </p>

      <dl className="mt-3 grid grid-cols-2 gap-x-3 gap-y-2 text-xs">
        <Field label="Market cap">
          <MarketFigure value={token.market_cap} reason={token.market_cap_reason} format={formatCompactUsd} />
        </Field>
        <Field label="Liquidity">
          <MarketFigure value={token.liquidity} reason={token.liquidity_reason} format={formatCompactUsd} />
        </Field>
        <Field label="24h change">
          {/* The contract does not put a 24h change on the token header --
              only the coin list carries `change_pct`. Shown here only when a
              caller has it; nothing here invents one to fill the cell. */}
          <span className="text-[var(--color-absent)]" title="Not part of this endpoint's contract">
            see coin list
          </span>
        </Field>
        <Field label="Decimals">
          <span className="text-[var(--color-absent)]" title={token.decimals_reason ?? undefined}>
            per trade
          </span>
        </Field>
      </dl>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <dt className="text-[10px] uppercase tracking-wide text-[var(--color-dim)]">{label}</dt>
      <dd className="tabular-nums">{children}</dd>
    </div>
  );
}

/**
 * The star beside the coin's name: filled when this mint is on the
 * signed-in wallet's watchlist, toggled by clicking.
 *
 * Never a silent no-op. With no wallet connected the star still renders --
 * disabled, but its label says why a click does nothing, rather than looking
 * like a button that is simply broken. `watchlist.load` "loading" or "failed"
 * gets the same treatment: a star cannot say whether it is filled without
 * having read the list, so it disables rather than guessing empty.
 */
function WatchlistStar({
  mint,
  watchlist,
}: {
  mint: string;
  watchlist: ReturnType<typeof useWatchlist>;
}) {
  const { load, toggleError, toggle, pending } = watchlist;

  if (load.state === "signed-out") {
    return (
      <button
        type="button"
        disabled
        title={watchlistMessage({ kind: "signed-out" })}
        aria-label={watchlistMessage({ kind: "signed-out" })}
        className="text-[var(--color-dim)] opacity-60"
      >
        ☆
      </button>
    );
  }

  if (load.state !== "ready") {
    const title =
      load.state === "loading"
        ? "Reading your watchlist…"
        : watchlistMessage(
            isWalletSessionRefusal(load.reason)
              ? { kind: "session-refused" }
              : { kind: "could-not-look", detail: load.detail },
          );
    return (
      <button
        type="button"
        disabled
        title={title}
        aria-label={title}
        className="text-[var(--color-dim)] opacity-60"
      >
        ☆
      </button>
    );
  }

  const watching = load.value.coins.includes(mint);
  const label = watching ? "Remove from your watchlist" : "Add to your watchlist";

  return (
    <span className="inline-flex items-baseline gap-1">
      <button
        type="button"
        onClick={() => void toggle(mint)}
        disabled={pending}
        title={label}
        aria-label={label}
        aria-pressed={watching}
        className={watching ? "text-[var(--color-warn)]" : "text-[var(--color-dim)] hover:text-[var(--color-text)]"}
      >
        {watching ? "★" : "☆"}
      </button>
      {toggleError && (
        <span className="text-[10px] text-[var(--color-warn)]">
          {watchlistToggleFailure(toggleError.reason, toggleError.detail)}
        </span>
      )}
    </span>
  );
}

/**
 * The right rail's watchlist: the coins the signed-in wallet keeps an eye
 * on, each linking to that coin the way `CoinList` does.
 *
 * Rule 9 on this screen too: a reader watching an empty panel is owed which
 * of three facts made it empty -- not signed in, signed in with nothing
 * saved, or Radar could not read the list at all.
 */
function WatchlistPanel({ watchlist }: { watchlist: ReturnType<typeof useWatchlist> }) {
  const { load } = watchlist;

  if (load.state === "loading") {
    return <p className="text-xs text-[var(--color-dim)]">Reading your watchlist…</p>;
  }

  if (load.state === "signed-out") {
    return <p className="text-xs text-[var(--color-dim)]">{watchlistMessage({ kind: "signed-out" })}</p>;
  }

  if (load.state === "failed") {
    const message = isWalletSessionRefusal(load.reason)
      ? watchlistMessage({ kind: "session-refused" })
      : watchlistMessage({ kind: "could-not-look", detail: load.detail });
    return <p className="text-xs text-[var(--color-warn)]">{message}</p>;
  }

  const { coins } = load.value;

  return (
    <div>
      <h2 className="mb-2 text-[10px] font-medium uppercase tracking-wide text-[var(--color-dim)]">
        Watchlist
      </h2>
      {coins.length === 0 ? (
        <p className="text-xs text-[var(--color-dim)]">{watchlistMessage({ kind: "empty" })}</p>
      ) : (
        <ul className="space-y-1">
          {coins.map((coin) => (
            <li key={coin}>
              <Link
                href={tokenPath(coin)}
                className="block truncate font-mono text-xs text-[var(--color-text)] hover:text-[var(--color-dim)]"
              >
                {shortenAddress(coin)}
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/**
 * The right rail's positions panel: what the signed-in wallet actually holds
 * on chain right now, read and priced by Radar's own server -- never by the
 * browser talking to Solana directly (see `positions.rs`'s module doc for
 * why: the public RPC node refuses any request that carries a browser
 * Origin header).
 *
 * Rule 9 again: `busy` (Radar is rate-limiting reads) and `could-not-look`
 * (the chain read itself failed) are both "no holdings shown", but one says
 * "wait" and the other says "this is not evidence of what the wallet holds"
 * -- collapsing them into one sentence would lose that difference.
 */
function PositionsPanel({ positions }: { positions: ReturnType<typeof usePositions> }) {
  if (positions.state === "loading") {
    return <p className="text-xs text-[var(--color-dim)]">Reading this wallet's holdings…</p>;
  }

  if (positions.state === "signed-out") {
    return <p className="text-xs text-[var(--color-dim)]">{positionsMessage({ kind: "signed-out" })}</p>;
  }

  if (positions.state === "failed") {
    const message = isWalletSessionRefusal(positions.reason)
      ? positionsMessage({ kind: "session-refused" })
      : positions.reason === "busy"
        ? positionsMessage({ kind: "busy" })
        : positionsMessage({ kind: "could-not-look", detail: positions.detail });
    return <p className="text-xs text-[var(--color-warn)]">{message}</p>;
  }

  const { sol, tokens, age_seconds } = positions.value;

  return (
    <div>
      <div className="mb-2 flex items-baseline justify-between">
        <h2 className="text-[10px] font-medium uppercase tracking-wide text-[var(--color-dim)]">
          Holdings
        </h2>
        <span className="text-[10px] text-[var(--color-dim)]">as of {formatAge(age_seconds)} ago</span>
      </div>
      {tokens.length === 0 ? (
        <p className="text-xs text-[var(--color-dim)]">
          {positionsMessage({
            kind: "empty",
            solUiAmount: sol.ui_amount,
          })}
        </p>
      ) : (
        <ul className="space-y-1">
          {tokens.map((token) => (
            <li key={token.mint} className="flex items-baseline justify-between gap-2 text-xs">
              <Link
                href={tokenPath(token.mint)}
                className="truncate font-mono text-[var(--color-text)] hover:text-[var(--color-dim)]"
              >
                {shortenAddress(token.mint)}
              </Link>
              <span className="shrink-0 tabular-nums text-[var(--color-dim)]">
                <span>{token.ui_amount}</span>
                {" · "}
                <span>
                  {token.priced && token.value_usd !== null
                    ? formatCompactUsd(token.value_usd)
                    : "Radar does not price this coin"}
                </span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function SignalsPlaceholder() {
  return (
    <div className="rounded-md border border-dashed border-[var(--color-edge)] p-4 text-xs text-[var(--color-dim)]">
      <p className="font-medium text-[var(--color-text)]">Radar&rsquo;s signals</p>
      <p className="mt-2">
        Not live yet. When they are, this panel will show what Radar decided
        about this token and why — the same reason list the decision record
        always carried, not a second price chart.
      </p>
      <p className="mt-2">
        This space is reserved rather than filled, because a confident wrong
        number here is worse than an honest blank one.
      </p>
    </div>
  );
}
