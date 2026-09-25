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
  private listeners = new Map<string, Array<(event: { data?: string }) => void>>();

  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }

  addEventListener(type: string, handler: (event: { data?: string }) => void) {
    const list = this.listeners.get(type) ?? [];
    list.push(handler);
    this.listeners.set(type, list);
  }

  close() {
    this.closed = true;
  }

  /** `data`, when given, stands in for the `as_of` watermark frame the real
   *  server sends as `event.data` (a JSON string, e.g. `{"as_of":5}`). */
  emit(type: string, data?: string) {
    const event: { data?: string } = data === undefined ? {} : { data };
    for (const handler of this.listeners.get(type) ?? []) handler(event);
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

  it("applies a streamed update immediately, without waiting on the 15s fallback poll, and never polls while the stream stays healthy", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("EventSource", FakeEventSource);

    const { result } = renderHook(() => useMarketTicker());
    expect(result.current).toBe(0);

    const source = FakeEventSource.instances[0]!;
    expect(source.url).toBe("/v1/market/events");

    // Well under `FALLBACK_POLL_MS`: if this only worked via the fallback
    // timer, the tick would still read 0 here.
    await act(async () => {
      source.emit("store", JSON.stringify({ as_of: 1 }));
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(result.current).toBe(1);

    // The fallback poll starts defensively at the top of `connect()` (in
    // case the stream opens silently behind a buffering proxy), but a
    // genuine `store` frame must stop it -- advancing well past its own
    // interval, with the stream never erroring, must not add any further
    // bumps.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(FALLBACK_POLL_MS * 2);
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
      second!.emit("store", JSON.stringify({ as_of: 2 }));
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current).toBe(2);

    // The recovery must actually have stopped the fallback poll -- advancing
    // well past two of its intervals with no further `store` frame and no
    // further error must leave the tick untouched.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(FALLBACK_POLL_MS * 2);
    });
    expect(result.current).toBe(2);
  });

  it("backs off exponentially between reconnect attempts, doubling the wait each time", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("EventSource", FakeEventSource);

    renderHook(() => useMarketTicker());
    const first = FakeEventSource.instances[0]!;

    await act(async () => {
      first.error();
      await vi.advanceTimersByTimeAsync(0);
    });

    // First reconnect waits exactly RECONNECT_BASE_MS (1s): one tick short
    // must not have reconnected yet.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(999);
    });
    expect(FakeEventSource.instances.length).toBe(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(FakeEventSource.instances.length).toBe(2);

    const second = FakeEventSource.instances[1]!;
    await act(async () => {
      second.error();
      await vi.advanceTimersByTimeAsync(0);
    });

    // The second failure must double the wait to exactly 2s, not repeat the
    // first delay or jump straight to the ceiling.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_999);
    });
    expect(FakeEventSource.instances.length).toBe(2);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(FakeEventSource.instances.length).toBe(3);
  });

  it("only bumps the tick when a frame's as_of differs from the last one seen", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("EventSource", FakeEventSource);

    const { result } = renderHook(() => useMarketTicker());
    const source = FakeEventSource.instances[0]!;

    await act(async () => {
      source.emit("store", JSON.stringify({ as_of: 5 }));
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current).toBe(1);

    // The same as_of again -- a duplicate frame, or a reconnect landing on a
    // snapshot the client already applied -- must not bump a second time.
    await act(async () => {
      source.emit("store", JSON.stringify({ as_of: 5 }));
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current).toBe(1);

    // A genuinely new as_of must still bump.
    await act(async () => {
      source.emit("store", JSON.stringify({ as_of: 6 }));
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current).toBe(2);
  });
});
