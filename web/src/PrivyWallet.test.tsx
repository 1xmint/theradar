// SPDX-License-Identifier: Apache-2.0
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import PrivyWallet from "./PrivyWallet";

const sdk = vi.hoisted(() => ({ authenticated: true, walletsReady: true, id: "alice", token: vi.fn(), create: vi.fn(), login: vi.fn(), config: null as unknown }));
vi.mock("@privy-io/react-auth", () => ({
  PrivyProvider: ({ children, config }: { children: React.ReactNode; config: unknown }) => { sdk.config = config; return children; },
  usePrivy: () => ({ ready: true, authenticated: sdk.authenticated, user: { id: sdk.id }, getAccessToken: sdk.token, login: sdk.login, logout: vi.fn() }),
}));
vi.mock("@privy-io/react-auth/solana", () => ({ useCreateWallet: () => ({ createWallet: sdk.create }), useWallets: () => ({ ready: sdk.walletsReady }) }));
beforeEach(() => { sdk.authenticated = true; sdk.walletsReady = true; sdk.id = "alice"; sdk.token.mockReset().mockResolvedValue("test-token"); sdk.create.mockReset().mockResolvedValue({}); sdk.login.mockReset(); });
afterEach(() => vi.unstubAllGlobals());

function server(wallet: unknown = { address: "verified-address", id: "wallet", delegated: false }, failedBalance = false) {
  return vi.fn(async (url: RequestInfo | URL, options?: RequestInit) => {
    if (url === "/automation/wallet") return Response.json({ wallet, execution_enabled: false });
    if (url === "/automation/limits") {
      return Response.json({ preferences: options?.body ? JSON.parse(options.body as string) : null, execution_enabled: false });
    }
    return failedBalance ? Response.json({ error: "RPC unavailable" }, { status: 502 })
      : Response.json({ wallet: "verified-address", slot: 123, age_seconds: 0, sol: { ui_amount: "1.5" }, tokens: [] });
  });
}

