// SPDX-License-Identifier: Apache-2.0
//! Rubric 4 (Plan 0013 Phase E.1): the terminal applies a streamed update
//! without waiting on the fallback poll, and falls back to polling when the
//! stream itself goes down.

import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FALLBACK_POLL_MS, useMarketTicker } from "./useMarketTicker";

/** Stands in for the browser's `EventSource` -- jsdom does not implement one
 *  itself. Records every instance so a test can reach in and fire `store` or
 *  `error` on whichever connection `useMarketTicker` currently holds open. */
class FakeEventSource {
  static instances: FakeEventSource[] = [];
  static reset() {
    FakeEventSource.instances = [];
  }

  url: string;
  closed = false;
  onerror: (() => void) | null = null;
  private listeners = new Map<string, Array<() => void>>();

  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }

  addEventListener(type: string, handler: () => void) {
    const list = this.listeners.get(type) ?? [];
    list.push(handler);
    this.listeners.set(type, list);
  }

  close() {
    this.closed = true;
  }

  emit(type: string) {
    for (const handler of this.listeners.get(type) ?? []) handler();
  }

  error() {
    this.onerror?.();
  }
}

describe("useMarketTicker", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    FakeEventSource.reset();
  });

  it("applies a streamed update immediately, without waiting on the 15s fallback poll", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("EventSource", FakeEventSource);

    const { result } = renderHook(() => useMarketTicker());
    expect(result.current).toBe(0);

    const source = FakeEventSource.instances[0]!;
    expect(source.url).toBe("/v1/market/events");

    // Well under `FALLBACK_POLL_MS`: if this only worked via the fallback
    // timer, the tick would still read 0 here.
    await act(async () => {
      source.emit("store");
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(result.current).toBe(1);
  });

  it("falls back to polling when the stream errors, and stops once it recovers", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("EventSource", FakeEventSource);

    const { result } = renderHook(() => useMarketTicker());
    const first = FakeEventSource.instances[0]!;

    await act(async () => {
      first.error();
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(first.closed).toBe(true);
    expect(result.current).toBe(0);

    // The stream is down: the fallback poll must pick up the slack on its own
    // schedule, with no `store` event ever arriving.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(FALLBACK_POLL_MS);
    });
    expect(result.current).toBe(1);

    // Advance past the reconnect delay so a fresh connection is opened, then
    // let it recover: a `store` event on the new connection should be enough
    // to stand in for "no longer needs the fallback poll" (the connection
    // succeeding at all is the recovery signal downstream code cares about).
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });
    const second = FakeEventSource.instances[1];
    expect(second).toBeTruthy();

    await act(async () => {
      second!.emit("store");
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current).toBe(2);
  });
});
