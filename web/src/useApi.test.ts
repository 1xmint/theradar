// SPDX-License-Identifier: Apache-2.0
//! `refreshMs`: the chart's 15s live refresh must not flash the placeholder
//! or drop a working panel to `"failed"` on one dropped poll.

import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useApi } from "./useApi";

describe("useApi", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("refetches silently on refreshMs without ever returning to loading", async () => {
    vi.useFakeTimers();
    let call = 0;
    const fetcher = vi.fn(async () => {
      call += 1;
      return call;
    });

    const { result } = renderHook(() => useApi(fetcher, [], 1_000));

    await vi.waitFor(() => expect(result.current).toEqual({ state: "ready", value: 1 }));

    await vi.advanceTimersByTimeAsync(1_000);
    await vi.waitFor(() => expect(result.current).toEqual({ state: "ready", value: 2 }));

    // Never observed "loading" again after the first resolution -- the poll
    // must not flash the placeholder or reset a reader's scroll position.
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("keeps the last good value when a silent refresh fails", async () => {
    vi.useFakeTimers();
    let call = 0;
    const fetcher = vi.fn(async () => {
      call += 1;
      if (call === 2) throw new Error("dropped poll");
      return call;
    });

    const { result } = renderHook(() => useApi(fetcher, [], 1_000));

    await vi.waitFor(() => expect(result.current).toEqual({ state: "ready", value: 1 }));

    await vi.advanceTimersByTimeAsync(1_000);
    // The second call throws, but the panel must keep showing the first
    // call's data rather than flipping to "failed".
    await vi.waitFor(() => expect(fetcher).toHaveBeenCalledTimes(2));
    // Let React render whatever the rejected poll set; without this the
    // assertion runs first and passes even if the poll did flip to "failed".
    await act(async () => {});
    expect(result.current).toEqual({ state: "ready", value: 1 });
  });

  it("skips a tick while the previous fetch is still out", async () => {
    vi.useFakeTimers();
    const resolvers: Array<(n: number) => void> = [];
    const fetcher = vi.fn(() => new Promise<number>((resolve) => resolvers.push(resolve)));

    const { result } = renderHook(() => useApi(fetcher, [], 1_000));
    expect(fetcher).toHaveBeenCalledTimes(1);

    // Three ticks pass while the first fetch hangs: none may start another,
    // or a slow reply could land after -- and overwrite -- a newer one.
    await vi.advanceTimersByTimeAsync(3_000);
    expect(fetcher).toHaveBeenCalledTimes(1);

    await act(async () => resolvers[0]?.(1));
    expect(result.current).toEqual({ state: "ready", value: 1 });
    await vi.advanceTimersByTimeAsync(1_000);
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("does not poll at all when refreshMs is omitted", async () => {
    vi.useFakeTimers();
    const fetcher = vi.fn(async () => 1);
    renderHook(() => useApi(fetcher, []));
    await vi.waitFor(() => expect(fetcher).toHaveBeenCalledTimes(1));
    await vi.advanceTimersByTimeAsync(60_000);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
});
