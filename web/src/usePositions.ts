// SPDX-License-Identifier: Apache-2.0
//! The signed-in wallet's own on-chain holdings, read once per wallet change.
//!
//! Simpler than `useWatchlist`: positions has no toggle, so there is nothing
//! optimistic to protect and no second state (`toggleError`) to keep beside
//! the read. One effect, one `Load`-shaped result -- closer to what `useApi`
//! gives every other panel, except this one must fall back to a `signed-out`
//! state instead of firing a request with no wallet to ask about.

import { useEffect, useState } from "react";
import { customer, PositionsError, type Positions } from "./api";
import { useWalletToken } from "./Wallet";

/** What the positions read is doing right now. */
export type PositionsLoad =
  | { state: "signed-out" }
  | { state: "loading" }
  | { state: "ready"; value: Positions }
  | { state: "failed"; status: number; reason: string; detail: string };

/**
 * `refreshKey` is not read, only compared: bumping it (after a trade sends,
 * for instance -- see `TradePanel.tsx`'s `onTraded`) forces the effect below
 * to re-run and re-read the chain, the same wallet notwithstanding. Callers
 * that never need a manual refresh can omit it entirely.
 */
export function usePositions(refreshKey: number = 0): PositionsLoad {
  const token = useWalletToken();
  const [load, setLoad] = useState<PositionsLoad>(
    token ? { state: "loading" } : { state: "signed-out" },
  );

  useEffect(() => {
    if (!token) {
      setLoad({ state: "signed-out" });
      return;
    }
    const controller = new AbortController();
    setLoad({ state: "loading" });
    customer.positions
      .get(token, controller.signal)
      .then((value) => {
        if (!controller.signal.aborted) setLoad({ state: "ready", value });
      })
      .catch((e: unknown) => {
        if (controller.signal.aborted) return;
        setLoad(failureFrom(e));
      });
    return () => controller.abort();
  }, [token, refreshKey]);

  return load;
}

function failureFrom(e: unknown): PositionsLoad {
  if (e instanceof PositionsError) {
    return { state: "failed", status: e.status, reason: e.reason, detail: e.detail };
  }
  return { state: "failed", status: 0, reason: "unreachable", detail: String(e) };
}
