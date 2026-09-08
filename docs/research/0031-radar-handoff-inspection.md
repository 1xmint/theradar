<!-- SPDX-License-Identifier: Apache-2.0 -->
# Research 0031 — Source inspection for the next Radar implementation

**Status:** source-inspected, 2026-09-07; proposed regression cases, not a production measurement or completed fix.

**Baseline:** `4d14032f493d01ef76d27744769efda786956fc7`, local `main`.

This note supports [design 0015](../design/0015-radar-implementation-handoff.md). The work product is an implementation handoff. No production store was read, no trading edge was measured, and no new regression was executed. The designer also sampled the prior signer/tag fixes in source rather than repeating research 0030's entire audit.

## Reproducible source findings

Run the following read-only searches from the repository root. They locate the implementations inspected on the date above; line numbers may change after the baseline.

```powershell
git rev-parse HEAD
rg -n 'fn trade_coverage|fn covered|fn trades_to_t|reader.read' crates/radar-research/src/features.rs
rg -n 'fn points_of|filter_map|fn split' crates/radar-research/src/edge.rs
rg -n 'succeeded:|SYSTEM_PROGRAM|realised_lamports|tx_index:' crates/radar-backfill/src/extract.rs
rg -n 'fn scan_ranking|best >=|fn merge_verified' crates/radar-analyst/src/contest.rs
rg -n 'pub const fn score|None => self.raw_score' crates/radar-contest/src/score.rs
rg -n 'pub struct Fact|pub rendered|pub fn substitute|is_ascii_digit' crates/radar-roast/src/sheet.rs crates/radar-roast/src/tags.rs
rg -n 'export async function leaderboard|export async function pool|live \?\?' site/src/api.ts
rg -n 'honest way|looks|inside their own|community token|sm:table-cell' site/src
```

### HIGH — Trade coverage and successful-event semantics

`trade_coverage` in the research feature builder enumerates trade partition filenames; `covered` treats every named partition as covered. There is no completed-range provenance in that decision. `trades_to_t` iterates trades and filters mint/time but does not require successful execution. In backfill, `row.ok.as_deref() != Some("0")` evaluates true when success is absent. Trade extraction also uses a system-program actor placeholder and can lack realised amounts and transaction order.

**Inference:** a partial source can look like a complete quiet interval, failed transactions can enter successful-activity features, and placeholders can look like actors. This is an input-validity problem before it is a statistical problem.

**Required regression:** incomplete versus complete-empty ranges; explicit failure versus unknown success; unknown actor/order; future coverage added to an earlier replay. No historical data cleanup is authorized by this note.

### HIGH — Labels determine the population before folds

`points_of` uses `filter_map` and returns no point when the requested gross label is absent; `split` then constructs folds from those surviving points. Label availability therefore changes the tested population and can move boundaries. Separate review also identified overlapping price-observation lookbacks, which require explicit source/freshness evidence rather than treating the presence of a value as a fresh executable exit.

**Inference:** complete-case selection can bias the result. This inspection does not quantify its size or direction.

**Required regression:** removing labels cannot move frozen cohort boundaries; one reused old fill cannot demonstrate distinct entry and exit observations; censoring remains in denominators and reports.

### HIGH — The contest mixes scoring bases after incomplete scans

`scan_ranking` stops on a meter refusal or read error and returns the portion scanned. `merge_verified` leaves other candidates with `verified: None`. `Metrics::score` resolves that value to the raw score. The scan's early stop compares `best >= next_raw`, whereas final ordering also has tie behavior.

**Verified mechanism:** an unscanned candidate retains its raw score. Whether a particular candidate wins is a fixture to reproduce; no live improper payout was observed or claimed.

**Owner's decision changes the remedy:** incomplete engagement must not prevent selecting a winner. Design 0015 specifies one common scoring mode for the whole week, explicit fallbacks, deterministic ties, and an audit certificate. It does not ban farmed participation, impose human judging, or require no-winner rollovers.

### HIGH — Numeric slots do not bind a complete claim

`Fact` stores label and rendered value separately. `tags::substitute` inserts the rendering where the model puts the tag. This protects the value path but does not, by itself, prove that surrounding prose uses the right subject, time, unit, or negation.

**Inference:** the system's semantic guarantee is weaker than “every public statement is measured.” No adversarial model call was run for this note. The proposed fix uses whole typed clauses and a restricted selection grammar; test mismatched subjects and negations before claiming the gap closed.

### HIGH — Site error/empty conflation and overclaiming

`leaderboard()` substitutes an empty week when its fetch returns null. `pool()` substitutes a null vault. The fetch helper returns null for non-success status and exceptions. Those are not the same observations as a successful response confirming no week or token.

Home uses “the honest way” and launch-block wording; About states that a community token exists. The classifier/window and actual launch status must supply the public wording. Mobile table classes hide evidence cells below the small breakpoint.

**Required regression:** API unavailable, confirmed unminted token, confirmed empty week, stale example, and live data render distinct states; all public pages agree on launch status; mobile evidence remains accessible.

## Browser observation and limits

A read-only browser reviewer reported a 375px viewport with a 394px document width and the Home summon section starting about 2,703px below the top. These are a dated layout observation, not a performance benchmark or a guarantee about the current deployment. No screenshot artifact is committed with this note. The implementer must repeat browser checks and retain screenshots before declaring the redesign complete.

Useful reproduction measurements after selecting a 375px viewport are `document.documentElement.scrollWidth`, `window.innerWidth`, and the document-relative top of the section whose ID is `ask`. The desktop source review independently confirms the late placement of that section and the mobile evidence-hiding classes.

## Primary-source checks and limits

The handoff links to the primary Jito, Solana, pump.fun, SEC, Tennessee DFI, and X contest documentation consulted on 2026-09-07. These establish reference behavior or questions, not that Radar's live instrument matches them. Jito's transaction-order and atomicity description does not justify the inverse claim that observed adjacent transactions prove bundle membership; its uncle/rebroadcast exception is material.

**Owner decision:** Josh reports direct X developer evidence that compliant operation does not need additional written approval, and explicitly directs the implementation to leave that question settled. Do not reopen it or create an approval-request task from this note.

No legal clearance, acquisition coefficient, successful attack, new signal lift, account independence, or world-class rating was measured. The next research note should contain actual repaired-instrument results with dataset/commit hashes and commands, not promote these inspection findings into measurements they are not.
