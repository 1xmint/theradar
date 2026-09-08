# Brief for Astra — Radar: sharper intelligence, a coin built to spread, a face worth looking at

You are auditing and then **improving** `hey-vera/radar`, a Rust workspace on
Solana. Read the repository first. Everything below is context so you spend your
budget on judgement rather than archaeology — but treat it as claims to verify,
not as truth. Where this brief and the code disagree, the code is right and this
brief is a bug.

Start with `AGENTS.md` (the operating policy), then `GOAL.md`, then
`docs/STATE.md`, then `docs/research/0030-the-adversarial-audit.md`. Run
`just orient`.

---

## Read this correction first, because it changes what you are being asked

A prior adversarial audit (research 0030, 2026-09-07) concluded: *"It cannot make
the bot famous. Fame needs a fact worth quoting and a person quoting it."*

That is true about **that audit's scope** and misleading as a verdict, and if you
inherit the framing you will do the wrong work. Nobody is asking you to make
something go viral. **The task is to build the machine that gives it the best
possible chance, and to remove every reason it would fail to.** Those are
different jobs. The second one is engineering, and it is yours.

Assume the goal is a memecoin and an X account that reach a very large audience,
legally, on the strength of being genuinely useful and genuinely unlike anything
else in the category. Then work backwards from that to what the repository must
become.

---

## What Radar is, as of 2026-09-07

Two products in one repo, and a third thing that does not exist yet.

