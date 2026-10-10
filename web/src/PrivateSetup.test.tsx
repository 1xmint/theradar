// SPDX-License-Identifier: Apache-2.0
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PrivateSetup } from "./PrivateSetup";
import { Agent } from "./Agent";

vi.mock("./PrivyWallet", () => ({ default: ({ appId }: { appId: string }) => <p role="status">App configured: {appId}. Wallet ownership and signing delegation have not been verified.</p> }));

afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });

function server(link: unknown = { error: "not configured" }, status = 404, privy: unknown = { error: "not configured" }, privyStatus = 503) {
  return vi.fn(async (url: RequestInfo | URL, options?: RequestInit) => {
    if (url === "/health") return new Response(JSON.stringify({ agent: { configured: false } }));
    if (url === "/v1/customer/config") return new Response(JSON.stringify(privy), { status: privyStatus });
    if (options?.method === "POST") return new Response(JSON.stringify({
      state: "waiting", verification_url: "https://auth.openai.com/device", user_code: "ABCD-EFGH", seconds_elapsed: 0,
    }));
    return new Response(JSON.stringify(link), { status });
  });
}

describe("private setup", () => {
  it("shows an unavailable connection as unknown and keeps linking disabled", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("Network unavailable")));
    render(<PrivateSetup />);
    expect(await screen.findByText("Could not check the ChatGPT connection. Reload to try again.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Connect ChatGPT" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByText(/Checking the ChatGPT connection/)).toBeNull();
  });
  it("shows missing configuration and cannot initiate a login or a trade", async () => {
    const fetchImpl = server();
    vi.stubGlobal("fetch", fetchImpl);
    render(<PrivateSetup />);
    expect(await screen.findByText(/ChatGPT connection is not configured/)).toBeTruthy();
    expect(await screen.findByText("No Privy app is configured on this instance.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Connect ChatGPT" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText("Autonomous execution: not enabled")).toBeTruthy();
    expect(fetchImpl.mock.calls.every(([, options]) => options?.method !== "POST")).toBe(true);
  });

  it("only starts subscription linking when the owner presses Connect", async () => {
    const fetchImpl = server({ state: "idle" }, 200);
    vi.stubGlobal("fetch", fetchImpl);
    render(<PrivateSetup />);
    await screen.findByText(/Connect your ChatGPT account/);
    expect(fetchImpl.mock.calls.every(([, options]) => options?.method !== "POST")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Connect ChatGPT" }));
    expect(await screen.findByText("ABCD-EFGH")).toBeTruthy();
    expect(screen.getByRole("link", { name: "https://auth.openai.com/device" }).getAttribute("href"))
      .toBe("https://auth.openai.com/device");
    expect(fetchImpl.mock.calls.filter(([, options]) => options?.method === "POST")).toHaveLength(1);
  });

  it("does not equate a configured Privy app with wallet delegation", async () => {
    vi.stubGlobal("fetch", server({ state: "idle" }, 200, { privy_app_id: "my-public-app-id" }, 200));
    render(<PrivateSetup />);
    expect(await screen.findByText(/App configured: my-public-app-id/)).toBeTruthy();
    expect(screen.getByRole("status").textContent).toContain("have not been verified");
  });

  it("keeps a completed login visible and stops polling", async () => {
    vi.useFakeTimers();
    let polls = 0;
    vi.stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL) => {
      if (url === "/v1/customer/config") return new Response("{}", { status: 503 });
      polls++;
      return new Response(JSON.stringify(polls === 1 ? {
        state: "waiting", verification_url: "https://auth.openai.com/device", user_code: "ABCD-EFGH", seconds_elapsed: 0,
      } : { state: "linked" }));
    }));
    await act(async () => { render(<PrivateSetup />); });
    await act(async () => { await vi.advanceTimersByTimeAsync(3000); });
    expect(screen.getByText(/Linked\. The CLI holds/)).toBeTruthy();
    await act(async () => { await vi.advanceTimersByTimeAsync(9000); });
    expect(polls).toBe(2);
  });

  it("checks inference separately from subscription linking", async () => {
    const fetchImpl = server({ state: "idle" }, 200);
    vi.stubGlobal("fetch", fetchImpl);
    render(<Agent alwaysShow />);
    expect(await screen.findByText(/No model provider is configured/)).toBeTruthy();
    expect(fetchImpl.mock.calls.map(([url]) => url)).toEqual(["/health"]);
    expect(screen.queryByRole("button", { name: "Connect ChatGPT" })).toBeNull();
  });
});
