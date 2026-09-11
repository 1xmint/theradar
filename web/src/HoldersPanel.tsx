// SPDX-License-Identifier: Apache-2.0
//! Ranked holders, with the caption the contract requires: what kind of fact
//! this list is. "Folded from transfer history over a stated window, not a
//! read of current account state" is not a footnote here -- it is the
//! difference between a balance and an inference, and `holdersBasisCaption`
//! in `honesty.ts` refuses to let a response drop it silently.

import { market } from "./api";
import { capCaption, holdersBasisCaption } from "./honesty";
import { Address, MarketFigure } from "./Figures";
import { explorerUrl, formatCompactNumber } from "./format";
import { useApi } from "./useApi";

const LIMIT = 50;

export function HoldersPanel({ mint }: { mint: string }) {
  const load = useApi((signal) => market.holders(mint, { limit: LIMIT }, signal), [mint]);

  if (load.state === "loading") {
    return <p className="p-3 text-xs text-[var(--color-dim)]">Reading holders…</p>;
  }

  if (load.state === "failed") {
    return (
      <p className="p-3 text-xs text-[var(--color-warn)]">
        Could not read the holder list: {load.detail}.
      </p>
    );
  }

  const { holders, basis } = load.value;
  const cap = capCaption(holders.length, LIMIT, "holders");

  return (
    <div className="flex h-full flex-col">
      {/* The caption is not decoration -- it is the fact the whole panel
          rests on, and it renders even when the list below is empty. */}
      <p className="border-b border-[var(--color-line)] px-3 py-1.5 text-[11px] text-[var(--color-dim)]">
        {holdersBasisCaption(basis)}
      </p>

      {holders.length === 0 ? (
        <p className="p-3 text-xs text-[var(--color-dim)]">
          No holders recorded under this basis.
        </p>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <table className="w-full text-xs">
            <thead className="sticky top-0 bg-[var(--color-surface)] text-[10px] uppercase tracking-wide text-[var(--color-dim)]">
              <tr>
                <th scope="col" className="py-1 pl-3 text-left font-medium">#</th>
                <th scope="col" className="py-1 text-left font-medium">Address</th>
                <th scope="col" className="py-1 text-right font-medium">Amount</th>
                <th scope="col" className="py-1 pr-3 text-right font-medium">% of supply</th>
              </tr>
            </thead>
            <tbody>
              {holders.map((holder, index) => (
                <tr key={holder.address} className="border-b border-[var(--color-line)] hover:bg-[var(--color-ink)]">
                  <td className="py-1 pl-3 tabular-nums text-[var(--color-dim)]">{index + 1}</td>
                  <td className="py-1">
                    <span className="inline-flex items-center gap-1">
                      <Address value={holder.address} />
                      <a
                        href={explorerUrl(holder.address)}
                        target="_blank"
                        rel="noreferrer"
                        title="Open in explorer"
                        className="text-[var(--color-dim)] hover:text-[var(--color-text)]"
                      >
                        ↗
                      </a>
                    </span>
                  </td>
                  <td className="py-1 text-right tabular-nums text-[var(--color-dim)]">
                    {formatCompactNumber(holder.amount)}
                  </td>
                  <td className="py-1 pr-3 text-right tabular-nums">
                    <MarketFigure
                      value={holder.pct_of_supply}
                      reason={holder.pct_of_supply === null ? "supply unknown" : null}
                      format={(v) => `${v.toFixed(2)}%`}
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {cap && (
        <p className="border-t border-[var(--color-line)] px-3 py-1 text-[10px] text-[var(--color-dim)]">
          {cap}
        </p>
      )}
    </div>
  );
}
