// SPDX-License-Identifier: Apache-2.0
//! The interface's claims, tested where they are computed.
//!
//! This file is the frontend's answer to the question the Rust side answers with
//! 856 tests: **which of the things this code says are true?**
//!
//! It deliberately does not test that buttons render. A snapshot of a `<div>` is
//! a test that fails when someone changes a class name and passes when the page
//! lies. What is worth pinning is the small set of pure functions that decide
//! *what the page claims* — the median it prints, the count it says cleared
//! costs, and which refusals it collapses into "policy closed".
//!
//! Each of those has a wrong version that looks right, which is the only reason
//! to test any of them.

import { describe, expect, it } from "vitest";

import {
  POLICY_ARTIFACTS,
  capCaption,
  clearedCost,
  emptyTapeMessage,
  holdersBasis,
  holdersBasisCaption,
  isNarrowerThanRequested,
  isPossiblyCapped,
  isWalletSessionRefusal,
  landingMessage,
  launchesEmptyMessage,
  median,
  netOfCost,
  partitionReasons,
  pct,
  positionsMessage,
  roundTripCostBps,
  roundTripCostCaption,
  swapRefusalMessage,
} from "./honesty";

describe("median", () => {
  it("is the middle of the sorted values, not of the given order", () => {
    // The bug this catches: forgetting to sort. Returns can arrive from the API
    // in decision order, and the "median" of an unsorted list is whatever
    // happened to land in the middle — a number that looks plausible and means
    // nothing.
    expect(median([500, -1000, 200])).toBe(200);
    expect(median([-1000, 200, 500])).toBe(200);
  });

  it("sorts numerically rather than as strings", () => {
    // JavaScript's default sort is lexicographic, so `[10, 9, 100].sort()` is
    // `[10, 100, 9]`. On basis points that is not a rounding error, it is a
    // different token.
    expect(median([10, 9, 100])).toBe(10);
    expect(median([-2000, -300, -40])).toBe(-300);
  });

  it("averages the two middles on an even count", () => {
    expect(median([100, 300])).toBe(200);
    // And rounds rather than emitting a fraction of a basis point, which would
    // render as `-13.405%` and imply precision nobody measured.
    expect(median([100, 301])).toBe(201);
  });

  it("is null for an empty cohort rather than zero", () => {
    // Rule 9, in the interface. A cohort with nothing in it has no median, and
    // rendering that as 0% would print "broke even" for a measurement that was
    // never taken.
    expect(median([])).toBeNull();
  });
});

describe("clearedCost", () => {
  it("counts strictly above the cost, not at it", () => {
    // A round trip that returned exactly its cost cleared nothing. Counting it
    // makes the headline figure — the share of tokens that beat costs —
    // overstate itself at precisely the boundary the number exists to describe.
    expect(clearedCost([849, 850, 851], 850)).toBe(1);
  });

  it("counts nothing in an empty cohort", () => {
    expect(clearedCost([], 850)).toBe(0);
  });

  it("does not treat a loss as a gain", () => {
    expect(clearedCost([-9000, -100, 0], 850)).toBe(0);
  });
});

describe("pct", () => {
  it("signs a gain and does not sign a loss twice", () => {
    expect(pct(1234)).toBe("+12.3%");
    expect(pct(-1340)).toBe("-13.4%");
  });

  it("does not sign zero as a gain", () => {
    // "+0.0%" reads as a gain that rounded away. Zero is zero.
    expect(pct(0)).toBe("0.0%");
  });
});

describe("POLICY_ARTIFACTS", () => {
  it("contains every refusal a closed policy produces on its own", () => {
    // The seven that fire together under `Policy::CLOSED` because every limit
    // is zero and every comparison against zero fails. Rendering them
    // individually tells a novice there are seven problems with a token when
    // there is one fact about the policy.
    for (const artifact of [
      "NoAutonomy",
      "OverPositionLimit",
      "OverDeploymentLimit",
      "OverCreatorLimit",
      "DailyLossReached",
      "RoundTripTooExpensive",
      "InputsTooStale",
    ]) {
      expect(POLICY_ARTIFACTS.has(artifact)).toBe(true);
    }
  });

  it("does not swallow a refusal that is about the token", () => {
    // The other direction, and the one that matters more. These are findings —
    // the exit could not be simulated, or was too small — and collapsing them
    // into "policy closed" would hide the only refusals that say anything about
    // the token being looked at.
    for (const finding of [
      "ExitNotSimulated",
      "ExitCapacityTooSmall",
      "OverCanaryLimit",
      "Halted",
      "TooManyFailures",
    ]) {
      expect(POLICY_ARTIFACTS.has(finding)).toBe(false);
    }
  });
});

