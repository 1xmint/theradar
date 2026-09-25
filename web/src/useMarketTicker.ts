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
//!
//! The fallback starts at the top of `connect()`, before `EventSource` is
//! even constructed, rather than only from `onerror`. A compressing proxy
//! sitting in front of `/v1/market/events` can hold the response open
//! without ever flushing a byte to the browser -- the connection is "open"
//! but silent, so `onerror` never fires and a poll that only started there
//! would never start at all. Starting the poll unconditionally means a
//! silent stream degrades to the old 15s refresh instead of a frozen page;
//! a healthy stream just stops it again the moment a real `store` frame
//! lands.
//!
//! Each `store` frame carries the snapshot's `as_of` watermark. The tick
//! only bumps when that watermark actually moved, so mounting the hook (or
//! reconnecting onto a snapshot the client already has) does not trigger a
//! redundant refetch downstream.

import { useEffect, useRef, useState } from "react";

/** Pulls `as_of` out of a `store` frame's `event.data`, or `null` when it
 *  cannot be read. `null` is treated as "unknown, assume changed" by the
 *  caller -- the safe default is to refetch, not to silently sit still on
 *  a payload we could not parse. */
function parseAsOf(data: unknown): number | null {
  if (typeof data !== "string") return null;
  try {
    const parsed: unknown = JSON.parse(data);
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      "as_of" in parsed &&
      typeof (parsed as { as_of: unknown }).as_of === "number"
    ) {
      return (parsed as { as_of: number }).as_of;
    }
    return null;
  } catch {
    return null;
  }
}

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

  const lastAsOf = useRef<number | null>(null);

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
      // Start the safety net before the connection even opens: a proxy that
      // holds the response open without flushing anything leaves the socket
      // "open" but silent, so `onerror` never fires to start it for us.
      startFallback();
      const source_ = new EventSource("/v1/market/events");
      source = source_;
      source_.addEventListener("store", (event: Event) => {
        // A live push arrived: the connection has recovered (if it was ever
        // down), so the fallback poll is redundant and the next outage should
        // start backing off from the base delay again, not from wherever the
        // last one left off.
        reconnectDelay = RECONNECT_BASE_MS;
        stopFallback();
        const asOf = parseAsOf((event as MessageEvent).data);
        if (asOf !== null && asOf === lastAsOf.current) {
          // The same watermark as last time -- a duplicate frame, or the
          // first frame after a reconnect landing on a snapshot the client
          // already applied. Nothing downstream changed, so don't bump.
          return;
        }
        if (asOf !== null) lastAsOf.current = asOf;
        setTick((t) => t + 1);
      });
      source_.onerror = () => {
        source_.close();
        // The stream just dropped; if a `store` frame had stopped the
        // fallback, restart it so the terminal keeps moving while we
        // reconnect.
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
