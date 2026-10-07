// SPDX-License-Identifier: Apache-2.0
import { useEffect, useRef, useState } from "react";
import { PrivyProvider, usePrivy } from "@privy-io/react-auth";
import { useCreateWallet } from "@privy-io/react-auth/solana";

type Preferences = { capital_usd: string; max_trade_usd: string; daily_loss_usd: string; autonomous_requested: boolean };
type Wallet = { address: string; id: string; delegated: boolean };
type Holdings = { wallet: string; slot: number; age_seconds: number; sol: { ui_amount: string }; tokens: { mint: string; ui_amount: string }[] };
const empty: Preferences = { capital_usd: "", max_trade_usd: "", daily_loss_usd: "", autonomous_requested: false };
const button = "rounded border border-[var(--color-line)] px-3 py-2 text-sm disabled:opacity-50";

async function request<T>(path: string, token: string, signal: AbortSignal, body?: Preferences): Promise<T> {
  const response = await fetch(`/automation/${path}`, {
    method: body ? "POST" : "GET", credentials: "same-origin", cache: "no-store", signal,
    headers: { Authorization: `Bearer ${token}`, ...(body ? { "Content-Type": "application/json" } : {}) },
    ...(body ? { body: JSON.stringify(body) } : {}),
  });
  const payload: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const message = payload && typeof payload === "object" && "error" in payload && typeof payload.error === "string"
      ? payload.error : `Wallet request failed (${response.status}).`;
    throw new Error(message);
  }
  if (!payload || typeof payload !== "object") throw new Error("Wallet response could not be read.");
  return payload as T;
}

export default function PrivyWallet({ appId }: { appId: string }) {
  return <PrivyProvider appId={appId} config={{
    loginMethods: ["email"], appearance: { walletChainType: "solana-only" },
    embeddedWallets: { ethereum: { createOnLogin: "off" }, solana: { createOnLogin: "off" } },
  }}><Connection /></PrivyProvider>;
}

function Connection() {
  const { ready, authenticated, user, login, logout } = usePrivy();
  if (!ready) return <p role="status">Starting Privy…</p>;
  if (!authenticated || !user) return <div className="mt-3 space-y-3">
    <p className="text-sm text-[var(--color-dim)]">Sign in to access your dedicated Solana wallet. Creating a wallet is a separate step.</p>
    <button className={button} onClick={() => login()}>Sign in with Privy</button>
  </div>;
  return <div className="mt-3 space-y-3">
    <button className={button} onClick={() => { void logout(); }}>Sign out of Privy</button>
    <OwnerWallet key={user.id} />
  </div>;
}

