// SPDX-License-Identifier: Apache-2.0
//! `Terms.tsx` is gated the same way `TradePanel` is -- `legal.ts`'s
//! `TERMS_APPROVED` and `useHealth.ts`'s `useTrading()` -- and today
//! `TERMS_APPROVED` is a hardcoded `false` (see `legal.ts` and
//! `legal.test.ts`'s placeholder-forces-false test), so the only reachable
//! behaviour in the real app is the fallback. This file pins that down: the
//! page renders the same "No such page." shape as `App.tsx`'s own
//! `NotFound`, regardless of what `/health` says, rather than a
//! feature-specific reason that would itself hint the feature exists.

import { render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { Terms } from "./Terms";

function jsonResponse(body: unknown, status = 200): Response {
  return { ok: status >= 200 && status < 300, status, json: async () => body } as Response;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("shows the same 'no such page' fallback even when trading is live server-side", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => jsonResponse({ trading: true })));
  render(<Terms />);
  expect(await screen.findByText("No such page.")).toBeTruthy();
});

it("shows the same fallback when trading is off server-side too", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => jsonResponse({ trading: false })));
  render(<Terms />);
  expect(await screen.findByText("No such page.")).toBeTruthy();
});
