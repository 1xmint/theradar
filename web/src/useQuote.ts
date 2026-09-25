// SPDX-License-Identifier: Apache-2.0
//! A debounced read of `/v1/market/quote`, kept in step with what someone is
//! typing without firing a request per keystroke.
//!
//! Not built on `useApi`: that hook fires its effect immediately on every
//! dependency change, which for a quote means a request per keystroke of an
//! amount field -- the public route is rate-limited per-IP (ADR 0024,
//! "courtesy only, not a security boundary" -- courtesy Radar owes the
//! upstream route, not a reason to abuse it), and a screen re-fetching on
//! every digit would exhaust that budget on one visitor typing one amount.
//! This hook waits for typing to pause before asking.

import { useEffect, useState } from "react";
import { market, SwapError, type Quote, type SwapSide } from "./api";

/** What the debounced quote read is doing right now. */
export type QuoteLoad =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "ready"; value: Quote }
  | { state: "failed"; status: number; reason: string; detail: string };

/** How long to wait, after the last change, before asking for a quote. */
const DEBOUNCE_MS = 400;

/**
 * `amount`, in base units, or `null` when there is nothing valid to quote
 * yet (an empty field, or one `toBaseUnits` rejected) -- `null` is `idle`,
 * never a request for a zero-amount quote nobody asked for.
 */
export function useQuote(
  mint: string,
  side: SwapSide,
  amount: string | null,
  slippageBps: number,
): QuoteLoad {
  const [load, setLoad] = useState<QuoteLoad>({ state: "idle" });

  useEffect(() => {
    if (amount === null) {
      setLoad({ state: "idle" });
      return;
    }
    setLoad({ state: "loading" });
    const controller = new AbortController();
    const timer = setTimeout(() => {
      market
        .quote({ mint, side, amount, slippage_bps: slippageBps }, controller.signal)
        .then((value) => {
          if (!controller.signal.aborted) setLoad({ state: "ready", value });
        })
        .catch((e: unknown) => {
          if (controller.signal.aborted) return;
          setLoad(failureFrom(e));
        });
    }, DEBOUNCE_MS);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [mint, side, amount, slippageBps]);

  return load;
}

function failureFrom(e: unknown): QuoteLoad {
  if (e instanceof SwapError) {
    return { state: "failed", status: e.status, reason: e.reason, detail: e.detail };
  }
  return { state: "failed", status: 0, reason: "unreachable", detail: String(e) };
}
