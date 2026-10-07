// SPDX-License-Identifier: Apache-2.0
//! Operator-only setup for the owner's subscription and autonomous Privy lane.

import { lazy, Suspense, useEffect, useState } from "react";
import { SubscriptionLink } from "./Agent";
import { ApiError, connections } from "./api";

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
          Connect ChatGPT for reasoning and a Privy wallet for autonomous execution.
          Signing in to ChatGPT does not grant permission to move funds.
        </p>
      </div>
      <div>
        <h3 className="mb-3 font-medium">ChatGPT subscription</h3>
        <SubscriptionLink />
        <p className="text-sm text-[var(--color-dim)]">You can connect before enabling inference or trading.</p>
      </div>
      <div className="rounded-md border border-[var(--color-line)] bg-[var(--color-surface)] p-4">
        <h3 className="font-medium">Privy wallet</h3>
        {privy ? <Suspense fallback={<p role="status">Loading wallet connection…</p>}><PrivyWallet appId={privy} /></Suspense>
          : <p className="mt-2 text-sm" role="status">{error ?? "Checking Privy configuration…"}</p>}
      </div>
      <div className="rounded-md border border-[var(--color-line)] p-4">
        <h3 className="font-medium">Autonomous execution: not enabled</h3>
        <p className="mt-2 text-sm text-[var(--color-dim)]">
          The execution supervisor still needs to be connected to the Privy signing lane.
          Before enabling it, set capital, position and daily-loss limits, allowed assets,
          an inference allowance, and a session expiry. Missing limits keep trading off.
        </p>
      </div>
    </section>
  );
}