describe("private Privy wallet", () => {
  it("does not offer another creation when Privy completed it but lookup is still absent", async () => {
    vi.stubGlobal("fetch", server(null)); render(<PrivyWallet appId="app" />);
    await screen.findByText("No embedded Solana wallet yet.");
    fireEvent.click(screen.getByRole("button", { name: "Create Solana wallet" }));
    await screen.findByText("Privy completed wallet creation, but Radar has not verified it yet. Refresh wallet to check again; do not create another wallet.");
    expect(screen.queryByRole("button", { name: "Create Solana wallet" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Refresh wallet" }));
    await waitFor(() => expect((screen.getByRole("button", { name: "Refresh wallet" }) as HTMLButtonElement).disabled).toBe(false));
    expect(screen.queryByRole("button", { name: "Create Solana wallet" })).toBeNull();
    expect(sdk.create).toHaveBeenCalledOnce();
  });
  it("shows a verified device wallet and balance without a server wallet ID", async () => {
    vi.stubGlobal("fetch", server({ address: "verified-address", id: null, delegated: false }));
    render(<PrivyWallet appId="app" />);
    await screen.findByText("1.5 SOL");
    expect(screen.queryByRole("button", { name: "Create Solana wallet" })).toBeNull();
    expect(sdk.create).not.toHaveBeenCalled();
  });
  it("waits for the Solana wallet connection before allowing creation", async () => {
    sdk.walletsReady = false; vi.stubGlobal("fetch", server(null));
    const view = render(<PrivyWallet appId="app" />);
    await screen.findByText("No embedded Solana wallet yet.");
    await waitFor(() => expect((screen.getByRole("button", { name: "Refresh wallet" }) as HTMLButtonElement).disabled).toBe(false));
    const create = screen.getByRole("button", { name: "Create Solana wallet" }) as HTMLButtonElement;
    expect(create.disabled).toBe(true);
    fireEvent.click(create); expect(sdk.create).not.toHaveBeenCalled();
    sdk.walletsReady = true; view.rerender(<PrivyWallet appId="app" />);
    expect(create.disabled).toBe(false);
    fireEvent.click(create); await waitFor(() => expect(sdk.create).toHaveBeenCalledOnce());
  });
  it("only signs in or creates a wallet after an explicit owner action", async () => {
    sdk.authenticated = false;
    const fetch = server(null); vi.stubGlobal("fetch", fetch);
    const view = render(<PrivyWallet appId="app" />);
    expect(sdk.config).toMatchObject({ embeddedWallets: { ethereum: { createOnLogin: "off" }, solana: { createOnLogin: "off" } } });
    expect(fetch).not.toHaveBeenCalled(); expect(sdk.create).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Sign in with Privy" }));
    expect(sdk.login).toHaveBeenCalledOnce();
    sdk.authenticated = true; view.rerender(<PrivyWallet appId="app" />);
    await screen.findByText("No embedded Solana wallet yet.");
    expect(sdk.create).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Create Solana wallet" }));
    await waitFor(() => expect(sdk.create).toHaveBeenCalledOnce());
  });
  it("shows a verified balance and saves explicit limits without enabling execution", async () => {
    const fetch = server(); vi.stubGlobal("fetch", fetch); render(<PrivyWallet appId="app" />);
    await screen.findByText("1.5 SOL");
    await waitFor(() => expect((screen.getByLabelText("Capital budget") as HTMLInputElement).disabled).toBe(false));
    expect((screen.getByLabelText("Capital budget") as HTMLInputElement).value).toBe("");
    expect(fetch.mock.calls.every(([, options]) => options?.method !== "POST")).toBe(true);
    for (const [label, value] of [["Capital budget", "100"], ["Maximum per trade", "10"], ["Daily loss limit", "5"]]) fireEvent.change(screen.getByLabelText(label!), { target: { value } });
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Save wallet settings" }));
    await screen.findByText("Settings saved. Autonomous trading remains inactive until execution is connected.");
    const post = fetch.mock.calls.find(([, options]) => options?.method === "POST")!;
    expect(JSON.parse(post[1]!.body as string)).toEqual({ capital_usd: "100", max_trade_usd: "10", daily_loss_usd: "5", autonomous_requested: true });
    expect(post[1]!.headers).toMatchObject({ Authorization: "Bearer test-token" });
  });
  it("reports failed balances as unknown instead of zero", async () => {
    vi.stubGlobal("fetch", server(undefined, true)); render(<PrivyWallet appId="app" />);
    await screen.findByText("Balance unknown. RPC unavailable");
    expect(screen.queryByText("0 SOL")).toBeNull();
  });
  it("does not show balances for a different wallet", async () => {
    const fetch = server(); vi.stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL, options?: RequestInit) => url === "/automation/balance"
      ? Response.json({ wallet: "another-wallet", slot: 123, sol: { ui_amount: "900" }, tokens: [] }) : fetch(url, options)));
    render(<PrivyWallet appId="app" />);
    await screen.findByText("Balance unknown. Wallet balance could not be verified.");
    expect(screen.queryByText("900 SOL")).toBeNull();
  });
  it("clears account data and discards an earlier identity's pending response", async () => {
    let resolve!: (value: Response) => void;
    const pending = new Promise<Response>((done) => { resolve = done; });
    vi.stubGlobal("fetch", vi.fn((url: RequestInfo | URL) => url === "/automation/wallet" && sdk.id === "alice" ? pending : Promise.resolve(Response.json({ wallet: null }))));
    const view = render(<PrivyWallet appId="app" />);
    await waitFor(() => expect(sdk.token).toHaveBeenCalledOnce());
    sdk.id = "bob"; view.rerender(<PrivyWallet appId="app" />);
    await screen.findByText("No embedded Solana wallet yet.");
    resolve(Response.json({ wallet: { address: "alice-address", id: "alice-wallet" } }));
    await waitFor(() => expect(screen.queryByText("alice-address")).toBeNull());
  });
  it("does not read wallet data when Privy has no valid session token", async () => {
    sdk.token.mockResolvedValue(null); const fetch = server(); vi.stubGlobal("fetch", fetch);
    render(<PrivyWallet appId="app" />);
    await screen.findByText("Privy session expired. Sign in again.");
    expect(fetch).not.toHaveBeenCalled();
  });
});