describe("partitionReasons", () => {
  // Moved here from `Figures.test.tsx` when `ReasonList` -- the only component
  // that called this -- was deleted with the decision-record pages. The logic
  // stayed: it is still one of this file's testable claims about what the
  // interface says, even without a page rendering it today.
  it("splits the three kinds the kernel already distinguishes", () => {
    const split = partitionReasons([
      "NoRoute",
      "CreatorNeverGraduated",
      "OverPositionLimit",
    ]);
    expect(split.structural).toEqual(["NoRoute"]);
    expect(split.evidence).toEqual(["CreatorNeverGraduated"]);
    expect(split.policy).toEqual(["OverPositionLimit"]);
  });

  it("shows an unrecognised reason rather than hiding it", () => {
    // The direction that matters. A reason wrongly sorted into `policy` is
    // collapsed into one line and effectively hidden, and findings are the only
    // refusals that say anything about the token being looked at.
    const split = partitionReasons(["SomethingAddedNextYear"]);
    expect(split.evidence).toEqual(["SomethingAddedNextYear"]);
    expect(split.policy).toEqual([]);
  });

  it("preserves the order within each group", () => {
    // The strategy emits reasons worst-first and re-sorting would throw that
    // away.
    const split = partitionReasons(["ExitUnmeasurable", "NoRoute"]);
    expect(split.structural).toEqual(["ExitUnmeasurable", "NoRoute"]);
  });
});

describe("netOfCost", () => {
  it("takes the whole round trip off, and does not clamp", () => {
    // The bug this exists for: the scoreboard rendered the *gross* median under
    // a footnote saying "Returns are net of an assumed 850 bps round trip". The
    // subtraction existed nowhere, so nothing could catch it.
    expect(netOfCost(2000, 850)).toBe(1150);

    // Signed, deliberately. A return that does not cover its costs is negative,
    // and flooring it at zero would turn every losing trade into a break-even
    // one -- which is the exact direction this whole screen exists to resist.
    expect(netOfCost(21, 850)).toBe(-829);
    expect(netOfCost(-1272, 850)).toBe(-2122);
  });

  it("is not a no-op, which is the way it would silently come back", () => {
    // If somebody reverted this to `gross` the page would look identical except
    // for a label, which is precisely how the original defect survived.
    const gross = 500;
    expect(netOfCost(gross, 850)).not.toBe(gross);
  });

  it("leaves a zero cost alone", () => {
    // The boundary. A cost of zero must not shift the figure, or the function
    // is doing something other than subtracting.
    expect(netOfCost(-863, 0)).toBe(-863);
  });
});

describe("emptyTapeMessage", () => {
  it("tells a coin that never traded apart from a feed Radar could not reach", () => {
    // The rule from the packet, verbatim: "an empty tape because the coin never
    // traded, and an empty tape because the data source was unreachable, are
    // different screens with different words." Same zero rows, and the words
    // must not collide.
    const neverTraded = emptyTapeMessage("never-traded");
    const unreachable = emptyTapeMessage("unreachable", "504: gateway timeout");
    expect(neverTraded).not.toBe(unreachable);
    expect(neverTraded.toLowerCase()).not.toContain("unreachable");
    expect(unreachable.toLowerCase()).not.toContain("no trades");
  });

  it("folds the transport's own detail in rather than dropping it", () => {
    expect(emptyTapeMessage("unreachable", "504: gateway timeout")).toContain(
      "504: gateway timeout",
    );
  });

  it("still says something when there is no detail to fold in", () => {
    expect(emptyTapeMessage("unreachable")).toContain("Radar could not look");
  });
});

