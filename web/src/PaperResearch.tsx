// SPDX-License-Identifier: Apache-2.0
import { useEffect, useState } from "react";
import { paper } from "./api";
import type { PaperResearchThread } from "./api";

function valid(thread: PaperResearchThread): boolean {
  return thread?.version === "alpaca-paper-research-v1" && typeof thread.account_id === "string"
    && thread.account_id.length > 0 && Number.isSafeInteger(thread.as_of_unix_ms) && thread.as_of_unix_ms > 0
    && thread.execution_authority === false && thread.source_claims_verified === false
    && thread.truncated === false && typeof thread.available === "boolean" && Array.isArray(thread.messages)
    && thread.messages.length <= 128 && (thread.available || thread.messages.length === 0)
    && thread.messages.every((m) => typeof m?.id === "string" && (m.role === "user" || m.role === "assistant")
      && typeof m.text === "string" && m.text.trim().length > 0 && new TextEncoder().encode(m.text).length <= 4000
      && Number.isSafeInteger(m.received_at_unix_ms) && m.received_at_unix_ms > 0
      && m.received_at_unix_ms <= thread.as_of_unix_ms);
}

export function PaperResearch() {
  const [thread, setThread] = useState<PaperResearchThread | null>(null);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [blocked, setBlocked] = useState(false);
  const [pending, setPending] = useState<{ nonce: string; text: string } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    paper.research(controller.signal).then((value) => {
      if (controller.signal.aborted) return;
      if (!valid(value)) throw new Error("Research response could not be verified.");
      setThread(value);
    }).catch(() => {
      if (!controller.signal.aborted) setError("Research thread unavailable. Reload the thread to check again.");
    });
    return () => controller.abort();
  }, []);

  async function reload() {
    setBusy(true); setError(null);
    try {
      const value = await paper.research();
      if (!valid(value)) throw new Error("Invalid research response");
      setThread(value); setBlocked(false);
      if (pending && value.messages.some((m) => m.id === `radar_research_u_${pending.nonce}` && m.text === pending.text)) {
        setText(""); setPending(null);
        setNotice("Your message is retained. Check the conversation for its reply; no automatic resend was made.");
      }
    } catch {
      setError("Research thread unavailable. The previous send outcome remains unresolved.");
      setBlocked(true);
    } finally { setBusy(false); }
  }

  async function send() {
    if (busy || blocked || !thread || !text.trim() || new TextEncoder().encode(text).length > 4000) return;
    const message = pending ?? { nonce: crypto.randomUUID(), text };
    setPending(message); setBusy(true); setError(null); setNotice(null);
    try {
      const result = await paper.researchSend(message.nonce, message.text);
      if (!valid(result.thread) || !["completed", "unavailable"].includes(result.reply_status)
        || !result.thread.messages.some((m) => m.id === `radar_research_u_${message.nonce}` && m.text === message.text)) {
        throw new Error("Invalid research receipt");
      }
      setThread(result.thread); setPending(null); setText("");
      setNotice(result.reply_status === "completed" ? "Conversation saved and shared with the paper supervisor."
        : "Your message is saved and shared with the supervisor, but a model reply is unavailable. No automatic retry was made.");
    } catch {
      setBlocked(true);
      setError("Send outcome unresolved. Reload the thread before sending again; your draft is preserved.");
    } finally { setBusy(false); }
  }

  return <div className="space-y-3 rounded-md border border-[var(--color-line)] p-4">
    <h3 className="font-medium">Talk to the paper agent</h3>
    <p className="text-sm text-[var(--color-dim)]">Share an idea, question or source. Recent messages inform the autonomous supervisor;
      it must verify evidence before acting within your handover settings. Chat replies cannot change those settings.</p>
    <p className="text-sm text-[var(--color-dim)]">Links are saved as text and are not opened by this chat. Source claims and assistant replies are unverified.
      The supervisor receives the latest six messages. This thread currently retains up to 128 messages.</p>
    {thread && <p className="text-sm">{thread.available ? `Research account: ${thread.account_id}` : "No initialized research history is available yet."}</p>}
    <ol aria-label="Paper research conversation" className="space-y-3">
      {thread?.messages.map((message) => <li key={message.id} className="rounded border border-[var(--color-line)] p-3">
        <p className="text-xs text-[var(--color-dim)]">{message.role === "user" ? "You" : "Agent"} · {new Date(message.received_at_unix_ms).toISOString()}</p>
        <p className="whitespace-pre-wrap break-words">{message.text}</p>
      </li>)}
    </ol>
    <label className="block text-sm">Research message
      <textarea aria-label="Paper research message" value={text} disabled={busy || blocked || pending !== null}
        onChange={(event) => setText(event.target.value)} rows={4}
        className="mt-1 w-full rounded border border-[var(--color-line)] bg-[var(--color-surface)] p-2" />
    </label>
    <p className="text-xs text-[var(--color-dim)]">{new TextEncoder().encode(text).length} / 4000 UTF-8 bytes. Keep API keys and secrets out of chat.</p>
    <div className="flex gap-2">
      <button type="button" onClick={() => { void send(); }} disabled={busy || blocked || !thread || !text.trim() || new TextEncoder().encode(text).length > 4000}
        className="rounded border border-[var(--color-line)] px-3 py-2 text-sm">{busy ? "Working…" : "Send research message"}</button>
      <button type="button" onClick={() => { void reload(); }} disabled={busy}
        className="rounded border border-[var(--color-line)] px-3 py-2 text-sm">Reload research thread</button>
    </div>
    {notice && <p role="status" className="text-sm">{notice}</p>}
    {error && <p role="alert" className="text-sm">{error}</p>}
  </div>;
}
