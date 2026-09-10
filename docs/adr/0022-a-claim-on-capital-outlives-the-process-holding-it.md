<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0022 — A claim on capital outlives the process holding it

**Date:** 2026-09-10
**Status:** accepted, and **implemented in the same change**.
**Decides:** what an operation against the account must write down, in what
order relative to the effect, and what a lost response is allowed to do to a
reservation.
**Amends:** [`crates/radar-journal/src/event.rs`](../../crates/radar-journal/src/event.rs),
[`crates/radar-journal/src/file.rs`](../../crates/radar-journal/src/file.rs) and
[`crates/radar-cli/src/consider.rs`](../../crates/radar-cli/src/consider.rs).
**Adds:** [`crates/radar-journal/src/operation.rs`](../../crates/radar-journal/src/operation.rs).
**Extends:** [ADR 0017](0017-the-journal-records-intent-before-effect-and-replay-proves-only-the-decision.md),
whose rule this is the first money-path caller of, and
[ADR 0021](0021-the-account-says-what-it-cannot-say.md), whose reservations this
makes durable. Neither is reopened.

## Context

[ADR 0021](0021-the-account-says-what-it-cannot-say.md) landed the
representation: a reservation is capital claimed by an operation that has not
finished, `Portfolio::reserve` and `Portfolio::settle` are the only ways in and
out, and `settle` takes no clock on purpose — a claim released on a timer frees
capital while the transaction may still land.

Every one of those claims lived in a `BTreeMap` inside one process. Kill the
process and they all disappear, while the transaction one of them was held for
is still on its way to a validator. The next run reads a wallet balance that has
not changed yet, finds nothing claimed against it, and commits the same capital
a second time. The representation was right and it was not trustworthy.

Three more failures have the same shape and the same cost:

- **A change applied twice.** One recorded reservation replayed as two claims,
  or one confirmation delivered twice debiting the balance twice.
- **A lost response read as a failure.** `radar-analyst`'s `Publisher` trait
  already says in a comment that it *"cannot express 'accepted, response
  lost'"*. `Outcome::Uncertain` was the right word in the journal and had no
  state machine behind it, so nothing stopped a caller mapping a transport error
  straight to *it did not happen* — and freeing the claim.
- **An effect released before the record of intending it.** ADR 0017 states the
  rule and `Recorded` enforces it for publication. Nothing on the money path had
  a caller at all.

## Decision

**One durable operation record, in the journal, with six states.**

`Proposed`, `Reserved`, `SubmissionUnknown`, `Confirmed(Settlement)`, `Failed`,
`Reconciled(Settlement)`. The record names what was intended (asset, amount,
slot), what it reserved, and where it got to — one line per change, each line a
complete description, which is the shape a future `audit replay` can use.

**`SubmissionUnknown` is not a failure, and the type refuses to let it become
one.** It is the state a process is in from the instant *before* it releases an
effect until an answer comes back. `OperationState::advance` returns
`OperationError::UnknownIsNotFailed` for the move to `Failed`; the only exits
are `Confirmed` and `Reconciled`, and **both carry a `Settlement` somebody had to
establish**. There is no exit that costs no evidence. AGENTS.md §5's ladder at
level 1 rather than level 4.

**Two ordering rules, because the two steps differ in kind.**

- A reservation is **taken first and recorded second**. It is a map entry in one
  process; nothing outside can observe it, and a crash between the two loses a
  claim nothing acted on — because acting requires the `SubmissionUnknown`
  record, which requires the `Reserved` one. A failed write rolls the claim back,
  so the file never says `Reserved` about capital that was refused.
- A submission is **recorded first and released second**. `OperationLog::submit`
  takes the effect as a closure and calls it only after the write returned, so a
  journal error means the effect never ran. A crash between the two leaves an
  operation in `SubmissionUnknown` whose effect never happened: the account holds
  capital it did not need to.

Both failure directions **hold capital rather than free it**, which is the same
reasoning `radar_store`'s `Writer::flush` writes its coverage claim after the
rows it claims. Lose the claim, keep the truth.

**Identity is the id of the journal event that proposed the operation.** A
digest over the intent and the whole chain before it, filled in by
`Journal::record`. A caller cannot choose it; you cannot hold one without the
proposal having reached disk; and a counter reset by a crash cannot hand a new
operation the id of one still outstanding. Replay is idempotent **by that
identity** — never by a timestamp, never by position in the file.

**Rebuilding state and re-taking claims are separate steps.** `OperationLog::open`
folds the file into operations and touches no balances. `rehold` re-takes only
what is still outstanding, through `Portfolio::reserve`, so the account checks
each claim against the balance rather than being told what to believe. Replaying
a *confirmed* fill against balances would debit units the wallet read already
lacks. A claim that cannot be re-taken is an error, never a claim quietly
dropped.

**The caller is `radar consider`**, which reopens the log and re-holds every
outstanding claim before the risk kernel sizes anything.

## Consequences

**Nothing about this creates authority.** The record says an operation was
started; the deterministic kernel is still the only thing that turns a proposal
into an authorization, and `Policy::CLOSED` is still the shipped default. Rule 1
is untouched.

**`Event` gains an optional operation entry.** It is skipped when absent and
hashed **only when present**, so every chain already on disk keeps the ids it
has and still verifies. A field that joined the digest unconditionally would
have turned every intact journal into `Verified::Broken` at sequence one.

**Partial fills are refused rather than mishandled.** A settlement that leaves
part of the claim outstanding would put the operation in a terminal state that
`rehold` does not re-take, dropping the remainder at the next restart. `close`
returns `OperationError::RemainderWouldBeLost` instead. That is a named
limitation, not an omission.

**Nothing reconciles against the chain yet.** `Reconciled` records what an
observation established; making that observation is
[`radar-onchain`](../../crates/radar-onchain)'s atomic read, which is
deliberately not wired in. Until it is, an operation that reaches
`SubmissionUnknown` and never hears back holds its claim until a person resolves
it. That is the safe direction and it is a real operational cost: an unattended
loop can starve itself of capital this way, and it cannot lose it.

**No instance has an operations journal today.** Execution is shut, nothing
writes the file, and an absent one opens as a log with nothing outstanding — a
measurement, kept apart from a file that could not be read, which is an error.
