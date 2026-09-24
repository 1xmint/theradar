// SPDX-License-Identifier: Apache-2.0
//! Chart drawings -- horizontal price lines and trend lines the visitor
//! placed themselves -- kept per mint in this browser only.
//!
//! Plan 0012 P6, 9-11-0019: "Drawings persist per mint **in the browser**,
//! labelled as the visitor's own -- not in the tenant store, because a
//! drawing is not something Radar cannot recover, it is something Radar never
//! had." Radar's server never sees these lines: there is no route that
//! accepts one, and this file never calls `fetch`. That is not an oversight
//! to fix later, it is the design -- a line a visitor draws on their own copy
//! of the chart is exactly as much Radar's business as a note scrawled on a
//! printout of it.
//!
//! Mirrors `Wallet.tsx`'s `storedSession`: every read is wrapped in `try`, and
//! anything unreadable is discarded rather than repaired. A partially-valid
//! list of drawings silently trimmed down to its valid half is still showing
//! the visitor something they did not draw.

/** A horizontal line at one price, spanning the whole visible chart. */
export interface HorizontalLine {
  id: string;
  kind: "horizontal";
  price: number;
}

/** A line between two points the visitor placed with two clicks. */
export interface TrendLine {
  id: string;
  kind: "trend";
  from: { time: number; price: number };
  to: { time: number; price: number };
}

export type Drawing = HorizontalLine | TrendLine;

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isPoint(value: unknown): value is { time: number; price: number } {
  if (value === null || typeof value !== "object") return false;
  const p = value as Record<string, unknown>;
  return isFiniteNumber(p.time) && isFiniteNumber(p.price);
}

function isDrawing(value: unknown): value is Drawing {
  if (value === null || typeof value !== "object") return false;
  const d = value as Record<string, unknown>;
  if (typeof d.id !== "string" || d.id.length === 0) return false;
  if (d.kind === "horizontal") return isFiniteNumber(d.price);
  if (d.kind === "trend") return isPoint(d.from) && isPoint(d.to);
  return false;
}

/** One storage key per mint -- the reason a drawing on one coin's chart never
 *  appears on another's. Prefixed rather than bare so it cannot collide with
 *  `Wallet.tsx`'s `radar.wallet.session` or a future key that also happens to
 *  be a mint address. */
function keyFor(mint: string): string {
  return `radar.chart.drawings.${mint}`;
}

/**
 * The drawings saved for `mint`, or `[]` if there are none or the stored
 * value is not a list of drawings.
 *
 * All-or-nothing, like `Wallet.tsx`'s `storedSession`: a stored array with
 * even one entry that does not parse as a `Drawing` is treated as corrupt in
 * full and discarded, not filtered down to the entries that do parse. Keeping
 * the valid-looking remainder would still be showing the visitor lines they
 * did not draw, from whatever partial write or format change corrupted the
 * rest.
 */
export function loadDrawings(
  mint: string,
  store: Pick<Storage, "getItem" | "removeItem"> = localStorage,
): Drawing[] {
  const key = keyFor(mint);
  let raw: string | null;
  try {
    raw = store.getItem(key);
  } catch {
    // A private window, or a browser set to block site data.
    return [];
  }
  if (raw === null) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed) && parsed.every(isDrawing)) {
      return parsed;
    }
  } catch {
    // Not JSON.
  }
  try {
    store.removeItem(key);
  } catch {
    // Nothing to do; the read already failed safely.
  }
  return [];
}

/** Saves `mint`'s drawings. Never throws -- an unstorable browser still gets
 *  working drawings for this page's lifetime, just not ones that survive a
 *  reload. */
export function saveDrawings(
  mint: string,
  drawings: readonly Drawing[],
  store: Pick<Storage, "setItem"> = localStorage,
): void {
  try {
    store.setItem(keyFor(mint), JSON.stringify(drawings));
  } catch {
    // Unstorable; see above.
  }
}

/** Removes every drawing saved for `mint`. Leaves every other mint's
 *  drawings untouched -- there is no key that could remove more than one
 *  mint's worth, by construction of [`keyFor`]. */
export function clearDrawings(
  mint: string,
  store: Pick<Storage, "removeItem"> = localStorage,
): void {
  try {
    store.removeItem(keyFor(mint));
  } catch {
    // Already unreadable, so already gone as far as this page is concerned.
  }
}

/** A short id for a newly-placed drawing. Not cryptographic -- these never
 *  leave the browser and never need to be unguessable, only distinct from
 *  the visitor's other lines on the same chart. */
export function newDrawingId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}
