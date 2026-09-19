// SPDX-License-Identifier: Apache-2.0
//! Every page in the table is a page the shell can actually show.
//!
//! # The bug this exists to catch
//!
//! `ROUTES` and `App.tsx` are two lists of the same thing. Adding a page to the
//! table and forgetting the `<Route>` does not crash and does not warn: static
//! hosting serves `index.html` for any path, the router falls through to the
//! catch-all, and the reader gets "No such page" on a link the footer is
//! rendering. The header and the footer are both derived from `ROUTES`, so the
//! site advertises a page it cannot show.
//!
//! Nothing in the type system can hold this — the table is data and the routes
//! are JSX — so `AGENTS.md` §5 puts it at level 3, a test, and this is it.
//!
//! Verified by re-applying the bug: deleting `<Route path="/contact">` from
//! `App.tsx` fails `renders every page in the table` on `/contact` with
//! `No such page` present, and nothing else in the suite notices.
//!
//! # Rendered through `App`, never a bare page component
//!
//! `empty.test.tsx` renders page components directly, which is right for what
//! it asserts. It would be wrong here: a page component renders fine whether or
//! not the shell routes to it, so a test built that way passes on exactly the
//! bug above.

import { cleanup, render, screen } from "@testing-library/react";
import { Router } from "wouter";
import { memoryLocation } from "wouter/memory-location";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";
import { ROUTES, footer, nav } from "./routes";

/** No endpoint, which is production today and is not what this file tests. */
beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(() => Promise.reject(new Error("no server"))),
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/**
 * The whole application, showing one path.
 *
 * `memoryLocation` rather than `history.pushState`: the default hook reads the
 * real browser location, and a jsdom document shared across a file would leak
 * one test's path into the next.
 */
function renderAt(path: string) {
  const { hook } = memoryLocation({ path, static: true });
  return render(
    <Router hook={hook}>
      <App />
    </Router>,
  );
}

describe("the shell can show every page it lists", () => {
  for (const route of ROUTES) {
    it(`renders ${route.path}`, () => {
      const { container } = renderAt(route.path);
      // The catch-all. Its presence is what a missing `<Route>` looks like,
      // and it is the assertion the re-applied bug fails on.
      expect(
        screen.queryByText(/No such page/i),
        `${route.path} (${route.label}) fell through to the 404`,
      ).toBeNull();
      // And something was actually drawn. A `<Route>` pointing at a component
      // that renders nothing would pass the check above.
      const main = container.querySelector("main")?.textContent ?? "";
      expect(main.length, `${route.path} rendered an empty main`).toBeGreaterThan(
        200,
      );
    });
  }

  it("shows the 404 for a path that is not in the table", () => {
    // The control. Without it the assertion above could pass because the
    // catch-all copy changed, rather than because every route is wired.
    renderAt("/not-a-page");
    expect(screen.getByText(/No such page/i)).toBeTruthy();
  });
});

describe("the three trust pages", () => {
  const TRUST = ["/privacy", "/terms", "/contact"] as const;

  it("are in the table, and out of the header", () => {
    for (const path of TRUST) {
      const route = ROUTES.find((r) => r.path === path);
      expect(route, `${path} is not in ROUTES`).toBeTruthy();
      expect(route?.inNav, `${path} would crowd the header`).toBe(false);
    }
    const header = nav().map((r) => r.path);
    for (const path of TRUST) expect(header).not.toContain(path);
  });

  it("are linked from the footer, so they are reachable without typing", () => {
    // A trust page nobody can find is the same as no trust page. The footer is
    // where a stranger looks for these, and it is derived from the table --
    // this checks the derivation actually reaches the document.
    const { container } = renderAt("/");
    const hrefs = Array.from(container.querySelectorAll("footer a")).map((a) =>
      a.getAttribute("href"),
    );
    for (const path of TRUST) expect(hrefs).toContain(path);
  });

  it("puts every non-header page in the footer and nothing else", () => {
    expect(footer().map((r) => r.path)).toEqual([...TRUST]);
    expect([...nav(), ...footer()]).toHaveLength(ROUTES.length);
  });

  it("says what it collects, in words, on the privacy page", () => {
    renderAt("/privacy");
    expect(screen.getByText(/collects nothing about you/i)).toBeTruthy();
    // The unknown is recorded as unknown -- rule 9 in prose. Which switches
    // are on in the operator's Cloudflare account is not a fact this
    // repository holds, and the page must not fill the gap with a comfortable
    // sentence.
    expect(screen.getByText(/will not do is guess/i)).toBeTruthy();
  });

  it("names the operator and refuses advice, on the terms page", () => {
    const { container } = renderAt("/terms");
    const text = container.textContent ?? "";
    expect(text).toMatch(/operated by Josh Fair/);
    expect(text).toMatch(/not financial advice/i);
    expect(text).toMatch(/never takes\s+custody/i);
    // No jurisdiction, entity or arbitration clause is established anywhere in
    // this repository, so the page says the omission out loud rather than
    // borrowing the paragraph every other site uses. If counsel adds one, this
    // assertion is the reminder to remove the disclaimer with it.
    expect(text).toMatch(/no governing law, no jurisdiction, no arbitration/i);
  });

  it("publishes no email address on the contact page", () => {
    // The strongest thing this page can say is the true thing: there is no
    // address, so a message claiming to be one is not from here. An invented
    // address would be worse than the blank it filled.
    const { container } = renderAt("/contact");
    const text = container.textContent ?? "";
    expect(text).toMatch(/publishes no email address/i);
    expect(text).not.toMatch(/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/);
  });

  it("refuses to print a handle it cannot verify", () => {
    // VITE_X_HANDLE is unset under vitest, which is the same state the site
    // ships in until the operator sets it. The page must not guess: a wrong
    // handle sends somebody with a prize claim to a stranger's profile.
    renderAt("/contact");
    expect(screen.getByText(/not announced here yet/i)).toBeTruthy();
  });
});