describe("launchesEmptyMessage", () => {
  it("tells a quiet launch window apart from a snapshot Radar could not read", () => {
    // `/v1/market/launches` answers `not_collected` for both an empty index
    // and a snapshot that failed to build -- same status, same empty list --
    // and the server's own `message` (not the shared `error` code) is the
    // only thing that tells them apart. Reversing which branch fires, or
    // collapsing both to one sentence, must make this fail.
    const quiet = launchesEmptyMessage("Radar has recorded no launches in its window");
    const unreachable = launchesEmptyMessage(
      "the market snapshot has not been built yet; check back shortly",
    );
    expect(quiet).not.toBe(unreachable);
    expect(quiet.toLowerCase()).not.toContain("could not look");
    expect(unreachable.toLowerCase()).toContain("could not look");
  });

  it("folds the server's own detail into the could-not-look sentence", () => {
    expect(
      launchesEmptyMessage("the market snapshot has not been built yet; check back shortly"),
    ).toContain("the market snapshot has not been built yet");
  });
});

describe("positionsMessage", () => {
  const signedOut = positionsMessage({ kind: "signed-out" });
  const emptyNoSol = positionsMessage({ kind: "empty", solLamports: 0, solUiAmount: "0.000000000" });
  const emptyWithSol = positionsMessage({ kind: "empty", solLamports: 1_500_000_000, solUiAmount: "1.500000000" });
  const sessionRefused = positionsMessage({ kind: "session-refused" });
  const busy = positionsMessage({ kind: "busy" });
  const couldNotLook = positionsMessage({ kind: "could-not-look", detail: "502: bad gateway" });

  it("gives every reason its own sentence", () => {
    // The five ways this panel can show something other than a wallet's real
    // holdings must never collide -- a reader who sees "no holdings" should
    // be able to tell "you have none" from "Radar couldn't check" from "try
    // again shortly" without reading a status code.
    const all = [signedOut, emptyNoSol, emptyWithSol, sessionRefused, busy, couldNotLook];
    expect(new Set(all).size).toBe(all.length);
  });

  it("says nothing about what the wallet holds when the chain read failed", () => {
    // Rule 9: a failed read is not an empty holding. This sentence must not
    // read as though it already knows the answer is zero.
    expect(couldNotLook.toLowerCase()).not.toContain("holds no tokens");
    expect(couldNotLook).toContain("says nothing about what the wallet holds");
  });

  it("folds the server's own detail into the could-not-look sentence", () => {
    expect(couldNotLook).toContain("502: bad gateway");
  });

  it("names the rate limit rather than reusing the could-not-look wording", () => {
    expect(busy.toLowerCase()).toContain("rate-limiting");
    expect(busy).not.toBe(couldNotLook);
  });

  it("adds the SOL balance only when it is non-zero", () => {
    expect(emptyNoSol).not.toContain("SOL");
    expect(emptyWithSol).toContain("1.500000000 SOL");
  });

  it("compares the raw lamport integer, not the formatted ui_amount string (item 4)", () => {
    // A wallet with dust-level SOL formatted as "0.000000000" must not read
    // as holding SOL: the old check compared `solUiAmount !== "0"`, which a
    // nine-decimal-place "0.000000000" (never the bare string "0") always
    // passed, wrongly claiming a truly empty wallet "holds 0.000000000 SOL".
    const zeroLamportsFormattedLong = positionsMessage({
      kind: "empty",
      solLamports: 0,
      solUiAmount: "0.000000000",
    });
    expect(zeroLamportsFormattedLong).toBe("This wallet holds no tokens");
    expect(zeroLamportsFormattedLong).not.toContain("SOL");
  });

  it("tells a session refusal apart from an invitation to sign in", () => {
    expect(sessionRefused).not.toBe(signedOut);
    expect(sessionRefused.toLowerCase()).toContain("again");
  });
});

describe("isWalletSessionRefusal", () => {
  it("covers every reason tenant.rs's Tenant extractor can refuse with", () => {
    // One list, shared by the watchlist and positions alike -- a route that
    // gains a new wallet-session refusal reason and forgets to add it here
    // would silently mislabel that refusal as "Radar could not look".
    expect(isWalletSessionRefusal("no_session")).toBe(true);
    expect(isWalletSessionRefusal("session_invalid")).toBe(true);
    expect(isWalletSessionRefusal("session_expired")).toBe(true);
    expect(isWalletSessionRefusal("not_a_wallet")).toBe(true);
  });

  it("does not treat a fact about the read itself as a session problem", () => {
    expect(isWalletSessionRefusal("busy")).toBe(false);
    expect(isWalletSessionRefusal("unreadable_chain")).toBe(false);
    expect(isWalletSessionRefusal("not_configured")).toBe(false);
  });
});

