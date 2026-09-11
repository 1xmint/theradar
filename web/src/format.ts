// SPDX-License-Identifier: Apache-2.0
//! Presentation only: how a known number is spelled.
//!
//! Separate from `honesty.ts` on purpose. That file decides *whether* a number
//! may be shown at all -- these functions only run once a caller has already
//! established the value is real, and none of them takes a `null`. A formatter
//! that accepted `null` and printed `"$0.00"` for it would be the rule 9
//! failure with extra steps, so the type system is the enforcement here: there
//! is nothing to test beyond what TypeScript already refuses to compile.

/** A USD price, with enough precision to matter at meme-coin magnitudes.
 *  A $0.0000041 token rendered to two decimal places is "$0.00" on every row,
 *  which is a chart with no chart. */
export function formatPrice(value: number): string {
  if (value === 0) return "$0.00";
  const abs = Math.abs(value);
  if (abs >= 1) return `$${value.toFixed(2)}`;
  if (abs >= 0.01) return `$${value.toFixed(4)}`;
  // Small enough that fixed decimals would mostly print zeroes. Enough
  // significant figures to compare two sub-cent prices against each other,
  // which is the actual reading task at this magnitude.
  return `$${value.toPrecision(3)}`;
}

/** A large quantity -- market cap, liquidity, volume -- compacted to K/M/B. */
export function formatCompactUsd(value: number): string {
  const sign = value < 0 ? "-" : "";
  const abs = Math.abs(value);
  if (abs >= 1_000_000_000) return `${sign}$${(abs / 1_000_000_000).toFixed(2)}B`;
  if (abs >= 1_000_000) return `${sign}$${(abs / 1_000_000).toFixed(2)}M`;
  if (abs >= 1_000) return `${sign}$${(abs / 1_000).toFixed(1)}K`;
  return `${sign}$${abs.toFixed(2)}`;
}

/** A signed percentage change, always carrying its sign -- the same reasoning
 *  as `pct()` in `honesty.ts`, for a plain percentage rather than basis
 *  points. */
export function formatChangePct(value: number): string {
  const sign = value > 0 ? "+" : "";
  return `${sign}${value.toFixed(1)}%`;
}

/** A token quantity, compacted the same way volume is -- a supply is
 *  routinely in the billions and nobody reads twelve digits at a glance. */
export function formatCompactNumber(value: number): string {
  const sign = value < 0 ? "-" : "";
  const abs = Math.abs(value);
  if (abs >= 1_000_000_000) return `${sign}${(abs / 1_000_000_000).toFixed(2)}B`;
  if (abs >= 1_000_000) return `${sign}${(abs / 1_000_000).toFixed(2)}M`;
  if (abs >= 1_000) return `${sign}${(abs / 1_000).toFixed(1)}K`;
  return `${sign}${abs.toLocaleString()}`;
}

/** An age in seconds, as the coarsest unit that keeps it legible. */
export function formatAge(seconds: number): string {
  if (seconds < 0) return "0s";
  if (seconds < 60) return `${Math.floor(seconds)}s`;
  const minutes = seconds / 60;
  if (minutes < 60) return `${Math.floor(minutes)}m`;
  const hours = minutes / 60;
  if (hours < 24) return `${Math.floor(hours)}h`;
  const days = hours / 24;
  return `${Math.floor(days)}d`;
}

/** A wallet or mint address, abbreviated the way the tape and holders list
 *  both need it -- shorter than `Address`'s truncation, because a table row
 *  here already carries a side, two amounts and a price. */
export function shortenAddress(value: string, keep = 4): string {
  return value.length > keep * 2 + 1
    ? `${value.slice(0, keep)}…${value.slice(-keep)}`
    : value;
}

/** Where an address explorer link points. Solscan, because it is what the
 *  rest of this interface's operator tooling already assumes for a Solana
 *  address, and picking a second explorer would be a second thing to keep
 *  consistent for no reader benefit. */
export function explorerUrl(address: string): string {
  return `https://solscan.io/account/${encodeURIComponent(address)}`;
}
