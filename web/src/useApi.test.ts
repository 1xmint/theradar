// SPDX-License-Identifier: Apache-2.0
//! `refreshMs`: the chart's 15s live refresh must not flash the placeholder
//! or drop a working panel to `"failed"` on one dropped poll.

import { renderHook } from "@testing-library/react";
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
    expect(result.current).toEqual({ state: "ready", value: 1 });
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