describe("isPossiblyCapped / capCaption", () => {
  it("treats a full page as possibly cropped, and says so", () => {
    // No route in the contract reports a cap flag, so a full page is the
    // ambiguous case: it might be the whole list, or the first slice of a
    // longer one, and there is no way to tell them apart from the response
    // alone. Presenting it as complete is the failure the rule exists to
    // prevent, so the safe direction is to warn on the full page.
    expect(isPossiblyCapped(50, 50)).toBe(true);
    expect(capCaption(50, 50, "trades")).toMatch(/most recent 50 trades/);
  });

  it("says nothing when the whole list plainly came back", () => {
    expect(isPossiblyCapped(3, 50)).toBe(false);
    expect(capCaption(3, 50, "trades")).toBeNull();
  });

  it("does not read more-than-requested as evidence of completeness", () => {
    // Defensive: this should not happen, and if it does, it is not proof the
    // list is whole.
    expect(isPossiblyCapped(51, 50)).toBe(true);
  });
});

describe("holdersBasis", () => {
  it("says a list seen since launch is every wallet", () => {
    const caption = holdersBasis("balances_since_launch", "wallet", "2026-09-13 10:00:00", "x");
    expect(caption).toContain("since launch");
    expect(caption).toContain("2026-09-13 10:00:00");
  });

  it("says a list seen only while watching is missing older holders", () => {
    // The dangerous reading is "these are the holders". For a coin the feed
    // did not see launch, a holder who bought last week and never moved is
    // not in this list, and the caption has to say so.
    const caption = holdersBasis("balances_seen_while_watching", "wallet", "2026-09-13 10:00:00", "x");
    expect(caption).toContain("not everyone");
    expect(caption).toContain("2026-09-13 10:00:00");
  });

  it("still explains the transfer fold the store-backed route sends", () => {
    expect(holdersBasis("folded_transfers", "token_account", "a", "b")).toContain("over-counts");
  });

  it("says a net-traded-in-window list is bought minus sold, not a true balance", () => {
    // The free path with no live feed folds the trade tape it already stores
    // rather than refusing outright -- the dangerous reading here is "these
    // are the holders" when it is really "net activity Radar happened to see".
    const caption = holdersBasis(
      "net_traded_in_window",
      "wallet",
      "2026-09-17 23:59:00",
      "2026-09-18 00:00:00",
    );
    expect(caption).toContain("Bought minus sold");
    expect(caption).toContain("2026-09-17 23:59:00");
    expect(caption).toContain("2026-09-18 00:00:00");
    expect(caption).toContain("missing or understated");
  });
});

describe("holdersBasisCaption", () => {
  it("passes the server's own words through unchanged", () => {
    const basis = "folded from transfer history over the last 30 days";
    expect(holdersBasisCaption(basis)).toBe(basis);
  });

  it("never drops the caption for being missing -- it substitutes a warning", () => {
    // The rule: "that caption is not decoration and must not be dropped for
    // being ugly." A response that omits `basis` must not render as a ranked
    // list with nothing said about it; it renders a caption that says the
    // methodology is unknown, which is still true.
    for (const missing of [null, undefined, "", "   "]) {
      const caption = holdersBasisCaption(missing);
      expect(caption.length).toBeGreaterThan(0);
      expect(caption.toLowerCase()).toContain("does not know");
    }
  });
});

describe("isNarrowerThanRequested", () => {
  it("is false when the response covers everything that was asked for", () => {
    expect(isNarrowerThanRequested(100, 200, 100, 200)).toBe(false);
    // A response allowed to cover *more* than asked is still a full answer.
    expect(isNarrowerThanRequested(100, 200, 50, 250)).toBe(false);
  });

  it("catches a start clipped later than requested", () => {
    // A token younger than the requested window: the earliest candle is later
    // than `from`, so the left edge of what was asked for is missing.
    expect(isNarrowerThanRequested(100, 200, 150, 200)).toBe(true);
  });

  it("catches an end clipped earlier than requested", () => {
    expect(isNarrowerThanRequested(100, 200, 100, 180)).toBe(true);
  });
});

