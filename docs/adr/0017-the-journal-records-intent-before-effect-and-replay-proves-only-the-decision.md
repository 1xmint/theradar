<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0017 — The journal records intent before effect, and replay proves only the decision

**Date:** 2026-09-07
**Status:** accepted, and **not yet implemented**. The reply log it extends
ships today in
[`crates/radar-analyst/src/log.rs`](../../crates/radar-analyst/src/log.rs); the
journal, outbox and `radar audit` commands are item 4 of
[plan 0010](../plans/0010-radar-actualization.md).
**Decides:** what the autonomous surface must write down, in what order relative
to the effect it is writing about, and what a replay is entitled to claim.
**Extends:** the analyst's append-only JSONL log, which keeps its shape and its
reason.

## Context

Radar is about to run a money-bearing loop with no human in it: mentions in,
receipts published, a weekly winner selected, a payout signed. `log.rs` already
records what was asked, what was measured and what was said, and its rationale
is exactly right — the log is what turns a public mistake into a correction
rather than an argument.

It records **the reply**. It does not record the winner selection, the scoring
mode, the spend reservation, the payout signature and its validity bounds, or
the ordering of an external effect against the durable record of intending it.
Those are the places where an unattended process loses money or publishes twice.

**The specific failure this is written against** is not a missing metric. It is
a restart in the middle of an effect: a post accepted by X whose response was
lost, a transaction broadcast whose confirmation never arrived, a week whose
pagination was half-done. In each of those the honest state is `uncertain`, and
the tempting state is "retry" — which is how one payout becomes two.

## Decision

### One append-only journal, hash-chained, written before the effect

Schema version, monotonically increasing sequence, stable event id, correlation
id, UTC time, duration, build SHA, rule/decoder/model versions, redacted
configuration hash, and the previous event's hash. Mention, receipt, nomination,
week, claim and payout ids correlate across components.

**The durable intent and its evidence exist before the effect happens.** A
journal write that fails **blocks** the effect and reports through stderr,
systemd and the existing operator alert path. Checkpoints are replaced
atomically and validated on restart: a torn final write is distinguishable from
an empty history, and a gap, a malformed record or a bad hash is a **visible
fault**, not a skipped line.

Never written: credentials, signing keys, authorization headers, full
credential-bearing URLs, chain-of-thought. Untrusted content is escaped and
stays data (rule 4). Developer diagnostics are separate from public reason text.

### Effects are keyed by operation, not by attempt

A persistent outbox state machine, keyed by the operation. A restart resumes the
same work: it does not recharge a settled reservation, skip a mention, reopen a
week or award twice.

**Solana payouts.** The expected signature and its validity bounds are persisted
**before** broadcast, then reconciled through configured RPC. No replacement is
signed until expiry or non-confirmation is established under the payout policy.
An ambiguous response is `uncertain` — it is not failure, and it is not
permission to send a different payment.

**X publication.** Confirmed returned post ids, and bounded reconciliation
against the account's own permitted records. Where the API offers neither an
idempotency guarantee nor enough evidence to separate acceptance from loss,
`uncertain` is preserved and blind duplicate posting is blocked.

### `radar audit`, four subcommands

`explain --id`, `replay --id`, `verify --from --to`, `export --week`. Replay is
**offline and deterministic**: facts, admission, scoring and payout-policy
decisions, compared by canonical output hash, with differences explained. It
reuses the recorded model response — a fresh model call is not deterministic and
a replay that made one would be proving something else.

### Retention has two tiers and expiry is loud

Decision manifests, rendered public facts, contest rules and payout receipts are
retained **indefinitely** unless a legal deletion obligation applies. Bulky and
private captures default to **30 days** with explicit expiry markers: after
expiry a replay says *evidence expired* and never passes silently. Retention
that would remove an artifact required by an unresolved payout is prohibited.
Storage quotas fail closed for new effectful work rather than pruning active
audit evidence.

## What replay proves, and what it does not

This is the half that has to be stated, because "fully auditable" is the kind of
phrase that gets read as more than it is.

**Replay proves** that a recorded decision follows deterministically from the
recorded inputs under the recorded rules — the same property that makes
`radar-risk`'s refusals reproducible, applied to selection, admission and
payout policy.

**Replay does not prove** that the recorded inputs are what the world contained.
A provider that omitted a page, a decoder that misread a layout, a corrupted
historical capture and a legally required deletion all limit it identically, and
none of them is visible from inside a consistent chain. This is the distinction
AGENTS.md §2 exists for: consistency checking is not instrument accuracy, and
[LEARNINGS 10](../../LEARNINGS.md)'s zero-is-a-measurement-about-your-instrument
is the same sentence from the other end.

**Hash chaining detects alteration relative to a trusted checkpoint.** It does
not resist a host attacker who rewrites the whole chain. Off-host checkpointing
to the existing operator backup destination is therefore part of this decision;
where none is configured the state is `backup_unconfigured`, surfaced rather
than papered over, and **no destination is invented**. Restoration is tested
from a real backup artifact before launch, because an untested backup is a claim
and not a capability.

## What this commits to

1. The journal, the outbox, and the four `radar audit` subcommands.
2. Alerts on: journal failure, prolonged publish uncertainty, a winner not
   selected by the deadline after a healthy scheduler tick, payout uncertainty,
   ingestion lag, repeated provider refusal, backup failure. Deduplicated, with
   one recovery event.
3. Fallback scoring recorded as an **operating metric**. It never blocks
   selection — [ADR 0015](0015-the-prize-is-an-evidence-relay-and-a-winner-is-always-selected.md).
4. Fault tests, each leaving an explainable state and never a duplicate payout
   or a falsely confirmed publication: crash either side of every durable
   boundary, accepted effect with lost response, duplicate input, replay, disk
   full, torn checkpoint, corrupted evidence, provider 429/403/timeout, restart
   mid-pagination, transaction expiry, repeat claim, changed destination,
   restored backup.
5. Public endpoints expose concise service states — never raw provider messages
   and never spend detail.

## What this does not decide

- **Where the off-host checkpoints go.** That is an operator configuration with
  a real cost, and it belongs to whoever owns the backup destination.
- **The legal retention rules for X content.** Counsel settles that before the
  collection it governs is enabled; until then the 30-day default stands as a
  default, not as a cleared position.
