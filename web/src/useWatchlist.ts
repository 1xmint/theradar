// SPDX-License-Identifier: Apache-2.0
//! One per-wallet watchlist, shared by the star button and the watchlist
//! panel so a toggle in one place is visible in the other without a second
//! request.
//!
//! Not built on `useApi`: a toggle needs to replace the loaded value with the
//! server's answer -- never ahead of it, since an optimistic flip could drift
//! from what was actually saved -- and `useApi`'s `Load<T>` has no setter,
//! only its own effect may write it. Small enough, and different enough from
//! that one lifecycle, to be its own hook.

import { useCallback, useEffect, useState } from "react";
import { customer, WatchlistError, type Watchlist } from "./api";
import { isWatchlistSessionRefusal } from "./honesty";
import { useWalletToken } from "./Wallet";

/** What the watchlist read is doing right now. */
export type WatchlistLoad =
  | { state: "signed-out" }
  | { state: "loading" }
  | { state: "ready"; value: Watchlist }
  | { state: "failed"; status: number; reason: string; detail: string };

export interface UseWatchlist {
  load: WatchlistLoad;
  /**
   * The last add/remove that did not stick, e.g. a full list -- shown beside
   * the star, not folded into `load`. A failed *change* is not evidence the
   * *read* that already populated `load` was wrong, so it must not blank the
   * list out from under a reader who was looking at it.
   *
   * Carries the server's own reason and detail rather than a rendered
   * sentence, so the caller picks the wording with `watchlistToggleFailure`
   * -- the same split `honesty.ts` keeps everywhere else in this file.
   */
  toggleError: { reason: string; detail: string } | null;
  /** Adds `mint` if the last read did not have it, removes it otherwise. */
  toggle: (mint: string) => Promise<void>;
  /** A change is on its way to the server. The star waits for its answer:
   *  a second click decided from the pre-change list would send the same
   *  change again rather than undo it. */
  pending: boolean;
}

export function useWatchlist(): UseWatchlist {
  const token = useWalletToken();
  const [load, setLoad] = useState<WatchlistLoad>(
    token ? { state: "loading" } : { state: "signed-out" },
  );
  const [pending, setPending] = useState(false);
  const [toggleError, setToggleError] = useState<{ reason: string; detail: string } | null>(
    null,
  );

  useEffect(() => {
    if (!token) {
      setLoad({ state: "signed-out" });
      return;
    }
    const controller = new AbortController();
    setLoad({ state: "loading" });
    customer.watchlist
      .list(token, controller.signal)
      .then((value) => {
        if (!controller.signal.aborted) setLoad({ state: "ready", value });
      })
      .catch((e: unknown) => {
        if (controller.signal.aborted) return;
        setLoad(failureFrom(e));
      });
    return () => controller.abort();
  }, [token]);

  const toggle = useCallback(
    async (mint: string) => {
      if (!token) return;
      const watching = load.state === "ready" && load.value.coins.includes(mint);
      setToggleError(null);
      setPending(true);
      try {
        const value = watching
          ? await customer.watchlist.unwatch(token, mint)
          : await customer.watchlist.watch(token, mint);
        setLoad({ state: "ready", value });
      } catch (e) {
        const failure = failureFrom(e);
        // A session refusal invalidates the read too -- everything the panel
        // is showing came from a session the server no longer honours. Every
        // other refusal (full, not_a_coin, a server fault) is a fact about
        // this one change, not about the list already on screen.
        if (failure.state === "failed" && isWatchlistSessionRefusal(failure.reason)) {
          setLoad(failure);
        } else if (failure.state === "failed") {
          setToggleError({ reason: failure.reason, detail: failure.detail });
        }
      } finally {
        setPending(false);
      }
    },
    [token, load],
  );

  return { load, toggleError, toggle, pending };
}

function failureFrom(e: unknown): WatchlistLoad {
  if (e instanceof WatchlistError) {
    return { state: "failed", status: e.status, reason: e.reason, detail: e.detail };
  }
  return { state: "failed", status: 0, reason: "unreachable", detail: String(e) };
}
