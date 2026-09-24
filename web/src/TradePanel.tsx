// SPDX-License-Identifier: Apache-2.0
//! Buy and sell, from the token screen.
//!
//! Gated on two independent switches -- `legal.ts`'s `TERMS_APPROVED` and
//! `useHealth.ts`'s `useTrading()` -- and mounted only by `TokenHeader.tsx`,
//! which checks both before this component exists at all. This file does not
//! re-check them: a panel that rendered its own "trading is not available
//! yet" placeholder would still be a visible trace of an unshipped feature,
//! which is exactly what shipping dark means to avoid.
//!
//! Every number shown before "Review trade" is sent comes from the public,
//! unauthenticated `/v1/market/quote` (via `useQuote`) and can move between
//! keystrokes. Every number shown after that button is pressed comes from
//! that one `/v1/customer/swap` response's own `quote` field, never from the
//! live debounced one -- the two can disagree (the market moved, or the
//! server priced a different route), and showing the live figure next to an
//! "Approve" button would be showing a promise nobody made.

import { useEffect, useState } from "react";
import { Link } from "wouter";

import { customer, SwapError, type PositionsToken, type SwapResponse, type SwapSide } from "./api";
import { amountErrorMessage, toBaseUnits } from "./amounts";
import { roundTripCostCaption, swapRefusalMessage } from "./honesty";
import { transactionUrl } from "./format";
import { useQuote } from "./useQuote";
import { useWalletToken } from "./Wallet";
import { signAndSend, type SigningProvider } from "./sign";
import { detect } from "./siws";
import type { PositionsLoad } from "./usePositions";
import { NOTICE_TEXT } from "./legal";

const SOL_DECIMALS = 9;
const DEFAULT_SLIPPAGE_BPS = 100;
const MAX_SLIPPAGE_BPS = 500;
const STALE_MS = 60_000;

export interface TradePanelProps {
  mint: string;
  symbol: string | null;
  /** The right rail's own positions read, reused rather than fetched again --
   *  see `TokenHeader.tsx`, which owns the single `usePositions` call. */
  positions: PositionsLoad;
  /** Called once a trade actually lands, so the caller can refresh its own
   *  positions read. Not called on a decline or a failure -- nothing changed
   *  on chain in either of those. */
  onTraded: () => void;
}

/** Where the built-and-reviewed transaction is, from "not asked for one yet"
 *  through to a signature or a reason it never got one. `response`/`builtAt`
 *  travel with every state from `built` onward so a decline or a stale check
 *  can still show, and re-send, the exact transaction that was built. */
type ReviewStatus =
  | { kind: "idle" }
  | { kind: "building" }
  | { kind: "built"; response: SwapResponse; builtAt: number }
  | { kind: "sending"; response: SwapResponse; builtAt: number }
  | { kind: "sent"; signature: string }
  | { kind: "declined"; response: SwapResponse; builtAt: number }
  | { kind: "failed"; message: string };

/** Why a typed slippage tolerance could not be used as-is. */
function parseSlippageBps(input: string): { ok: true; bps: number } | { ok: false; message: string } {
  const trimmed = input.trim();
  if (trimmed === "") return { ok: false, message: "Enter a slippage tolerance." };
  if (!/^[0-9]+$/.test(trimmed)) {
    return { ok: false, message: "Slippage must be a whole number of basis points." };
  }
  const bps = Number(trimmed);
  if (bps <= 0) return { ok: false, message: "Enter a slippage tolerance greater than zero." };
  if (bps > MAX_SLIPPAGE_BPS) {
    return {
      ok: false,
      message: `That slippage tolerance is wider than Radar allows (max ${MAX_SLIPPAGE_BPS / 100}%).`,
    };
  }
  return { ok: true, bps };
}

/**
 * A u64 base-unit string as a decimal string, built with `BigInt` rather
 * than a float division -- the same reason `amounts.ts` never puts one of
 * these through `Number`. `decimals: null` means the contract itself does
 * not know this mint's decimals; that is shown as raw base units with a
 * label, never guessed at.
 */
function formatBaseUnits(raw: string, decimals: number | null): string {
  if (decimals === null) return `${raw} base units (decimals unknown)`;
  if (decimals === 0) return raw;
  const value = BigInt(raw);
  const negative = value < 0n;
  const abs = negative ? -value : value;
  const digits = abs.toString().padStart(decimals + 1, "0");
  const whole = digits.slice(0, digits.length - decimals);
  const frac = digits.slice(digits.length - decimals).replace(/0+$/, "");
  return `${negative ? "-" : ""}${whole}${frac ? "." + frac : ""}`;
}

