<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0056 — Acknowledge late paper opening cash without double counting

**Status:** first GitHub run failed; fixture/lint/format repairs and application checks prepared; not deployed or applied.

Branch fix/alpaca-opening-cash-reconciliation, base bb118e7. Parent documentation
CI 38432857228 passed. The preceding goal turn installed and exercised fixed
passwordless paper administration; it made progress, not a wait or no-progress
turn. Fresh SSH inspection still shows the observation timer stopped and the
162228-byte retained journal intact in metadata. No order authority exists.

## Demonstrated problem

Alpaca now reports one executed JNLC cash journal for 100000 USD dated October 12,
ID 20261012000000000::dde9ccf3-cf7e-4926-aa97-6f522edcc4ff. Current cash remains
100000. The initial activity history was empty and ordinary maintenance refuses
the newly visible unsupported event. This is consistent with late opening funding,
but date/amount alone do not prove broker posting time. Neither adding a fresh
100000 deposit nor resetting retained history is an acceptable repair.

## Implementation and authority

Caller: the private operator command alpaca-opening-cash-acknowledge. It requires
an existing journal, selected activity ID and the explicit already-in-opening-balance
flag. It is absent from the model, website and five-command admin grant. The
operator acknowledges an accounting interpretation, not new capital or trades.
No automatic adoption is added to the ordinary checkpoint/maintenance path.

BrokerLog exposes its first retained cash observation under existing ownership.
The command verifies original/current retained cash agree, both have empty
activity/position history, and no retained order exists. Missing history, legacy
position data and changed opening cash refuse before credential loading.
Fresh bracketed provider reads must show exactly the selected executed JNLC,
positive amount equal to original and current cash, no trade/share fields and a
canonical date before the opening UTC calendar day. This date check is coarse
eligibility, not a claim of causal timing; the explicit operator selection is
recorded as such with broker_posting_time_verified=false.

The existing account/position brackets, pagination, snapshot/time checks, journal
owner and durable cash append hold. Acknowledgement is appended in the new
observation with current availability; the old prefix/genesis is never rewritten
or backdated. Future ordinary ticks skip the same unchanged historical event;
changed/removed activities and any newly unsupported event still refuse.
Repeated acknowledgement refuses because retained activities are then nonempty.
Cash remains the same observed amount, not doubled. Profit, economic completeness,
claim release, retry and execution authority stay false.

## Work

- [x] Prepare explicit selected-event reconciliation and first-checkpoint access.
- [x] Add four unit tests for successful append/reopen/follow-up, strict selected
  event/amount/date/status/time checks, history gates and actual provider races.
- [x] Add actual CLI dispatch test for missing/empty/reserved history and an absent
  or substituted acknowledgement flag before key loading.
- [ ] Inspect GitHub tests/build/lint/mutation results; repair demonstrated issues.
- [ ] Build/hash a verified Linux release and prepare its controlled VPS application.
- [ ] Verify actual retained opening cash and apply one acknowledged correction;
  prove successful resumed observation with unchanged capital and no orders.

Six exact regression reapplications require four executed unit tests or the one
actual-process test, never compiler failures as kills. The generic journal tests
also check first cash survives later appends/reopen and poisoned ownership refuses
opening reads. No new Rust floor is predicted; parent measured 2842 remains.
All tests/build/lint/mutation jobs run on GitHub. No local jobs.

First run 38435943148 at f38c7cd completed with failures. The successful append
fixture compared sparse input JSON with normalized Activity JSON containing null
fields; the assertion now compares normalized typed activities. Lint required an
explicit empty collection assertion. All four mutation shards failed that same
unmutated fixture; no mutation coverage was obtained. GitHub's 27 formatter
diagnostic hunks were applied as source edits. A new full GitHub run is required.

The one-time apply-alpaca-opening.py application pins the installed admin source
and existing reader hashes, checks a supplied verified new reader hash and
protected observation configuration/units, stops scheduling, and acquires the
existing observation lock without creating missing state. It installs the new
reader alongside the unchanged recurring reader, then runs only the selected
acknowledgement in the existing isolated service policy. It requires an appended
checkpoint, preserved original journal prefix, empty claims and no authority
before releasing the lock, probing old maintenance and enabling its timer.
Any partial failure retains the staged reader and history, with no automatic
retry; an existing target refuses. Invoke only through isolated Python (-I)
after GitHub verification. Six application tests and fourteen executed exact
regressions are prepared; no results claimed yet.

## Handback

At 12e03ba, CI 38438493362 is still running. Formatting passed; lint found one
107-line test after formatting. A local source correction extracts repeated
snapshot construction without removing assertions or changing test count. Do not
push until the current full run and release-linux 38438649651 finish; inspect all
results and mutation artifacts first. That release belongs to the earlier head
and cannot verify the pending correction. No artifact has been staged or applied.
Fresh direct admin inspect and activities calls both succeeded: original journal
metadata remains 162228 bytes, timer inactive, and the broker still reports the
same single selected JNLC with unchanged 100000 USD simulated cash. This turn made
progress through CI corrections and controlled application source/test coverage;
it is not a repeated access-blocked turn.

Source implementation needs complete GitHub verification before any
operator acknowledgement is requested. The installed admin helper
continues to provide direct fixed diagnostics; its grant does not include arbitrary
root updates or this accounting acknowledgement. A concrete verified deployment
step may still need the owner's private sudo entry. No history, capital policy,
credentials, broker orders or website deployment changed in this implementation.

Full autonomy still requires a defined paper execution model for currently unknown
capacity/cost inputs, the proposal/risk/entry/exit/revocation supervisor, effective
site controls with manual/Agent decides options and research chat, and measured
held-out/forward outcomes after costs. Preserve this full objective; repairing
opening history is one prerequisite, not completion of the trading goal.
