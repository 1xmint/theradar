// SPDX-License-Identifier: Apache-2.0
//! Contract, creator, mint authority, links. The dry facts a trader checks
//! once per token rather than every tick, which is why they sit in a tab
//! instead of the header.

import type { ReactNode } from "react";
import type { MarketToken } from "./api";
import { Address } from "./Figures";
import { explorerUrl } from "./format";
import type { Load } from "./useApi";

export function InfoPanel({ load }: { load: Load<MarketToken> }) {
  if (load.state === "loading") {
    return <p className="p-3 text-xs text-[var(--color-dim)]">Reading token info…</p>;
  }

  // Loading and failed collapse to the same "nothing here yet" if this is not
  // split out: a reader watching this tab hang on "Reading…" forever after a
  // real failure would have no way to tell a slow read from a dead one.
  if (load.state === "failed") {
    return (
      <p className="p-3 text-xs text-[var(--color-warn)]">
        Could not read this token's info: {load.detail}.
      </p>
    );
  }

  const token = load.value;

  return (
    <div className="space-y-4 overflow-y-auto p-3 text-xs">
      <Row label="Contract">
        <span className="inline-flex items-center gap-1">
          <Address value={token.mint} />
          <a
            href={explorerUrl(token.mint)}
            target="_blank"
            rel="noreferrer"
            className="text-[var(--color-dim)] hover:text-[var(--color-text)]"
          >
            ↗
          </a>
        </span>
      </Row>

      <Row label="Creator">
        {token.creator ? (
          <span className="inline-flex items-center gap-1">
            <Address value={token.creator} />
            <a
              href={explorerUrl(token.creator)}
              target="_blank"
              rel="noreferrer"
              className="text-[var(--color-dim)] hover:text-[var(--color-text)]"
            >
              ↗
            </a>
          </span>
        ) : (
          <span className="text-[var(--color-absent)]">unknown</span>
        )}
      </Row>

      <Row label="Mint authority">
        <MintAuthority state={token.mint_authority} reason={token.mint_authority_reason} />
      </Row>

      <Row label="Decimals">
        {token.decimals === null ? (
          <span className="text-[var(--color-absent)]">unknown</span>
        ) : (
          <span className="tabular-nums">{token.decimals}</span>
        )}
      </Row>

      <Row label="Links">
        <div className="flex flex-wrap gap-3">
          <a
            href={`https://solscan.io/token/${encodeURIComponent(token.mint)}`}
            target="_blank"
            rel="noreferrer"
            className="underline hover:text-[var(--color-text)]"
          >
            Solscan
          </a>
          <a
            href={`https://pump.fun/coin/${encodeURIComponent(token.mint)}`}
            target="_blank"
            rel="noreferrer"
            className="underline hover:text-[var(--color-text)]"
          >
            pump.fun
          </a>
        </div>
      </Row>
    </div>
  );
}

/**
 * The mint authority's state -- rendered against AGENTS.md rule 5: this latch
 * only closes. `"active"` here means "not observed as revoked", never
 * "verified unrevoked", so it is worded that way rather than as reassurance,
 * and `null` gets the same "unknown" treatment as every other unmeasured
 * market fact rather than defaulting to either state.
 */
function MintAuthority({
  state,
  reason,
}: {
  state: "revoked" | "active" | null;
  reason: string | null;
}) {
  if (state === "revoked") {
    return <span className="text-[var(--color-good)]">Revoked</span>;
  }
  if (state === "active") {
    return <span className="text-[var(--color-warn)]">Not revoked — the creator can still mint more</span>;
  }
  return (
    <span className="text-[var(--color-absent)]" title={reason ?? "Radar did not say why"}>
      unknown{reason ? ` — ${reason}` : ""}
    </span>
  );
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-4 border-b border-[var(--color-line)] pb-3">
      <dt className="shrink-0 text-[var(--color-dim)]">{label}</dt>
      <dd className="text-right">{children}</dd>
    </div>
  );
}