describe("roundTripCostBps", () => {
  it("doubles the quoted price impact", () => {
    expect(roundTripCostBps(40)).toBe(80);
  });

  it("is null when the quote reported no impact -- not a free round trip", () => {
    expect(roundTripCostBps(null)).toBeNull();
  });

  it("is zero, not null, when the quote reported zero impact", () => {
    // Zero is a fact the route reported; null is that it reported nothing.
    // Collapsing them would either claim a free round trip that was never
    // measured, or throw away a real, reported zero.
    expect(roundTripCostBps(0)).toBe(0);
  });
});

describe("roundTripCostCaption", () => {
  it("states the estimate as a plain sentence, in percent", () => {
    const caption = roundTripCostCaption(40);
    expect(caption).toContain("0.8%");
    expect(caption.toLowerCase()).toContain("buying and selling straight back");
  });

  it("says plainly that no estimate exists rather than claiming a free round trip", () => {
    const caption = roundTripCostCaption(null);
    expect(caption.toLowerCase()).toContain("cannot estimate");
    expect(caption).not.toContain("0.0%");
  });
});

describe("swapRefusalMessage", () => {
  it("gives every documented refusal code its own, distinct sentence", () => {
    const codes = [
      "no_session",
      "unscoped",
      "busy",
      "trading_off",
      "no_route",
      "unreadable_route",
      "bad_request",
      "slippage_too_wide",
      "sanctioned",
      "chain_unreadable",
    ];
    const texts = codes.map((code) => swapRefusalMessage(code, "server detail"));
    expect(new Set(texts).size).toBe(texts.length);
  });

  it("folds every wallet-session refusal into the same sign-in-again sentence", () => {
    const sessionCodes = ["no_session", "session_invalid", "session_expired", "not_a_wallet"];
    const texts = sessionCodes.map((code) => swapRefusalMessage(code, "irrelevant detail"));
    for (const text of texts) {
      expect(text).toBe(texts[0]);
      expect(text.toLowerCase()).toContain("sign in");
    }
  });

  it("passes the server's detail through for an unrecognised code", () => {
    expect(swapRefusalMessage("some_new_code", "a fact from the server")).toBe(
      "a fact from the server",
    );
  });

  it("states a sanctioned refusal as a fact about the list, not an accusation", () => {
    const text = swapRefusalMessage("sanctioned", "irrelevant detail");
    expect(text.toLowerCase()).toContain("sanctions list");
    expect(text.toLowerCase()).not.toContain("you are");
    expect(text.toLowerCase()).not.toContain("illegal");
  });

  it("includes the server's detail for a bad_request", () => {
    expect(swapRefusalMessage("bad_request", "amount must be positive")).toContain(
      "amount must be positive",
    );
  });
});

describe("landingMessage", () => {
  it("gives every landing state its own, distinct sentence", () => {
    const texts = [
      landingMessage({ kind: "pending" }),
      landingMessage({ kind: "landed" }),
      landingMessage({ kind: "failed", reason: "some on-chain reason" }),
      landingMessage({ kind: "expired" }),
      landingMessage({ kind: "unknown" }),
    ];
    expect(new Set(texts).size).toBe(texts.length);
  });

  it("includes the on-chain reason for a failed transaction", () => {
    expect(landingMessage({ kind: "failed", reason: "insufficient funds" })).toContain(
      "insufficient funds",
    );
  });

  it("never says expired for unknown, and never says unknown for expired -- the honesty rule", () => {
    // `expired` is a proven on-chain fact; `unknown` is Radar's own failure to
    // find out. Confusing either sentence for the other is the exact bug F11
    // exists to prevent (a read failure or a timed-out poll must never read as
    // "your money is safe, nothing happened").
    const expired = landingMessage({ kind: "expired" }).toLowerCase();
    const unknown = landingMessage({ kind: "unknown" }).toLowerCase();
    expect(expired).not.toContain("unknown");
    expect(expired).not.toContain("check solscan");
    expect(unknown).not.toContain("expired");
    expect(unknown).not.toContain("nothing was spent");
  });
});
