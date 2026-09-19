// SPDX-License-Identifier: Apache-2.0
//! A coin's image, fetched by the browser from its own off-chain metadata
//! document -- never by the server, and never trusted further than a picture.
//!
//! `uri` is creator-supplied and `Trust::Untrusted` (AGENTS.md rule 4): it may
//! point anywhere, name any scheme, or serve a document with no `image` field
//! at all. This module reads it as data, extracts one string, and refuses
//! anything that is not an `https:` URL before ever handing it to an `<img>`
//! tag. A failure at any step -- an unreachable host, a timeout, a malformed
//! document, a non-https image -- degrades to a plain placeholder. It must
//! never blank the row it lives in or throw past its own boundary.

import { useEffect, useState } from "react";

const FETCH_TIMEOUT_MS = 4_000;

/**
 * Resolved image URLs, keyed by the metadata `uri` they came from.
 *
 * Module-level and never evicted: the coin list re-renders every ~15s on a
 * fresh market poll, and without this cache every poll would re-fetch every
 * visible row's metadata document, for a value that never changes once
 * fetched. `undefined` means "not attempted yet"; `null` means "attempted and
 * failed, permanently" -- the same terminal outcome as a fetch, since the
 * document is a static file that will not become well-formed on retry.
 */
const cache = new Map<string, string | null>();

/** In-flight fetches, so two rows racing on the same `uri` share one request
 *  rather than firing a second. */
const inflight = new Map<string, Promise<string | null>>();

/** `ipfs://<cid>[/path]` rewritten to a gateway URL the browser can actually
 *  fetch -- the scheme itself has no browser-native resolver. Anything else
 *  passes through unchanged for the https check below to accept or refuse. */
function rewriteIpfs(url: string): string {
  const match = /^ipfs:\/\/(.+)$/.exec(url);
  return match ? `https://ipfs.io/ipfs/${match[1]}` : url;
}

/** Only an `https:` URL is ever handed to `<img src>`. This is the one gate
 *  that matters: `javascript:`, `data:`, `file:`, and plain `http:` (mixed
 *  content, and no better than the untrusted document that named it) are all
 *  refused rather than rendered. */
function asHttpsImageUrl(value: unknown): string | null {
  if (typeof value !== "string" || value.length === 0) return null;
  const rewritten = rewriteIpfs(value);
  try {
    const parsed = new URL(rewritten);
    return parsed.protocol === "https:" ? parsed.toString() : null;
  } catch {
    return null;
  }
}

async function resolveImage(uri: string): Promise<string | null> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), FETCH_TIMEOUT_MS);
  try {
    const response = await fetch(uri, {
      signal: controller.signal,
      headers: { accept: "application/json" },
    });
    if (!response.ok) return null;
    const body = (await response.json()) as { image?: unknown };
    return asHttpsImageUrl(body.image);
  } catch {
    // Any of: network failure, timeout, non-JSON body, or a body with no
    // `image` field. All of them mean "no image", never a thrown error.
    return null;
  } finally {
    clearTimeout(timer);
  }
}

/** Fetches and caches `uri`'s `image` field. Returns `null` while unresolved
 *  or on any failure -- callers render a placeholder for `null`, exactly as
 *  they would for "not loaded yet", because the two are indistinguishable to
 *  a reader and neither should look like an error. */
function useCoinImage(uri: string | null | undefined): string | null {
  const [image, setImage] = useState<string | null>(() =>
    uri ? (cache.get(uri) ?? null) : null,
  );

  useEffect(() => {
    if (!uri) {
      setImage(null);
      return;
    }
    if (cache.has(uri)) {
      setImage(cache.get(uri) ?? null);
      return;
    }
    let cancelled = false;
    let request = inflight.get(uri);
    if (!request) {
      request = resolveImage(uri).finally(() => inflight.delete(uri));
      inflight.set(uri, request);
    }
    request.then((resolved) => {
      cache.set(uri, resolved);
      if (!cancelled) setImage(resolved);
    });
    return () => {
      cancelled = true;
    };
  }, [uri]);

  return image;
}

/**
 * A coin's image, or a plain placeholder -- the first letter of `symbol`, or
 * a neutral dot when there is no symbol either. Never blank: a broken image
 * for one row must not read as a missing row.
 */
export function CoinImage({
  uri,
  symbol,
  className = "h-6 w-6 rounded-full",
}: {
  uri: string | null | undefined;
  symbol?: string | null | undefined;
  className?: string | undefined;
}) {
  const image = useCoinImage(uri);
  const [broken, setBroken] = useState(false);

  if (!image || broken) {
    const letter = symbol?.trim() ? symbol.trim()[0]!.toUpperCase() : "•";
    // Not `role="img"": that role would let a query for the real `<img>`
    // below match this placeholder instead, on either element, and a reader
    // asking "did the image load" would get a false yes from a letter.
    return (
      <span
        aria-label={symbol ? `${symbol}, no image` : "no image"}
        className={`flex items-center justify-center bg-[var(--color-ink)] text-[10px] font-medium text-[var(--color-dim)] ${className}`}
      >
        {letter}
      </span>
    );
  }

  return (
    <img
      src={image}
      alt={symbol ?? ""}
      loading="lazy"
      referrerPolicy="no-referrer"
      onError={() => setBroken(true)}
      className={className}
    />
  );
}
