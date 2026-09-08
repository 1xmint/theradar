<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0016 — The model selects whole clauses, never a number in its own sentence

**Date:** 2026-09-07
**Status:** accepted, and **implemented 2026-09-08** in
[`crates/radar-roast/src/clause.rs`](../../crates/radar-roast/src/clause.rs).
Commitments 1 and 2 are done in the narrow form described below; commitment 1's
*metadata* half — typed scope, measurement time, watermark, source references,
completeness, schema version — is item 5b, the receipt, and is **not** in the
type yet. Commitment 3 is unchanged and still gates any new fact family.
**Decides:** what unit of language the model is allowed to choose, now that it
has been established that a correct number can be attached to a wrong claim.
**Replaces:** the fact-slot design that lived in the `radar-roast` crate's `tags`
module, which this change **deletes** — so it is named here without a path, since
there is no longer a file for a link to reach. The first draft of this ADR was
wrong about its own consequence: once the model selects whole clauses it writes
no prose, so there is no text for a tag to sit in and no digit for the refusal to
catch. Keeping the module would have left 475 lines that nothing could reach. Its
argument is kept verbatim in `clause.rs`'s header, which is where it is now
answered rather than merely cited.

## Context

The `tags` module closed a real hole. The model writes `[F1]` and no digits; Radar
substitutes its own rendering; a digit outside a tag or an unknown tag rejects
the whole reply and ships the deterministic template instead. The number in a
published sentence is therefore always a number Radar wrote. That property is
worth keeping and this ADR does not weaken it.

**It is not the property the account's credibility rests on.** A
[`Fact`](../../crates/radar-roast/src/sheet.rs) binds a `label` and a `rendered`
string. The substitution binds the *value*. Nothing binds the **subject**, the
**window**, the **unit**, the **comparison** or the **negation** in the prose
around the tag. The model writes that prose freely, and a sentence of the form

> "only [F3] of coins like this one ever recover"

is a fabricated claim carrying an authorised figure, indistinguishable at the
substitution boundary from an honest one. [Research
0031](../research/0031-radar-handoff-inspection.md) records this as inferred
from source, with no adversarial model run behind it — which is the right
strength of claim, and it is also the reason not to wait for one. The defence
that exists proves "every published digit was measured". The sentence the
account will be judged on is "every published *claim* was measured", and those
are not the same sentence.

## Decision

**The model's output grammar becomes a bounded list of clause selections, not
prose with holes in it.**

A `Fact` gains a stable `kind`, a typed `scope` (which mint, which population,
which window), a measurement time and watermark, source references, a
completeness marker, and — the part that matters here — its **complete
deterministic clause renderings**. Subject, verb, number, unit, window and
limitation are all written in code, in one string, by the same crate that read
the measurement.

The model chooses:

- **which** facts to publish, from the sheet it is shown,
- **in what order**,
- and **which vetted voice variant** of each clause to use.

The model does not write a subject, a number, a negation, a comparison or a
verdict. The connective tissue between clauses is a short dry connector, also
written in code. The existing digit refusal and the existing tag exception are
unchanged and now apply to a smaller surface.

**Two sentences of that paragraph did not survive implementation, and they are
corrected here rather than quietly.**

- **The connector is a space, and there is no table of dry connectors.** "But",
  "and yet" and "despite that" each assert a *relationship* between the two
  clauses they join, and a relationship between two measurements is a
  comparison — which the paragraph above forbids the model from writing. A
  connector table sitting between two facts it does not know cannot write one
  honestly either. So the clauses are complete sentences and nothing joins them.
- **The digit refusal and the tag exception are not "unchanged on a smaller
  surface"; they are gone, with the module that held them.** A model that emits
  only `F1.plain` has no prose position, so there is no text for a digit to
  appear in. `forbidden::check` and `fidelity::check` both survive and now read
  *Radar's own clauses* — which is a real job, and a different one: the author
  they catch is a person adding a badly worded variant.

**Why this and not a stronger checker.** AGENTS.md §5's ladder: make it
impossible, then one mechanical check, then a test, then prose. A checker that
reads generated prose and decides whether the subject matches the measurement is
a natural-language judgement — it fails in both directions, and its false
positives spend the credibility of every check beside it. Removing the model's
ability to write the claim is level 1. It is also the cheaper implementation.

**What is given up, stated plainly.** Expressive range. A roast assembled from
vetted clauses is drier than one written freely, and the joke is part of the
product. Two things bound the loss: the model still chooses *which* facts land
and in what order, which is most of what makes a reply feel pointed; and the
variants are authored per clause, so voice is a thing that gets written and
reviewed rather than a thing that gets hoped for. Whether that is enough is a
question to answer by reading the output, not here. **It is not a reason to
reopen unconstrained prose** — the failure mode being closed is a screenshotted
sentence that was never measured, and that is not recoverable by apologising.

## What this commits to

1. `Fact` carries typed scope, kind, measurement time and watermark, source
   references, completeness, and complete clause renderings, with an explicit
   schema version.
2. The accepted model output is a selection over those clauses. No free text
   outside that grammar reaches publication.
3. Every published fact family has, before it may be selected: a
   capture-backed positive fixture, an ordinary counterexample, an evasive
   variant, its exact observation window, its source coverage, a dated
   population comparison, and a caller in the sheet path.
4. Mismatched-subject and negation cases are named regressions, applied by hand
   before they are claimed fixed.

## What this does not decide

- **Which clause variants are good enough to publish.** That is editorial and it
  is item 5's work, judged by reading rendered output.
- **Whether the model is still worth calling at all.** If selection turns out to
  be as good deterministically, that is a later and welcome simplification, and
  it needs a measurement rather than an opinion.
