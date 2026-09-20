// SPDX-License-Identifier: Apache-2.0
//! Coins Radar recorded the launch of, newest first.
//!
//! **Not every coin that launched on Solana.** Only what this instance's own
//! collector decoded and wrote, within the launch index's lookback window --
//! the header says so plainly, because "newly launched" reads as a claim
//! about the whole chain unless a screen states its own limits. Zero
//! CryptoHouse queries: this reads `/v1/market/launches`, itself a read of
//! the already-cached launch index (`market/mod.rs`'s own doc comment).

import { market } from "./api";
import { launchesEmptyMessage } from "./honesty";
import { Address } from "./Figures";
import { explorerUrl } from "./format";
import { useApi } from "./useApi";

const LIMIT = 50;

export function NewlyLaunchedPanel() {
  const load = useApi((signal) => market.launches({ limit: LIMIT }, signal), []);

  if (load.state === "loading") {
    return <p className="p-3 text-xs text-[var(--color-dim)]">Reading recorded launches…</p>;
  }

  if (load.state === "failed") {
    // A 503 `not_collected` and a genuine transport failure both land here --
    // `launchesEmptyMessage` reads the server's own sentence (carried in
    // `detail` since `api.ts`'s `get()` prefers `message` over `error`) to
    // tell "nothing launched recently" apart from "Radar could not look",
    // rather than this component guessing from the status code alone.
    return (
      <p className="p-3 text-xs text-[var(--color-warn)]">
        {launchesEmptyMessage(load.detail)}
      </p>
    );
  }

  const { launches, window } = load.value;

  return (
    <div className="flex h-full flex-col">
      <p className="border-b border-[var(--color-line)] px-3 py-1.5 text-[11px] text-[var(--color-dim)]">
        Coins Radar recorded the launch of between slot {window.from_slot} and {window.to_slot} --
        not every coin that launched on Solana in that span, only what this instance saw and kept.
      </p>

      {launches.length === 0 ? (
        <p className="p-3 text-xs text-[var(--color-dim)]">
          Radar has not recorded a launch in its window.
        </p>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <table className="w-full text-xs">
            <thead className="sticky top-0 bg-[var(--color-surface)] text-[10px] uppercase tracking-wide text-[var(--color-dim)]">
              <tr>
                <th scope="col" className="py-1 pl-3 text-left font-medium">#</th>
                <th scope="col" className="py-1 text-left font-medium">Coin</th>
                <th scope="col" className="py-1 pr-3 text-right font-medium">Slot</th>
              </tr>
            </thead>
            <tbody>
              {launches.map((launch, index) => (
                <tr
                  key={launch.mint}
                  className="border-b border-[var(--color-line)] hover:bg-[var(--color-ink)]"
                >
                  <td className="py-1 pl-3 tabular-nums text-[var(--color-dim)]">{index + 1}</td>
                  <td className="py-1">
                    <span className="inline-flex items-center gap-1">
                      <span title={launch.name || launch.mint}>
                        {launch.symbol || launch.name || <Address value={launch.mint} />}
                      </span>
                      <a
                        href={explorerUrl(launch.mint)}
                        target="_blank"
                        rel="noreferrer"
                        title="Open in explorer"
                        className="text-[var(--color-dim)] hover:text-[var(--color-text)]"
                      >
                        ↗
                      </a>
                    </span>
                  </td>
                  <td className="py-1 pr-3 text-right tabular-nums text-[var(--color-dim)]">
                    {launch.slot}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
