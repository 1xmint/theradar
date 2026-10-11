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
  let preferences: unknown = null;
  return vi.fn(async (url: RequestInfo | URL, options?: RequestInit) => {
    if (url === "/automation/wallet") return Response.json({ wallet, execution_enabled: false });
    if (url === "/automation/limits") {
      if (options?.body) preferences = JSON.parse(options.body as string);
      return Response.json({ preferences, execution_enabled: false });
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
    fireEvent.click(screen.getByRole("checkbox", { name: "Use daily loss cap" }));
    for (const [label, value] of [["Capital budget", "100"], ["Maximum per trade", "10"], ["Daily loss limit", "5"]]) fireEvent.change(screen.getByLabelText(label!), { target: { value } });
    fireEvent.click(screen.getByRole("button", { name: "Save wallet settings" }));
    await screen.findByText("Settings saved. Autonomous trading remains inactive until execution is connected.");
    const post = fetch.mock.calls.find(([, options]) => options?.method === "POST")!;
    expect(JSON.parse(post[1]!.body as string)).toEqual({ capital_usd: "100", max_trade_usd: "10", daily_loss_usd: "5", daily_loss_enabled: true, autonomous_requested: true,
      agent_decides: { capital_usd: false, max_trade_usd: false, daily_loss_usd: false } });
    expect(post[1]!.headers).toMatchObject({ Authorization: "Bearer test-token" });
  });
  it("saves independent agent choices and restores manual values when unchecked", async () => {
    const fetch = server(); vi.stubGlobal("fetch", fetch); render(<PrivyWallet appId="app" />);
    await screen.findByText("1.5 SOL");
    fireEvent.click(screen.getByRole("checkbox", { name: "Use daily loss cap" }));
    for (const [label, value] of [["Capital budget", "100"], ["Maximum per trade", "10"], ["Daily loss limit", "5"]]) fireEvent.change(screen.getByLabelText(label!), { target: { value } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Agent decides maximum per trade" }));
    expect((screen.getByLabelText("Maximum per trade") as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByLabelText("Capital budget") as HTMLInputElement).disabled).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Save wallet settings" }));
    await screen.findByText("Settings saved. Autonomous trading remains inactive until execution is connected.");
    const post = fetch.mock.calls.find(([, options]) => options?.method === "POST")!;
    expect(JSON.parse(post[1]!.body as string).agent_decides).toEqual({ capital_usd: false, max_trade_usd: true, daily_loss_usd: false });
    fireEvent.click(screen.getByRole("button", { name: "Refresh wallet" }));
    await screen.findByText("1.5 SOL");
    expect((screen.getByRole("checkbox", { name: "Agent decides maximum per trade" }) as HTMLInputElement).checked).toBe(true);
    fireEvent.click(screen.getByRole("checkbox", { name: "Agent decides maximum per trade" }));
    expect((screen.getByLabelText("Maximum per trade") as HTMLInputElement).disabled).toBe(false);
    expect((screen.getByLabelText("Maximum per trade") as HTMLInputElement).value).toBe("10");
    fireEvent.click(screen.getByRole("button", { name: "Save wallet settings" }));
    await screen.findByText("Settings saved. Autonomous trading remains inactive until execution is connected.");
    const posts = fetch.mock.calls.filter(([, options]) => options?.method === "POST");
    expect(JSON.parse(posts[1]![1]!.body as string).agent_decides.max_trade_usd).toBe(false);
  });
  it("allows all options to be agent chosen without entering manual numbers", async () => {
    const fetch = server(); vi.stubGlobal("fetch", fetch); render(<PrivyWallet appId="app" />);
    await screen.findByText("1.5 SOL");
    for (const checkbox of screen.getAllByRole("checkbox")) fireEvent.click(checkbox);
    fireEvent.click(screen.getByRole("button", { name: "Save wallet settings" }));
    await screen.findByText("Settings saved. Autonomous trading remains inactive until execution is connected.");
    const post = fetch.mock.calls.find(([, options]) => options?.method === "POST")!;
    expect(JSON.parse(post[1]!.body as string)).toEqual({ capital_usd: "", max_trade_usd: "", daily_loss_usd: "", daily_loss_enabled: true, autonomous_requested: true,
      agent_decides: { capital_usd: true, max_trade_usd: true, daily_loss_usd: true } });
  });
  it("keeps older saved numeric limits manual when agent choices are absent", async () => {
    const fetch = server();
    vi.stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL, options?: RequestInit) => url === "/automation/limits"
      ? Response.json({ preferences: { capital_usd: "100", max_trade_usd: "10", daily_loss_usd: "5", autonomous_requested: true }, execution_enabled: false }) : fetch(url, options)));
    render(<PrivyWallet appId="app" />);
    await screen.findByDisplayValue("100");
    for (const checkbox of screen.getAllByRole("checkbox", { name: /^Agent decides/ })) expect((checkbox as HTMLInputElement).checked).toBe(false);
    expect((screen.getByRole("checkbox", { name: "Use daily loss cap" }) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("Maximum per trade") as HTMLInputElement).value).toBe("10");
  });
  it("defaults new drafts to no daily cap and preserves the chosen cap across toggles and refresh", async () => {
    const fetch = server(); vi.stubGlobal("fetch", fetch); render(<PrivyWallet appId="app" />);
    await screen.findByText("1.5 SOL");
    const cap = screen.getByRole("checkbox", { name: "Use daily loss cap" }) as HTMLInputElement;
    const amount = screen.getByLabelText("Daily loss limit") as HTMLInputElement;
    const agent = screen.getByRole("checkbox", { name: "Agent decides daily loss limit" }) as HTMLInputElement;
    expect(cap.checked).toBe(false); expect(amount.disabled).toBe(true); expect(amount.required).toBe(false); expect(agent.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Capital budget"), { target: { value: "100" } });
    fireEvent.change(screen.getByLabelText("Maximum per trade"), { target: { value: "10" } });
    fireEvent.click(cap); expect(amount.disabled).toBe(false); expect(amount.required).toBe(true);
    fireEvent.change(amount, { target: { value: "5" } });
    fireEvent.click(agent); expect(amount.disabled).toBe(true);
    fireEvent.click(cap); expect(agent.disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Save wallet settings" }));
    await screen.findByText("Settings saved. Autonomous trading remains inactive until execution is connected.");
    const post = fetch.mock.calls.find(([, options]) => options?.method === "POST")!;
    expect(JSON.parse(post[1]!.body as string)).toMatchObject({ daily_loss_enabled: false, daily_loss_usd: "5", agent_decides: { daily_loss_usd: true } });
    fireEvent.click(screen.getByRole("button", { name: "Refresh wallet" }));
    await screen.findByText("1.5 SOL");
    await waitFor(() => expect((screen.getByRole("button", { name: "Save wallet settings" }) as HTMLButtonElement).disabled).toBe(false));
    const restoredCap = screen.getByRole("checkbox", { name: "Use daily loss cap" }) as HTMLInputElement;
    const restoredAgent = screen.getByRole("checkbox", { name: "Agent decides daily loss limit" }) as HTMLInputElement;
    expect(restoredCap.checked).toBe(false); expect(restoredAgent.checked).toBe(true);
    fireEvent.click(restoredCap); fireEvent.click(restoredAgent);
    const restoredAmount = screen.getByLabelText("Daily loss limit") as HTMLInputElement;
    expect(restoredAmount.value).toBe("5"); expect(restoredAmount.disabled).toBe(false);
  });
  it("does not acknowledge agent choices when saving fails", async () => {
    const fetch = server();
    vi.stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL, options?: RequestInit) => options?.method === "POST"
      ? Response.json({ error: "Could not save wallet settings. Trading remains inactive." }, { status: 503 }) : fetch(url, options)));
    render(<PrivyWallet appId="app" />);
    await screen.findByText("1.5 SOL");
    await waitFor(() => expect((screen.getByLabelText("Capital budget") as HTMLInputElement).disabled).toBe(false));
    for (const checkbox of screen.getAllByRole("checkbox")) fireEvent.click(checkbox);
    fireEvent.click(screen.getByRole("button", { name: "Save wallet settings" }));
    await screen.findByRole("alert");
    expect(screen.queryByText("Settings saved. Autonomous trading remains inactive until execution is connected.")).toBeNull();
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
