# Progress: Plan 0013 Phase D — server half (ADR 0024)

Worktree: `.claude/worktrees/d-swap`, branch `plan-0013-d-swap-server`, base `origin/main`.
**Never commit this file** — it is working scratch, not part of the PR.

## Status: ALL 9 BUILD ITEMS DONE. Not yet committed/pushed/PR'd.

## Wire contract (coordinator-supplied, authoritative). Now live verbatim as
`crates/radar-serve/src/trade.rs`'s module doc comment — see that file rather
than this section if the two ever disagree; this copy is historical.

Amounts are JSON decimal STRINGS everywhere (u64 base units can exceed 2^53).

`GET /v1/market/quote?mint=<base58>&side=buy|sell&amount=<string>&slippage_bps=<optional int>`
200:
```
{
  "mint": "<base58>", "side": "buy"|"sell",
  "in_mint": "<base58>", "out_mint": "<base58>",
  "in_amount": "<string>", "out_amount": "<string>", "worst_out": "<string>",
  "in_decimals": <int|null>, "out_decimals": <int|null>,
  "slippage_bps": <int>, "impact_bps": <int|null>,
  "venues": ["<label>", ...],
  "quoted_at": <unix seconds int>
}
```
`POST /v1/customer/swap` (behind Tenant), body `{"mint","side","amount":"<string>","slippage_bps"?:int}`
200:
```
{
  "transaction": "<base64 unsigned v0 tx, fee payer = session wallet>",
  "last_valid_block_height": <int>,
  "quote": { ...exactly the quote object above... }
}
```
Both responses: `Cache-Control: no-store`. Refusals: existing `{error,reason}`
shape. `/health` gains `"trading": true|false`.

## Status by BUILD item

1. **assemble.rs — DONE.** `crates/radar-exec/src/assemble.rs`, hand-rolled v0
   tx compiler (no solana-* crate in the workspace). 6/6 lib tests pass.
2. **route.rs refactor + `Router::build`/`quote_at` — DONE.**
3. **`GET /v1/market/quote` (public) — DONE**, in `crates/radar-serve/src/trade.rs`.
4. **`POST /v1/customer/swap` (Tenant-scoped) — DONE**, same file. Default
   slippage 100bps, hard cap 500bps (`slippage_too_wide`, never clamped).
   Nothing persisted; the wallet address is never logged.
5. **`RADAR_TRADE` switch + `/health` `"trading"` field — DONE.** `=on` with no
   `RADAR_JUPITER_API_KEY` refuses to *start* rather than answering
   `trading_off` for a misconfiguration.
6. **Three-tier rate limiting (global 30/min, per-visitor 6/min, per-wallet
   6/min) — DONE**, dual-lock pattern copied from `positions.rs`.
7. **Refusal shapes `{error,reason}` — DONE**: `no_route` 404, `busy` 503,
   `trading_off` 503, `unreadable_route` 502, `bad_request` 400,
   `slippage_too_wide` 400.
8. **No-network test suite — DONE.**
   `crates/radar-serve/tests/a_swap_is_priced_and_built_by_radar.rs`, 8 tests,
   all passing: trading off refuses both routes without a Jupiter call;
   slippage over cap refused before any call; `no_route` refusal; the public
   quote route works signed-out with exact fixture-derived field values;
   visitor-cap 7th-call refusal proven against the upstream double's own
   request count; wallet-cap 7th-call refusal, same proof; global-cap 31st
   call refused across five visitors; two wallets each get their own
   transaction naming themselves as fee payer (checked via an independently
   re-derived minimal Solana v0 wire decoder — NOT `radar_signer::tx::decode`,
   which rejects lookup-table transactions).
9. **Handback paragraph — DONE**, appended to
   `docs/plans/0013-the-terminal-find-look-track-trade.md`'s Handback section.
   **ADR 0024 itself was also missing and has been written this segment**:
   `docs/adr/0024-a-signed-in-wallet-may-ask-radar-to-build-a-swap.md`. The
   plan's Phase D item 1 requires the ADR to exist *before* the routes it
   governs, and `trade.rs`'s module doc already linked to that exact path —
   the file just didn't exist on disk until now. Written to match the ADR
   0019 format (Context/Decision/Consequences/What this does not decide).

## Verification run this session (all scoped to `-p radar-exec` / `-p radar-serve`,
none full-workspace, none `--release`, per AGENTS.md)

