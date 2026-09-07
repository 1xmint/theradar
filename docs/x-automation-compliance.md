<!-- SPDX-License-Identifier: Apache-2.0 -->
# X automation: the compliance position, and why no approval request was filed

**Status:** decided 2026-09-07. **No written approval request is being filed.**
The design fits the published conditions, and a request would ask a question
that has already been answered in public. What remains is not a policy problem.

Bot account: **@thecabalhunter** · Operator account: **@1xmint_**

---

## The answer that settles it

On 2026-08-31, in `Rules and Policies` on `devcommunity.x.com`, `@Tally_DE` asked
for approval of a mention-triggered AI reply bot (`@MarlowStudent`, App ID
32973549). `@taycaldwell` — X staff — answered:

> Mention- or quote-triggered only, one reply per interaction, official X API,
> no unsolicited mentions or trend-jacking. If it fits those rules, no extra
> written approval.
> https://docs.x.com/developer-guidelines.md

That is a condition, not a discretionary grant. Filing a request that says "we
meet the four conditions, please confirm we meet the four conditions" asks a
reviewer to restate their own documentation, and it puts a thread on the record
inviting scrutiny of a design nobody had questioned.

## How this account measures against the four

| condition | us | where |
|---|---|---|
| **Mention- or quote-triggered only** | Yes. The service polls `GET /2/users/:id/mentions` and answers nothing else. No keyword search, no timeline read, no trend or hashtag monitoring — those endpoints are not called and no code path turns an observed post into a reply. | [`x.rs`](../crates/radar-analyst/src/x.rs), [`daemon.rs`](../crates/radar-analyst/src/daemon.rs) |
| **One reply per interaction** | Yes, in reply to the summoning post. Plus self-imposed ceilings well under the API's: 3 per account per day, 50 per day total, a burst of 10 then ~2/hour, and one hour before the same token is answered again — a second asker inside that hour gets a pointer to the existing answer, not a second reply. | [`admission.rs`](../crates/radar-analyst/src/admission.rs) |
| **Official X API only** | Yes. No scraping, no undocumented endpoints, no third-party client. No automated DMs, follows, likes or reposts. | [`x.rs`](../crates/radar-analyst/src/x.rs) |
| **No unsolicited mentions or trend-jacking** | No automated reply reaches a post that did not mention or quote the account, and **no automated post @-mentions anyone**. See the note below on the two scheduled posts. | [`daemon.rs`](../crates/radar-analyst/src/daemon.rs) |

**The two scheduled posts, which is where we differ from the precedent.** The
account posts daily at 12:00 UTC (what became of coins it was already asked
about) and weekly on Monday at 00:00 UTC (a summary of the week's replies).
Neither mentions any account, neither is triggered by a trend, a hashtag or a
trending token, and both are about the bot's own prior answers.

That is an account posting its own content, which is what every account does, and
it is the opposite of trend-jacking — the trigger is our own reply history, not
somebody else's momentum. **Recorded as a judgement, not a certainty.** If it is
ever raised, both posts sit behind the same switch as everything else and can be
turned off on their own.

## What is still open, and it is not a policy question

**The account was temporarily locked (code 326) on 2026-09-06**, hours after it
first posted. A forum thread would not have fixed that and it is the only real
risk here.

The likely cause is a new account posting through the API in its first hours
tripping an anti-spam heuristic, not an automation-rules enforcement — **but that
is inference, not something verified**, and the difference matters. Before posting
is enabled again:

1. Clear the lock through the app's own unlock flow.
2. If it recurs, the reason will now be legible: `Unreachable::Refused` keeps 300
   bytes of the platform's body ([#201](https://github.com/hey-vera/radar/pull/201)),
   where it previously kept `String::new()` — which is how a lock, a 403 code or
   a policy notice became invisible.
3. Turn posting on and watch the first day rather than the first week.

## What to do anyway, because it is required regardless

These are standing Automation Rules obligations, not approval conditions, and one
of them is currently not met.

**The bio is the disclosure and it does not disclose.** It reads *"Hunting
cabals. Exposing tokens. No mercy."* — no operator, no statement that it is
automated. Replace it with something like:

> Automated. Reads pump.fun launch data and answers when you @ it with a contract
> address. Operated by @1xmint_. Measured, not predicted. Not financial advice.

"No mercy" is also a verdict, and this account's rule — enforced in code by
[`forbidden.rs`](../crates/radar-roast/src/forbidden.rs) — is that it publishes
measurements and never verdicts. A bio that breaks the rule the replies are held
to is the first thing an adversarial reader points at.

**Confirm the Automated label is on and linked to @1xmint_**: Settings → Your
account → Account information → Automation.

## Opt-out: what exists, and what does not

There is **no keyword opt-out**, and no unsolicited contact to opt out of. The
bot never initiates; it replies only to a post that mentions or quotes it, at most
once, and never DMs, follows or reposts. A user who does not want a reply does not
mention it, and blocking ends every interaction. There is a server-side ignore
list.

If one is ever wanted, it is a small change: the gate already carries an ignore
list ([`admission.rs`](../crates/radar-analyst/src/admission.rs)); it needs
persisting and a word to look for. Written down here so that the absence is a
decision on the record rather than an oversight.

## If this is ever challenged

The full request — App ID, endpoint table, the four conditions answered one by
one, the AI boundary, the rate limits, and a disclosure of the lock — is drafted
and ready at
[`x-automation-approval-request.md`](x-automation-approval-request.md). It is kept
rather than deleted because the expensive part of answering a challenge is
assembling the facts, and they are assembled. Posting it is then a copy and one
App ID.
