<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0020 — The model may ask, and the answer it writes is a recommendation nobody obeys

**Date:** 2026-09-10
**Status:** accepted, and **implemented in the same change**.
**Decides:** whether a model may request evidence, what its output is allowed to
be, and where the loop that drives it lives.
**Amends:** [`crates/radar-agent/src/investigate.rs`](../../crates/radar-agent/src/investigate.rs)
(new), [`crates/radar-serve/src/evidence.rs`](../../crates/radar-serve/src/evidence.rs)
and [`crates/radar-serve/src/chat.rs`](../../crates/radar-serve/src/chat.rs).
**Does not amend** AGENTS.md rule 1, rule 4 or rule 8. It rebuilds the argument
for rule 1 in a different place and reaches the same conclusion.

## Context

The chat route gathered evidence deterministically before the model saw
anything, and the system prompt said so, verbatim:

> You cannot request more evidence: what you have been given is what Radar
> looked up, so say what is missing rather than asking for it.

That is safe and it is why the model could not investigate. Radar chose the
instruments by pulling base58 addresses out of the question and calling the two
creator instruments. A thread Radar did not anticipate could not be followed.

The reason the direction was one-way is written in the same file: letting a
model ask means **parsing its output**, and parsing model output is the path an
injected instruction travels. The safety argument rested on there being nothing
to parse.

Plan 0011's P3 and
[design 0017](../design/0017-a-private-autonomous-trader.md) both call for the
opposite, and 0017 is specific about what the output must be: *specialists
return evidence IDs, facts versus inferences, coverage gaps, counterarguments
and expiry. Untrusted metadata and social text remain data, including when
quoted by another agent.*

## Decision

**A model may ask for a named piece of evidence, and what it writes is a typed
recommendation a deterministic adapter validates.** Five things make that safe,
and each is a mechanism rather than a promise.

### 1. A request is a name and one string

`Wanted { tool, argument }`. There is no field for a URL, a path, a query or a
body, so a request cannot describe an action even when the model has been
persuaded to write one. Extra fields in the JSON are discarded by the parser
rather than carried.

The name is checked against the existing read-only `Allowlist`, which admits a
name only if it is registered *and* survives `is_read_only`. A name outside it
is **refused by name** — the model is told which tool and why — because ignoring
it leaves an operator unable to tell an invented capability from a broken one.

### 2. Every fact resolves to its source, its availability and its unknowns

`Fact` replaces `radar_serve::evidence::Block`. `Block` carried a source and a
body, which is enough for a citation and not enough for an investigation: an
instrument that found nothing and an instrument that broke both arrived as no
block at all. That is AGENTS.md rule 9's failure in the one place a model
reasons over the gap.

`Availability` is `Recorded { as_of_slot }`, `Absent { why }` or
`Unavailable { why }`, and the three are treated differently: the first two are
evidence, and the third **stops the investigation**. `Fact::found` returns
`None` for a blank source, so a fact with no source is not returned.

### 3. The recommendation is typed, and abstain is first class

`Action` is `Abstain`, `Investigate`, `Enter`, `Hold` or `Reduce`. Each carries
an expiry, the strategy version it reasoned about, its evidence references, and
what would invalidate it. `Adapter::adopt` checks every one of those against
something the caller knows without asking the model: the watermark, the running
version, and the set of sources Radar actually returned.

A vocabulary in which the only way to say "I do not know" is to say nothing
produces confident nonsense, because saying something is always the shorter
path. Abstain is a complete answer here, and every failure below becomes one.

### 4. An amount is a requested bound, never permission

`Adapter::closed` ships with a ceiling of `MicroUsd::ZERO`. `Enter` and `Reduce`
must carry a positive bound, and every positive bound is over zero, so **the
shipped adapter cannot adopt an entry at any size**. That is the same shape as
`Policy::CLOSED` one layer up, arrived at independently: two closed doors are
cheaper than one door and a guard.

