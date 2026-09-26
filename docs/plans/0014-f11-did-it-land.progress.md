<!-- SPDX-License-Identifier: Apache-2.0 -->
# F11 progress (scratch handback file, not part of plan 0014's own doc)

**Done:** Milestone 1 (server route, rate limiter, RPC reads) and the typed
half of Milestone 2 (`api.ts`'s `txStatus`/`TxStatus`, `honesty.ts`'s
`chain_unreadable` refusal and five landing-state sentences) are committed
on `f11-did-it-land` at `38c7000`. `cargo fmt`/`clippy`/`check -p radar-serve`
are clean (owner rule: no local `cargo test`).

**Next:** integration tests in a new `crates/radar-serve/tests/` file
(pending/landed/failed/expired + RPC-failure), extend
`web/src/tradeContract.test.ts` for `TxStatus`, then Milestone 3
(`TradePanel.tsx` polling + stale-build rebuild, `usePositions.ts`
landed-only refresh), web tests, then raise `MIN_WEB_TESTS` in `justfile`.

**Watch out for:** target/ cache went stale mid-session (radar-serve
couldn't see radar-onchain's new pub items until `cargo clean -p
radar-onchain -p radar-serve`) -- if `cargo check` claims a symbol you just
added doesn't exist, clean those two crates before assuming the code is
wrong. Branch was rebased-by-merge onto `origin/main`'s `a5f1f3d` (the
plan-0014 doc + `tradeContract.test.ts` landed there after this branch was
cut) -- already merged in, no action needed.
