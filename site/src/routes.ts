// SPDX-License-Identifier: Apache-2.0
//! The site's pages.
//!
//! A flat list rather than the console's audience-classified table, because
//! every page here is public by construction: this application is served as
//! static files from a host that has no idea who is reading. There is nothing to
//! classify and nothing that could leak.
//!
//! That is the whole argument for it being a separate application. In `web/`,
//! a page added to the routes and forgotten in `access::audience_of` is not a
//! 404 -- it is a page that silently requires operator identity. Here there is
//! no such seam to get wrong.

/** One page. */
export interface Route {
  readonly path: string;
  readonly label: string;
  /**
   * What the header calls it, when that differs.
   *
   * "Prize pool" wrapped onto two lines at 375px and made the sticky header
   * eat a third of the screen — on the width most of this traffic arrives at,
   * since the whole distribution is a link in a reply on a phone. The page's
   * own heading still says "The prize pool", so nothing is lost.
   */
  readonly short?: string;
  /**
   * Whether it appears in the header.
   *
   * `false` does not mean hidden. It means the footer, which is where a reader
   * looks for a privacy policy, terms, or a way to reach whoever runs the
   * thing — and it keeps the header at six items on a 375px phone, which is
   * the width most of this traffic arrives at.
   *
   * Both lists are derived from this table rather than written out again, so
   * the way to lose a page is to leave it out of the table entirely. A page in
   * the table with no `<Route>` in `App.tsx` renders the 404, and
   * `routes.test.tsx` fails on it.
   */
  readonly inNav: boolean;
}

export const ROUTES = [
  { path: "/", label: "Home", inNav: true },
  { path: "/leaderboard", label: "Leaderboard", inNav: true },
  { path: "/pool", label: "Prize pool", short: "Pool", inNav: true },
  { path: "/history", label: "Past weeks", short: "History", inNav: true },
  { path: "/token", label: "Tokenomics", short: "Token", inNav: true },
  { path: "/about", label: "About", inNav: true },
  // The three trust pages. Footer, not header: a stranger looks for these
  // before deciding whether to believe the rest of the site, and a young
  // domain talking about tokens without any of them reads to a reputation
  // classifier -- and to a person -- exactly the way it reads.
  { path: "/privacy", label: "Privacy", inNav: false },
  { path: "/terms", label: "Terms of use", short: "Terms", inNav: false },
  { path: "/contact", label: "Contact", inNav: false },
] as const satisfies readonly Route[];

/** The pages the header shows. */
export function nav(): readonly Route[] {
  return ROUTES.filter((r) => r.inNav);
}

/**
 * The pages the footer shows.
 *
 * The complement of [`nav`], deliberately: every page is in exactly one of the
 * two, so a page cannot be added to the table and then be reachable only by
 * typing its address.
 */
export function footer(): readonly Route[] {
  return ROUTES.filter((r) => !r.inNav);
}