Nothing downstream reads the amount as a size to use. There is no path from a
`Recommendation` to a `Proposal` in this change, and building one is the risk
kernel's caller's job, not the chat route's.

### 5. The loop lives at an outer caller

`repo-conformance` forbids `radar-agent` and `radar-model` depending on
`radar-risk`, `radar-exec`, `radar-strategy` or `radar-store`. Driving turns
needs a store and an instrument registry, so **the orchestration is in
`radar-serve`** and `radar-agent` keeps the types, the parser and the validator
— none of which can reach anything. The check was not weakened; it is the shape
of rule 1 and it decided where the code goes.

### Bounds, and why the budget is not enough on its own

`Bounds` carries `max_turns`, `max_wanted_per_turn`, `max_retries` and
`deadline_micros`, all finite, and `Bounds::default()` is `Bounds::CLOSED` —
zero turns. The budget bounds the *money*; these bound the *work*, and a model
asking for one cheap tool forever stays inside a daily budget for a long time
while a request handler never returns.

There is **one turn counter for the whole investigation and no recursive entry
point**, which is how "a child cannot create an unmetered grandchild" is
expressed: there is no level. A turn that wanted turns of its own would have to
spend the same counter and reserve against the same meter.

**Spend is reserved before the call and settled after**, and the ledger is
written at both points. An exhausted budget, a tool outage, a passed deadline
and a rejected recommendation each abstain **carrying what the investigation
already spent**. Losing the cost of failed work is how a fleet looks cheaper
than it is.

## What this costs

**The chat answer is now a step, not prose.** A provider that replies in prose
produces `Rejected::NotAStep`, and the investigation abstains carrying the
model's words for a reader. That is honest — a model that did not write a
recommendation has not made one — but it means an unprompted model gets one
retry and then an abstention, and the reply body has grown a `recommendation`,
`refusals`, `turns` and `abstained`.

**A broken instrument now stops the investigation.** It used to be skipped. The
conservative direction, and it will produce abstentions that the previous
behaviour would have answered.

**The model may ask for `simulate_exit`.** It is on the shipped allowlist
because the whole registry is, and it is a `Cold` instrument that makes a
network call. Bounded at `max_turns × max_wanted_per_turn` calls per question,
and worth knowing before somebody adds a paid instrument to the registry.

## What was considered and rejected

**Giving the model action tools.** The vendor considerations document proposes
`prepare_trade`, `request_approval` and `execute_trade`. `tools.rs` already
refuses those by name and the tests name them; nothing here changes that.

**Letting the adapter clamp an over-large amount instead of refusing it.**
Clamping turns a model asking for a hundred times too much into a model that got
a plausible number, and the mistake becomes invisible. It refuses.

**Building a `Proposal` from an adopted recommendation.** That is a path from a
reasoning layer toward the decision lane, and rule 1 says stop. The
recommendation is where this change ends.

**Specialists.** Design 0017 says a fleet must earn its cost against a strong
single agent, and that comparison needs the single agent first. One supervisor.

## How it is verified

[`the_loop_refuses_what_it_reads.rs`](../../crates/radar-serve/tests/the_loop_refuses_what_it_reads.rs)
drives the loop with a fake model that is **credulous on purpose**: it scans
what it was shown for an instruction and obeys it. A model that follows a script
proves the script.

Each of the four named properties was verified by re-applying the bug:

| Property | Bug re-applied | What failed |
|---|---|---|
| injection does not change what the loop does | the allowlist check removed | the persuaded model reached an instrument, 2 turns not 4 |
| a fabricated tool instruction is refused | refused silently instead of by name | `the request was ignored, not refused` |
| an expired recommendation is rejected | the adapter's expiry check dropped | the stale recommendation was adopted |
| an exhausted budget abstains | reserving zero instead of the estimate | 3 calls against a 2-call allowance |

No live model call is involved, and none is needed: the properties are about the
loop, the allowlist, the adapter and the meter, all of which are deterministic.
