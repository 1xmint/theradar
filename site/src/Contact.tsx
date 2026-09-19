// SPDX-License-Identifier: Apache-2.0
//! How to reach whoever runs this, using only the channels that exist.
//!
//! # There is no email address, and this page says so
//!
//! The repository was searched for one. `SECURITY.md` routes vulnerabilities to
//! GitHub's private advisory form, `/about` says corrections arrive by replying
//! to the account, and nothing anywhere records a contact address. So this page
//! publishes the three channels that are real and states plainly that there is
//! no fourth — which is worth more than a made-up address, because a stranger
//! who is told "there is no email" can recognise the message that claims to be
//! one.
//!
//! # The handle is configuration, not a constant
//!
//! Same rule as [`Summon`]: `account()` reads `VITE_X_HANDLE` and validates it
//! against X's own rule, and when it is absent this page refuses rather than
//! printing a guess. A wrong handle on the contact page sends somebody with a
//! complaint — or a prize claim — to a stranger's profile, and that is the one
//! link on this site that cannot be walked back.

import { ADVISORY, ISSUES, SOURCE, account, handleHref } from "./honesty";
import { useTitle } from "./title";
import { Block, Card, Heading, Here, Nothing, Out, Section } from "./ui";

export function Contact() {
  useTitle("Contact");
  const handle = account();
  const profile = handle === null ? null : handleHref(handle);
  return (
    <Section>
      <Heading kicker="Contact">How to reach the operator</Heading>

      <div className="max-w-2xl">
        <Card className="mb-10 border-[var(--color-edge)]">
          <p className="text-[var(--color-text)]">
            <strong>Cabal Hunter is operated by Josh Fair.</strong> There are
            three ways to reach him, all of them public, and they are the only
            three. Everything below is a channel that exists today; nothing has
            been added here to make the page look fuller.
          </p>
        </Card>

        <Block title="On X, in the open">
          {profile === null || handle === null ? (
            <Nothing
              what="The account is not announced here yet."
              why="This page will not print a handle it cannot verify — a wrong one sends you to somebody else's profile. It appears here once the operator sets it."
            />
          ) : (
            <>
              <p>
                Reply to the account, or mention it:{" "}
                <Out href={profile}>@{handle}</Out>. It reads mentions, and a
                reply in the thread is the fastest way to be answered.
              </p>
              <p>
                This is also where corrections belong. If a number looks wrong,
                or a coin is being described unfairly, say so in the thread. The
                evidence behind every reply is kept, so a disagreement can be
                settled by looking rather than by arguing — and a correction is
                published in the same place as the thing it corrects.
              </p>
            </>
          )}
        </Block>

        <Block title="On GitHub, for anything about the software">
          <p>
            The whole system is <Out href={SOURCE}>published</Out>. A bug on this
            site, a figure that does not add up, a page that contradicts another
            one: open an issue at{" "}
            <Out href={ISSUES}>github.com/hey-vera/radar/issues</Out>. Issues are
            public, which is the point — so is the answer.
          </p>
        </Block>

        <Block title="Privately, for a security problem">
          <p>
            Report anything exploitable through{" "}
            <Out href={ADVISORY}>GitHub's private advisory form</Out>, which is
            visible only to the maintainers until an advisory is published.
            Please do not open a public issue for it. There is no bounty and no
            response-time commitment; what you get is an acknowledgement, a
            considered reply, and credit unless you would rather not have it.
          </p>
        </Block>

        <Block title="There is no email address">
          <p>
            <strong>
              This project publishes no email address, and it has no support
              inbox.
            </strong>{" "}
            If you receive a message from one claiming to be Cabal Hunter, it did
            not come from here.
          </p>
          <p>
            Nobody running this will message you first, ask you for a private key
            or a seed phrase, ask you to connect a wallet, or ask you to send
            anything anywhere. There is nothing here to connect and nothing to
            sign. If you have won, the account replies to you in your own thread,
            under the reply that won, and you claim by replying to that post with
            a Solana address — described step by step on{" "}
            <Here href="/leaderboard">the leaderboard page</Here>.
          </p>
        </Block>

        <Block title="What is not a channel">
          <p>
            There is no phone number, no postal address published here, no
            Telegram group and no Discord run by this operator. If one exists
            under this name, it is not this project's.
          </p>
        </Block>

        <p className="mt-12 text-sm text-[var(--color-faint)]">
          Last reviewed 2026-09-08. See also{" "}
          <Here href="/privacy">what this site collects</Here> and{" "}
          <Here href="/terms">the terms of use</Here>.
        </p>
      </div>
    </Section>
  );
}
