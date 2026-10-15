// SPDX-License-Identifier: Apache-2.0
import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, paper, type PaperLimit, type PaperPreferences, type PaperStatus } from "./api";

const empty: PaperPreferences = { capital_usd: "", max_trade_usd: "", daily_loss_usd: "", daily_loss_enabled: false,
  agent_decides: { capital_usd: false, max_trade_usd: false, daily_loss_usd: false } };
const fields: [PaperLimit, string][] = [["capital_usd", "Capital budget"], ["max_trade_usd", "Maximum per trade"], ["daily_loss_usd", "Daily loss limit"]];

function checked(value: PaperStatus): PaperStatus {
  if (value.version !== "alpaca-paper-control-v1" || value.environment !== "paper"
      || !Number.isSafeInteger(value.generation) || value.generation < 0
      || value.execution_authority !== false || value.agent_lifecycle_verified !== false || value.profit_measured !== false
      || typeof value.handover_enabled !== "boolean" || value.account?.id !== value.account_id
      || typeof value.account?.cash !== "string" || typeof value.account?.equity !== "string"
      || value.account.currency !== "USD" || value.account.status !== "ACTIVE") {
    throw new Error("Paper account status could not be verified.");
  }
  return value;
}

function message(cause: unknown): string {
  return cause instanceof ApiError ? cause.detail : "Paper account status could not be verified.";
}

export function AlpacaPaper() {
  const [snapshot, setSnapshot] = useState<PaperStatus | null>(null);
  const [preferences, setPreferences] = useState<PaperPreferences>(empty);
  const [ready, setReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const pending = useRef(false);

  const read = useCallback(async (signal?: AbortSignal) => {
    const value = checked(await paper.status(signal));
    if (signal?.aborted) return;
    setSnapshot(value);
    setPreferences(value.preferences ?? empty);
    setReady(true);
  }, []);

  async function refresh(signal?: AbortSignal) {
    if (pending.current) return;
    pending.current = true; setBusy(true); setReady(false); setError(null); setNotice(null);
    try { await read(signal); }
    catch (cause) { if (!signal?.aborted) setError(message(cause)); }
    finally { pending.current = false; if (!signal?.aborted) setBusy(false); }
  }

  useEffect(() => {
    const controller = new AbortController();
    setBusy(true);
    void read(controller.signal)
      .catch((cause: unknown) => { if (!controller.signal.aborted) setError(message(cause)); })
      .finally(() => { if (!controller.signal.aborted) setBusy(false); });
    return () => controller.abort();
  }, [read]);

  async function change(action: "save" | "enable" | "pause") {
    if (!ready || !snapshot || pending.current) return;
    pending.current = true; setBusy(true); setReady(false); setError(null); setNotice(null);
    try {
      if (action === "pause") await paper.pause(snapshot.generation);
      else await paper.configure(snapshot.generation, preferences, action === "enable");
      await read();
      setNotice(action === "pause" ? "Paper handover paused. Existing orders and positions are unchanged."
        : action === "save" ? "Preferences saved with paper handover disabled." : "Paper permission updated. Check the status below for agent readiness.");
    } catch (cause) {
      setError(`Action outcome is unresolved. Reload status before another action. ${message(cause)}`);
    } finally { pending.current = false; setBusy(false); }
  }

  return <div className="space-y-3">
    <div className="flex items-center justify-between gap-3">
      <h4 className="font-medium">Alpaca paper account</h4>
      <button type="button" disabled={busy} onClick={() => void refresh()} className="rounded border px-3 py-1 text-sm">Reload paper status</button>
    </div>
    {!snapshot && !error && <p>Checking paper account…</p>}
    {error && <p role="alert">{error}</p>}
    {notice && <p role="status">{notice}</p>}
    {snapshot && <>
      {!ready && !busy && <p>Last snapshot below is unconfirmed. Reload before changing handover.</p>}
      <p className="text-sm">Account: {snapshot.account_id}</p>
      <dl className="grid grid-cols-2 gap-2 text-sm">
        <dt>Simulated cash</dt><dd>{snapshot.account.cash} USD</dd>
        <dt>Simulated equity</dt><dd>{snapshot.account.equity} USD</dd>
        <dt>Positions</dt><dd>{snapshot.positions === null ? "Unavailable" : snapshot.positions.length}</dd>
        <dt>Regular session</dt><dd>{snapshot.market_clock === null ? "Unknown" : snapshot.market_clock.is_open ? "Open" : "Closed"}</dd>
      </dl>
      {snapshot.market_clock && <p className="text-xs">Broker clock: {snapshot.market_clock.timestamp}. Reload for a new account snapshot.</p>}
      <p className="text-sm">{snapshot.handover_enabled
        ? "Paper permission granted; autonomous agent readiness is not yet verified."
        : "Paper handover is disabled."}</p>
      <form onSubmit={(event) => { event.preventDefault(); void change("save"); }} className="space-y-3">
        <fieldset disabled={!ready || busy} className="space-y-3">
          <legend className="font-medium">Paper trading preferences (USD)</legend>
          <p className="text-sm">Set each amount yourself, or check Agent decides to let the agent choose within the simulated account balance. Saving preferences disables handover; enabling it is a separate action.</p>
          <label className="flex gap-2 text-sm"><input type="checkbox" aria-label="Use paper daily loss cap" checked={preferences.daily_loss_enabled}
            onChange={(event) => setPreferences({ ...preferences, daily_loss_enabled: event.target.checked })} />Use daily loss cap (optional)</label>
          {preferences.daily_loss_enabled && <p className="text-sm">Daily loss measurement is not connected yet. Selecting this cap currently blocks new entries.</p>}
          {fields.map(([field, label]) => {
            const active = field !== "daily_loss_usd" || preferences.daily_loss_enabled;
            return <div key={field} className="space-y-1 text-sm">
              <label>{label}<input aria-label={`Paper ${label.toLowerCase()}`} type="text" inputMode="decimal" autoComplete="off"
                required={active && !preferences.agent_decides[field]} pattern="[0-9]+(\.[0-9]{1,6})?"
                disabled={!active || preferences.agent_decides[field]} value={preferences.agent_decides[field] ? "" : preferences[field]}
                onChange={(event) => setPreferences({ ...preferences, [field]: event.target.value })} className="ml-2 rounded border bg-transparent px-2 py-1" /></label>
              <label className="flex gap-2"><input type="checkbox" aria-label={`Agent decides paper ${label.toLowerCase()}`} disabled={!active}
                checked={preferences.agent_decides[field]} onChange={(event) => setPreferences({ ...preferences,
                  agent_decides: { ...preferences.agent_decides, [field]: event.target.checked } })} />Agent decides</label>
            </div>;
          })}
          <button type="submit" disabled={!ready || busy} className="rounded border px-3 py-2 text-sm">Save paper preferences</button>
          <button type="button" disabled={!ready || busy} onClick={(event) => { if (event.currentTarget.form?.reportValidity()) void change("enable"); }}
            className="ml-2 rounded border px-3 py-2 text-sm">Enable paper handover</button>
        </fieldset>
      </form>
      <button type="button" disabled={!ready || busy} onClick={() => void change("pause")} className="rounded border px-3 py-2 text-sm">Pause paper handover</button>
      <p className="text-xs">Pause prevents new submissions after revocation checks. It does not cancel orders already accepted or close positions. Paper results do not establish live profit.</p>
    </>}
  </div>;
}
