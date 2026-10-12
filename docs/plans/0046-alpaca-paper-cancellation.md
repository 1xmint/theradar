<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0046 — Cancel retained paper orders without inferring settlement

**Status:** Implemented and GitHub verified; not deployed. Activation remains closed.

Branch feat/alpaca-paper-cancel from 6028241, stacked on PR 406. Previous goal
turn made concrete progress: installed research worker verified, actual model
wait decision retained, and real cohort inventory owner-verified. Parent code
CI 38303144460 and documentation CI 38303848069 both passed all 14 jobs,
inspected. The fully autonomous trading goal remains active.

## Checklist

- [x] Actual private cancellation CLI with fixed paper host and existing journal.
- [x] Bracket broker lookup with pinned account reads and bind full retained intent.
- [x] Limit targets to supported working simple orders and UUID endpoint IDs.
- [x] Persist cancellation uncertainty before DELETE; retain claims and no retry.
- [x] Replay cancellation transitions without changing submission state or old hashes.
- [x] Bind subsequent checkpoints to the cancellation's broker order identity.
- [x] Inspect GitHub build, tests, lint, formatting and mutation diagnostics.
- [ ] Complete isolated supervisor, verified execution inputs and website handover.

## Boundary

Caller is radar alpaca-paper-cancel with a protected existing journal, pinned
paper account and retained client order ID. It cannot cancel arbitrary account
orders, bulk cancel, override the host, replace an order or open a position.
Same existing redirect-disabled credential agent. The selected broker payload
must match the immutable simple day limit intent under journal ownership and
unchanged selected account observations. Only new, accepted and partially_filled
are supported first-cancel states; missing, terminal or pending_cancel require
reconciliation instead. UUID validation prevents path/query/host injection.

The journal appends cancel_unknown_order_id before releasing the effect. Failed
or unknown writes poison ownership; duplicate cancellation refuses across
restart. BrokerState stays SubmissionUnknown, so existing claims/economic
checkpoints retain their meaning. Optional absent field is omitted from old
event serialization. Cash checkpoints must use the same broker ID; only later
verified terminal/fill accounting can reduce residual liabilities. Fee reserves
remain held. Cancellation does not require enabling new orders, so disabling
capital authority need not disable the eventual supervisor's cancellation path.
That authenticated supervisor transport and owner permission handling remain
unfinished; this private CLI is not a model-accessible tool or web route.

[Alpaca's cancellation endpoint](https://docs.alpaca.markets/us/reference/deleteorderbyorderid-1)
documents DELETE /v2/orders/{order_id}, 204 acceptance and 422 refusal when the
order is no longer cancelable. A response never proves final cancellation or
the absence of racing fills. All non-204/transport errors retain uncertainty;
response bodies are not printed. No actual broker cancellation is authorized or
claimed by these tests; no deployed order was observed to exist.

## Handback

Source prepared. Eight new Rust tests cover actual CLI dispatch, supported and
rejected broker observations, durable-before-effect ordering, response loss,
restart/refusal, unchanged claims, checkpoint identity and poisoned persistence.
Tests and mutation reapplications run on GitHub only. The test
floor rises to the first run's observed 2824, not an inferred new count.
Real research outcome report requested in the prior turn remains pending;
independent replay of slot one's retained capture returned quote exceeds
configured age. Failed slot retained, no retry or relaxed quote-age bound.
No broker effect, deployment or activation performed. Goal active.

First verification: code 1213e52, CI 38305306698. Tests passed 2824 Rust tests;
build, MSRV and ordinary website checks passed. Lint found an unnecessarily
wrapped fixture result and an empty-output assertion; formatting changes were
applied from GitHub's exact formatter output, with no local formatting job.
All four mutation artifacts and compiler-unviable diagnostics inspected:
54 total, 37 caught, 16 unviable, one missed, no timeouts. Unviable reasons were
unused/dead code, unreachable statements or absent Default implementations,
never counted as test catches.

Exact survivor broker.rs:251:88 replaced && with || between broker identity
equality and nonterminal checkpoint state. Existing tests added a later
checkpoint, whose own identity validation masked that weakened final-record
replay check. New focused test appends an intact-hash final cancellation against
either a different broker ID or an already-terminal checkpoint; both must
refuse. Added GitHub-only exact predicate reapplication requiring this executed
test to fail, with source bytes restored in finally. No exclusion added.
Repair verification is recorded below; no cancellation deployment or broker effect.

Repair source 0ef633afbb02f3069c63897ef4f9c4fdd4a6655b, GitHub run
38306187381 passed all 14 jobs, inspected. Tests job 115000696684 passed
2825 Rust tests and 47 Python tests. Exact cancellation replay reapplication
broker.rs:251:88 caught by the executed focused test. Both existing exact
subscription regressions caught; 9 cohort, 12 campaign, 8 installer and 32 prior
Python reapplications caught. Both systemd unit pairs verified.
All four mutation shards inspected, 54 total, 38 caught,
16 compiler-unviable, no missed or timed-out mutants. Compiler refusals were
unused/dead code or arguments, unreachable statements, and missing Default
implementations for BrokerLog, Applied or generic T. They are not test kills.
Build, MSRV, lint, format, web, site, license and dependency checks passed.
No workstation jobs, broker effects, deployment or activation.

Independent VPS read at 20:44 UTC found slot zero decision and slots one through
three failed. Replaying each failed slot's retained capture with the installed
alpaca-quote command at its own availability watermark, max age 120000 ms and
max spread 20 bps returned quote exceeds configured age. Both recorder and
research timers remained active. These are retained failures, not skipped
observations or retries. Complete cohort outcome report remains pending.

Draft PR 407 attached, stacked on PR 406. Documentation handback commits are
separate from the code proof above. Next: close the actual execution-input gaps
and isolated supervisor/site handover; obtain complete fixed-horizon cohort
results and improve the quote-only research baseline with usable historical
signals and held-out comparisons. No measured profitable edge is claimed.
The fully autonomous trading goal remains active.
