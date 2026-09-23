// SPDX-License-Identifier: Apache-2.0
//! "Your trades in this coin" -- plan 0013, phase C item 4, the private
//! column's trade half.
//!
//! **This panel exists to be honest about what it cannot show.** The tape
//! names the trader on one trade in five (research 0037): every sell, and
//! only those buys paid in wrapped SOL. The server recovers most of the rest
//! by deriving this wallet's own token account and matching buys against it,
//! and counts whatever is left over in `unattributable_trades`. Built without
//! that count on screen, a missing trade would render exactly like a trade
//! never made -- which is the one outcome worth building this panel to avoid.
//!
//! Every empty state here is a different sentence, because every empty state
//! is a different fact: not signed in, no trades of yours, no trades of this
//! coin at all, Radar could not look, and Radar could not be reached.
//! `yourTradesMessage` in `honesty.ts` holds the words and is tested on its
//! own.

import { market, type OwnTrade } from "./api";
import { MarketFigure, Side } from "./Figures";
import { yourTradesMessage, yourTradesRefusal } from "./honesty";
import { formatCompactNumber, formatPrice, formatStampTime, transactionUrl } from "./format";
import { useApi } from "./useApi";
import { useWalletAddress } from "./Wallet";

/** How many of the reader's trades to ask for. The server caps at 500. */
const LIMIT = 100;

export function YourTradesPanel({ mint }: { mint: string }) {
  const wallet = useWalletAddress();

  // No wallet, no request. Asking with an empty address would be a refusal the
  // reader did not cause, reported as though Radar had failed.
  const load = useApi(
    (signal) =>
      wallet
        ? market.history(mint, { wallet, limit: LIMIT }, signal)
        : new Promise<never>(() => {}),
    [mint, wallet],
  );

  if (!wallet) {
    return <Empty>{yourTradesMessage({ kind: "signed-out" })}</Empty>;
  }

  if (load.state === "loading") {
    return <p className="p-3 text-xs text-[var(--color-dim)]">Reading your trades…</p>;
  }

  if (load.state === "failed") {
    // 503 is the server declining to answer and saying which of its own
    // reasons applies; anything else is the connection, and the two must not
    // borrow each other's words.
    const why =
      load.status === 503
        ? yourTradesRefusal(load.detail)
        : ({ kind: "unreachable", detail: load.detail } as const);
    return <Empty tone="warn">{yourTradesMessage(why)}</Empty>;
  }

  const { trades, unattributable_trades: unattributable, truncated } = load.value.fold;

  return (
    <div className="flex h-full flex-col">
      {trades.length === 0 ? (
        <Empty>{yourTradesMessage({ kind: "none-of-yours", unattributable })}</Empty>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <table className="w-full text-xs">
            <thead className="sticky top-0 bg-[var(--color-surface)] text-[10px] uppercase tracking-wide text-[var(--color-dim)]">
              <tr>
                <th scope="col" className="py-1 pl-3 text-left font-medium">Time</th>
                <th scope="col" className="py-1 text-left font-medium">Side</th>
                <th scope="col" className="py-1 text-right font-medium">Amount</th>
                <th scope="col" className="py-1 text-right font-medium">Price</th>
                <th scope="col" className="py-1 pr-3 text-right font-medium">Matched</th>
              </tr>
            </thead>
            <tbody>
              {trades.map((trade) => (
                <Row key={trade.signature} trade={trade} />
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* The footer is the panel, as far as honesty goes. It renders under a
          full list and under an empty one, because "these are all the trades
          Radar could tie to you" is the same claim either way. */}
      <div className="border-t border-[var(--color-line)] px-3 py-1.5 text-[10px] leading-relaxed text-[var(--color-dim)]">
        {unattributable > 0 && trades.length > 0 && (
          <p className="text-[var(--color-warn)]">
            {unattributable === 1
              ? "1 trade of this coin names nobody, so it could be yours and is not listed."
              : `${unattributable} trades of this coin name nobody, so any of them could be yours and none are listed.`}
          </p>
        )}
        {truncated && (
          <p>Showing your {LIMIT} most recent. There are more than this panel asked for.</p>
        )}
        <p>{load.value.caveat}</p>
      </div>
    </div>
  );
}

function Row({ trade }: { trade: OwnTrade }) {
  return (
    <tr className="border-b border-[var(--color-line)] hover:bg-[var(--color-ink)]">
      <td className="py-1 pl-3 tabular-nums text-[var(--color-dim)]">
        {formatStampTime(trade.ts)}
      </td>
      <td className="py-1 uppercase">
        <Side side={trade.side} />
      </td>
      <td className="py-1 text-right tabular-nums">{formatCompactNumber(trade.token_amount)}</td>
      <td className="py-1 text-right tabular-nums text-[var(--color-dim)]">
        {/* Absent, not zero. `MarketFigure` spells out "unknown" rather than
            showing a dash or a 0 a reader could act on. */}
        <MarketFigure
          value={trade.price}
          reason={trade.price === null ? "Radar could not price this fill" : null}
          format={formatPrice}
        />
      </td>
      <td className="py-1 pr-3 text-right">
        <a
          href={transactionUrl(trade.signature)}
          target="_blank"
          rel="noreferrer"
          title={
            trade.matched_by === "trader"
              ? "The tape names your wallet on this trade"
              : "The tape names no trader; this is yours because the coin was paid into your own token account"
          }
          className="text-[10px] uppercase text-[var(--color-dim)] hover:text-[var(--color-text)]"
        >
          {trade.matched_by === "trader" ? "named" : "derived"} ↗
        </a>
      </td>
    </tr>
  );
}

function Empty({
  children,
  tone = "dim",
}: {
  children: React.ReactNode;
  tone?: "dim" | "warn";
}) {
  return (
    <p
      className={`p-3 text-xs ${
        tone === "warn" ? "text-[var(--color-warn)]" : "text-[var(--color-dim)]"
      }`}
    >
      {children}
    </p>
  );
}
