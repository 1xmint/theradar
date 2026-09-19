// SPDX-License-Identifier: Apache-2.0
//! The terms this site is offered under, and the ones it refuses to invent.
//!
//! # What is deliberately missing
//!
//! No governing law, no jurisdiction, no arbitration clause, no limitation
//! figure and no company entity. **None of those is established anywhere in
//! this repository**, and a terms page is the worst place on a website to write
//! down something nobody decided: it is the page a reader consults precisely
//! when they want to know what was actually agreed.
//!
//! So the omission is stated on the page rather than filled with the paragraph
//! every other site uses. Counsel adds those clauses, dated, or they stay out.
//!
//! # Everything else here is already true elsewhere
//!
//! The refusals are not new promises invented for a legal page. "Not financial
//! advice", "never predicts a price", "the operator holds zero tokens",
//! "entry is free" and "corrections are published in the same place as the
//! original" are the product's own rules — ADR 0013 and `forbidden.rs` enforce
//! them on the bot, `/about` and `/token` state them, and this page restates
//! them in the register a reader expects to find them in. A terms page that
//! contradicted the rest of the site would be the failure worth avoiding.

import { ISSUES, SOURCE } from "./honesty";
import { useTitle } from "./title";
import { Block, Card, Heading, Here, Out, Section } from "./ui";

export function Terms() {
  useTitle("Terms of use");
  return (
    <Section>
      <Heading kicker="Terms of use">What this is, and what it is not</Heading>

      <div className="max-w-2xl">
        <Card className="mb-10 border-[var(--color-edge)]">
          <p className="text-[var(--color-text)]">
            <strong>
              Cabal Hunter is measurement and information, not financial advice.
            </strong>{" "}
            It is operated by Josh Fair. Nothing here is an offer to trade for
            you, to hold your money, or to buy or sell anything. Using this site
            means accepting the terms below.
          </p>
        </Card>

        <Block title="Who operates this">
          <p>
            Cabal Hunter is operated by Josh Fair. The account on X is
            automated, and it says so on the account and on{" "}
            <Here href="/about">the about page</Here>. The software behind it is{" "}
            <Out href={SOURCE}>published in full</Out> under the Apache License
            2.0, so the rules described here can be checked against the code
            that implements them rather than taken on trust.
          </p>
        </Block>

        <Block title="What the service does">
          <p>
            It reads public Solana chain data — token launches, the accounts
            paid in a launch block, bonding curves, what a creator has launched
            before — and reports what it measured, with the moment it measured
            it. Every figure is published with a date because every figure is
            expected to move.
          </p>
          <p>
            It does not predict prices. It does not rank coins as investments,
            it does not tell you a coin will rise or fall, and it does not tell
            you to buy or sell. That is a limit of the measurements, not
            modesty: nothing in this data supports a claim about where a price
            is going.
          </p>
        </Block>

        <Block title="Not financial advice">
          <p>
            Nothing on this site, and nothing in a reply the account posts, is
            financial, investment, legal, tax or accounting advice, a personal
            recommendation, or a suggestion that any trade is suitable for you.
            It is general information about public data.
          </p>
          <p>
            Decisions you make with it are yours. Trading tokens of this kind
            loses money for most people who try it, and this site publishes the
            figures that say so rather than the ones that would sell it.
          </p>
        </Block>

        <Block title="No offer, no custody, no management">
          <p>
            Nothing here is an offer or a solicitation to buy or sell any token
            or security. The operator does not accept deposits, does not manage
            money for anybody, does not trade on your behalf, and never takes
            custody of anything of yours. There is nothing to connect and
            nothing to sign.
          </p>
          <p>
            <strong>
              Nobody running this will ever ask you for a private key or a seed
              phrase, and nobody will message you first asking for one.
            </strong>{" "}
            Anyone who does is not us, whatever name they are using.
          </p>
        </Block>

        <Block title="No guarantee of accuracy">
          <p>
            The figures are measurements taken at a moment, from data that
            changes, using instruments that have been wrong before. One earlier
            measurement of these same quantities was out by a factor of 2.7 nine
            days later, which is why nothing here is presented as a constant and
            everything carries its date.
          </p>
          <p>
            When something is found to be wrong, the correction is published in
            the same place as the original and the account posts it. Corrections
            are not quietly edited in. But no accuracy is guaranteed, and you
            should check anything that matters to you against the chain itself —
            which is why the links to do so are on the page.
          </p>
        </Block>

        <Block title="No guarantee of availability">
          <p>
            The site, the account and the data it reads may be unavailable,
            delayed, incomplete or discontinued at any time and without notice.
            The account may not answer. A figure may fall back to an older
            committed snapshot, in which case the page says so.
          </p>
          <p>
            Everything is provided as it is, without warranty of any kind, to
            the extent the law allows.
          </p>
        </Block>

        <Block title="The contest and the token">
          <p>
            Entry to the weekly contest is free and never requires holding
            anything. The rules, the scoring and the claim steps are on{" "}
            <Here href="/leaderboard">the leaderboard page</Here>, and the
            token's rules are on <Here href="/token">the token page</Here>.
            Where those pages and this one differ, those pages are the specific
            statement and this is the summary.
          </p>
          <p>
            The token is a badge. It is not a share of anything, it grants no
            vote, it buys no feature, and it entitles you to no part of any
            revenue. The operator holds none of it. A prize is paid to whoever
            the published rule ranks first and who claims within the published
            window, and a week can be voided with the reason stated in public.
          </p>
        </Block>

        <Block title="Using the site">
          <p>
            Read it, share it, quote it. Do not attack it, do not try to
            interfere with its availability for other people, and do not present
            yourself as this account or as its operator.
          </p>
        </Block>

        <Block title="Links to other sites">
          <p>
            Links to X, to Solscan and to the repository lead to services this
            operator does not run and cannot vouch for. Their terms govern what
            happens once you arrive.
          </p>
        </Block>

        <Block title="What these terms deliberately do not say">
          <p>
            They name no governing law, no jurisdiction, no arbitration
            procedure, no liability cap and no company. None of that has been
            established for this project, and writing down a clause nobody
            decided would be exactly the kind of confident, unchecked claim this
            whole site exists to argue against.
          </p>
          <p>
            If a lawyer adds that language later it will appear here, dated,
            like everything else. Until then this page tells you what is true
            and stops.
          </p>
        </Block>

        <Block title="Changes, and how to raise one">
          <p>
            This page is built from a public repository, so every change to it
            is a commit anybody can read and compare. If something here is
            wrong, unclear, or contradicts another page, say so on{" "}
            <Out href={ISSUES}>the repository's issues</Out> or through{" "}
            <Here href="/contact">the contact page</Here>.
          </p>
        </Block>

        <p className="mt-12 text-sm text-[var(--color-faint)]">
          Last reviewed 2026-09-08. Measured, not predicted. Not financial
          advice, not a recommendation, and not a solicitation to buy or sell
          anything.
        </p>
      </div>
    </Section>
  );
}
