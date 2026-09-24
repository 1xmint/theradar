// SPDX-License-Identifier: Apache-2.0
import { describe, expect, it } from "vitest";
import {
  hasPlaceholder,
  NOTICE_TEXT,
  TERMS_APPROVED,
  TERMS_SAFE_TO_APPROVE,
  TERMS_TEXT,
} from "./legal";

describe("hasPlaceholder", () => {
  it("finds a bracketed placeholder", () => {
    expect(hasPlaceholder("governed by the laws of [JURISDICTION]")).toBe(true);
  });

  it("does not fire on ordinary text with no brackets", () => {
    expect(hasPlaceholder("Radar never holds your money or your keys.")).toBe(false);
  });

  it("finds a long-form bracketed instruction, not just a single word", () => {
    expect(
      hasPlaceholder("[RESTRICTED REGIONS: if any, list them here]"),
    ).toBe(true);
  });
});

describe("the terms text as it stands today", () => {
  it("still contains placeholders -- this draft has not been filled in", () => {
    expect(hasPlaceholder(TERMS_TEXT)).toBe(true);
  });

  it("the short notice has no placeholders of its own", () => {
    expect(hasPlaceholder(NOTICE_TEXT)).toBe(false);
  });

  it("is therefore not safe to approve yet", () => {
    expect(TERMS_SAFE_TO_APPROVE).toBe(false);
  });
});

describe("TERMS_APPROVED", () => {
  // The load-bearing invariant: whatever the literal is set to, it must never
  // be `true` while the text it gates still has a `[PLACEHOLDER]` in it. This
  // is the test that would fail if someone flipped the switch on without
  // reading the draft.
  it("can never be true while the text is not safe to approve", () => {
    expect(TERMS_SAFE_TO_APPROVE || !TERMS_APPROVED).toBe(true);
  });

  it("is false today, because the owner has not approved it", () => {
    expect(TERMS_APPROVED).toBe(false);
  });
});
