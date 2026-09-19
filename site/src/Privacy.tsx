// SPDX-License-Identifier: Apache-2.0
//! What this site collects, and the parts it cannot speak for.
//!
//! # Why a privacy policy on a site with no accounts
//!
//! Two reasons, and the first is the smaller one. A young domain using crypto
//! vocabulary, with a leaderboard, a prize and a token, and with no privacy
//! policy, no terms and no way to reach whoever runs it, is the textbook input
//! to a reputation classifier. Guardio flagged `cabalhunter.org` on 2026-09-08.
//! These pages do not clear that on their own — an appeal does — but their
//! absence is the part a stranger can see.
//!
//! The larger reason is that the operator intends to take money eventually, and
//! a page like this is owed before that rather than after it.
//!
//! # The hard part is the second half, not the first
//!
//! "We collect nothing" is easy to write and easy to get wrong. It was checked:
//! nothing under `site/` reads a cookie, `localStorage`, `sessionStorage` or
//! `navigator.sendBeacon`, and no analytics script is loaded anywhere in the
//! bundle or in `index.html`. The typeface is a bundled dependency served from
//! this site's own origin, so opening the page does not announce the reader to
//! a font CDN either.
//!
//! What could not be established from the repository is which switches are on
//! in the operator's Cloudflare account, because a dashboard setting is not a
//! file. AGENTS.md rule 9: that is recorded on the page as unknown rather than
//! guessed in the flattering direction.

import { ISSUES, SOURCE } from "./honesty";
import { useTitle } from "./title";
import { Block, Card, Heading, Here, Out, Section } from "./ui";

export function Privacy() {
  useTitle("Privacy");
  return (
    <Section>
      <Heading kicker="Privacy">What this site collects</Heading>

      <div className="max-w-2xl">
        <Card className="mb-10 border-[var(--color-edge)]">
          <p className="text-[var(--color-text)]">
            <strong>This site collects nothing about you.</strong> There is no
            account, no login, no cookie, no analytics and no form that sends
            anything anywhere. It is a folder of static files. That is a claim
            about code anybody can read, and the code is{" "}
            <Out href={SOURCE}>public</Out>.
          </p>
        </Card>

        <Block title="No cookies, no analytics, no tracking">
          <p>
            The pages set no cookies. They store nothing in your browser — no
            local storage, no session storage, nothing to clear afterwards.
            There is no analytics script, no tag manager, no advertising pixel
            and no session recorder. Nothing measures you.
          </p>
          <p>
            The typeface is served from this site's own address rather than from
            a font service, so even loading the page does not tell a third party
            that you are reading it.
          </p>
        </Block>

        <Block title="What the host sees, because every host does">
          <p>
            The site is served as static files through Cloudflare. Any web host
            necessarily sees a request arrive: the address it came from, the
            page asked for, the browser that asked, and roughly when. That is
            how the web works and it is not something this site chooses or
            switches off.
          </p>
          <p>
            <strong>What this page will not do is guess.</strong> Cloudflare
            also offers the operator a traffic dashboard, and whether it is
            switched on is a setting inside an account rather than a file in the
            repository this page was built from. So it is written down as
            unknown. When it has been established, it will be stated here with
            the date it was checked, like every other figure on this site.
          </p>
        </Block>

        <Block title="The numbers on the page">
          <p>
            The figures come from small public JSON documents — the population
            statistics, the week's leaderboard, the prize pool and the past
            weeks. Your browser asks for them from Radar's own server, which
            sees that request the same way any web server sees one.
          </p>
          <p>
            Nothing about you travels with it. No cookie, no identifier, no
            query string, and nothing about which coin you were reading —
            because the site never sends that anywhere at all. If the request
            fails, the page falls back to a snapshot committed to the repository
            and tells you it is showing an older measurement.
          </p>
        </Block>

        <Block title="Pasting an address here sends it nowhere">
          <p>
            The box on the front page checks the shape of a mint address in your
            own browser and then builds a link to X with the post already
            written. It is your post, sent from your account, when you press
            send. The address you pasted never reaches this site's host, and
            there is no endpoint here for it to reach.
          </p>
        </Block>

        <Block title="The one place a name can appear">
          <p>
            If you mention the account on X, you have posted in public, and X's
            own terms govern that post. When a week closes, the leaderboard and
            the past-weeks page republish what that public record holds: the
            numeric account id of an entrant, the handle if one was read at the
            close, the coin they asked about, and a link to the reply. Accounts
            that did not count are published as counts, never as names.
          </p>
          <p>
            If you win and claim, you reply in public with a Solana address and
            the prize is paid on chain. The site links that transaction, because
            a prize nobody can check is not evidence of anything. The address is
            public because the chain is public, not because this site disclosed
            it.
          </p>
        </Block>

        <Block title="Nothing is sold or shared">
          <p>
            There is no data to sell, no advertising, no third-party analytics
            provider, no mailing list and no data broker. Nothing is shared with
            anybody, because nothing is gathered in the first place.
          </p>
        </Block>

        <Block title="Links away from here">
          <p>
            Links to X and to Solscan take you to sites this operator does not
            run. What they collect is theirs to state, and their terms apply
            once you arrive.
          </p>
        </Block>

        <Block title="If this is wrong">
          <p>
            If you find anything on this site that gathers data and is not
            described above, that is a bug and it will be treated as one. Report
            it on <Out href={ISSUES}>the repository's issues</Out>, or reply to
            the account —{" "}
            <Here href="/contact">both are on the contact page</Here>
            .
          </p>
        </Block>

        <p className="mt-12 text-sm text-[var(--color-faint)]">
          Last reviewed 2026-09-08, against the source in this repository. This
          page changes by a public commit, so every version of it can be read
          alongside every other.
        </p>
      </div>
    </Section>
  );
}