**1. The X bot — `@thecabalhunter`, site `cabalhunter.org`.** Someone @-mentions
it with a Solana contract address; it replies with measurements read from the
chain and from Radar's own recorded history of the chain. It also posts daily
(12:00 UTC, what became of coins it was asked about a week ago) and weekly
(Monday 00:00 UTC, the week's summary).

Honest state: the account went live 2026-09-06. **6 followers. 2 posts. One
reply ever, to the operator's own account.** The launch gate — 200 distinct
summoners in 30 days before a token is minted — is at **0/200**. Posting is
currently switched off pending an X automation question that has since been
resolved (`docs/x-automation-compliance.md`).

**2. The trading lane** — `radar-risk`, `-strategy`, `-exec`, `-signer`,
`-pumpfun`, `-sim`. Built, composed in tests, and **shut by `Policy::CLOSED`**
because the repository's own research measured its edge at **0 bps against a
~456 bps bar** (research 0017, 0022). Do not reopen it. Do not propose reopening
it. Its correctness matters because the bot's facts come from the same store and
the same decoders.

**3. The token — it does not exist.** This is the most important sentence in the
brief. Nothing has been minted, so the token's design is still fully open inside
six constraints (ADR 0013) and six decisions already taken (design 0009,
L1–L6): 100% of creator fees become a weekly prize, no burn-for-access, one
contest, status over cash, Telegram as a free lane, a daily "seven days later"
post. At $10k/week of volume the prize is about **$30** — the money is not the
prize, the status is, and the design says so.

**The store:** ~592,000 launches, ~1.67M outcomes, ~11,000 decisions, recorded
with a point-in-time watermark guarantee.

**Scale of the code:** 26 crates, ~2,089 tests, `just check` runs build, test,
clippy (pedantic, denied) and fmt. CI additionally runs sharded mutation testing.
There is a `repo-conformance` crate that fails the build when a document names a
file that does not exist or makes a dependency claim that is false.

---

## The one structural fact that constrains everything about distribution

**On X, this bot can only reply when it is summoned.** Replies are accepted only
where the parent post's author mentioned or quoted the account. It cannot start
conversations. It cannot reply to a trending post. It cannot search for coins
being discussed and volunteer an answer. This is a platform rule the repo
complies with deliberately, not a missing feature.

So the bot cannot *push*. It can only be **worth summoning, and worth
screenshotting once it answers.** Every growth idea has to survive that
constraint. The operator's own account quoting the bot's replies is free
distribution the rules permit; the bot may reply to a quote.

Josh's working hypothesis, which you should stress-test rather than accept:

> The attention flywheel — the more people use the bot, the more popular it gets,
> the more people see the token, and the loop continues.

Tell him honestly whether that loop closes, where it leaks, what its actual
coefficient looks like, and what the real mechanism is if that one is too weak.

---

## Your three objectives, in priority order

### 1. Make the intelligence genuinely harder to fool — cabals, bundles, and beyond

This is the product. A reply is worth screenshotting only if it states a fact
about *this* coin that nobody else states.

What exists today: launch-block recipient counts (the coordination signal),
creator history, bonding-curve and fee facts, round-trip cost, and published
population base rates.

What the repo has already measured, and you should not re-derive:

- Research 0024: **10–13 recipients in the launch block → 10.1× the base rate of
  instant graduation.** 1–3 recipients is **70.5% of all launches** and graduates
  instantly **0.02%** of the time. This *corrected* research 0008, whose headline
  was wrong by 2.7× nine days later — the recipient distribution is a
  configuration of whatever tool the launchers are running, not a law. Anything
  you build on it must carry a date and be re-measurable.
- Research 0007: a creator's prior organic graduation predicts ~2× the future
  rate, with non-overlapping 95% intervals.
- Design 0010 §7.2 already enumerates **ten candidate facts** with a verdict
  each: mint/freeze authority and Token-2022 extensions, holder concentration
  outside the pool, launch-block contiguity (Jito bundle shape), trades-to-depth,
  trader prevalence, creator funding source one hop, authority prevalence,
  repeated metadata, post-launch bundling, and a refused list.
- Design 0010 §7.1 is the **admission test** a fact must pass before it may be
  stated. Read it before proposing any new fact.

**What I actually want from you here, and it is more than picking from that
list:**

- Is *recipient count* even the right primitive? It is a proxy. What is the thing
  it is a proxy for, and can that be measured directly?
- Bundle detection today is inferential. Jito bundles are atomic, sequential and
  inside one slot — what does that let you assert with certainty rather than
  correlation, and what fixture proves it?
- What detections are **missing entirely** from 0010's list of ten? Come with
  candidates the repo has not thought of. This is where I most want your
  mathematics.
- Where is the current signal set **gameable by a launcher who has read this
  repository**, which is public? Assume an adversary who knows exactly what is
  measured and optimises against it. What survives that? What is only working
  because nobody has bothered yet?
- Signal decay: which of these die once they are known, and which are structural
  and cannot be evaded without giving up the thing the launcher wants?
- The instrument `radar features` / `radar edge` exists and **has never been
  run** against the production store. Is the walk-forward protocol it implements
  actually sound? Look for look-ahead, leakage, survivorship, multiple-comparison
  problems, and a base rate computed over a population that is not the
  population.

### 2. Design the coin and the loop for the largest legal reach

The coin does not exist. You have a blank slate inside ADR 0013 and design 0009.

- Stress-test the attention flywheel above. Where does it leak? What is the
  actual acquisition mechanism for the first 200 summoners, then the first
  20,000? Be concrete and be honest about what does not work.
- The contest as designed rewards *engagement on the reply you were given* —
  which means it rewards asking first, which rewards automation, which is the
  exact behaviour this account exists to expose. That race is partly fixed (a
  duplicate now gets a pointer), but the incentive shape is still yours to
  examine.
- What makes a reply screenshot-worthy rather than merely correct? The account's
  own rule is that it publishes the measurement and never the verdict, and a
  refusal list enforces it in code. Work inside that. "Savage and dry, the
  numbers are the joke" is the intended voice.
- What is the honest, legal relationship between the coin and the product? The
  operator holds zero tokens by design. There is a real question here about
  whether a prize contest reads as a promotion, a sweepstake or something
  regulated, and design 0010 §7.5 flags legal exposure as *"flagged, not
  answered"*. Answer it, or say precisely what a lawyer must answer.
- **Legally** is not a footnote. Naming coins and creators in public is
  defamation-adjacent; that is why `forbidden.rs` exists and why the account
  states measurements and never verdicts. Any growth idea that requires calling
  someone a scammer is out. Find the ideas that do not.

### 3. Make the site actually good

`site/` is React + Vite + Tailwind on Cloudflare Pages, ~2,500 lines, 62 tests,
pages: Home, About, History, Leaderboard, Pool, Token. It was written in
essentially one pass and it shows. It is the page every stranger loads.

It needs to be **visually excellent** — real art direction, a memorable identity,
motion where motion earns its place, information design that makes the data the
hero, and navigation somebody can use on a phone in ten seconds. It currently
reads as a competent developer's site. It should read as the front of something
people want to be seen using.

Constraints that are not negotiable: every figure on it is checked against the
file it came from by `figures.test.ts`; the hero must state a measurement, not a
verdict (it recently claimed "most launches are coordinated" while the card below
it measured the opposite); and the site must stay honest about empty states —
the leaderboard and the pool are honest empty states because the token does not
exist.

---

## Constraints you must not break

These are invariants. A change that breaks one is wrong even if it compiles and
the tests pass — in which case the tests are also wrong. `AGENTS.md` §4 is
authoritative; the short version:

1. **Model judgement never authorises capital.** A model emits inert data; only
   the deterministic risk kernel authorises; only a separate signer process
   signs, after re-decoding the transaction against the authorisation's bounds.
2. **The risk kernel is pure.** No clock, no network, no ambient state.
3. **Nothing reads past its watermark.** Replays must be reproducible.
4. **Untrusted content is never an instruction.** Token names, symbols, metadata
   and social text are data, never a system prompt, never a justification.
5. **A latch may only close, never open.**
6. **Never buy parsed transactions.** Decoding is the step Radar owns.
7. **Deny by default when config is missing.**
8. **Absent is not zero, and unknown is not safe.**

And these product-level refusals, each already argued in a document:

- **No social data, no metadata-URI fetching** (a stranger's mint must not choose
  Radar's outbound requests), **nothing that links an address to a person**, no
  "bundled %" collapsed into a single score, no verdict words.
- The strongest single predictor in the literature (arXiv 2607.02823: a Telegram
  channel at launch, 8.94× lift) is **unavailable by decision**. If you want to
  reopen that, argue it explicitly as a decision rather than smuggling it in.
- `Policy::CLOSED` stays closed.
- The model that writes replies **cannot emit a digit**. It writes `[F1]`-style
  slot tags into prose and Radar substitutes its own measured strings; a digit
  anywhere in its output discards the reply. Any voice work must live inside
  that.

---

## What has already been audited — do not repeat it

Research 0030 is a 24-finding adversarial audit completed and fully merged on
2026-09-07 (PRs 198–207). It found and fixed, among others: a signer that never
bounded the size of a swap (every buy scored zero against every ceiling); a gate
a stranger could exhaust for free; a monitor resolving its paths against the
wrong filesystem root; a daily post that would have published false statements
about named coins; a fidelity check that authorised most small integers.

**Read it, verify a sample of its claims, and then go past it.** Its own open
items are listed with reasons. Repeating that sweep is the least valuable thing
you could do. The four highest-value questions it did **not** answer are the
three objectives above plus this one: *is the intelligence actually any good, or
does it merely look rigorous?*

---

## Evidence standard

This repository's core value is that claims are backed by things that run. Match
it or you will be worse than useless here.

- Run the command, read the output, quote it. Under-claiming costs nothing;
  over-claiming costs the benefit of the doubt on everything else.
- Distinguish **verified fact** from **reasonable inference** from **assumption**,
  and say which you are offering.
- Check a number before deciding on it. Verify current Solana, pump.fun, Jito,
  X API and Token-2022 realities against primary sources rather than recalling
  them — several documented behaviours in this space changed inside the last
  year, and the repo has been burned by exactly that twice (LEARNINGS 3, 25).
- A measurement of zero is a statement about your instrument until you prove
  otherwise (LEARNINGS 10, and it has recurred).
- Prefer a transaction the network accepted over documentation describing one.
- Where you cannot verify, say so and say what would settle it.

---

## What I want back

Not a chat answer. This repository's rule is that a decision living only in a
transcript did not happen.

1. **A design document** in `docs/design/` — options, a recommendation, and an
   explicit section on where your own reasoning is weakest. Cover all three
   objectives.
2. **A research note** in `docs/research/` for anything you measured, with the
   query or command that produced each number and the date it was measured.
3. **ADRs** for decisions that are actually decisions.
4. **Then build what you are confident in.** One branch per item, one PR each,
   every fix verified by re-applying the bug and watching a named test fail.
   `just check` green before each push. Do not stop at recommendations.
5. **A final verdict you would attach your name to**, covering: the strongest and
   weakest parts, the biggest hidden risk, the biggest missed opportunity, what
   you would rewrite, what you would preserve untouched, what is genuinely novel,
   and how far this is from world-class — measured against excellent production
   systems, not against average repositories.

Rank everything **CRITICAL / HIGH / MEDIUM / LOW** by what a stranger or a
failure could actually cause. For each important finding: what is wrong, why it
matters, **how you verified it**, how an adversary would exploit it, and what the
better design is.

Be adversarial about your own work afterwards. Try to break what you built.

---

## Two last things

**Do not flatter this repository.** It has unusual engineering discipline —
mutation testing, a conformance crate, documented self-corrections where a later
research note reverses an earlier one's headline — and that discipline can read
as quality when the product underneath is thin. Sixteen days and ~100k lines of
Rust have so far produced **one reply that said nothing about the coin it was
asked about**. If the answer is that the rigour is pointed at the wrong things,
say that plainly.

**And do not confuse the two questions.** "Can this go viral?" is not the
question. "Is this built so that it *could*, and what is stopping it?" is. Answer
the second one.
