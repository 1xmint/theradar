<!-- SPDX-License-Identifier: Apache-2.0 -->
# X automation approval — the request to post, and what to do first

**Status:** **drafted and deliberately not filed.** The decision, and the reasons
for it, are in [`x-automation-compliance.md`](x-automation-compliance.md): the
design meets the four published conditions, so a request would ask a reviewer to
restate their own documentation while putting a thread on the record inviting
scrutiny of a design nobody had questioned.

This file is kept rather than deleted because the expensive part of answering a
challenge is assembling the facts, and they are assembled. If it is ever needed,
posting it is a copy, one App ID, and the two steps in §1 -- both of which are
worth doing regardless, and §1b is currently not done.

**Why this file is in the repository.** The approval is the one finding in
research 0030 ([PR #206](https://github.com/hey-vera/radar/pull/206)) that no change here can
close, and the request is a set of claims about what this software does. Claims
about this software belong beside it, where the next person can check them
against the code rather than against a memory of a forum post.

Bot account: **@thecabalhunter** · Operator account: **@1xmint_**

---

## The precedent this is written against

On 2026-08-31, in the same category, `@Tally_DE` asked the same question for
`@MarlowStudent` (App ID 32973549). `@taycaldwell` — X staff — answered:

> Mention- or quote-triggered only, one reply per interaction, official X API,
> no unsolicited mentions or trend-jacking. If it fits those rules, no extra
> written approval.

That changes what to ask for. The productive request is **not** "please approve
us" — it is "here is the design, please confirm it needs no separate approval,
and tell us if anything does." That is what got a clean answer in a day, and the
post below is structured so a reviewer can check our design against those four
conditions in the order they were given.

It also tells us where we are least obviously compliant: **"no unsolicited
mentions or trend-jacking."** Our replies are strictly summoned, but the account
also makes two scheduled posts of its own. §3 addresses that head on rather than
leaving it to be discovered — those posts contain no @-mentions and are about
coins the account was already asked about, which is the opposite of trend-jacking,
but it is a reviewer's call and not ours.

---

## 1. Do these two things first

### 1a. Stop the account posting — before the thread goes up

The account posted a weekly summary on 2026-09-06 and was temporarily locked the
same day. Posting AI-generated replies while an approval request is open is the
specific thing that turns a request into a suspension, and the post says the bot
is disabled. It has to be true when you write it.

```bash
ssh guardian-vps-tail
sudo sed -i 's/^RADAR_X_PUBLISH=.*/# RADAR_X_PUBLISH= (off pending X confirmation)/' /etc/radar/analyst.env
sudo systemctl restart radar-analyst
journalctl -u radar-analyst -n 5
```

`may_publish` requires the exact word `on`
([`daemon.rs:254`](../crates/radar-analyst/src/daemon.rs)), so a commented-out
line is silence. Reading continues, so the reply log keeps filling — which is the
launch gate being satisfied for free: the first hundred replies read beside their
fact sheets, which we wanted before publishing anyway.

### 1b. Fix the bio, because it is the disclosure

It reads **"Hunting cabals. Exposing tokens. No mercy."** That names no operator
and does not say it is automated, so the post would claim a disclosure that is not
there — and a reviewer checks the profile first.

> Automated. Reads pump.fun launch data and answers when you @ it with a contract
> address. Operated by @1xmint_. Measured, not predicted. Not financial advice.

Also: "No mercy" is a verdict, and this account's rule — enforced in code by
[`forbidden.rs`](../crates/radar-roast/src/forbidden.rs) — is that it publishes
measurements and never verdicts. A bio that breaks the rule the replies are held
to is the first thing an adversarial reader points at.

And confirm the **Automated** label is on and linked to @1xmint_: Settings → Your
account → Account information → Automation. The post asserts it.

---

## 2. Where to post

1. Log in to **https://devcommunity.x.com** as **@1xmint_** — the account that
   owns the developer project. Not as the bot.
2. **Rules and Policies**:
   https://devcommunity.x.com/c/dev-rules-and-policies/rules-and-policies/13
3. **+ New Topic**. Title:

   ```
   Approval request: mention-triggered AI reply bot (@thecabalhunter) — App ID [APP ID]
   ```

   Putting the App ID in the title matches the thread that got answered.

4. Body: §3 below. The only edit is `[APP ID]` — the numeric id from
   console.x.com → your project → your app → Settings.

---

## 3. The post

Copy from the line below to the end of the section.

---

**What's your App ID?**
[APP ID]

**What endpoint are you using?**
`GET /2/users/:id/mentions` and `POST /2/tweets` with
`reply.in_reply_to_tweet_id`. Once a week, for scoring our own posts:
`GET /2/tweets?ids=`, `GET /2/users?ids=`, and
`GET /2/tweets/:id/{liking_users,retweeted_by,quote_tweets}`. The bio is set with
`POST /1.1/account/update_profile.json`. All posting is currently disabled.

**What API version are you on?**
X API v2. App-only bearer for reading; OAuth 1.0a user context for posting.

**Are you using a library or SDK? Which one?**
No X SDK. A Rust service makes direct HTTPS requests to the X API.

**What is the issue?**
Not a bug report. I operate @thecabalhunter and am requesting prior written
approval for its mention-triggered AI reply workflow — or written confirmation
that this design does not require separate approval, as was given for
@MarlowStudent in Rules and Policies on 2026-08-31.

I have read that answer and am asking against its four conditions directly.

**1. Mention- or quote-triggered only.** Yes. The service polls
`GET /2/users/:id/mentions` and answers nothing else. There is no keyword search,
no timeline read, no trend or hashtag monitoring — those endpoints are not called
and no code path can turn an observed post into a reply. A mention naming a
Solana contract address gets one reply with measurements read from the chain: how
many token accounts were paid in the token's launch block, what the creator has
launched before and how those turned out, the round-trip cost, and what could not
be read. A mention naming only a `$SYMBOL` gets one sentence saying a symbol is
not a token and asking for the address.

**2. One reply per interaction.** Yes, in reply to the post that summoned it.
Self-imposed ceilings, well under anything the API enforces: 3 replies per account
per day, 50 per day in total, a burst of 10 then about 2 an hour so the day's
allowance cannot be spent in the first minute, and one hour before the same token
is answered again at all — a second person asking inside that hour gets a
one-line pointer to the answer that already exists rather than a second reply.

**3. Official X API only.** Yes. No scraping, no unofficial or undocumented
endpoints, no third-party client. No automated DMs, follows, likes or reposts.

**4. No unsolicited mentions or trend-jacking.** No automated reply is ever sent
to a post that did not mention or quote the account, and no automated post ever
@-mentions anyone. **I want to be explicit about the one thing that is not a
reply**, because a reviewer looking at the timeline will see it and I would rather
say it than have it found: the account makes two scheduled posts of its own.

- **Daily, 12:00 UTC** — what became of the coins it was already asked about,
  each figure labelled with the horizon it was actually measured over.
- **Weekly, Monday 00:00 UTC** — a summary of the week's replies.

Neither mentions any account, neither is triggered by a trend, hashtag or
trending token, and both are about the bot's own prior answers rather than about
anyone else's post. I read that as an account posting its own content rather than
as automated engagement, but it is your call and not mine — if you would prefer
these disabled, or disclosed differently, say so and I will do it.

**Where the AI is, and where it cannot reach**

A language model chooses which measured facts to use and how to phrase them. It
does not choose the facts and it cannot introduce a figure:

- The model is shown a numbered fact sheet and asked to write prose containing
  slot tags (`[F1]`, `[F2]`) and **no digits at all**. Our code substitutes its
  own measured strings for those tags. A digit anywhere in the model's output, or
  a tag naming a fact it was not given, discards the reply and ships a
  deterministic template instead. A fabricated number is not filtered out — it
  cannot reach a post.
- Two further checks run on the finished text: a refusal list for verdicts,
  advice and price predictions ("scam", "rug", "honeypot", "should buy", "100x",
  "guaranteed", "to the moon" and others), and a check that every numeral appears
  on the fact sheet.
- Nothing a user writes reaches the model as an instruction. The request is built
  from the fact sheet alone; token names and symbols, which are
  attacker-controlled, are fenced as untrusted evidence and never sit in an
  instruction position.
- Every reply is written to an append-only log **before** it is posted, with the
  fact sheet it came from and the chain slot it was read at. If a reply is ever
  wrong we can show what it was built from rather than argue about it.

The whole reply pipeline is public: https://github.com/hey-vera/radar

**Opt-out, stated plainly**

There is no keyword opt-out today, and no unsolicited contact to opt out of. The
bot never initiates: it replies only to a post that mentions or quotes it, at most
once, and never DMs, follows or reposts. A user who does not want a reply does not
mention it, and blocking the account ends every interaction. There is a
server-side ignore list.

If you would prefer an explicit "stop"/"unsubscribe" opt-out that durably
suppresses replies to an account and is re-checked immediately before sending, say
so and I will add it before enabling posting. I would rather build what you want
than claim a control I have not built.

**One thing I should disclose**

On 2026-09-06 the account posted twice and sent one reply — to my own operator
account — while I was testing. It was temporarily locked the same day (code 326)
and has been reading only since. I had read the Automation Rules as covering
unsolicited automation and did not read summoned-only replies as needing prior
approval; on re-reading, and on reading the @MarlowStudent thread, I think that
was my mistake. That is why posting is off and why I am asking before doing
anything else.

**Steps to reproduce the issue**
N/A — policy approval request.

**What is the error message?**
N/A.

**When did it start?**
The implementation is complete and posting has been disabled since 2026-09-07,
pending this answer.

**What have you tried to troubleshoot?**
Read the current Automation Rules and Developer Guidelines, and the
@MarlowStudent thread in this category (2026-08-31) confirming that a
mention-triggered, one-reply-per-interaction design on the official API needs no
extra written approval. I am asking whether that also covers this account, given
the two scheduled non-mention posts described above, and whether any further
conditions apply.

Happy to show sample replies with the fact sheets they were generated from.

---

## 4. After you post

- **Do not set `RADAR_X_PUBLISH=on` until there is an answer.** The post says the
  bot is disabled; that has to stay true.
- Reading keeps working, so the reply log keeps filling — the launch gate
  satisfied while you wait.
- If a reviewer asks for the keyword opt-out, it is a small change: the gate
  already carries an ignore list
  ([`admission.rs`](../crates/radar-analyst/src/admission.rs)); it needs
  persisting and a word to look for.
- If a reviewer objects to the two scheduled posts, they are behind the same
  switch as everything else and can be left off on their own.
- Paste the thread URL into [`docs/STATE.md`](STATE.md) when it exists, so the
  next session can find it.
