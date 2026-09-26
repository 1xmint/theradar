<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0014 G1 — per-unit CryptoHouse query counts — progress

Branch: `g1-per-unit-queries`, off `origin/main` at `a5f1f3d`.

## Done, and compiles

- `crates/radar-backfill/src/cryptohouse.rs`: `QueryError::is_quota_exceeded()`
  (detects 0036's vendor message, distinct from the existing
  `is_budget_exhausted()`); `Client` gained `queries: Cell<u64>`,
  `quota_refused: Cell<u64>`, `queries_issued()`, `quota_refusals()`; `query()`
  is now a counting wrapper around the original body, moved verbatim into
  private `query_uncounted()`. Five new tests in the existing test module.
  Verified: `cargo +stable-x86_64-pc-windows-gnullvm check -p radar-backfill --tests`.

- `crates/radar-store/src/query_meter.rs` (new file): the sidecar state file
  (`.query-meter`, text not JSON — modeled on `cursor.rs`, not a new
  dependency). `UnitTally { queries, refused }`, `record(store, unit, day, q,
  r)` (accumulates, rolls on a new day), `today(store, unit, day) ->
  Option<UnitTally>` (`None` on absence — rule 9). Six unit tests. Registered
  in `crates/radar-store/src/lib.rs` (`pub mod query_meter;` +
  `pub use query_meter::UnitTally;`). Verified: `cargo
  +stable-x86_64-pc-windows-gnullvm check -p radar-store --tests`.

- `crates/radar-backfill/src/launch_block.rs`: `CryptoHouseBlocks` gained
  public `queries_issued()` / `quota_refusals()`, delegating to its private
  `client`.

- `crates/radar-cli/src/consider.rs`: after `record_pass`, calls
  `radar_store::query_meter::record(Path::new(store), "consider", &today,
  blocks.queries_issued(), blocks.quota_refusals())`, `today` from
  `radar_store::from_epoch(radar_store::now_epoch())[..10]`. A write failure is
  `eprintln!`'d, not propagated. Verified: `cargo
  +stable-x86_64-pc-windows-gnullvm check -p radar-cli --tests`.

- `crates/radar-backfill/src/main.rs`: new private `record_query_meter(store,
  unit, client, seen: &mut (u64,u64))` helper (delta-based, for the two units
  whose `Client` lives the whole process). Wired into:
  - `follow()` (unit `"radar-follow"`): called in the failed-window branch
    before `continue`, and at the bottom of the loop after a successful
    window.
  - `market_tape()` (unit `"radar-market-tape"`): called after the candidates
    query fails, after the trades fetch fails, and at the bottom of the loop
    after a successful pass. (Not called on the `is_between_passes` idle
    `continue` — no query happens there, so the delta would be zero.)
  - `measure()` (unit `"radar-backfill --outcomes"`, single-pass so recorded
    with a fresh `(0, 0)` seed rather than threaded state): called on the
    "nothing due" early return and at the very end of a normal run.
  Verified: `cargo +stable-x86_64-pc-windows-gnullvm check -p radar-backfill --tests`.

- `crates/radar-cli/src/brief.rs`: `QUERY_METER_UNITS` (the four unit-name
  strings, must match the strings used above exactly:
  `"consider"`, `"radar-follow"`, `"radar-market-tape"`,
  `"radar-backfill --outcomes"`), `grade_unit_queries(unit, Option<UnitTally>)
  -> Check` (pure; refused > 0 → Warn, else Ok, `None` → Ok "no record for
  today yet" — never "0 queries", per rule 9), `unit_queries(store, unit, day)
  -> Check` (reads via `radar_store::query_meter::today`). Wired into `run()`
  right after `query_budget(store)`, one `Check` per unit. Six new tests
  mirroring `grade_query_budget`'s style (refused warns and shows the count,
  clean run is Ok and doesn't borrow the warn sentence, absent is Ok and says
  "no record" not "0", the two don't read alike, a fresh store reports "no
  record" via the real `unit_queries` path). Verified: `cargo
  +stable-x86_64-pc-windows-gnullvm check -p radar-cli --tests`.

## Not done

1. **Docs.** I had not yet found or edited any doc describing `radar brief`'s
   output list or the query meter. I looked at `docs/STATE.md` (a dated,
   append-mostly log — the newest entries are near the end, around line 876
   "Where to start"; the most recent per-feature entry I found was "The public
   analyst, as of 2026-09-06" at line 541) and `deploy/README.md` (mentions
   specific brief checks inline but has no enumerated list to extend). The
   right move is almost certainly a new dated entry in `docs/STATE.md` (style:
   "`radar brief` gains N lines ... as of 2026-09-26"), inserted near the end
   before "## Where to start" (line 876) — I did not write it.
   **Do not touch `docs/research/0036-...md`** — the task says its addendum
   is a separate, owner-facing step; tick nothing in `docs/plans/0014-...md`
   either.

2. **`cargo fmt`** has not been run on the touched files.

3. **Clippy** has not been run on any touched crate.

4. No `-p radar-store --tests`, `-p radar-backfill --tests`, `-p radar-cli
   --tests` recheck since the very last edit (brief.rs tests) — it was run
   once right after adding them and passed, but re-verify after `cargo fmt`.

5. No PR opened yet (explicitly not requested this round — the coordinator
   asked only to commit and push WIP).

## Decisions made and why

- Per-unit query counts live in a **new small sidecar state file**
  (`.query-meter` beside the store), not a `radar-store` schema change,
  because `Coverage` is about which slots were observed, not how many network
  calls were spent, and a schema change would be a far larger diff. This is
  not "a new store" in the repo's sense (nothing here is queried, joined,
  replayed, or `AsOf`-gated) — it is the same category of file as
  `cursor.rs`'s follow cursor, which documents several such siblings.
- The meter file is **hand-rolled text**, not JSON, to avoid promoting
  `serde_json` from a dev-dependency to a real one in `radar-store` — not
  something this task asked for.
- `is_quota_exceeded()` (vendor's actual refusal, substring `"has been
  exceeded"`) is kept **distinct** from the existing `is_budget_exhausted()`
  (a unit's own declared ceiling running out) — 0036 explicitly warns
  conflating them would hide CryptoHouse being down behind a budget message.
- `measure()`'s two record call sites reset the delta-tracker to `(0, 0)`
  each time rather than threading state, because `measure()` is a genuine
  single pass per process invocation (unlike `follow`/`market_tape`, which
  loop forever in one process) — its `client`'s lifetime total *is* the run's
  total.
- Unit-name strings are exactly `"consider"`, `"radar-follow"`,
  `"radar-market-tape"`, `"radar-backfill --outcomes"` — chosen to match
  0036's own table. **Any doc addition must reuse these verbatim.**

## Watch out for

- `main.rs`'s `market_tape()` has three `continue` points and one
  loop-bottom; I instrumented three of the four query-issuing paths (skipped
  the true idle path deliberately). Double check I didn't miss a fourth path
  if the function is touched again.
- `consider.rs`'s query-meter write happens after `record_pass` but **before**
  the `if let Some(dir) = record_to { ... write_decisions(dir, ...)? ... }`
  block — so a `write_decisions` failure (which returns early via `?`) still
  leaves the query count recorded. This is intentional (the decision already
  happened) but is worth confirming a reviewer agrees with.
- Test names and file locations for reference: `crates/radar-store/src/query_meter.rs`
  tests at the bottom of that file; `crates/radar-cli/src/brief.rs` new tests
  are inserted into the existing `mod tests` block right before "A manifest
  with one real-looking sha line" (`fn a_manifest()`).
