<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0015 — The prize is an evidence relay, and a winner is always selected

**Date:** 2026-09-07
**Status:** accepted, and **not yet implemented**. This is Josh's decision,
recorded. The code it describes lives in
[`crates/radar-contest`](../../crates/radar-contest) and
[`crates/radar-analyst`](../../crates/radar-analyst) and still implements the
rule this ADR supersedes; the branches that change it are items 6 and 7 of
[plan 0010](../plans/0010-radar-actualization.md).
**Decides:** what a contest entry is, how a week is scored when the engagement
data are incomplete, and what happens when no evidence arrives at all.
**Supersedes:** the entry mechanism of
[ADR 0013](0013-a-community-token-exists-and-radar-holds-none-of-it.md)
constraint 4 ("anyone who mentions the bot is entered") and the weighted
engagement formula of
[design 0007](../design/0007-the-end-to-end-plan.md) §6.2. Constraint 4's
substance — **entry is free and never requires holding the token** — is kept
exactly. Constraint 3 — **100% of the creator fee is the prize** — is untouched.

## Context

Three facts forced this, and only the first was known when the current rule was
written.

**1. The current rule scores the wrong post, and can score two things at once.**
Every summoned reply is an entry; the score is
`3·reposts + 3·quotes + 1·likes + 1·replies` over the **bot's own reply**
(design 0007 §6.2). So the
entrant's contribution is asking, and everything after that is the bot's
audience rather than the entrant's reach. Worse, the week-close walk stops early
([`scan_ranking`](../../crates/radar-analyst/src/contest.rs)), and an entry the
walk never reached keeps `verified: None`, which
[`Metrics::score`](../../crates/radar-contest/src/score.rs) resolves to the
**raw** score. A week that hits a meter refusal or a read error therefore ranks
some entries by distinct accounts and others by raw counts, in the same list.
That inequality is documented and it is real — but a ranking is not sound
because its units are ordered, it is sound because they are the same unit.
[Research 0031](../research/0031-radar-handoff-inspection.md).

**2. The owner has decided that incomplete evidence must not withhold a
winner.** Asked directly, Josh rejected a no-winner outcome for a tied week, an
unresolved-score rollover, and a fraud-suspicion veto. He accepts farmed
participation as a cost of distribution. That is a decision about what the
contest is *for*: it is a distribution mechanism whose weekly result must land
on time, not a fairness tribunal.

**3. Nothing an open contest on pseudonymous accounts can measure proves account
independence.** An attacker with aged accounts produces the same observable
inputs as an audience. A deterministic function must treat identical inputs
identically; a model cannot recover identity evidence that was never
observable. The current `min_account_age_days` floor and
[`Excluded::AccountAgeUnknown`](../../crates/radar-contest/src/score.rs) do not
change that — they price a farm at a few dollars and make a lookup failure
decide a real entrant's week.

## Decision

### Entry is an explicit relay of an existing receipt

An entrant quotes a **published Radar receipt post**, mentions the bot, and
includes `#hunt`. The receipt must already be in Radar's own publication log.
The mention is what carries it into the existing ingress, so identity and
consent come from the same path that already handles summons, and an arbitrary
quoted link cannot choose a destination.

One active nomination per account per UTC week. A later valid nomination
replaces the account's earlier one and takes the new quote's timestamp; **every
replacement is stored**, because the record has to show what the rule was
applied to. Ordinary summons carry no entry requirement and are not entries.

**This removes the first-asker's exclusive claim on a coin.** Anyone can relay
the same receipt, so being early is a tie-break, not ownership.

### One scoring mode for the whole week, chosen at the deadline

The week closes; active valid nominations and the rules version are frozen from
the durable journal; engagement is collected over the following 24 hours, with
each observation interval published. At the deadline **exactly one** mode is
selected for **every** nomination in the week, first applicable:

1. **Observed accounts.** Every entry's repost and quote pagination completed,
   every returned action carries a usable author id, and no unresolved source or
   schema error remains. The score is the size of the union of distinct sharing
   account ids, after removing the entrant itself and the operator set. One
   account contributes at most one unit however many times it acts.
2. **Reported actions.** Mode 1 unavailable, but every entry has a frozen raw
   counter capture. The score is `reposts + quote_posts` from those captures, in
   checked arithmetic. This counts **actions**, and may count the same account
   twice; the public label says so. Likes, replies and views count nothing in
   either mode.
3. **Recorded entry order.** Some entry has no usable counters. Scores are
   `null` — not zero — and the winner is the earliest valid active nomination
   under the tie order below.

Ties, in modes 1 and 2 after score descending, and alone in mode 3: earliest
creation time of the account's active nominated quote, then numeric post id
ascending, then numeric account id ascending. Ids are compared **numerically**,
in a type that cannot overflow the platform's format; decimal ids compared
lexicographically order `9` above `10`.

**A week with at least one valid entry always produces exactly one winner** —
including an all-zero week and a total engagement-API outage. A week with no
valid entrant records `no_entries` and manufactures nobody. Corrupted or
unreadable entry history is a **system fault**, not an incomplete-engagement
case: it is restored and replayed before a result is declared.

Every selection writes a **winner certificate**: the frozen candidate set, the
chosen mode, the fallback reason, the observed scores or nulls, the tie
comparisons that ran, the rules version, the input hashes, and the selected
account.

### What is deleted, and why

**The account-age floor and the age-unknown exclusion go.** They were rule 9
applied to the wrong question. Rule 9 says an unmeasured thing must not be
recorded as a measured zero; it does not say an unavailable third-party lookup
should decide whether a locally recorded valid entrant can win. Age is kept as a
**diagnostic** field, published beside the result, deciding nothing.

**The four weights go.** Modes 1 and 2 are counts, not a formula. `REPLY_WEIGHT`
and `LIKE_WEIGHT` were already zero under the verified rule; the remaining
`3·reposts + 3·quoters` is a monotone transform of `reposts + quoters` and the
multiplication only made the published rule harder to check.

The operator exclusion set and the winner cooldown stay, checked against local
records at nomination time.

## What this costs, stated plainly

**A farm can win.** Modes 1 and 2 count accounts and actions; neither observes
who owns them. This is not hidden behind a confidence threshold and a winner is
never retrospectively disqualified for looking coordinated. Nothing here
authorises Radar to create accounts, buy engagement, or encourage a breach of
X's rules.

**The fallback is an attack surface.** An entrant who nominates early and then
provokes an API failure gets mode 3, where earliest nomination wins. That is a
real cost of guaranteed selection under unavailable evidence. The response is to
**measure how often each mode fires** and publish it, not to add a secret veto.

**Selecting a winner is not paying one.** A certificate is inert data. The
existing deterministic payout policy in
[`crates/radar-payout`](../../crates/radar-payout) separately establishes a
permitted week, a valid claim, an eligible jurisdiction configuration, the
destination, available earmarked fees, the reserve floor, replay protection and
the absence of a prior payout. A selected winner with no valid claim stays
visible and cannot cause a payment.

## What this does not decide

- **The prize's legal footing.** ADR 0013's legal precondition is unchanged and
  uncleared. Guaranteed selection makes the rule simpler to publish; it does not
  make it lawful anywhere.
- **Whether mode 1 is reachable in practice** on the API tier Radar pays for.
  That is a measurement, and the first four closed weeks are where it comes
  from.
- **Whether the relay produces repeat summoners.** Distribution is the reason
  for this design and it is not evidence for it. Farmed sharing can inflate a
  score without producing one returning reader, and those outcomes are reported
  separately.