export function TradePanel({ mint, symbol, positions, onTraded }: TradePanelProps) {
  const token = useWalletToken();
  const label = symbol ?? "this token";

  const [side, setSide] = useState<SwapSide>("buy");
  const [amountInput, setAmountInput] = useState("");
  const [slippageInput, setSlippageInput] = useState(String(DEFAULT_SLIPPAGE_BPS));
  const [review, setReview] = useState<ReviewStatus>({ kind: "idle" });
  const [now, setNow] = useState(() => Date.now());

  // A stale review must never be sent silently: whatever was built for a
  // different side or amount is thrown away the moment either changes.
  useEffect(() => {
    setReview({ kind: "idle" });
  }, [side, amountInput, mint]);

  // Only ticks while a built transaction exists, so the 60s staleness check
  // below has a clock to compare against.
  useEffect(() => {
    if (!("builtAt" in review)) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [review]);

  const heldToken: PositionsToken | undefined =
    positions.state === "ready" ? positions.value.tokens.find((t) => t.mint === mint) : undefined;

  const sellReady = positions.state === "ready";
  const sellUnavailable = side === "sell" && (!sellReady || !heldToken);

  const decimals = side === "buy" ? SOL_DECIMALS : (heldToken?.decimals ?? null);
  const maxBaseUnits = side === "sell" ? heldToken?.amount : undefined;
  const amountSymbol = side === "buy" ? "SOL" : label;

  const amountResult =
    sellUnavailable || decimals === null ? null : toBaseUnits(amountInput, decimals, maxBaseUnits);
  const amountBaseUnits = amountResult?.ok ? amountResult.baseUnits : null;

  const slippage = parseSlippageBps(slippageInput);
  const quoteAmount = amountBaseUnits !== null && slippage.ok ? amountBaseUnits : null;
  const quote = useQuote(mint, side, quoteAmount, slippage.ok ? slippage.bps : DEFAULT_SLIPPAGE_BPS);

  const isStale = "builtAt" in review && now - review.builtAt > STALE_MS;

  async function startReview() {
    if (!token || amountBaseUnits === null || !slippage.ok) return;
    setReview({ kind: "building" });
    try {
      const response = await customer.swap(token, {
        mint,
        side,
        amount: amountBaseUnits,
        slippage_bps: slippage.bps,
      });
      setReview({ kind: "built", response, builtAt: Date.now() });
    } catch (e) {
      if (e instanceof SwapError) {
        setReview({ kind: "failed", message: swapRefusalMessage(e.reason, e.detail) });
      } else {
        setReview({ kind: "failed", message: `Could not reach Radar: ${String(e)}` });
      }
    }
  }

  async function approve(response: SwapResponse, builtAt: number) {
    const provider = detect();
    if (!provider) {
      setReview({
        kind: "failed",
        message: "No wallet extension found. Install Phantom or Solflare to sign this trade.",
      });
      return;
    }
    setReview({ kind: "sending", response, builtAt });
    // `WalletProvider` (sign-in) and `SigningProvider` (this call) are two
    // narrow interfaces over the same real injected object -- see siws.ts's
    // and sign.ts's doc comments for why neither declares the other's method.
    const result = await signAndSend(provider as unknown as SigningProvider, response.transaction);
    if (result.ok) {
      setReview({ kind: "sent", signature: result.signature });
      onTraded();
    } else if (result.error.kind === "declined") {
      // Closing the wallet popup is a choice, not a fault -- it keeps the
      // same built transaction so approving again does not rebuild it.
      setReview({ kind: "declined", response, builtAt });
    } else {
      setReview({ kind: "failed", message: result.error.detail });
    }
  }

  return (
    <div className="flex flex-col gap-3 text-sm">
      <div className="flex gap-2" role="group" aria-label="Buy or sell">
        {(["buy", "sell"] as const).map((s) => (
          <button
            key={s}
            type="button"
            onClick={() => setSide(s)}
            aria-pressed={side === s}
            className={`flex-1 rounded-md border px-3 py-1.5 capitalize ${
              side === s
                ? "border-[var(--color-accent)] text-[var(--color-accent)]"
                : "border-[var(--color-line)] text-[var(--color-dim)]"
            }`}
          >
            {s}
          </button>
        ))}
      </div>

      {sellUnavailable && (
        <p className="text-[var(--color-dim)]">
          {!token
            ? "Connect a wallet to sell. Radar can only sell from a wallet it can see the balance of."
            : !sellReady
              ? "Reading this wallet's holdings before it can be sold from."
              : `This wallet does not hold ${label}, so there is nothing to sell.`}
        </p>
      )}

      <label className="flex flex-col gap-1">
        <span className="text-[var(--color-dim)]">
          {side === "buy" ? "Pay (SOL)" : `Sell (${label})`}
        </span>
        <input
          type="text"
          inputMode="decimal"
          value={amountInput}
          disabled={sellUnavailable}
          onChange={(e) => setAmountInput(e.target.value)}
          className="rounded-md border border-[var(--color-line)] bg-transparent px-2 py-1.5 disabled:opacity-50"
          placeholder="0.0"
        />
        {amountResult && !amountResult.ok && (
          <span className="text-[var(--color-warn)]">
            {amountErrorMessage(amountResult.error, amountSymbol)}
          </span>
        )}
      </label>

      <label className="flex flex-col gap-1">
        <span className="text-[var(--color-dim)]">Max slippage (bps, {MAX_SLIPPAGE_BPS / 100}% cap)</span>
        <input
          type="text"
          inputMode="numeric"
          value={slippageInput}
          onChange={(e) => setSlippageInput(e.target.value)}
          className="rounded-md border border-[var(--color-line)] bg-transparent px-2 py-1.5"
        />
        {!slippage.ok && <span className="text-[var(--color-warn)]">{slippage.message}</span>}
      </label>

      {quote.state === "loading" && <p className="text-[var(--color-dim)]">Getting a quote…</p>}
      {quote.state === "failed" && (
        <p className="text-[var(--color-warn)]">{swapRefusalMessage(quote.reason, quote.detail)}</p>
      )}
      {quote.state === "ready" && (
        <div className="flex flex-col gap-1 rounded-md border border-[var(--color-line)] p-2">
          <div className="flex justify-between">
            <span className="text-[var(--color-dim)]">You pay</span>
            <span>{formatBaseUnits(quote.value.in_amount, quote.value.in_decimals)}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-[var(--color-dim)]">You receive (estimate)</span>
            <span>{formatBaseUnits(quote.value.out_amount, quote.value.out_decimals)}</span>
          </div>
          <div className="flex justify-between font-semibold">
            <span>Worst case</span>
            <span>{formatBaseUnits(quote.value.worst_out, quote.value.out_decimals)}</span>
          </div>
          <div className="flex justify-between text-[var(--color-dim)]">
            <span>Price impact</span>
            <span>{quote.value.impact_bps === null ? "not reported" : `${(quote.value.impact_bps / 100).toFixed(2)}%`}</span>
          </div>
          <div className="flex justify-between text-[var(--color-dim)]">
            <span>Venues</span>
            <span>{quote.value.venues.length > 0 ? quote.value.venues.join(", ") : "none reported"}</span>
          </div>
          {slippage.ok && quote.value.slippage_bps !== slippage.bps && (
            <p className="text-[var(--color-warn)]">
              Note: this quote reflects a {(quote.value.slippage_bps / 100).toFixed(2)}% slippage
              tolerance, not the {(slippage.bps / 100).toFixed(2)}% set above.
            </p>
          )}
          <p className="text-[var(--color-dim)]">{roundTripCostCaption(quote.value.impact_bps)}</p>
        </div>
      )}

      {!token && <p className="text-[var(--color-dim)]">Connect a wallet to trade.</p>}

      {review.kind === "idle" && (
        <button
          type="button"
          onClick={startReview}
          disabled={!token || amountBaseUnits === null || !slippage.ok || quote.state !== "ready"}
          className="rounded-md border border-[var(--color-accent)] px-3 py-1.5 text-[var(--color-accent)] disabled:opacity-50"
        >
          Review trade
        </button>
      )}

      {review.kind === "building" && <p className="text-[var(--color-dim)]">Building the transaction…</p>}

      {(review.kind === "built" || review.kind === "sending" || review.kind === "declined") && (
        <div className="flex flex-col gap-2 rounded-md border border-[var(--color-line)] p-2">
          <p className="font-semibold">Review before you approve</p>
          <div className="flex justify-between">
            <span className="text-[var(--color-dim)]">You pay</span>
            <span>{formatBaseUnits(review.response.quote.in_amount, review.response.quote.in_decimals)}</span>
          </div>
          <div className="flex justify-between font-semibold">
            <span>Worst case you receive</span>
            <span>{formatBaseUnits(review.response.quote.worst_out, review.response.quote.out_decimals)}</span>
          </div>
          {review.kind === "declined" && (
            <p className="text-[var(--color-dim)]">Cancelled. Nothing was sent.</p>
          )}
          {isStale ? (
            <>
              <p className="text-[var(--color-warn)]">
                This quote is more than a minute old. Rebuild it before approving.
              </p>
              <button
                type="button"
                onClick={startReview}
                className="rounded-md border border-[var(--color-line)] px-3 py-1.5"
              >
                Rebuild
              </button>
            </>
          ) : (
            <button
              type="button"
              onClick={() => approve(review.response, review.builtAt)}
              disabled={review.kind === "sending"}
              className="rounded-md border border-[var(--color-accent)] px-3 py-1.5 text-[var(--color-accent)] disabled:opacity-50"
            >
              {review.kind === "sending" ? "Waiting on your wallet…" : "Approve in wallet"}
            </button>
          )}
        </div>
      )}

      {review.kind === "sent" && (
        <div className="flex flex-col gap-1 rounded-md border border-[var(--color-line)] p-2">
          <p className="font-semibold">Sent to your wallet.</p>
          <a
            href={transactionUrl(review.signature)}
            target="_blank"
            rel="noreferrer"
            className="underline"
          >
            View on Solscan
          </a>
          <p className="text-[var(--color-dim)]">
            Radar did not check whether it landed. Solana confirmation can still fail or take a
            moment; Solscan is the record of what actually happened.
          </p>
        </div>
      )}

      {review.kind === "failed" && <p className="text-[var(--color-warn)]">{review.message}</p>}

      <p className="text-[var(--color-dim)]">
        {NOTICE_TEXT} <Link href="/terms" className="underline">Read the full terms</Link>.
      </p>
    </div>
  );
}