export function OwnerWallet() {
  const { getAccessToken } = usePrivy();
  const { createWallet } = useCreateWallet();
  const [wallet, setWallet] = useState<Wallet | null>(null);
  const [known, setKnown] = useState(false);
  const [holdings, setHoldings] = useState<Holdings | null>(null);
  const [balanceError, setBalanceError] = useState<string | null>(null);
  const [preferences, setPreferences] = useState<Preferences>(empty);
  const [settingsReady, setSettingsReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const lifetime = useRef<AbortController | null>(null);
  const generation = useRef(0);

  async function token() {
    const value = await getAccessToken();
    if (!value) throw new Error("Privy session expired. Sign in again.");
    return value;
  }

  async function load(signal: AbortSignal) {
    const current = ++generation.current;
    const active = () => !signal.aborted && current === generation.current;
    setBusy(true); setError(null); setKnown(false); setWallet(null); setHoldings(null); setBalanceError(null);
    setSettingsReady(false); setPreferences(empty); setNotice(null);
    try {
      const access = await token();
      if (!active()) return;
      const status = await request<{ wallet: Wallet | null }>("wallet", access, signal);
      if (!active()) return;
      if (status.wallet !== null && (!status.wallet?.address || !status.wallet.id)) throw new Error("Wallet ownership response could not be read.");
      setWallet(status.wallet); setKnown(true);
      if (!status.wallet) return;
      const verifiedAddress = status.wallet.address;
      await Promise.all([
        request<{ preferences: Preferences | null }>("limits", access, signal).then((saved) => {
          if (!active()) return;
          if (!("preferences" in saved)) throw new Error("Saved wallet settings could not be read.");
          setPreferences(saved.preferences ?? empty); setSettingsReady(true);
        }).catch((cause: unknown) => { if (active()) setError(cause instanceof Error ? cause.message : "Saved wallet settings are unknown."); }),
        request<Holdings>("balance", access, signal).then((balance) => {
          if (!active()) return;
          if (balance.wallet !== verifiedAddress || !balance.sol || typeof balance.sol.ui_amount !== "string" || !Array.isArray(balance.tokens)) throw new Error("Wallet balance could not be verified.");
          setHoldings(balance);
        }).catch((cause: unknown) => { if (active()) setBalanceError(cause instanceof Error ? cause.message : "Could not read the wallet balance."); }),
      ]);
    } catch (cause) { if (active()) setError(cause instanceof Error ? cause.message : "Could not check the wallet."); }
    finally { if (active()) setBusy(false); }
  }

  useEffect(() => {
    const controller = new AbortController(); lifetime.current = controller;
    void load(controller.signal);
    return () => { controller.abort(); lifetime.current = null; generation.current++; };
    // The verified Privy identity is the parent component key. SDK function identity may change each render.
  }, []);

  async function create() {
    const signal = lifetime.current?.signal;
    if (!signal || signal.aborted || busy) return;
    setBusy(true); setError(null);
    try { await createWallet(); if (!signal.aborted) await load(signal); }
    catch { if (!signal.aborted) { setError("Wallet creation did not complete. Refresh to check its status before trying again."); setBusy(false); setKnown(false); } }
  }

  async function save() {
    const signal = lifetime.current?.signal;
    if (!signal || signal.aborted || busy) return;
    setBusy(true); setError(null); setNotice(null);
    try {
      const access = await token();
      if (signal.aborted) return;
      await request("limits", access, signal, preferences);
      if (!signal.aborted) setNotice("Settings saved. Autonomous trading remains inactive until execution is connected.");
    } catch (cause) { if (!signal.aborted) setError(cause instanceof Error ? cause.message : "Could not save settings."); }
    finally { if (!signal.aborted) setBusy(false); }
  }

  return <div className="space-y-4">
    {error && <p role="alert" className="text-sm text-[var(--color-red)]">{error}</p>}
    {!known && !error && <p role="status">Checking wallet ownership…</p>}
    {known && !wallet && <div className="space-y-2">
      <p>No embedded Solana wallet yet.</p>
      <button className={button} disabled={busy} onClick={() => { void create(); }}>Create Solana wallet</button>
    </div>}
    {wallet && <>
      <p className="break-all text-sm">Wallet: <span className="font-mono">{wallet.address}</span></p>
      <div className="rounded border border-[var(--color-line)] p-3">
        <h4 className="font-medium">Live wallet balance</h4>
        {holdings ? <div className="mt-2 text-sm">
          <p>{holdings.sol.ui_amount} SOL</p>
          <p className="text-[var(--color-dim)]">Read at slot {holdings.slot}; snapshot age at retrieval: {holdings.age_seconds}s.</p>
          {holdings.tokens.length === 0 ? <p>No token holdings reported by this read.</p>
            : <ul>{holdings.tokens.map((held, index) => <li className="break-all" key={`${held.mint}-${index}`}>{held.ui_amount} — {held.mint}</li>)}</ul>}
        </div> : <p className="mt-2 text-sm">{balanceError ? `Balance unknown. ${balanceError}` : "Reading balance…"}</p>}
      </div>
      <form onSubmit={(event) => { event.preventDefault(); void save(); }} className="space-y-3">
        <h4 className="font-medium">Trading limits (USD)</h4>
        <p className="text-sm text-[var(--color-dim)]">Set the capital Radar may use, maximum amount per trade, and daily loss stop. These settings are saved as a draft; signing authority is not active.</p>
        <fieldset disabled={busy || !settingsReady} className="space-y-3">
          {([ ["capital_usd", "Capital budget"], ["max_trade_usd", "Maximum per trade"], ["daily_loss_usd", "Daily loss limit"] ] as const).map(([field, label]) => <label key={field} className="block text-sm">
            {label}<input aria-label={label} required type="text" inputMode="decimal" autoComplete="off" value={preferences[field]}
              onChange={(event) => { setPreferences({ ...preferences, [field]: event.target.value }); setNotice(null); }}
              className="mt-1 block w-full rounded border border-[var(--color-line)] bg-[var(--color-bg)] px-3 py-2" />
          </label>)}
          <label className="flex items-start gap-2 text-sm"><input type="checkbox" checked={preferences.autonomous_requested}
            onChange={(event) => { setPreferences({ ...preferences, autonomous_requested: event.target.checked }); setNotice(null); }} />
            Let ChatGPT choose trades and sizing within my saved limits</label>
          <p className="text-sm text-[var(--color-dim)]">Only you can change these limits. Requesting autonomy does not enable trading.</p>
          <button className={button} type="submit">Save wallet settings</button>
        </fieldset>
        {notice && <p role="status" className="text-sm">{notice}</p>}
      </form>
    </>}
    <button className={button} disabled={busy} onClick={() => { if (lifetime.current) void load(lifetime.current.signal); }}>Refresh wallet</button>
  </div>;
}