- `cargo check -p radar-serve --all-targets` — clean.
- `cargo clippy -p radar-serve --all-targets -- -D warnings` — clean.
- `cargo fmt -p radar-serve -- --check` — clean.
- `cargo fmt -p radar-exec -- --check` / clippy — clean (unchanged this segment).
- `cargo test -p radar-serve --test a_swap_is_priced_and_built_by_radar` — 8/8 pass.
- `cargo test -p radar-serve` (whole crate: every pre-existing test file plus
  the new one, unit tests, doc-tests) — **all green, 0 failures** across every
  binary (main lib 276, plus each integration test file, plus 4 doc-tests).

## Design decisions made (for the final RETURN summary)

- **No solana-* crate.** Hand-rolled compile in `assemble.rs`.
- **tipInstruction:** always omitted from the compiled instruction list;
  refuse (`RouteError::Malformed`-shaped assemble error) if it is ever
  non-null rather than guessing.
- **Signed-out taker for the public quote route:** `Address::SYSTEM_PROGRAM`
  is passed as `taker` to `QuoteRequest::new` for `GET /v1/market/quote` —
  used only for pricing via `Router::quote_at`, which never calls `assemble`.
  No transaction is ever compiled for a signed-out caller; the quote route
  returns no `transaction` field at all.
- **no_route status code:** settled as **404** (`RouteError::NoRoute` maps to
  `StatusCode::NOT_FOUND`) — an HTTP-native "nothing here," consistent with
  other `not_a_coin`/`unscoped`-style 4xx usage elsewhere in this crate.
- **Rate-limit key for the public route:** `CF-Connecting-IP` when present
  (untrusted but fine for isolating one abusive visitor from another sharing
  a guess — never trusted for the *global* cap, which is charged regardless),
  else the TCP peer address via `Option<Extension<ConnectInfo<SocketAddr>>>`
  (axum 0.8's connect-info delivery mechanism — plain `Option<ConnectInfo<_>>`
  does not implement the required `OptionalFromRequestParts` trait and fails
  to compile as a handler parameter; `Extension<ConnectInfo<_>>` does).

## Bugs found and fixed this segment (all in `trade.rs`, all now verified clean)

- `Option<ConnectInfo<SocketAddr>>` as a handler parameter does not satisfy
  axum 0.8's `Handler` trait (confirmed by reading axum-core/axum source).
  Fixed to `Option<Extension<ConnectInfo<SocketAddr>>>`.
- `clippy::result_large_err` on `sides_for` and `parse_request` (both return
  `Result<_, Response>`) — added `#[allow(clippy::result_large_err)]` with an
  explanatory comment; `Response` genuinely is the returned value here, not an
  error wrapper worth boxing.
- Broken intra-doc link `[`Mode::from_vars`]` (no `Mode` type in this module)
  — fixed to `[`from_vars`]`.
- `clippy::needless_pass_by_value` on the new test file's `jupiter(...)`
  helper (`body: String` never consumed by value) — changed to `body: &str`,
  updated all 7 call sites to pass `&fixture(...)`.

## Next steps, in order

1. Stage by path (not `-A`): the new/changed files listed below.
2. Review `git diff --stat` / full diff once more.
3. Commit with `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>`.
4. Push, open a **draft** PR via `gh pr create --draft --repo 1xmint/theradar`,
   description ending with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
5. Watch CI; fix up to 3 failed rounds (mutation testing runs there, sharded —
   never run `cargo mutants` locally over more than a single file).
6. Produce the under-300-word RETURN summary: PR number/head SHA, CI status,
   files changed, the design choices above, anything not done (item D.3's
   browser half and D.5's independent review, both explicitly out of scope
   for the server-side PR).

## Files touched (this whole worktree's history, all uncommitted)

- `crates/radar-exec/src/assemble.rs` (new)
- `crates/radar-exec/src/lib.rs` (added `pub mod assemble;` + re-exports)
- `crates/radar-exec/src/route.rs` (added `Router::build`, `Router::quote_at`,
  `fetch_body`, `pub struct Build`; `quote()` behavior unchanged)
- `crates/radar-serve/src/trade.rs` (new — both routes, `Trading`, rate
  limiting, `RADAR_TRADE`)
- `crates/radar-serve/src/lib.rs` / `main.rs` / `access.rs` (wired `trading`
  into `AppState`, routes, `/health`, admission rules)
- `crates/radar-serve/tests/a_swap_is_priced_and_built_by_radar.rs` (new, 8 tests)
- up to ~16 pre-existing `radar-serve` test files touched only to add the new
  `trading: None` (or equivalent) field to their `AppState` literals, no
  behavior changes
- `docs/adr/0024-a-signed-in-wallet-may-ask-radar-to-build-a-swap.md` (new)
- `docs/plans/0013-the-terminal-find-look-track-trade.md` (Handback paragraph
  appended)

None of the above are committed yet.
