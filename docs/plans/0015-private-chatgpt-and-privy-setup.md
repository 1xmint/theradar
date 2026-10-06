<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0015 — Private ChatGPT and Privy setup

**Status:** in progress; connection UI, sign-in fixes and security headers deployed;
ChatGPT and Privy server setup remains.
**Date:** 2026-10-06.
**Branch:** fix/wallet-signin-diagnostics.
**Base:** b55a47651c81ed8a1134f00deaeabc9b6cc4d792.

## Owner direction

Josh explicitly chose a private tool, fully autonomous execution, and a Privy
wallet. He already has a Privy app and is completing its security checklist.
This resumes the direction in [design 0017](../design/0017-a-private-autonomous-trader.md)
and [plan 0011](0011-private-autonomous-trader.md). It supplies no
wallet, delegation, capital mandate or inference allowance. Josh supplied the
public app ID `cmthhkznr0a3u0cl86prxlb7x` during this session. Server configuration
and the remaining identifiers and limits stay unset; no real trade is authorized.

## This increment

- Classify wallet exceptions by code: only 4001 is cancellation. Preserve
  technical failures and their stage, allow retry, reject malformed responses,
  and detect Solflare's namespaced provider. Display feedback below the button.
- Format SIWS issue times as ISO UTC and random nonces as alphanumeric hex.
  The live response used epoch seconds and base64url, contrary to the wallet
  specification. A wallet-side rejection is plausible, not yet confirmed by
  the owner's browser.
- Add the operator-only `/automation` page, reached by **Private setup** in the
  terminal. It connects ChatGPT, reports Privy app configuration, and states
  that autonomous execution is not connected. `/ask` contains only chat.
- Allow subscription linking with a configured CLI before an inference budget
  exists. Inference stays unavailable without its existing budget and ledger.
- Put CSP, X-Frame-Options and nosniff on the application's embedded responses.
  Privy's required wallet origins and Turnstile are allowed; framing Radar and
  inline/eval scripts remain forbidden. The Caddy example no longer sets a
  second CSP. Inline styles are allowed for the wallet UI.
  HTTPS metadata fetches and images preserve the existing creator-hosted token
  pictures; their content remains untrusted data.
- Update the vulnerable source-map-js build dependency without upgrading the
  Solana signing library across a major version.

## Evidence

The working tree's `just web` passed 343 tests, audit at the existing high
severity gate, type-check and build. `just site` passed 79 tests, audit,
type-check and build after the same source-map-js patch in its lockfile.
`cargo +stable-x86_64-pc-windows-gnullvm test -p radar-serve --lib --test chat_route --test access_guard`
and the repo-conformance tests passed, as did scoped clippy with warnings denied.
The mobile-width setup page was inspected in the browser and its overflowing
connect button corrected. These checks are local, not live-trading evidence.

Reversing the wallet rejection comparison failed four regressions. Requiring a
chat instance before subscription linking failed the new route regression;
restoring the fix passed all five chat-route tests. Plumbing and static copy are not
mutation subjects; CI owns the wider mutation gate.
Removing the application frame header failed the CSP/header regression.

The first CI web job at `39b9511` failed before tests: npm 10 required the
nested TypeScript 5 peer that npm 11 removed when updating source-map-js.
Restore that existing lockfile entry and verify `npm@10.9.9 ci` explicitly.
Do not deploy the first release artifact; it predates the SIWS format fix.
CI's Rust 1.99 lint also rejected ten pre-existing empty-vector assertions in
the CLI and stream tests. Their diagnostic comparisons are updated as a release
prerequisite, without changing the predicates or production behavior. The first
run's tests and all four mutation shards passed; web and lint were the failures.
The restored npm 10 install passed, followed by all 343 web tests and the build.
Reapplying both old SIWS field encodings failed their new regressions.
The next CI run passed web and its mutation shards but exposed the same new
lint in more crates. A workspace check with Rust 1.99 and `--keep-going`
collected all remaining diagnostics. Only test assertions changed, and the
full Rust 1.99 workspace clippy gate now passes. No lint was suppressed.

Live inspection on 2026-10-06 returned build
`a4d24f7f1db0a7bdb2a9ef271b2981e8dfc087a1`, `agent.configured=false`,
`trading=true`, `policyClosed=true`. The environment had no Privy app or model
provider setting. `codex-cli 0.131.0` is installed and the radar-agent user
exists, but the isolated wrapper was absent. Public HTML returned neither CSP
nor X-Frame-Options. A Caddy example file containing headers was not evidence
that the public response carried them.

Final code commit `49415d5d2f07e1289eea3b32c1afb38d8a1c7f22` passed every job
in CI run `37527711243`, including all mutation shards and their aggregate.
Release run `37527703900` produced the matching commit in `BUILD-INFO.txt`.
The downloaded and installed `radar-serve` both have SHA256
`ffa0d504fe4d03e6ccc42c5e471bdb280b0339bb9ae1fee20c584fa047265b69`.
Deployment used `scp` to `/tmp/radar-serve.new`, then the fixed
`ssh guardian-vps-tail "sudo radar-deploy"` command. The service reported active.
Public `/health` returned that exact build and `policyClosed=true`.

After deployment, `curl.exe` against public HTML returned HTTP 200, CSP with
Privy's iframe origins, `X-Frame-Options: DENY`, and nosniff. A dummy-address
challenge returned a 64-character alphanumeric nonce and ISO UTC issue time.
The live browser loaded market rows and a chart with no captured error/warning
logs. An actual owner wallet signature remains untested. The owner can now mark
the CSP and X-Frame-Options checklist items complete in Privy.

Customer config still returns HTTP 503 (not configured), and public `/v1/link`
redirects to the existing Cloudflare operator login. The Privy app ID is recorded,
not installed as server configuration. The Codex wrapper and service settings
need administrator access: ordinary sudo requires a password, and the permitted
passwordless command only deploys the Radar binary. No unrelated sudo entry was
used to bypass this boundary. PR 334 contains this increment.

## Next actions

1. Complete: PR 334, CI and release runs above, artifact hash, fixed deployment,
   live build, public headers, and challenge format verified at `49415d5`.
2. Use the supplied public Privy app ID and obtain the dedicated Solana wallet ID. Configure
   application credentials securely on the server; keep the authorization key
   only in the separate signer. Do not put secrets in chat or the web bundle.
3. Install the isolated Codex wrapper per the deploy guide and set
   RADAR_MODEL_CODEX. The owner completes the browser sign-in. Choose an
   inference allowance separately, then verify an evidence-backed model call.
4. Resume plan 0011's portfolio reconciliation, durable operations, proposal to
   risk-kernel adapter, Privy signing and execution supervisor. Connect typed
   recommendations to deterministic validation, never parse chat prose into
   transactions. Exercise the complete lane with stubs before a live canary.
5. Obtain principal, position/trade, loss, asset, inference and session bounds
   before enabling unattended own-money execution. Leave Policy::CLOSED intact.

## Handback

**Stopped at:** deployed connection and diagnostics increment at `49415d5`.
ChatGPT and Privy are not connected in production; no autonomous trading loop
has been enabled. The branch's broader private-autonomy work remains in progress.
**Next action:** owner checks both Privy security boxes and continues onboarding.
Use administrator access to install the isolated Codex wrapper and service
configuration from the deploy guide. Obtain the owner's Privy DID and dedicated
wallet ID to configure private admission, then continue steps 2–5.
**Do not:** claim a wallet is delegated from an app ID, treat connected ChatGPT
as capital authority, expose the private site, copy existing subscription
credentials between hosts, invent money limits, or turn on automated trading.
