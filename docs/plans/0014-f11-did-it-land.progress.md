<!-- SPDX-License-Identifier: Apache-2.0 -->
# F11 progress (scratch handback file, not part of plan 0014's own doc)

**Done:** Milestone 1 (server route) at `38c7000`. Milestone 2 in full at
`23299d5`: the WIP integration test file
`crates/radar-serve/tests/a_tx_status_says_whether_a_trade_landed.rs`
(committed unverified at `f2211b5`) compiles clean under
`cargo +stable-x86_64-pc-windows-gnullvm check/clippy/fmt -p radar-serve
--tests` and already covered all four states plus the RPC-failure/honesty
case (verified by reading it, not rewritten). Added the web half:
`tradeContract.test.ts` diffs `render_tx_status`'s keys (unioned across its
four `json!` literals, one per match arm) against `TxStatus`; `honesty.test.ts`
covers `chain_unreadable` and a new `landingMessage` suite including the
expired-vs-unknown honesty rule. `npx vitest run` -> 266 passed, `npx tsc
--noEmit -p .` clean.

**Next:** Milestone 3 -- `TradePanel.tsx` poll loop (1.5s, ~90s cap) after
`signAndSendTransaction`, landed/failed/expired/unknown copy, Solscan link in
every state, `usePositions.ts` refresh on landed only (not immediately on
send), stale-build (>60s) rebuild before signing. Web tests for each state
plus the rebuild path. Then Milestone 4: raise `MIN_WEB_TESTS` in `justfile`
(currently "262") to 262 + new `it(` count, dated comment; delete this
scratch file in the final commit.

**Watch out for:** `web/node_modules` was missing in this worktree --
`npm install` first or vitest/tsc fail with ERR_MODULE_NOT_FOUND. `npm
install` also touches `web/package-lock.json` with unrelated
optional/peer-dep churn (a `typescript` sub-entry under `@solana/web3.js`
appearing/disappearing) -- `git checkout -- web/package-lock.json` before
committing unless you deliberately changed a dependency. Do not run `cargo
test` locally (CI runs it) -- `check`/`clippy`/`fmt` are the local gate.
