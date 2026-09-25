// SPDX-License-Identifier: Apache-2.0
//! The terms page linked from `TradePanel`'s notice.
//!
//! Gated the same way the panel itself is -- `TERMS_APPROVED` and
//! `useHealth.ts`'s `useTrading()` -- and for the same reason: `TERMS_TEXT`
//! is `docs/legal/terms-and-trade-notice-draft.md`'s draft, unapproved and
//! still carrying `[PLACEHOLDER]` brackets (see `legal.test.ts`), and putting
//! it at a reachable URL before the owner approves it would publish a draft
//! nobody signed off on. When either switch is off this renders the same "no
//! such page" shape `App.tsx`'s own `NotFound` uses, rather than a
//! feature-specific reason that would itself hint the feature exists.

import type { ReactNode } from "react";
import { TERMS_APPROVED, TERMS_TEXT } from "./legal";
import { useTrading } from "./useHealth";

export function Terms() {
  const trading = useTrading();

  if (!(TERMS_APPROVED && trading)) {
    return (
      <div className="rounded-md border border-[var(--color-line)] bg-[var(--color-surface)] px-4 py-3 text-sm">
        <p>
          <strong className="text-[var(--color-warn)]">No such page.</strong> That is a
          fact about this address, not about the store.
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3 text-sm leading-relaxed">
      {TERMS_TEXT.split("\n\n").map((block, i) => (
        <TermsBlock key={i} text={block} />
      ))}
    </div>
  );
}

/** One paragraph of `TERMS_TEXT`: a `## ` heading, or an ordinary paragraph
 *  with its `**bold**` runs rendered as `<strong>`. Nothing fancier -- a real
 *  markdown renderer is a dependency this one page, shown only after an
 *  owner approves fixed text, does not earn. */
function TermsBlock({ text }: { text: string }) {
  if (text.startsWith("## ")) {
    return <h1 className="text-xl font-semibold">{text.slice(3)}</h1>;
  }
  return <p>{renderInlineBold(text)}</p>;
}

function renderInlineBold(text: string): ReactNode[] {
  return text.split(/(\*\*[^*]+\*\*)/g).map((part, i) =>
    part.startsWith("**") && part.endsWith("**") ? (
      <strong key={i}>{part.slice(2, -2)}</strong>
    ) : (
      <span key={i}>{part}</span>
    ),
  );
}
