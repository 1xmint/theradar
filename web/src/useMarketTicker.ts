// SPDX-License-Identifier: Apache-2.0
//! The terminal's live tick, off the shared ticker rather than a blind timer.
//!
//! `Terminal.tsx` used to re-fetch the coin list on a flat interval regardless
//! of whether the store had moved -- every open tab polling on its own timer,
//! the same shape as the server-side problem `radar_serve::ticker`'s module
//! doc describes, just moved one hop further out. `/v1/market/events` is the
//! fix on this side: the shared ticker's public projection (a bare watermark
//! -- `Watermark` in `lib.rs` -- no wallet, no address, ever), pushed once per
//! store change instead of polled.
//!
//! A dropped stream falls back to polling on [`FALLBACK_POLL_MS`] so the
//! terminal keeps moving rather than freezing on whatever it last saw, and
//! backs off on each failed reconnect (capped at [`RECONNECT_MAX_MS`]) so a
//! server bounce does not turn into a reconnect storm. The moment the stream
//! delivers a value again, the fallback poll stops and the backoff resets.

import { useEffect, useState } from "react";

/** How often the fallback poll re-fetches while the stream is down -- the
 *  same cadence the old blind timer polled at, kept only as a safety net now
 *  that a healthy connection is pushed to instead. */
export const FALLBACK_POLL_MS = 15_000;

/** First reconnect wait after a dropped stream. */
const RECONNECT_BASE_MS = 1_000;
/** Ceiling on the reconnect backoff, doubled on each further failure. */
const RECONNECT_MAX_MS = 30_000;

/**
 * Bumps once per store change, read off `/v1/market/events`.
 *
 * Callers use the return value the way the old `tick` state worked: a number
 * to put in a `useApi` dependency list, meaningful only in that it changes.
 */
export function useMarketTicker(): number {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let cancelled = false;
    let fallback: ReturnType<typeof setInterval> | null = null;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let reconnectDelay = RECONNECT_BASE_MS;
    let source: EventSource | null = null;

    const startFallback = () => {
      if (fallback !== null) return;
      fallback = setInterval(() => setTick((t) => t + 1), FALLBACK_POLL_MS);
    };
    const stopFallback = () => {
      if (fallback === null) return;
      clearInterval(fallback);
      fallback = null;
    };

    const connect = () => {
      if (cancelled) return;
      const source_ = new EventSource("/v1/market/events");
      source = source_;
      source_.addEventListener("store", () => {
        // A live push arrived: the connection has recovered (if it was ever
        // down), so the fallback poll is redundant and the next outage should
        // start backing off from the base delay again, not from wherever the
        // last one left off.
        reconnectDelay = RECONNECT_BASE_MS;
        stopFallback();
        setTick((t) => t + 1);
      });
      source_.onerror = () => {
        source_.close();
        startFallback();
        if (cancelled) return;
        reconnectTimer = setTimeout(() => {
          reconnectDelay = Math.min(reconnectDelay * 2, RECONNECT_MAX_MS);
          connect();
        }, reconnectDelay);
      };
    };

    connect();

    return () => {
      cancelled = true;
      source?.close();
      stopFallback();
      if (reconnectTimer !== null) clearTimeout(reconnectTimer);
    };
  }, []);

  return tick;
}
