<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0056 — Acknowledge late paper opening cash without double counting

**Status:** source and release verified; exact repair staged on VPS; private sudo application pending; no order authority.

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
- [x] CI 38439705359 at 1819896 passed all fourteen jobs after demonstrated repairs.
- [x] Release 38439694798 at 1819896 passed; downloaded and remote hashes agree.
- [ ] Verify actual retained opening cash and apply one acknowledged correction;
  prove successful resumed observation with unchanged capital and no orders.

Six exact regression reapplications require four executed unit tests or the one
actual-process test, never compiler failures as kills. The generic journal tests
also check first cash survives later appends/reopen and poisoned ownership refuses
opening reads. Run 38438493362 measured 2847 Rust tests; the floor now matches.
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
regressions passed in the verified source run below.

## Handback

At 12e03ba, CI 38438493362 completed with two failures: the 107-line fixture
triggered lint, and the second exact Rust regression compiled with an unused
activity_id field, so the harness correctly refused to count it as caught.
888d85f extracts repeated snapshot construction without removing assertions or
changing test count. The pending regression replacement keeps the selected ID
read while bypassing its check for every valid nonempty ID. No warning is disabled.

The run measured 2847 passed Rust tests and 98 Python tests, including all six
application tests. All fourteen exact application regressions were caught.
All four generic mutation shards passed: 52 caught, 9 unviable, no survivors or
timeouts. All retained unviable diagnostics were inspected: whole-function
replacements trigger unused/dead-code warnings, except default Opening and
BrokerCash substitutions which lack Default. These compiler refusals are not
executed regression coverage. Seven earlier exact maintenance checks passed;
the new opening script stopped after its second case and subsequent checks did
not run. The corrected complete run passed as recorded below.

Release-linux 38438649651 finished successfully at the earlier 12e03ba head.
Downloaded BUILD-INFO and independently computed reader SHA256 agree at
2b950ba2f32aafe6416ab5d150da0b00264109e3a91bee3d4a64fe89cac11394.
This earlier artifact remains unused. The corrected source head 1819896 was
pushed only after that run finished. Full CI 38439705359 passed all fourteen jobs,
including 2847 Rust tests, 98 Python tests, all six exact opening regressions and
all fourteen exact application regressions. The four generic shard artifacts
again contain 52 caught and nine compiler-unviable changes, with no survivors or
timeouts; all nine compiler diagnostics were inspected independently. Matching
release 38439694798 passed at 18198969782678a13625600b1aee98d5173b3cb5.
A temporary unavailable live test log was not treated as a terminal failure.

Downloaded BUILD-INFO and independently computed reader hash agree at
d8642180158cd4e77142f84d26d57ed861f295ed546d441d6a6fe936f8d4713c.
The apply wrapper was packaged from that exact Git object, with SHA256
8418fc794d50f44405d0fca14d9c1979e592b11fe63b26be3ecaf92139e7feed.
Staging first refused existing targets, then copied the binary to
/tmp/radar-opening-cash.new and wrapper to /tmp/radar-alpaca-opening-apply.py;
independent remote SHA256 values match both. Installed helper, old reader and
worker hashes were rechecked unchanged. No website artifact was deployed.

The owner was given the concrete isolated-Python sudo command with the verified
reader hash. Application is pending private sudo entry, not a permission request
for a new trading mandate. The five-command admin grant does not allow root
installation or this acknowledgement. After successful application, run direct
admin inspect and maintain; verify resumed scheduling, ordinary reconciliation,
unchanged simulated cash and no orders/claims before moving on. If application
fails, retain staged/installed files and history and inspect before any retry.
Fresh direct admin inspect and activities calls both succeeded: original journal
metadata remains 162228 bytes, timer inactive, and the broker still reports the
same single selected JNLC with unchanged 100000 USD simulated cash. This turn made
progress through CI corrections and controlled application source/test coverage;
it is not a repeated access-blocked turn.

Independent research inspection while CI runs: guardian can read the retained
context campaign files. All twelve slot decision records, not only first/last,
contain action=wait and strategy_id=snapshot-context-v1. Their reasons cite
incomplete context, no verified learned edge and explicit costs. The service
reports success; this proves retained abstention, not model provenance or
cost-adjusted outcome scores. Source alpaca_context.rs supplies one completed
minute bar and one previous daily bar, with learned_edge_verified=false. Repeating
this campaign is not evidence of learning. The next research work must add richer
historical features and forward baselines rather than relax the edge label.

This turn made progress through complete verification and exact VPS staging. The
installed admin helper continues to provide direct fixed diagnostics. No history,
capital policy, credentials, broker orders or website deployment changed yet.

Full autonomy still requires a defined paper execution model for currently unknown
capacity/cost inputs, the proposal/risk/entry/exit/revocation supervisor, effective
site controls with manual/Agent decides options and research chat, and measured
held-out/forward outcomes after costs. Preserve this full objective; repairing
opening history is one prerequisite, not completion of the trading goal.
