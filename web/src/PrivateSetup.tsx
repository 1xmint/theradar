// SPDX-License-Identifier: Apache-2.0
//! Operator-only setup: Alpaca paper first, optional Solana wallet.

import { lazy, Suspense, useEffect, useState } from "react";
import { SubscriptionLink } from "./Agent";
import { ApiError, connections } from "./api";
import { AlpacaPaper } from "./AlpacaPaper";
import { PaperResearch } from "./PaperResearch";

const PrivyWallet = lazy(() => import("./PrivyWallet"));

export function PrivateSetup() {
  const [privy, setPrivy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const controller = new AbortController();
    connections.privy(controller.signal)
      .then((config) => setPrivy(config.privy_app_id))
      .catch((cause: unknown) => {
        if (controller.signal.aborted) return;
        setError(cause instanceof ApiError && cause.status === 503
          ? "No Privy app is configured on this instance."
          : "Could not check the Privy connection. Reload to try again.");
      });
    return () => controller.abort();
  }, []);

  return (
    <section className="mx-auto max-w-3xl space-y-6">
      <div>
        <h2 className="text-xl font-semibold">Private trader setup</h2>
        <p className="mt-2 text-sm text-[var(--color-dim)]">
          Start with Alpaca paper stocks and ETFs, with ChatGPT for research and reasoning.
          Paper trading uses simulated funds. Signing in to ChatGPT does not enable trading.
        </p>
      </div>
      <div>
        <h3 className="mb-3 font-medium">ChatGPT subscription</h3>
        <SubscriptionLink />
        <p className="text-sm text-[var(--color-dim)]">You can connect before enabling inference or trading.</p>
      </div>
      <div className="rounded-md border border-[var(--color-line)] bg-[var(--color-surface)] p-4 space-y-3">
        <h3 className="font-medium">Alpaca paper trading — recommended starting venue</h3>
        <p className="text-sm text-[var(--color-dim)]">
          Create a free paper-only account, then open its Paper Trading dashboard.
          No deposit is needed. Use a simulated balance that matches the capital you intend to test.
          Start without leverage or short selling.
        </p>
        <a className="inline-block rounded border border-[var(--color-line)] px-3 py-2 text-sm"
          href="https://app.alpaca.markets/" target="_blank" rel="noopener noreferrer">Open Alpaca dashboard</a>
        <p className="text-sm text-[var(--color-dim)]">
          Keep paper API keys private;
          do not paste them into research chat. Your Solana wallet settings do not apply to Alpaca.
        </p>
        <AlpacaPaper />
        <PaperResearch />
        <p className="text-sm text-[var(--color-dim)]">
          Paper results help test the system, but simulated fills omit some live costs and execution effects.
          Profits in paper trading do not establish live profitability.
        </p>
      </div>
      <div className="rounded-md border border-[var(--color-line)] bg-[var(--color-surface)] p-4">
        <h3 className="font-medium">Optional Solana module — Privy wallet</h3>
        {privy ? <Suspense fallback={<p role="status">Loading wallet connection…</p>}><PrivyWallet appId={privy} /></Suspense>
          : <p className="mt-2 text-sm" role="status">{error ?? "Checking Privy configuration…"}</p>}
      </div>
      <div className="rounded-md border border-[var(--color-line)] p-4">
        <h3 className="font-medium">Autonomous trading readiness</h3>
        <p className="mt-2 text-sm text-[var(--color-dim)]">
          Use the Alpaca panel to manage paper handover. Verify supervisor health and the full order
          lifecycle before unattended trading: entry, exit, reconciliation, cancellation and pause.
          The Solana settings above remain separate drafts. Granting paper permission does not prove the agent is running.
        </p>
      </div>
    </section>
  );
}
