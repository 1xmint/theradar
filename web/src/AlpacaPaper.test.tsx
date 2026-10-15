// SPDX-License-Identifier: Apache-2.0
import { StrictMode } from "react";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AlpacaPaper } from "./AlpacaPaper";
import type { PaperStatus } from "./api";

afterEach(() => { vi.unstubAllGlobals(); });

function snapshot(): PaperStatus {
  return { version: "alpaca-paper-control-v1", environment: "paper", account_id: "paper-account", generation: 7,
    preferences: { capital_usd: "50", max_trade_usd: "10", daily_loss_usd: "5", daily_loss_enabled: false,
      agent_decides: { capital_usd: false, max_trade_usd: false, daily_loss_usd: false } },
    handover_enabled: false, effective_policy: { enabled: false, max_capital_microusd: 0, max_order_microusd: 0, max_daily_loss_microusd: null },
    account: { id: "paper-account", cash: "100000", equity: "100000", currency: "USD", status: "ACTIVE" },
    positions: [], market_clock: { timestamp: "2026-10-13T15:00:00Z", is_open: true,
      next_open: "2026-10-14T13:30:00Z", next_close: "2026-10-13T20:00:00Z" },
    execution_authority: false, agent_lifecycle_verified: false, profit_measured: false };
}

function server(initial = snapshot()) {
  let retained = initial;
  const fetch = vi.fn(async (url: RequestInfo | URL, options?: RequestInit) => {
    if (url === "/v1/paper/status") return Response.json(retained);
    const body = JSON.parse(options!.body as string) as { generation: number; preferences?: PaperStatus["preferences"]; enabled?: boolean };
    retained = { ...retained, generation: body.generation + 1, handover_enabled: body.enabled ?? false,
      preferences: body.preferences ?? retained.preferences,
      effective_policy: retained.effective_policy ? { ...retained.effective_policy, enabled: body.enabled ?? false } : null };
    return Response.json({ generation: retained.generation });
  });
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

describe("Alpaca paper handover", () => {
  it("reads the simulated account without enabling anything and survives StrictMode cleanup", async () => {
    const fetch = server();
    render(<StrictMode><AlpacaPaper /></StrictMode>);
    await screen.findAllByText("100000 USD");
    await waitFor(() => expect((screen.getByRole("button", { name: "Enable paper handover" }) as HTMLButtonElement).disabled).toBe(false));
    expect(screen.getByText("Paper handover is disabled.")).toBeTruthy();
    expect(fetch.mock.calls.every(([url, options]) => url === "/v1/paper/status" && !options?.method)).toBe(true);
    expect(screen.getByText(/Broker clock:/).textContent).toContain("2026-10-13T15:00:00Z");
  });

  it("saves manual values with handover explicitly disabled and the current revision", async () => {
    const fetch = server(); render(<AlpacaPaper />);
    await screen.findAllByText("100000 USD");
    fireEvent.change(screen.getByLabelText("Paper capital budget"), { target: { value: "40" } });
    fireEvent.click(screen.getByRole("button", { name: "Save paper preferences" }));
    await screen.findByText("Preferences saved with paper handover disabled.");
    const post = fetch.mock.calls.find(([url]) => url === "/v1/paper/preferences")!;
    expect(JSON.parse(post[1]!.body as string)).toMatchObject({ generation: 7, enabled: false,
      preferences: { capital_usd: "40", daily_loss_enabled: false } });
    expect(post[1]!.headers).toMatchObject({ "content-type": "application/json", "x-radar-paper-control": "1" });
    expect(fetch.mock.calls.map(([url]) => url)).toEqual(["/v1/paper/status", "/v1/paper/preferences", "/v1/paper/status"]);
  });

  it("delegates selected options only after an explicit handover click, preserving manual values", async () => {
    const fetch = server(); render(<AlpacaPaper />);
    await screen.findAllByText("100000 USD");
    fireEvent.click(screen.getByLabelText("Agent decides paper capital budget"));
    fireEvent.click(screen.getByLabelText("Agent decides paper maximum per trade"));
    expect((screen.getByLabelText("Paper capital budget") as HTMLInputElement).disabled).toBe(true);
    expect(fetch.mock.calls).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Enable paper handover" }));
    await screen.findByText("Paper permission granted; autonomous agent readiness is not yet verified.");
    const post = fetch.mock.calls.find(([url]) => url === "/v1/paper/preferences")!;
    expect(JSON.parse(post[1]!.body as string)).toEqual({ generation: 7, enabled: true, preferences: {
      capital_usd: "50", max_trade_usd: "10", daily_loss_usd: "5", daily_loss_enabled: false,
      agent_decides: { capital_usd: true, max_trade_usd: true, daily_loss_usd: false } } });
    fireEvent.click(screen.getByLabelText("Agent decides paper capital budget"));
    expect((screen.getByLabelText("Paper capital budget") as HTMLInputElement).value).toBe("50");
  });

  it("makes the daily cap optional and explains its current entry refusal", async () => {
    server(); render(<AlpacaPaper />); await screen.findAllByText("100000 USD");
    expect((screen.getByLabelText("Paper daily loss limit") as HTMLInputElement).disabled).toBe(true);
    fireEvent.click(screen.getByLabelText("Use paper daily loss cap"));
    expect(screen.getByText(/Daily loss measurement is not connected/)).toBeTruthy();
    expect((screen.getByLabelText("Paper daily loss limit") as HTMLInputElement).disabled).toBe(false);
    fireEvent.click(screen.getByLabelText("Agent decides paper daily loss limit"));
    fireEvent.click(screen.getByLabelText("Use paper daily loss cap"));
    expect((screen.getByLabelText("Agent decides paper daily loss limit") as HTMLInputElement).checked).toBe(true);
    expect(screen.queryByText(/Daily loss measurement is not connected/)).toBeNull();
  });

  it("pauses independently of invalid draft amounts and reloads the new revision", async () => {
    const initial = snapshot(); initial.handover_enabled = true;
    initial.effective_policy!.enabled = true;
    const fetch = server(initial); render(<AlpacaPaper />); await screen.findAllByText("100000 USD");
    fireEvent.change(screen.getByLabelText("Paper capital budget"), { target: { value: "invalid" } });
    fireEvent.click(screen.getByRole("button", { name: "Pause paper handover" }));
    await screen.findByText("Paper handover paused. Existing orders and positions are unchanged.");
    const post = fetch.mock.calls.find(([url]) => url === "/v1/paper/pause")!;
    expect(JSON.parse(post[1]!.body as string)).toEqual({ generation: 7 });
    expect(screen.getByText("Paper handover is disabled.")).toBeTruthy();
  });

  it("does not retry an unresolved write or let stale state enable another action", async () => {
    const retained = snapshot();
    const fetch = vi.fn(async (url: RequestInfo | URL) => url === "/v1/paper/status"
      ? Response.json(retained) : new Response(JSON.stringify({ error: "Private controller unavailable" }), { status: 503 }));
    vi.stubGlobal("fetch", fetch); render(<AlpacaPaper />); await screen.findAllByText("100000 USD");
    fireEvent.click(screen.getByRole("button", { name: "Enable paper handover" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("alert").textContent).toContain("Action outcome is unresolved");
    expect((screen.getByRole("button", { name: "Enable paper handover" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Enable paper handover" }));
    expect(fetch.mock.calls.filter(([url]) => url !== "/v1/paper/status")).toHaveLength(1);
    expect(screen.getByText(/Last snapshot below is unconfirmed/)).toBeTruthy();
    retained.generation = 9;
    fireEvent.click(screen.getByRole("button", { name: "Reload paper status" }));
    await waitFor(() => expect((screen.getByRole("button", { name: "Enable paper handover" }) as HTMLButtonElement).disabled).toBe(false));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("shows unavailable or invalid reads as unknown, never zero funds", async () => {
    for (const [value, status] of [[{ error: "Paper controller not installed" }, 503], [{ ...snapshot(), environment: "live" }, 200]] as const) {
      vi.stubGlobal("fetch", vi.fn(async () => Response.json(value, { status })));
      const view = render(<AlpacaPaper />); await screen.findByRole("alert");
      expect(screen.queryByText("Simulated cash")).toBeNull();
      expect(screen.queryByRole("button", { name: "Enable paper handover" })).toBeNull();
      view.unmount();
    }
  });

  it("keeps one write in flight and does not report success until the account reloads", async () => {
    let resolve: ((value: Response) => void) | undefined;
    const retained = snapshot();
    const fetch = vi.fn(async (url: RequestInfo | URL) => url === "/v1/paper/status" ? Response.json(retained)
      : new Promise<Response>((done) => { resolve = done; }));
    vi.stubGlobal("fetch", fetch); render(<AlpacaPaper />); await screen.findAllByText("100000 USD");
    fireEvent.click(screen.getByRole("button", { name: "Enable paper handover" }));
    fireEvent.click(screen.getByRole("button", { name: "Enable paper handover" }));
    expect(fetch.mock.calls.filter(([url]) => url !== "/v1/paper/status")).toHaveLength(1);
    expect(screen.queryByText(/Paper permission updated/)).toBeNull();
    retained.generation = 8; retained.handover_enabled = true;
    retained.effective_policy!.enabled = true;
    await act(async () => { resolve!(Response.json({ generation: 8 })); });
    await screen.findByText("Paper permission granted; autonomous agent readiness is not yet verified.");
  });
});
