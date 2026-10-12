<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0051 — Recovery tick before unattended paper entries

**Status:** GitHub verified at a92807a; not installed.

Branch: feat/alpaca-paper-maintenance, base 9487fae. Parent documentation run
38312767425 completed successfully. The previous goal turn established that
the contextual research installer still awaits owner sudo and investigated the
execution evidence gaps; it did not enable trading. That installation is
independent of this recovery work.

## Work

- [x] Verify actual maintenance CLI with fresh protected cash/share/order
  checkpoint before selecting any cancellation.
- [x] Verify observation mode has no DELETE; stop mode selects at most one
  working retained order and never retries an uncertain cancellation.
- [x] Verify unknown observations abort, terminal orders are skipped, and
  retained capital claims survive cancellation acceptance or transport failure.
- [x] Inspect GitHub tests, lint, formatting and changed-behavior mutations.

All four items proved by GitHub 38315068491 at a92807a, including four exact
executed recovery regressions. No installed supervisor is claimed.

The actual caller is radar alpaca-paper-maintain with an existing private
journal, pinned account, bounded activity pages and optional --cancel-working.
It owns the same journal throughout checkpoint, selection and cancellation.
No model callback, new order, retry or live endpoint exists in this command.
It shares the existing payload-bound cancellation implementation and fixed
paper DELETE transport. One cancellation per tick bounds network effects.

Working orders, pending cancellations, terminal orders and unresolved claims
are reported separately. Terminal progress does not prove fees are final.
The report retains current claims and grants no new-order or release authority.
Checkpoint failures produce no success report. Stop mode must still reconcile
first; missing orders or inconsistent fills can prevent cancellation, so this
is not an emergency broker-wide kill switch. Existing pending_cancel states
without a locally recorded cancellation are refused by the underlying cancel
caller rather than resent.

## Handback

Source a92807aa7c20f4992494211aafb62bc113d6c9c9: GitHub 38315068491 passed
all fourteen jobs, 2841 Rust and 59 Python tests, four new exact maintenance
regressions, all previous exact regressions and three systemd pairs. All four
mutation artifacts inspected: three caught, five compiler-unviable, no
survivors/timeouts. Compiler failures are unused arguments/imports or dead code,
not test kills. Verification is recorded on the worker child branch so the
verified parent remains stable. Next: worker verification and provisioning.

Prepared four coordinator tests, including the actual checkpoint collector on
an empty existing journal, one actual-process test for both command modes and
endpoint refusal, and shared cancellation transport. GitHub must run
all verification; no workstation tests/build/lint/mutations. No broker effects,
service installation or website deployment. Next inspect CI and mutation
diagnostics, repair any findings, then wire verified maintenance into the
isolated supervisor before enabling entries. Actual capacity/cost inputs,
effective site handover and measured forward outcomes remain incomplete.

The source review also found conflicting quote-size documentation: Alpaca's
[stock stream schema](https://docs.alpaca.markets/us/docs/real-time-stock-pricing-data)
still names round lots, its
[October 2025 changelog](https://docs.alpaca.markets/us/v1.1/changelog/marketdata-bid-and-ask-size-display-change)
specifically names CTA/UTP shares, and its
[official integration reference](https://github.com/alpacahq/alpaca-skills/blob/main/skills/broker-api/market-data/SKILL.md)
states shares more generally. None establishes IEX routed exit capacity for
this account. The
[paper simulator](https://docs.alpaca.markets/us/docs/paper-trading) does not
check order quantity against NBBO size and omits regulatory fees and several
live execution effects. A paper fill must not be used as live-liquidity proof;
the existing risk inputs remain unknown until their semantics are established.

Initial source 3029db4, GitHub 38313899616: formatting reported four test hunks,
lint required an equality assertion for useful empty-output diagnostics, and
the mutation baselines caught an invalid test fixture broker ID containing an
underscore. The journal correctly refused that ID. Prepared formatting/assertion
repairs and supported hyphenated fixture IDs; do not weaken journal validation.
The entire run then completed; all remaining ordinary jobs passed. All four
downloaded mutation artifacts and baseline logs confirmed the same fixture
failure before testing mutants, so this run supplies no mutation-coverage claim.
Repairs are ready for a fresh GitHub run; keep this branch stable while it runs.

Repair source 15a5546, GitHub 38314374131: all four mutation artifacts inspected;
three caught, five compiler-unviable, no survivors/timeouts. Caught the stop
conjunction inversion and both actual CLI match-arm deletions. Unviable logs
showed unused arguments/imports or dead code from removed function bodies;
these are not counted as test kills. Added four exact recovery-ordering
reapplications (skip refresh, terminal inversion, pending cancellation inversion,
first-to-last selection) to the GitHub private-setup job. Each must fail a
four-test execution and restore source; no compiler failure qualifies.
Lint additionally requires explicit BTreeMap::default() in the fixture helper;
that repair is prepared. The run then completed: tests and all remaining jobs
passed; only this lint diagnostic failed. All prior exact regressions passed.
Push the lint repair and four additional exact checks after reviewing this diff.
