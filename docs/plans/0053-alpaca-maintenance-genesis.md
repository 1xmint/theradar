<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0053 — Maintenance requires history before refreshing it

**Status:** verified at 37b56014; release hashed; not deployed. Provisioning is plan 0054.

Branch: fix/alpaca-maintenance-genesis, base b5b7b0d. The previous goal turn
verified maintenance and prepared the isolated worker. Worker source b5b7b0d
then passed all fourteen GitHub jobs at 38316159013, 2841 Rust/66 Python tests,
eleven worker mutations, four exact maintenance regressions, prior regressions
and four systemd pairs. All four shard logs report no changed Rust source files.

## Why this changes the next action

The existing maintenance collector called alpaca_cash::checkpoint before
checking whether a retained cash checkpoint existed. That collector deliberately
supports first initialization. The old actual-collector test started from an
empty BrokerLog and accepted this behavior, as verified by the parent tests.
An existing-but-empty journal could therefore be treated as new history.
This is unacceptable for a repeated recovery tick, even though a separate
operator initialization command legitimately supports it. No production
maintenance service or orders have used this path.

## Work

- [x] Verify missing checkpoint refuses before the refresh callback.
- [x] Verify actual CLI rejects empty and reservation-only existing journals
  in both modes, before credential loading and without changing journal bytes.
- [x] Verify deliberately initialized history still refreshes through the
  actual provider collector and retains all prior recovery behavior.
- [x] Reapply both new guard omissions, the inverted history predicate and
  the four earlier exact mistakes
  on GitHub; inspect all tests/build/lint/mutation results.

One existing_checkpoint helper holds the predicate. The CLI calls it before
credential loading; the coordinator calls it before its protected refresh
callback. The initialization-supporting cash checkpoint command remains
separate. Four coordinator tests still execute, and the actual-process test
suite now executes two tests. Targeted reapplications require those exact
executed counts rather than accepting compiler failures as caught mistakes.

## Handback

Prepared the guard, adjusted the actual collector fixture to initialize
explicitly first, and added an actual-process test for empty/reservation-only
history in observation and stop modes. Source and diff checks only locally;
tests/build/lint/mutation jobs belong on GitHub. The source correction must be
verified before producing any maintenance installation artifact.

Tailscale SSH required a fresh browser check. The owner completed it, the
original pending SSH handle resumed and produced the VPS time, and a new
read-only command succeeded. systemctl show reports the contextual research
timer LoadState=not-found, ActiveState=inactive. The earlier staged research
setup is still pending; do not infer installation from reauthentication.

Next inspect CI, then implement deliberate single-use journal initialization
and pinned provisioning. Never initialize missing/empty history as part of a
maintenance timer or infer that an empty journal proves no earlier effects.
The broader autonomous execution, effective website handover, execution-cost
semantics and measured learning outcomes remain incomplete.

Initial source f7eb295, GitHub 38346225428 completed. Formatting required two
assertion hunks and repository conformance caught the missing LEARNINGS 59
index row. Prepared those repairs and updated the index count from 44/58 to
45/59 mechanical entries. All other ordinary jobs passed. Three downloaded
mutation artifacts showed compiler-unviable function-body removals only (unused
arguments/imports/dead code), not test kills; shard 3 selected zero mutants.
The tests job stopped at repository conformance before exact reapplications,
so those seven checks remain unverified until the repair run completes.

Repair source 37b56014b0a8430918c5c03a87641cece9ef511f passed all fourteen
GitHub jobs at 38346644746: 2842 Rust/66 Python tests, seven exact maintenance
regressions, eleven worker mutations, all prior checks and four systemd pairs.
All three downloaded mutation artifacts were inspected: three compiler-unviable
function-body removals, zero test kills/survivors/timeouts. Shard 3 selected zero
mutants. Critical guard coverage comes from the seven executed exact checks,
not from compiler failures. Release 38346705855 succeeded at this same source;
BUILD-INFO and independent downloaded reader hashing both give
3f21fc1ae3b54b2333a5b809eda916c296be0948d69dba1f9da618adec9733d2.
No server artifact was deployed. This verified reader is pinned by plan 0054's
separate deliberate initialization/provisioning path. Context research has now
been installed and independently inspected; the earlier pending status above
describes the prior inspection, superseded by plan 0050's runtime handback.
