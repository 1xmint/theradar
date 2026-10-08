<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0015 — Private ChatGPT and Privy setup

**Status:** in progress; private ChatGPT linking and unlimited subscription allowance
are installed; Privy login, wallet creation, balances and draft limits deployed.
Owner reports a verified Solana wallet and live balance. Signer delegation and
autonomous execution remain.
**Date:** 2026-10-06.
**Branch:** fix/wallet-signin-diagnostics.
**Base:** b55a47651c81ed8a1134f00deaeabc9b6cc4d792.

## Owner direction

Josh explicitly chose a private tool, fully autonomous execution, and a Privy
wallet. He already has a Privy app and is completing its security checklist.
This resumes the direction in [design 0017](../design/0017-a-private-autonomous-trader.md)
and [plan 0011](0011-private-autonomous-trader.md). It supplies no
wallet, delegation, capital mandate or inference allowance. Josh supplied the
public app ID `cmthhkznr0a3u0cl86prxlb7x` during this session. Its server setup is
now installed; the owner later supplied a wallet identifier/address and confirmed
the balance read (see the dated notes below). Limits remain unset. No real trade
is authorized.

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

### Administrator setup increment

The owner authorized server setup and confirmed admin access. SSH still requires
interactive sudo password entry. Prepared `deploy/setup-private-connections.py`
for that entry and hidden Privy app-secret input on the owner's own terminal.
The script keeps root-only backups, preserves unrelated env settings, installs
the supplied app ID, and sets private admission closed without a money or
inference allowance. It applies no signer configuration and does not restart Radar.

Live systemd inspection exposed `NoNewPrivileges=yes` on Radar: the old documented
sudo wrapper cannot work there. Replaced that deployment approach with a socket
service under radar-agent, retaining service hardening. Tests exercise command
and peer refusals, cleared environment/stdin, separate output streams, and
process-group cleanup using a fake CLI. They make no vendor or trading call.

Documentation commit `97e7543`'s CI run `37528427538` failed one chart test at its
initial mode assertion; the same unchanged code passed the preceding code CI.
Wait for the chart effect before asserting its mode, as the neighboring saved-mode
test already does. The scoped five tests and full 343 web tests/build pass locally.

Setup code commit `d56f50a1ea6c9fb2f72bdce9f1f1831f462b2a51` passed all jobs in
CI run `37530529280`. Its required tests job ran the four Linux bridge tests with
NoNewPrivileges set. The same tests passed on the VPS, and removing the peer
refusal failed the peer-boundary test. Python/client-shell syntax and
`systemd-analyze verify` passed. All four deployment source hashes matched the
uploaded files. That version of `/tmp/radar-private-setup.py` had SHA256
`d36775b7213553276a16f87fdec35c1c1013bb819f6461f898a332057c13a72c`.
No administrator install, secret entry or vendor login has occurred yet.
The owner's next attempt stopped at the setup script's duplicate-setting check
for RADAR_CUSTOMER_SALT, before any mutation. Match EnvironmentFile's last-value
behavior instead of refusing an existing repeated assignment. Preserve both salt
lines verbatim; do not rotate the effective salt. Two fixture regressions check
last-value selection and unrelated-line preservation, without reading live salts.
Fix commit `c0546a45c185f561c1017595222a237f74ebd6ac` passed every job in CI run
`37532736530`. All six Linux setup/bridge tests also passed on the VPS with
NoNewPrivileges set; restoring the duplicate refusal failed both new regressions.
The corrected script is uploaded as `/tmp/radar-private-setup.py`, with matching
local and remote SHA256
`06128a7c2298a49a7f86f0d9845ce6c07b7922f2eacc745700506736a38a96c7`.
Remote Python compilation passed. The other three deployment files are unchanged.

The owner reported completing setup. Live inspection then confirmed an active
radar-codex socket, root-owned client/library, socket ownership radar-agent:guardian
with mode 0660, and env mode 0600. `setpriv --no-new-privs
/usr/local/bin/radar-codex login status` reached the installed vendor CLI and
returned "Not logged in". No secret contents were inspected.
Rechecked the `49415d5` release manifest and local binary hash, uploaded it to
`/tmp/radar-serve.new`, and ran the fixed `sudo radar-deploy`. Public `/health`
still reports the exact build and `policyClosed=true`; public customer config
now returns the supplied Privy app ID instead of 503. Radar retains
`NoNewPrivileges=yes` and `ProtectHome=read-only`. No inference allowance was
set, so `agent.configured=false` remains expected. This verifies installed app
configuration, not Privy credential validity or wallet delegation.
Opening `/automation` in the agent's browser was blocked by its client before
the page loaded; the owner must use their operator browser to complete linking.

The owner then reported "no Cloudflare Access assertion" and authorized control
of their logged-in Cloudflare browser. Dashboard inspection confirmed the new
`/automation` route had no Access application. Added that destination to existing
Radar app `c8b869e6-f831-4448-ba90-de3f7c324be9`, retaining its Josh-only policy.
Its AUD matches the installed Radar configuration. Cloudflare reported success;
an unauthenticated request now redirects to login with that exact audience.
The separate Radar link app `6194bf19-ac25-4a2d-8f57-6ca2cb554595` issues a different
audience, so `/v1/link` must move into the main application too. The main-app
form was prepared, but Cloudflare refused the duplicate destination. Deleting
the obsolete single-route app required explicit action-time confirmation under
browser-control policy. The owner replied "you have full permission".
Deleted that app and saved the main Radar app with `/ops`,
`/automation`, and `/v1/link`, retaining its existing Josh-only policy. Cloudflare
reported application deletion and the main app listed three destinations.
Unauthenticated HEAD requests to both setup and link now return 302 with AUD
`9ef62960f26e3c848e254acae4ec81b2d5a3127e8fdd907329f9c5f525b4c0d2` in the
login redirect, matching the installed server. Public health still reports
build `49415d5` and `policyClosed=true`. No server config or binary changed.
Brave blocked the login redirect with ERR_BLOCKED_BY_CLIENT after the page was
reloaded initially; a later reload reached the Cloudflare Access login page.
Following its existing Cloudflare identity-provider link then reached a browser
security rejection: the page's URL protocol was outside HTTP/HTTPS. Browser
policy prohibits workarounds, so the owner must finish that sign-in. No browser
protection was disabled or bypassed. The policy's existing allowed email is
joshfair2@gmail.com. Authenticated Radar access and ChatGPT linking remain unverified.

The owner clarified that their currently signed-in Cloudflare account uses their
other email address, confirmed control of both addresses, and approved whichever
works after being asked about adding that account to Radar's policy. Added the
current account's email to the existing Josh policy while preserving the original
email. Reopened the saved policy detail page and verified its Include Emails rule
lists both, with Action Allow. The reusable policy remains attached to eight
Radar applications. No public or domain-wide email rule was added. This verifies
the saved allowlist, not an authenticated Radar session; the browser sign-in
handoff remains necessary.

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

**Device prompt repair:** the owner confirms Access sign-in succeeds. The
installed Codex 0.131.0 prints its device prompt immediately through the bridge,
but ANSI colors prevented `parse_link` from recognizing it. Numeric SGR
formatting is now removed before parsing. The 66 radar-model tests and scoped
clippy pass; restoring the old parser fails the new captured-format regression.
Code commit `d73f8cfe4176d6e8e8dba2df1e732c77f671095d` passed every job in CI
37557211086, including all mutation shards. Release 37557206803 passed; its
matching manifest names that commit and radar-serve SHA-256
`c3af21a2a6fdad2a99aab64dfc604dcdab798d54895bca81611e1d42376911ce`.
After verifying the download, shipped it through `/tmp/radar-serve.new` and
`sudo radar-deploy`; `/health` reports the full code commit and status ok.
The service retains User guardian, NoNewPrivileges yes and ProtectHome read-only.
The owner subsequently completed the device flow and reported Linked in the
Radar page. `/usr/local/bin/radar-codex login status` independently reports
Logged in using ChatGPT. Public health still reports `agent.configured=false`,
as expected before an inference allowance is selected. Documentation commit
`91ea610` also passed all jobs in CI 37557612148.

**Owner inference choice:** after being offered daily call caps, Josh explicitly
selected unlimited. This is no Radar-imposed daily subscription call cap; it
does not change provider limits, per-investigation bounds or capital authority.
The explicit `unlimited` literal is supported only for a valid Codex provider;
the saturating meter continues recording usage and its maximum counter sentinel
cannot impose a daily refusal. Startup identifies the mode as subscription
usage rather than printing a fictitious dollar ceiling. Missing configuration
still disables inference. The existing administrator script has an explicit
mode to save this choice without re-entering or exposing Privy secrets.
Code `d1213e65853c23f85f344659c084eb2f8068dd65` passed all jobs in CI
37558536953. Release 37558534038 passed with that exact manifest commit;
radar-serve SHA-256 is
`93d8349868b3ffb5b718bac3964f187834e3684a98acc83561b4f2097b3f1f1c`.
Verified downloaded and staged VPS hashes, then deployed through the fixed
script. Public health reports that build and status ok, with policyClosed true.
The isolated CLI remains logged in and Radar retains its service hardening.
The current administrator script is uploaded at `/tmp/radar-private-setup.py`,
matching local SHA-256
`2b964e5e0c36143a1ae39fc515c75ae4c99a269e6c1a55587918401b203da76b`.
The owner ran the administrator command and fixed deployment. Local and public
health now report agent configured true, provider codex and build `d1213e6`;
login status remains Logged in using ChatGPT and hardening remains intact.
Local validation: 68 radar-model tests, the allowance reporting binary test,
33 conformance tests, scoped model/serve clippy and formatting pass. Disabling
the new unlimited literal makes its allowance regression fail. Eight Linux
bridge/setup tests pass under NoNewPrivileges, including identity preservation
and rejecting paid API/missing isolated-client settings. A real call through
the installed isolated client exits successfully with: "No trade conclusion
can be drawn because no instrument evidence was provided." This verifies
subscription inference through the bridge, not the still-disabled Radar chat
route or the complete service namespaces. No credential output was exposed.

**First site inference:** used the owner's authenticated Radar tab to ask a
read-only creator-history/refusal question through `/ask`. It reached the CLI,
then timed out at 90 seconds. The direct successful diagnostic drained both
streams, while Rust waited for exit before reading them. A pipe-filling child
therefore blocked. The caller now drains stdout and discards stderr concurrently
while retaining the deadline. Seventy model tests and scoped clippy pass;
restoring the old function fails the subprocess regression at five seconds.
CI 37559378293 and release 37559374718 passed at
`3e786a26e16bce2107af54ed696fa435b42cf538`. Verified artifact and staged VPS
SHA-256 `32a08fae7309ce93335a18cfd729c6fe194fe7d7e94bb764488920b3b795d439`,
deployed through the fixed script, and verified public health reports that build.
Repeating the authenticated question produced a site answer in about 30 seconds.
The UI reported no consulted sources; this proves the inference path only.
The nominal usage ledger recorded two calls, with no daily-cap refusals.

**Creator evidence resource repair:** a specific-address follow-up stalled
before inference while retaining all creators' launch rows. MemoryCurrent reached
805044224 bytes under the unchanged 805306368-byte cap. Restored service with
the fixed deploy script. A new decoder predicate retains only admitted matching
launch rows, used by both creator instruments. Scoped store/instrument tests and
clippy pass; bypassing the predicate fails its new watermark regression.
Launch repair `8d3a91d4303f800036c15d4d695fbf3099f248b8` passed all jobs
in CI 37561215606 and release 37561211248. Verified artifact and staged VPS
SHA-256 `c68a18e0af297f4f70f5c8ad314fff12573dbd31196ee86a3072d602ce3d13c4`,
then fixed-deployed and verified public health. The live repeat kept launch
scans below 458285056 bytes, then full outcome retention reached the 805306368
cap before inference. Restored service through fixed deployment. Matching outcome
reads now retain only measurements for the selected creator's recorded mints;
watermark gating and malformed-row errors remain. Outcome repair `efa6c74dce486f9d07e1cedbbf209a159a1de497` passed all jobs
in CI 37562058437 and release 37562054563. Verified artifact and staged VPS
SHA-256 `93bd4c5769138655913ebc8bd6c4c3ffc8984f03d0549bc7e5b95cbd9355b618`,
then fixed-deployed and verified health. The complete query stayed below the cap,
MemoryPeak 674140160 and process RSS about 289000 KiB, but took minutes and the
browser rendered neither answer nor error. The nominal ledger recorded one
additional call; last_call ok alone is not proof of successful site inference.
Creator-column selection now avoids decoding unrelated launch envelopes.
Selected-row decoding still validates matching records and enforces the watermark;
unrelated event payloads are outside this narrower query. Code
`2ddbc25decf931f97075de41ae683629f17cf7bb` passed every CI job in
37563010238 and release 37563005859. Verified artifact and staged VPS SHA-256
`78f7e7720a339fef808f4cbb26e55cb3070968de4cd8443fe6b67de9d7b7bff7`,
then fixed-deployed and verified public health reports that exact build.
The authenticated same-address question now renders an answer with both actual
creator_history and creator_track_record citations, at watermark 454092002.
It states that the supplied address is a mint, not a known creator, and missing
creator rows do not establish the real creator's history. The result completed
in roughly 90 seconds; that is still slow, not an indexed-query claim. Saved the
rendered answer as desktop screenshot evidence. The nominal ledger rose from
30000 to 50000, with zero daily-cap refusals. Final service MemoryPeak 736034816
remained below unchanged MemoryMax 805306368; User guardian, ProtectHome
read-only and NoNewPrivileges yes remain. No wallet action or trade occurred. Privy's owner
wallet dashboard was inspected read-only and reports No wallets yet; the app ID
alone does not establish a wallet or delegation.

**Stopped at:** unlimited subscription inference and the complete source-backed
site question are verified at `2ddbc25`. CLI pipe, creator retention and scan
repairs passed complete CI and verified releases. Private server hardening is
intact. Privy wallet inventory is empty; no autonomous trading loop is enabled.
**Next action:** settle the pending wallet-control scope, record the decision
in design 0017, then build the Privy owner login, dedicated embedded Solana
wallet, live balance and editable risk controls. Follow with the separate signer
and deterministic kernel integration. Numerical capital, trade, loss and session
bounds must come from the owner's saved controls before trading activation.
**Do not:** claim a wallet is delegated from an app ID, treat connected ChatGPT
as capital authority, expose the private site, copy existing subscription
credentials between hosts, invent money limits, or turn on automated trading.

**Owner interface request:** Josh asked for on-site editable capital, per-trade
and loss controls, a live wallet balance, and an option for ChatGPT to decide
what to do with the wallet. Recommendation given: autonomous trade selection
and sizing within owner-saved limits, with limit changes under owner control.
Josh subsequently said to continue. The interface increment is recorded in
design 0017: explicit owner Privy sign-in and Solana wallet creation, verified
live balance reads, and durable owner-entered draft limits with an autonomous
selection preference. Only the owner changes limits; the preference supplies
no kernel authorization. No numerical capital mandate was supplied.

**Wallet interface work in progress:** private `/automation` child endpoints
require the existing operator Access check plus a verified Privy access token.
The embedded wallet is fetched from Privy for that token's DID, never from a
request parameter. Draft settings are scoped to identity and wallet, stored
atomically under RADAR_STATE_DIR, and corrupt/unreadable settings are refused.
Balances reuse the existing all-or-nothing, rate-limited RPC reader. The private
page lazy-loads the Privy SDK; both EVM and Solana automatic wallet creation are
off. The owner must press Create Solana wallet. Draft controls are blank until
the owner enters them and cannot enable execution. No signing API was added.
Local validation passes: 318 serve unit tests, seven customer guard integration
tests, 33 conformance tests, scoped serve clippy and formatting, frontend
type-check/build and 349 web tests. Reversing the trade/capital comparison fails
its regression; reversing the balance/address comparison fails two UI tests.
Both fixes were restored. A full local serve integration build was stopped when
linking many targets delayed the workstation; CI owns the full workspace gate.
The SDK adds 732 dependencies and a roughly 445 kB gzip lazy private-page chunk.
The audit reports 26 moderate findings, zero high/critical after ws 8.21.0
overrides. npm 10 clean installation passed before the final nested ws patch;
its final lockfile dry-run passes. CI will prove the patched clean installation.
Numerical bounds remain unset and no signing or trading was tested.

CI 37566015723 at feddf7d passes web (including the final patched clean install),
workspace build/tests and MSRV. Linux lint reports a large Result error variant
not diagnosed by the Windows compiler: use a small status/message refusal type
and construct the HTTP response at the handler boundary. Release 37566017065
passed, but is held and will not be deployed because the lint repair needs a
fresh verified release. Await the remaining mutation shards before pushing.
Shard 1 reports a survivor at automation.rs:23:23, deleting the nonblank storage
check. Re-applied that exact predicate mutation and extracted configuration
opening behind an explicit-value seam. Its new writable/blank/missing/blocked
directory test fails with the reported mutation. The predicate is restored;
all five wallet unit tests and scoped clippy/formatting pass. Shards 0, 2 and 3
finished successfully; the run failed only for the lint issue and named survivor.
The local target directory is 25.4 GiB and no cargo process remains. Push both
repairs and verify complete CI and a fresh release before deployment.

CI 37566983148 and release 37566979755 both completed successfully at e69f67d,
including all four mutation shards. The verified artifact was staged but not
deployed. A final SDK readiness review added the Solana `useWallets().ready`
gate before owner-triggered creation, matching Privy's current Solana guide.
The new regression fails when the disabled readiness gate is removed; restored,
all seven private wallet tests and the frontend production build pass. A fresh
release containing this guard is required before deployment.

### Wallet interface handback — 2026-10-07 UTC

**Verified deployment:** `9ecfd70991e116ea03b0b292dd5fd207966c7585`.
CI 37568238443 completed successfully, including all four mutation shards;
release 37568234147 completed successfully and includes 350 passing web tests.
Downloaded BUILD-INFO.txt names the exact commit. radar-serve SHA256
`fa86d907459e1bc56d86d0f0e38d6529943b6e5375db41a215b17a439973ab1c`
matches the manifest, local file and staged VPS file. Ran the fixed
`ssh guardian-vps-tail 'sudo radar-deploy'` procedure; public `/health` reports
that exact build and status ok. `systemctl show` confirms active, User guardian,
MemoryMax 805306368, ProtectHome read-only and NoNewPrivileges yes.
Anonymous wallet, balance and limits URLs each return Cloudflare 302.

**Live interface proof:** reloaded the owner's authenticated private setup tab,
waited for real SDK initialization and clicked Sign in with Privy. Its email
login dialog renders correctly under the deployed CSP. Left it open for the
owner and saved screenshot radar-privy-setup.png in this task's visualization
directory. No email was submitted, wallet created, funds moved or delegation
granted. A direct browser navigation to the wallet JSON endpoint was blocked by
the browser client; did not bypass or retry it. The extra Privy JWT requirement
is proven by integration tests, not by an authenticated live API probe.

**Stopped at:** this interface increment is deployed. Balance and settings
handlers pass their tests; an actual owner wallet balance and saved production
settings remain unverified because there is no verified owner wallet yet.
Preferences remain drafts and Policy stays closed. No autonomous trade ran.

**Next action:** owner completes Privy sign-in and presses Create Solana wallet,
then enters capital, maximum per trade and daily loss values and saves the
autonomous-selection preference. Verify these reads and persistence against
that wallet. Follow with the separate signer, bounded delegation, deterministic
policy loading and execution worker; asset/session constraints must be resolved
before activation. Do not treat the saved checkbox as capital authority.

**Separate follow-up:** after restart, the ChatGPT widget still says Connect
because it displays the current linking flow, not persisted CLI credential
status. This does not undo the previously verified subscription link. Avoid
asking the owner to relink solely because of that label. No local build process
is left running; target was last measured at 25.4 GiB.

### Created wallet recognition repair — 2026-10-07

Owner pressed Create Solana wallet; Radar still said No embedded Solana wallet
yet. The owner confirms Privy's dashboard contains Solana wallet
`o4ppw6fo6qyhc2rhszktx69r`, created October 7. Thus creation succeeded; the
absence label is not a reliable account-creation result. Exact linked-account
response remains uncaptured. Browser inspection was denied by URL policy;
did not retry through another tab or browser. SSH requires a fresh Tailscale
admin check before remote inspection or deployment.

Found an independently reproducible parser defect: the installed Privy SDK's
BaseResponseWalletAccount permits `id: string | null`, but Radar required a
string and collapsed both missing IDs and malformed linked-account responses
into NoWallet. The reader now retains verified embedded Solana addresses with
optional server IDs; the private interface accepts null IDs without recreating
the wallet. Unknown malformed records are errors rather than absent wallets.
No signing caller was added, and no wallet ID or delegation is invented.
Draft preferences stay scoped to DID, optional wallet ID and verified address;
assignment of a previously missing ID changes that key and requires fresh
owner settings rather than silently migrating a capital mandate.

The interface also remembers completed SDK creation for the current identity
and offers refresh rather than another creation while the backend is absent.
Local checks pass: 12 wallet-reader integration tests, five private settings
unit tests, serve clippy, nine private wallet UI tests and frontend type-check.
Requiring the server ID again fails the nullable-ID backend regression; restoring
the old UI ID requirement and cleared creation state fails both new UI tests.
Restored code passes. Full CI/release/deployment remain pending; production
still runs 9ecfd70. Tailscale authentication subsequently succeeded. A sanitized
read-only helper was staged at /tmp/radar-privy-read-check.py; the owner must
execute it with sudo to inspect the exact supplied wallet's linked-account
fields without exposing credentials or email. Also requested one ordinary
Refresh wallet check to distinguish delayed recognition before deployment.

**Repair handback:** parser and duplicate-creation fixes are locally verified.
Do not claim the nullable-ID defect caused this owner's symptom until its actual
response is known. Next: inspect the owner's refresh/diagnostic answer, adjust
only if that evidence requires it, then run full CI and a verified release and
deploy through the fixed script. Never create another wallet to test recognition.

### Owner refresh result — 2026-10-07

Owner now reports the private page displays wallet
`JAnZAdjZjmgpr9YZ6i9eViQbbKA4VgQKPfYAQTSeFAcS`, native balance
0.000000000 SOL, slot 454242629, snapshot age at retrieval 0 seconds, no token
holdings, and the three USD limit inputs. The recognition issue resolved before
deployment of the nullable-ID/creation-state repair. Thus that repair cannot be
credited with fixing this owner's report; delayed recognition is plausible but
the exact propagation cause is unmeasured. The root diagnostic is no longer
required for this working wallet setup.

CI 37632513225 and release 37632504923 remain in progress at
`3f10f62aaaa43b5fa70e1e2f2850adaf9f2354ba`. Keep them intact; do not push this
documentation update while waiting on those runs. Production is still 9ecfd70.

**Handback:** owner reports a live verified address and fresh empty holdings;
production settings persistence remains untested until the owner enters limits
and saves them. No capital values or trade mandate were supplied. Owner next
enters capital, maximum per trade and daily loss values, and may select the
ChatGPT autonomy preference. These remain drafts and execution stays inactive.
Next code step is to inspect complete CI/release results and deploy the verified
recognition guard artifact. The separate signer and execution worker still need
integration. No wallet creation, signing or funding action was taken by the agent.


### Privy-only signer preparation — 2026-10-07

Owner requested continued movement toward autonomous trading. Starting from
`f230fa3`, this increment removes the otherwise mandatory local Solana key from
an explicit Privy-only mode, and binds Privy signing requests to startup-configured
app ID, wallet ID and Solana address. The existing executor composition caller
migrates to the bound library API. No key, delegation, policy allowance or worker
is installed on the server as part of this increment.

- [x] Add the trusted wallet scope and reject destination, app, HTTP method,
  RPC operation, encoding and chain substitution before using the key.
- [x] Add `RADAR_SIGNER_MODE=privy`: no local key load and no local signing.
- [x] Preserve the shipped closed policy, mandatory policy file and allowlist.
- [x] Update signer deployment example and repair its incomplete closed-policy
  JSON (all required fields, lowercase `observe`).
- [x] Scoped tests: `cargo test -p radar-signer` passed 101 tests; executor
  `the_customer_lane_composes` passed seven; all-target signer/executor Clippy
  passed with warnings denied on the Windows GNU LLVM toolchain.
- [x] `cargo mutants -f crates/radar-signer/src/privy.rs -p radar-signer
  --test-package radar-signer --in-place`: 22 mutants, 20 caught, two unviable,
  no survivors. Removing the process wallet-address comparison makes
  `privy_only_needs_no_local_key_and_refuses_local_signing` fail with an unwanted
  Authorised result; restored the guard. Startup/environment plumbing was
  exercised by real-process tests rather than redundant line mutations.
- [x] Code commit `3f38ee0ffaf2d1e349f54b43475f36c010bb335d`: full CI
  37634849379 passed, including all four mutation shards, 2,381 Rust tests and
  352 web tests. Release 37634890508 passed at the same commit. Verified manifest
  SHA-256 `2baf676b184063ffbfa9dc6dfa207879aaa65ed7921ab22a38c3aeae246ab195`,
  uploaded only radar-serve to `/tmp/radar-serve.new`, and ran the fixed
  `sudo radar-deploy`. Installed binary hash matches; loopback `/health` reports
  that exact build, status ok and policyClosed true.

Previous CI 37632513225 completed: all four mutation shards, tests/build/MSRV/web
and other checks passed; lint failed on an unnecessary raw-string marker in
`the_wallet_read_is_never_permission.rs`. That literal is corrected in this
increment. Release 37632504923 passed but is not deployed from a failed-CI commit.

Remaining before live autonomous trading: owner-saved capital/per-trade/daily-loss
mandate, authenticated kernel issuance and replay/expiry control, trusted state
and durable reservations/loss accounting, simulation and persistent submission /
confirmation reconciliation, separate signer deployment, independently verified
Privy policy refusals and owner delegation. The site preference is a draft;
ChatGPT may propose, and the deterministic kernel must approve.

**Handback:** wallet-recognition and pending-creation guards are deployed;
the owner had already reported a working wallet before this deployment, so its
earlier recovery is not attributed to these guards. Privy-only signer mode and
trusted wallet request binding are implemented and verified in the repository;
the released signer binary remains uninstalled. On-host `systemctl is-active
radar-signer.socket` returned inactive. Service User guardian, ProtectHome
read-only, NoNewPrivileges yes and MemoryMax 805306368 remain intact. The browser
tab is still unavailable under the browser URL policy; no new UI capture was
made. Owner may save limits through the existing site inputs. The next code
increment is authenticated issuance, replay/expiry control and durable capital /
submission accounting before delegation or worker activation. No real wallet
creation, delegated signing, funding or trade was performed by the agent.

### Privy single-attempt authorization state — 2026-10-07

Owner requested continuation. Starting from `4e99859`, implement the next P6
control at its existing caller, the signer's Privy stdin handler. Require an
existing persistent private nonce directory; consume a SHA-256-named marker
atomically and sync it before key use. This prevents reuse across processes and
restarts. Failed attempts stay consumed. Scope this increment to replay state;
do not describe it as issuer authentication, trusted expiry, enforceable site
limits or complete transaction accounting.

- [x] Add the replay store and mandatory Privy process configuration.
- [x] Preserve local-only startup without Privy state; permit only the protected
  nonce directory in the unit's filesystem write allowance.
- [x] Real-process tests cover restart, modified requests, concurrent processes,
  interrupted/rejected attempts, blank/path-shaped nonces and lost state.
  Unix CI also checks private directory permissions.
- [x] Replacing exclusive marker creation with overwrite/truncate makes the
  real-process restart regression fail with a second Authorised response.
  Restored exclusive creation and the regression passed.
- [x] Scoped signer tests passed (107 on Windows), all-target signer Clippy
  passed with warnings denied, and all 33 repository conformance tests passed.
  Fixed the context-file line budget by keeping the updated guarantee concise.
- [x] Code commit `225308a5a8c0e6d918992b7d42a10a835e3d79fd`: full CI
  37639125067 passed, including 2,388 Rust tests, 352 web tests and all four
  mutation shards. Unix private-directory refusal passed in that run. A manual
  wallet-guard removal also fails its regression with fresh test nonces, proving
  the new replay guard does not mask the older wallet-binding check; restored
  and verified the correct behavior.

No production signer configuration or delegation is changed. The site stays
at `3f38ee0` and the shipped policy stays closed. No new site artifact is needed
for this signer-only increment. Next: authenticate issuance and expiry and connect
the owner mandate to durable reservation/submission accounting before enabling
any signer or worker.

**Handback:** replay state is implemented and verified in the repository. No
new release/deploy was needed for this signer-only increment. On-host health
still reports build `3f38ee0`, status ok and policyClosed true; signer socket is
inactive. Saved numeric limits and wallet delegation remain unverified; do not
invent them. Remaining live controls include authenticated issuance, independent
expiry, durable portfolio/reservation/loss state, simulation and submission /
confirmation reconciliation, independent Privy policy refusal tests, and owner
delegation. No real wallet signing, funding or trade was performed. Local target
was measured at 25.9 GiB with about 122 GiB free. Automatic approval review
rejected cleanup of the generated `mutants.out` directory as "blocked by policy"
without a more specific reason; it remains ignored, and no deletion workaround
was attempted.

### Privy issuer proof and independent expiry — 2026-10-07

Owner requested continuation from `2286891`. The concrete caller is the existing
Privy stdin process handler. Add mandatory issuer proof verification and a
signed time window checked against the process's host clock. A startup public
key and positive maximum lifetime are required without defaults. The proof
binds the entire authorization, request, wallet and caller bounds under a v1
domain. No new crate, issuer private key or production financial values are added.

This is the receiving boundary, **not an implemented kernel/reservation issuer**.
That issuer must remain isolated from Serve/model/executor and derive decisions
from trusted state; otherwise a proof only relocates caller authority. Existing
library signing and local lane do not acquire these process guards. Host clock
rollback and HTTP resubmission of returned signatures remain separate concerns.

- [x] Required wire proof and startup issuer/lifetime config, exact transcript,
  strict signature check and host-clock expiry before nonce claim and key use.
- [x] Existing wallet/nonce/transaction regressions explicitly issue valid test
  proofs after mutations, so the new guard cannot mask their refusal behavior.
- [x] Seven new regressions: modified intent, wrong issuer/domain, incomplete proof,
  process clock/lifetime limits, exact/u64 boundaries, invalid curve key, non-exact payload.
- [x] Scoped signer tests passed 114 tests on Windows; all-target signer Clippy
  passed with warnings denied. Production policy remains closed, signer inactive.
- [x] Removing signature verification permits a forged fresh nonce and fails the
  real-process tampering regression. Removing expiry permits an old intent even
  with a caller slot of zero and fails the real-process clock regression.
  Restored both guards: scoped tests and all 33 conformance tests passed.
- [x] Code commit `7899833fa5787a86692398fe9b0730ac764b84a1`: full CI
  37642745249 passed, including 2,395 Rust tests, 352 web tests, all four
  mutation shards and their final gate. Linux signer tests passed as well.

**Handback:** the receiving boundary is implemented and verified in the repo;
do not deploy or enable a signer. Production stays at
Serve `3f38ee0`; no issuer or live time cap has been provisioned. Next integrate
trusted mandate/portfolio reservations with isolated issuance and reconciliation.
Latest loopback health confirms status ok, policyClosed true and build `3f38ee0`;
signer socket is inactive. Prior documentation CI 37641528789 completed with
overall conclusion failure, although all 13 listed jobs (including all four
mutation shards) report success and `gh run view --log-failed` returned no log.
The aggregate inconsistency is unresolved, not described as a clean CI pass.
The subsequent code commit's full CI passed, as recorded above. PR 334 describes
the actual guarantees and remaining gates. No real signing, delegation or trade
has been performed. The earlier rejected cleanup is not retried. Target remains
25.9 GiB with 122 GiB free; no local Cargo process remains.

Next-step inspection: reuse `radar-types::Portfolio` and
`radar-journal::OperationLog` rather than inventing another reservation ledger.
The latter already has reserve/submit/rehold and refuses to free unknown
submissions as failures; its existing runtime caller is CLI `consider`, not the
private trader. `Journal::open` does not acquire a cross-process writer lock.
An isolated authority needs exclusive ownership of that state, trusted wallet
asset snapshots and conservative USD valuation, mandate version/revocation,
and confirmed settlement reconciliation. The site's `automation::Preferences`
remains a Serve-owned draft; do not load it as autonomous authority unchanged.

### Exclusive intact reservation history — 2026-10-07

Owner requested continuation from `8c60a42`. Before an isolated approval process
can use existing reservations, close two inspected gaps in its actual
`OperationLog::open` path (existing runtime caller: CLI `consider`): concurrent
processes can replay the same balance/claims, and replay accepted history
without verifying its hash chain. This increment owns that prerequisite, not
the entire issuer or trusted mandate/valuation adapter.

- [x] Hold a nonblocking OS lock on the persistent journal sidecar before
  opening/verification/replay, until the log is dropped or process exits.
- [x] Refuse broken or torn operation history without repair. Keep read-only
  generic journal audit available and preserve duplicate-transition idempotence
  in a valid chain; raw repeated lines are corruption.
- [x] Four regressions cover competing instances and directory aliases,
  actual child-process death/reheld capital, corrupted/torn/repeated history,
  and failed lock provisioning. The ignored child fixture is run explicitly by
  the real-process regression, not counted as an independently passing test.
- [x] Removing the OS lock allows a second owner and fails its regression.
  Skipping integrity verification accepts damaged history and fails its
  regression. Restored both guards and reran the scoped journal suite: 31 tests
  passed (plus the explicitly invoked child fixture), all-target journal Clippy
  passed with warnings denied, and 33 conformance tests passed.
- [x] Full CI 37648838566 passed at `0dd8e4dc1f3e02bddd03d6759f5a01da882871d1`:
  2,400 Rust tests, web/site checks, all four mutation shards and final gate.

CI 37646978006 exposed two missed test assumptions: CLI inventory fixtures used
the production journal path without its prepared directory, and the issuer
tampering test's second clock read could reproduce the original expiry across
a second boundary. Inventory now takes an explicit history path (the runtime
still supplies the fixed production path); each fixture owns a temporary
directory. A caller regression refuses both an owned history and a missing
state directory. Tampering derives changed timestamps from the signed original,
so every asserted modification is a modification. No production guard is relaxed.
Local repair checks passed 211 CLI tests, 114 signer tests and all-target Clippy
for both crates with warnings denied. Reapplying an unchanged proof expiry
reproduces the CI assertion failure; restored timestamp modification passes.
All 33 conformance tests passed after indexing the recorded learning. The first
run completed before the repair push: shards 0 and 2 failed their unmutated
baselines for the two recorded issues, while shards 1 and 3 passed. No shard
was cancelled, no mutation exclusion was added, and the fresh run passed fully.

**Handback:** the reservation-history prerequisite and its caller fixes are
implemented and verified in PR 334. Production remains unchanged, policy closed, signer
inactive. No wallet key, issuer key, delegation or financial cap is provisioned.
This lock coordinates cooperating users of one history path, not hostile writes
or different journals for the same wallet. Protected stable paths and a trusted
checkpoint remain needed for rollback detection. The next layer must connect
an activated mandate and trusted asset valuations to reserved kernel decisions.
Latest loopback health is ok at Serve `3f38ee0`, policyClosed true; signer socket
is inactive. No trade or real signing was performed. Target is 26.8 GiB with
121.1 GiB free and no local Cargo process remaining. Previously rejected ignored
mutation-output cleanup was not retried. The verification follow-up edits only
this handback; the behavior commit's completed full CI is recorded above.

### Offline isolated issuer — 2026-10-07

Owner requested continuation from `bccb92d`. Build a real operator-facing
`radar-issuer` binary in the existing signer package, called over stdin/stdout;
no unused framework crate and no claim that the autonomous worker is connected.
The binary's evidence comes from private operator-provisioned files, not Serve
drafts or caller-supplied portfolio/price/exit/cost assertions. This is the
issuance/reservation process before the live trusted snapshot adapter.

- [x] Wallet-bound active policy, expiry and mandatory existing state/key paths.
  Regular/private Unix files; no network, wallet key or Privy key in the issuer.
  Reject changed mandates, missing state and stale/future/wrong-wallet evidence.
- [x] Match proposal and exact transaction against independently provisioned
  evidence, evaluate the existing kernel, narrow slot expiry and convert USD to
  lamports conservatively with integer arithmetic. Require fee coverage.
- [x] Persist the capital claim and SubmissionUnknown before signing a v1
  intent proof. Retain claims across output errors and restart; refuse all new
  issuance while an operation is outstanding. Correlate nonce and mint.
- [x] Nine actual-process regressions plus a deterministic time-boundary test,
  including broken-output persistence. Scoped signer suite passes 124 Windows
  tests; no real keys or transactions.
- [x] Removing the outstanding-operation guard permits a second issuance and
  fails its process regression. Removing the evidence match permits an invented
  creator and fails its process regression. Both guards restored.
- [x] All-target signer Clippy passed with warnings denied; all 33 conformance
  tests passed. No broad local mutation or release build.
- [x] Full CI 37655321913 passed at `6b05ae4eada70cb9bd019f8d6d109b9a0cb78195`:
  2,410 Rust tests, 352 web tests, site/build/lint/MSRV checks, all four mutation
  shards and the final gate.

First issuer CI 37652941215 exposed seven missing observations (exact file/input-size
and current-slot boundaries and nonce/mint correlation) plus an equivalent
operator-signature guard. Added the observations; reapplying all seven exact
reported mutations fails their tests. The duplicate guard is removed: startup
already permits only self-authorising policies. Linux's Rust 1.99.0 also caught
three assertion-style lints absent under local 1.97.1; fixed without suppression.
Restored source passes 124 signer tests and all-target scoped Clippy. No mutation
exclusion or local broad mutation run was used.

**Handback:** the offline issuer and CI repairs are implemented and verified in
PR 334. Do not deploy keys or enable live
delegation. Next replace operator-provisioned evidence with an independently
measured wallet/market adapter, activate site mandates outside Serve's write
authority, and implement confirmation/loss reconciliation before lifting the
single-flight restriction. Missing operator financial, fee, freshness and
expiry settings have no live defaults. Protected directories, identity
separation and checkpoints remain deployment prerequisites. Production stays
at the last verified Serve `3f38ee0`, closed policy, inactive signer. The final
SSH health recheck timed out on 2026-10-07; current VPS status is unverified.
No deployment, real issuer key, delegation, signature or trade occurred in
this increment. No local Cargo or issuer process remains; target is 27.2 GiB
with 120.5 GiB free. The previously rejected ignored mutation-output cleanup
was not retried. This follow-up changes only the verification handback; its
own CI may be pending, while the code commit above passed fully.

### Direct operator wallet reads — 2026-10-07

Owner requested continuation from `7761fd0`. The first live-input piece is the
actual `radar wallet-read --wallet <address> --rpc <URL>` operator command in
`crates/radar-cli/src/wallet_read.rs`. It reuses the existing read-only RPC client;
no new framework crate, Serve cache or signer dependency is introduced.

- [x] Require a valid explicit wallet and nonblank explicit RPC endpoint. Spend
  at most three RPC calls under the existing bounded-read budget shape.
- [x] Read native SOL, SPL and Token-2022 with explicit finalized commitment.
  Both RPC methods also serve the existing positions display; they now request
  finality explicitly. Their balance/owner/quantity parsing guards remain.
- [x] Refuse the entire command for failed/malformed reads or missing token
  context. No partial JSON and no raw provider error or credential-bearing URL.
- [x] Emit integer quantities as decimal strings, preserving token accounts and
  node-reported decimals separately. Never aggregate incompatible accounts or
  invent dollar value, realised P&L, deployed exposure or empty risk state.
- [x] Preserve every leg's context slot. `common_reported_slot` is populated
  only when all three reported slots match; it proves no common bank hash or
  atomic cross-request snapshot. Preserve host read start/completion times.
- [x] Scoped CLI suite passed 217 tests; scoped onchain suite passed. All-target
  Clippy for both packages passed with warnings denied. The CLI fixture's lint
  findings were corrected without suppressions; its process tests were rerun.
- [x] Replacing the common-slot conjunction with an OR fails the mixed-slot
  regression. Rounding token quantities through f64 fails the exact-quantity
  regression (u64::MAX becomes 18446744073709551616). Both changes restored.
- [x] Full CI 37671411086 passed at `744d949246a05ff1924c2355e6f6e3690b0ed4aa`:
  2,416 Rust tests, 352 web tests, site/build/lint/MSRV checks, all four mutation
  shards and the final gate.

The RPC parameters follow [getBalance](https://solana.com/docs/rpc/http/getbalance)
and [getTokenAccountsByOwner](https://solana.com/docs/rpc/http/gettokenaccountsbyowner).
This is node-reported evidence, not independent cryptographic verification of
RPC truth, recent chain head, network identity or the absence of unsupported
holdings. An explicit endpoint can still be untrustworthy. Operator setup must
bind it and the wallet outside the untrusted caller's control before this feeds
a live issuer. Output is deliberately not the offline issuer's full Snapshot;
there is no path that substitutes these reads for market capacity, fees, price,
portfolio exposure or loss accounting. No issuer key or delegated signing is
activated. The command's read deadline is not a financial policy default.

**Handback:** direct wallet measurement is implemented and verified in PR 334.
Remaining work includes
trusted endpoint/wallet provisioning, measured market and transaction evidence,
active mandates, settlement/loss reconciliation and independently tested Privy
refusals. Trading remains inactive; no deployment or real signing in this change.

Live command verification used the owner's previously supplied wallet and
`https://api.mainnet-beta.solana.com`, read-only, at Unix time 1791399631. The
node reported 0 native lamports at slot 454310379, no SPL token accounts at
454310380, and no Token-2022 accounts at 454310381. The command correctly emitted
null for common_reported_slot, USD value and realised P&L. These are those three
node responses, not a current valuation or proof of no other asset types.

Inspection for the next adapter: `crates/radar-onchain/src/dossier.rs` reads the
bonding curve and fee schedule separately and reports buy-within-impact capacity.
That reporting path cannot be relabelled as atomic market evidence or simulated
exit capacity. `crates/radar-sim/src/curve.rs` has a pure fee-adjusted sell quoter;
it still needs correctly owned, slot-bound reserves and fees, and its curve
arithmetic is a price for those reserves, not a real fill. A live issuer must
also bind the exact transaction, fees, wallet inventory, valuation and journal
exposure rather than accepting this wallet-read JSON as complete kernel state.

No production deployment or current VPS health claim was made in this increment.
The prior SSH health check timed out. No real signing, issuer/Privy key setup or
trade occurred. No local Cargo or wallet process remains; target measured 28.7
GiB with 124 GiB free. Previously rejected ignored mutation-output cleanup was
not retried. This follow-up records verification and the inspected next-step
constraints only; the code commit above passed full CI, while follow-up CI may
be pending.

### One-context curve exit measurements — 2026-10-07

Owner requested continuation from `213b555`. The actual operator caller is
`radar curve-exit --mint <address> --raw-tokens <N> --rpc <URL>`. The new
`crates/radar-onchain/src/curve_market.rs` reader derives curve and fee addresses
and requests them with the mint in one finalized getMultipleAccounts call.
No new dependency crate or key-bearing process is introduced.

- [x] Require a shared context, exact account count, presence and program owners.
  Parse existing curve, fee and token layouts; refuse unsupported extensions,
  noncanonical initialization and nonzero unknown trailing fee data. Require
  positive current mint supply no greater than the curve's recorded total;
  holder burns can reduce current supply without changing the original total.
- [x] Preserve creator, decimals, current mint supply and mint-authority fact
  from that read. A current authority tag is not a historical revocation latch.
- [x] Price an explicit hypothetical sell using existing integral curve math.
  Refuse active mint/freeze authority, graduated/unpriceable curves, requested
  quantities above supply and gross proceeds above observed real SOL reserves.
- [x] Require a schedule covering from zero. Bound venue fees by the highest
  total among every observed tier and flat row, without guessing the applicable
  market-cap tier. Round fees up and refuse bounds/rounding consuming the exit.
- [x] Emit raw quantities, gross/fee/net lamports, impact and shared slot as
  strings, with host read times. USD value, searched exit capacity, network fee
  and transaction simulation remain null; wallet ownership is unverified.
- [x] Scoped CLI tests: 219 unit and 4 process tests; onchain: 67 unit and 18 integration tests. Scoped Clippy with warnings denied and workspace fmt passed.
- [x] Manual mutations of owner OR to AND, maximum fee to minimum, and gross reserve > to >= each failed their targeted test; restored sources passed.
- [x] Repo-conformance: all 33 checks passed.
- [x] Full CI 37689898799 passed at d4b84f2df698f838ba05661e0c3a3c3de4fd70f1: 2,426 Rust tests, 352 web tests, all four mutation shards and final gate, build/lint/MSRV/site/fmt/licence/cargo-deny.

The read parameters follow
[getMultipleAccounts](https://solana.com/docs/rpc/http/getmultipleaccounts).
Existing multi-account callers also now request finalized commitment explicitly.
No provider response or credential URL is included in a refusal. Fee bounds
cover only the observed parsed schedule; an admin update or later reserve change
can invalidate the quote. This does not measure total execution costs or provide
an executable minimum receipt. It is not the issuer Snapshot, a largest-exit
search, a portfolio, an authorization or a live transaction simulation.

**Handback (verified code `d4b84f2`):** the market component remains read-only. Next measure
candidate buy/exit economics and exact transaction fee/simulation evidence,
trusted SOL/USD valuation, and journal-derived exposure/loss, then activate
owner mandates outside Serve's write authority and reconcile submissions before
wiring repeated worker execution. No deployment, real keys, delegation or trade.

A read-only mainnet invocation against the previously captured mint
6T1BNshzGAKAHvJ3NZ5n62X2eg5rqqsMipUMZJvLpump returned
`unknown fee trailing data`. The current account carries bytes outside the
known fee layout; no live quote was emitted and the refusal was preserved.
The synthetic process fixture verifies the supported layout only. Investigate
the changed layout with a raw capture before extending parsing.

**Observed fee-layout extension (not yet supported):** getAccountInfo finalized
returned the fee-program-owned account
8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt at slot 454343743,
length 4097 bytes. SHA-256 of decoded bytes:
3504fae20640e15bd441257ab118aa96fb4e99190defc06a01aae26b307bf204.
The first 177 bytes are below; all remaining bytes were zero.

```text
8f3492bbdb7b4c9bfdd3bb8cab341ce0528457f2c3817d3278441963dcd55fed58ba24c999ddac02aa00000000000000005f000000000000001e00000000000000010000000000000000000000000000000000000000000000000000005f000000000000001e00000000000000010000000000000000000000000000000000000000000000000000005f000000000000001e0000000000000000000000000000005f000000000000001e00000000000000
```

The published [fee-program IDL](https://github.com/pump-fun/pump-public-docs/blob/8cda1fa30ea658b20909d8aedf002047119388d2/idl/pump_fees.json)
at pinned commit 8cda1fa lists stable_fee_tiers then exotic_flat_fees after
fee_tiers. Interpreting the capture by that layout gives one stable tier at
threshold zero with LP/protocol/creator 0/95/30 bps, followed by exotic flat
0/95/30 bps. These are measurements of this account, not trading defaults.
The current reader deliberately refuses this extension. Next extend parsing
with this capture and truncation/fee-bound regressions; do not simply drop the
trailing-data refusal or claim a fee bound over ignored rows.

Verification follow-up: full CI is complete and successful at the code hash
above. This follow-up changes only the plan, including the captured fee-layout
finding. `cargo +stable-x86_64-pc-windows-gnullvm test -p repo-conformance
--locked` passed all 33 checks after that finding was added. No source guard was
relaxed for the live refusal; the strict unsupported-layout boundary remains.
Current site/VPS status was not rechecked or deployed. No real key, signature,
delegation or trade was used. No local Cargo/rustc/wallet process remains;
target measured 28.8 GiB with 120.6 GiB free. Previously rejected ignored mutation
output cleanup was not retried. Follow-up documentation CI may be pending.

### Captured fee extension support — 2026-10-07

Continuation from `31c3bea`; previous documentation CI 37692417065 passed.
Actual caller remains `radar curve-exit` through the one-context market reader.
No new dependency or crate, model-to-signer path or production deployment.

- [x] Preserve standard, stable and exotic schedules separately in
  `crates/radar-pumpfun/src/fee_schedule.rs`. Parse observed extension in full,
  bound vector allocation by bytes and reject incomplete or unknown suffixes.
  Historical zero padding remains an absent extension. Existing prefix-only
  FeeConfig callers retain their earlier behavior.
- [x] Include every observed row and both flats in the fee bound. Require coverage
  from zero in standard and observed stable tiers; no guessed classification.
- [x] Archive the decoded 4097-byte account with slot, owner and SHA-256 in
  `crates/radar-pumpfun/tests/fixtures/pumpfun_fee_extension.json`.
- [x] Four parser regressions cover the capture, legacy padding, each truncated
  extension prefix, unknown suffix, oversized vector, all fee components, a high
  stable threshold, larger exotic fees and missing schedule coverage.
  One actual-process test covers captured fees and a larger synthetic exotic fee.
- [x] Scoped pumpfun/onchain/CLI tests and all-target Clippy passed. Omitting exotic
  fees and removing unknown-tail refusal each failed their targeted regression;
  restored parser tests passed. Formatting passed. No mutation exclusions added.
- [x] Repo-conformance: all 33 checks passed.
- [x] Final full CI 37700903314 passed at 9ade385926c35faafe17718571c74131b1582fd2: 2,432 Rust tests, 352 web tests, all mutation shards and gate, build/lint/MSRV/site/fmt/licence/cargo-deny.

Read-only mainnet command against
6T1BNshzGAKAHvJ3NZ5n62X2eg5rqqsMipUMZJvLpump, raw quantity 1000000,
at host time 1791412653 returned finalized slot 454358569, gross 32 lamports,
venue upper fee 1 lamport (125 bps bound), net 31 lamports. These are observed
hypothetical economics for that requested quantity, not owned inventory, an
executable minimum receipt, a recommendation or a trade. USD value, network
costs, simulation and searched capacity remain null. The current mint supply
was below the curve's original total, which the burn-aware supply check allows.

**Handback (verified code `9ade385`):** extension and component-rounding support passed full CI.
Next add exact transaction simulation/cost evidence, valuation and journal loss/
exposure, protected owner mandate activation and settlement reconciliation before
repeated worker execution. Live signing/delegation remains closed. No keys or
signatures were used. No production status was reverified.

CI 37697528963 at f3c422f found a test-assertion lint under Linux Rust 1.99;
local Rust 1.97 had passed. The empty stable vector assertion now compares to
an empty array without changing the predicate; this repeats LEARNINGS 39.
All four parser tests pass after correction. Remaining shards are being allowed
to finish before repair push so no mutation check is cancelled.

Inspected next step: `crates/radar-exec/src/submit.rs` sends and polls but has
no simulation or network-fee measurement method. The offline issuer's
`crates/radar-signer/src/bin/radar-issuer.rs` Snapshot still accepts an
operator-provisioned fee_upper_lamports and exact reviewed transaction.
[simulateTransaction](https://solana.com/docs/rpc/http/simulatetransaction)
allows unsigned transactions when sigVerify is false; keep the exact recent
blockhash rather than enabling replacement, and retain context and explicit
simulation outcome. [getFeeForMessage](https://solana.com/docs/rpc/http/getfeeformessage)
prices the exact serialized message and can return null; null cannot be zero.
Neither proves a later fill. A future read-only command must bind both reads to
the exact transaction bytes and keep rent/account-creation costs distinct from
network fees; model input must not supply a claimed simulation success.

Mutation shard 0 reported fee_schedule.rs:45:13 `/` to `*`. Applying that exact
change left all four focused parser tests passing because later field reads
already reject truncation. Removed the redundant count-capacity arithmetic and
stopped reserving memory from the claimed count: the vector now grows only after
a complete row is read. Oversized counts stop at the first missing row. Parser
regressions and all-target scoped Clippy pass after repair. No excluded mutant
or suppressed lint. LEARNINGS 48 records the repeated redundant-guard pattern.

Initial CI 37697528963 completed: 2,431 Rust and 352 web tests passed;
all build/site/MSRV/fmt/licence/cargo-deny checks passed. Shards 1/2/3 passed;
shard 0 reported only the removed row-calculation survivor, and lint reported
only the corrected empty-vector assertion. Both are repaired locally.
Full repaired-head CI remains required.

A repaired-head Windows process rerun exposed the existing TCP fixture's
accepted socket inheriting listener nonblocking mode: header read failed with
WouldBlock before request bytes arrived. The fixture now explicitly makes that
socket blocking before applying its existing five-second read timeout. The
listener accept loop keeps its deadline. Production HTTP code is unchanged.

### Component rounding correction — 2026-10-07

Before declaring the fee cost conservative, inspected Pump's pinned published
buy formula: protocol and creator fees round separately. That does not prove
the sell implementation, but a conservative hypothetical exit must cover this
rounding. The previous gross-32 live read's fee 1/net 31 can be optimistic by
one lamport. This is a correction to a quoted bound, not to any signed trade.

- [x] Add `FeeSchedule::charge_upper`: separately ceil LP/protocol/creator costs
  in each standard/stable/flat/exotic row, then choose largest cost and clamp to
  gross. Keep maximum total bps as a separate rate ceiling; the largest-rate
  row need not be the largest-cost row. Existing Fees::charge callers unchanged.
- [x] CLI uses that cost bound; exhausted exits refuse and output names component
  rounding. Update exact process quote expectations by one lamport.
- [x] Regression covers rounding, differing row order, all components, zero,
  exact divisions, overflow/exhaustion and missing coverage. Restoring combined
  rounding fails at Some(1) versus Some(2); restored five parser tests, four
  quote unit tests, five actual-process tests and scoped all-target Clippy pass.
- [x] Full final-code CI 37700903314 passed at 9ade385. Repair CI 37699338470
  also passed at 909aa7e before the next push; no checks were cancelled.
  LEARNINGS 49 records the correction and what catches recurrence.

Live trading remains off. No USD valuation, network costs or simulation result
was invented. This schedule bound covers observed parsed fields and rounded
component costs; it cannot guarantee a later fill or unobserved program charges.

**Final handback — verified code 9ade385926c35faafe17718571c74131b1582fd2:**
full CI 37700903314 passed, including 2,432 Rust and 352 web tests, all four
mutation shards/final gate and build/lint/MSRV/site/fmt/licence/cargo-deny.
The complete observed fee layout is now accepted, while unknown suffixes,
truncation and missing coverage refuse. The lamport ceiling evaluates separately
rounded components across every observed row; it does not infer tier selection,
USD valuation, network costs, wallet ownership or future executable proceeds.

After the final conversion change, all 224 CLI tests and five parser regressions
passed locally, along with scoped all-target Clippy, fmt and 33 conformance checks.
Rebuilt final-code mainnet read at host 1791414486, finalized slot 454365388,
returned gross 32, venue fee upper 2, net 30 lamports for explicit raw quantity
1000000 of the previously captured mint. The prior net 31 output is superseded
as a conservative component-rounded bound. No signature or trade was involved.

Next build a read-only exact-transaction simulation/network-fee adapter as
outlined above; retain missing costs as unknown. Then bind independently measured
valuation, portfolio/loss and owner mandates to the issuer, reconcile submitted
operations and verify Privy policy refusals before activating repeated execution.
The site limits remain drafts. No deployment or current VPS health check was made.
No real issuer/Privy key, delegation, signature or trade was created. No local
Cargo/rustc/wallet process remains; target measured 29.3 GiB with 120.1 GiB free.
Previously rejected ignored mutation-output cleanup was not retried. This
follow-up changes verification documentation only; its CI may be pending while
the source head above is fully verified.

### Exact transaction simulation and network fees — 2026-10-07

- [x] Add the actual operator caller `radar transaction-read --transaction
  <binary-file> --min-slot <N> --rpc <URL>`. Bounded binary input, canonical
  single-zero-signature legacy envelope only; no signer dependency or send RPC.
- [x] Simulate the exact bytes at finalized commitment with explicit minimum
  slot, no signature verification and no blockhash replacement. Require an
  explicit null error and reject reported replacement. Price the exact message
  suffix with getFeeForMessage; null/missing fee cannot become zero.
- [x] Preserve full transaction/message base64 for equality binding, separate
  response slots, integer lamports and optional units, host read timestamps.
  Missing units stay null. Both contexts must meet the caller's minimum slot.
  Rent/other instruction costs and USD stay unknown; execution is not guaranteed.
- [x] Five read-unit regressions, one CLI argument regression and three actual
  process regressions cover byte/option preservation, unsupported framing,
  stale/missing contexts, missing/error simulation, unexpected replacement,
  optional/bad units, unknown versus measured-zero fees, budget failure,
  no partial output/provider details and the 1,232/1,233-byte boundary.
  Test floor raised by nine. Existing wallet/curve behavior remains covered.
- [x] Manual unsafe missing-error and null-fee-as-zero changes both failed the
  intended regressions; source restored. Scoped Clippy passes. Full scoped
  test/conformance and complete CI validation are recorded in the handback below.
- [x] Mainnet read-only probe with a zero/unavailable blockhash refused without
  evidence. This is failure-path verification, not a successful funded trade
  simulation. No wallet key, signed transaction or submission was involved.
- [x] Full source CI 37705356170 passed at d311614, including all four mutation
  shards and the final gate. No check was cancelled or skipped.

This adapter only inspects envelope framing locally; the RPC validates message
structure and the independent signer must still decode and authorize content.
The output is operator read evidence, not the issuer's complete live snapshot.
Next bind fresh valuation, exposure/loss, exact-transaction evidence and owner
mandates to the issuer; close settlement reconciliation and Privy policy refusal
checks before repeated execution. Site limits are still drafts and live signing
remains closed. This increment does not require a Serve deployment.

**Handback — verified source d311614c2a51afc756382256570a60ee98dde29f:**
CI 37705356170 passed build/tests/lint/fmt/MSRV/web/site/licence/cargo-deny,
all four mutation shards and the final gate. Totals: 2,441 Rust and 352 web tests.
Local validation: 228 CLI tests, 90 onchain tests, 33 conformance checks, scoped
all-target Clippy and formatting. The two manual unsafe changes failed their
intended regressions; restored source passed. No lint suppression or mutation
exclusion was introduced.

Mainnet RPC confirmed BlockhashNotFound for the zero-blockhash unsigned probe;
the command refused without partial JSON. Positive simulation/fee response and
byte/option binding are verified against controlled RPC, not claimed as a funded
mainnet success. The refreshed owner-wallet read at host 1791418201 found zero
lamports at slot 454379222 and empty SPL/Token-2022 accounts at slots
454379224/454379225. Distinct read slots are retained; this is not an atomic
snapshot or USD valuation.

Next connect these exact-transaction reads to the issuer with protected owner
mandates and measured fresh valuation, exposure/loss and complete cost bounds;
then close settlement reconciliation and verify Privy policy refusals/delegation
before repeated live execution. Wallet funding and activated numeric limits are
still prerequisites. No real key, delegation, signature or trade was created.
No Serve deployment or current VPS health verification was made. Working tree
was clean after the source push; no local Cargo/rustc/wallet process remains.
Target measured 29.4 GiB with 119.2 GiB free. Previously rejected ignored output
cleanup was not retried. This final follow-up records verification only; its CI
may be pending while the source head above is fully verified.

### Bind exact-transaction evidence to offline issuer — 2026-10-07

- [x] Require `transaction_evidence` in the private snapshot, accepting the JSON
  emitted by transaction-read. Candidate stdin cannot provide it. Older snapshots
  refuse. No network or new dependency added to the issuer/signer.
- [x] Consume the existing Checked transaction to compare exact transaction and
  signable-message base64, avoiding a second decoder. Require read-only/finalized
  version-1 metadata, explicit null simulation error and no blockhash replacement.
- [x] Both RPC slots must meet the requested minimum, that minimum cannot predate
  the proposal's oldest input, and neither slot may exceed the decision slot.
  Measured network fee must fit the operator-reviewed full fee upper bound and
  mandate reserve. Optional units and unknown rent/USD fields grant no authority.
- [x] Require an ordered non-future read window, fresh from its start under the
  existing mandate age bound. Proof expiry also follows that start, with checked
  addition; recent completion cannot renew an old read. Refuse before reservation.
- [x] Four new actual-process regressions cover every required field, mismatches,
  slot/fee boundaries, missing versus measured-zero fees, different read slots,
  stale/future/reversed windows, proof expiry and overflow. Existing issuer tests
  use the required evidence and retain their kernel/reservation/replay checks.
  Raise the Rust test floor by four. Scoped signer tests and Clippy pass locally.
- [x] Manual removal of message binding fails the missing-message regression.
  Manual removal of evidence expiry cap fails the proof-lifetime regression;
  both restored before final validation.
- [x] Final local validation: 128 signer tests, 33 conformance checks, scoped
  all-target Clippy and formatting passed on restored source.
- [x] Full initial CI 37708213731 passed at 3bfac0f, including all mutation
  shards; final request-correction CI 37710263172 passed at 90cc0a9.

This is protected operator-file integration, not independently authenticated RPC
provenance. A live collector must still populate wallet/valuation/exposure/loss
and complete costs, and the owner mandate must remain outside Serve's authority.
Settlement reconciliation and verified Privy policy/delegation still precede
repeated live execution. Site limits remain drafts; no live key or trade enabled.

Final request review found the shared base64 decoder's deliberate legacy
leniency (whitespace and suffixes after padding). The new evidence check bound
decoded bytes, while the signing request still forwarded the provisioned string.
The request now serializes from Checked bytes, guaranteeing the canonical read
payload also enters the Privy request and issuer proof. Decoder compatibility
is unchanged. A fifth process regression supplies an equivalent lenient spelling,
requires canonical request output and verifies its attestation. Restoring the
original forwarded spelling fails that regression; source restored. Final local
count is 129 signer tests, with 33 conformance checks, scoped Clippy and fmt.
The test floor gains one more. Await the initial source CI before pushing this
request correction, then verify its final source head without cancelled checks.

**Final handback — verified source 90cc0a945400a0ca16f7ed9ee9e78160fe07a30e:**
CI 37710263172 passed all build/tests/lint/fmt/MSRV/web/site/licence/cargo-deny
checks, all four mutation shards and the final gate. Totals: 2,446 Rust and 352
web tests. Initial issuer CI 37708213731 also passed before the correction push;
the previous documentation CI completed before initial source was pushed.
No check was cancelled. Local final validation: 129 signer tests, 33 conformance
checks, scoped all-target Clippy and formatting. Three manual wrong behaviors
(missing message binding, missing expiry cap, original-string forwarding) each
failed their intended regression and were restored. LEARNINGS 50 records the
encoded-request correction. No new lint suppression or mutation exclusion.

The offline issuer now consumes protected transaction-read evidence before any
reservation/proof. The evidence and canonical signing request bind the same
Checked bytes. This is operator-file trust, not independently authenticated live
RPC provenance. A live collector for wallet/valuation/exposure/loss and complete
costs, protected activated owner mandates, settlement reconciliation and verified
Privy policy/delegation still precede repeated trading. Site settings remain
drafts; funding and active numeric limits were not established this increment.
No real credential or delegation was provisioned, no wallet signature or trade
was produced, and no Serve deployment or current VPS health check was made.

Read-only browser inspection was attempted with the computer-use skill's browser
alternative. Browser URL policy rejected selection of the suspended Privy tab
(chrome-extension protocol); no alternate surface or workaround was used. The
owner was asked to reopen the dashboard normally; no reply had arrived at
handback, and live delegation status remains unverified.

No local Cargo/rustc/radar/issuer/signer process remains. Target measured 29.4 GiB
with 119.1 GiB free. Previously rejected ignored-output cleanup was not retried.
This follow-up changes documentation only; its CI may be pending while the
source head above is fully verified.

### Combined read collection and wallet balance binding (2026-10-07)

The existing offline issuer consumes protected read packets. The operator now
has `radar evidence-read` to collect wallet and transaction evidence together;
its output fields fit the issuer snapshot. No network dependency or model
signing authority is added to the issuer. Complete live snapshot construction
and portfolio reconciliation remain unfinished.

- [x] Require `wallet_evidence` in the private snapshot. Bind read-only/finalized
  version 1 metadata, configured wallet, native decimals and exact reviewed
  integer SOL balance before reservation. Candidate stdin cannot replace it.
- [x] Require native, SPL and Token-2022 contexts within the proposal/decision
  slot window and both token account arrays. Preserve independent contexts;
  do not infer an atomic bank, USD exposure or realised loss from these reads.
- [x] Share ordered, non-future read-window validation with transaction evidence.
  Cap proof lifetime by the wallet read start, not its recent completion.
- [x] Three issuer process regressions cover required fields, identity/balance
  mismatches, malformed quantities, absent holdings reads, slot boundaries,
  independent contexts, stale/future/reversed reads and proof lifetime. Existing
  capital boundary tests now provision matching balance evidence.
- [x] Removing balance equality causes missing quantities to issue incorrectly;
  removing the wallet expiry cap fails the lifetime test by 90 seconds. Both
  wrong behaviors restored. Scoped tests passed: 132 signer tests plus 33
  conformance tests. Rust 1.99 signer Clippy and format checks passed.
- [x] Add operator `radar evidence-read` with an explicit wallet, unsigned file,
  minimum simulation slot and RPC endpoint. Reuse wallet/preflight readers and
  bounded file input under one five-call, twenty-second budget. Emit both
  packets only after every read succeeds; preserve individual slots/windows.
- [x] Two CLI process regressions cover exact quantities/bytes, independent
  contexts/windows and failure at each RPC read with no partial packet or
  provider details. One unit test refuses invalid/missing collection scope
  before file/network reads. Raise the Rust floor by six in total.
- [x] Premature wallet output makes the collector failure test fail when
  simulation refuses. Source restored; all 231 CLI tests passed and the restored
  ten process tests passed again. Scoped CLI Clippy, final formatting and all 33
  conformance checks passed.
- [x] Prior documentation CI 37712131181 passed before the push. Full source CI
  37713716962 passed at 5385aa4, including all four mutation shards and final gate.

The existing Jupiter SOL-to-USDC helper is a point quote without the upper USD
price bound required for live spending. Protected JSON does not authenticate RPC
origin or reconcile token valuations, exposure and losses. Complete protected
snapshot construction, full cost/searched exit evidence, activated owner mandate,
settlement reconciliation and verified Privy policy/delegation remain necessary
before autonomous execution. Site limits remain drafts. No real credential,
delegation, signature, trade or Serve deployment was provisioned this increment.

**Final handback — verified source 5385aa4d31bbb65d268825336dd9858d4f2d6abc:**
CI 37713716962 passed build/tests/lint/fmt/MSRV/web/site/licence/cargo-deny,
all four mutation shards and the final gate. Totals: 2,452 Rust and 352 web
tests. Previous documentation CI 37712131181 completed successfully before
this source push; no check was cancelled. Local checks: 231 CLI tests, 132
signer tests, 33 conformance checks, scoped all-target Clippy and formatting.
Three manual wrong behaviors (omitted balance binding, omitted wallet expiry
cap, premature partial packet output) failed regressions; source restored.
No new lint suppression or mutation exclusion.

The operator collector now produces both read packets consumed by the offline
issuer, and the issuer requires native balance/identity/context/time binding
before reservation. Evidence remains protected-file trust, not authenticated
RPC provenance. This packet is not a complete live snapshot. Next: live USD
bounds and portfolio/settlement accounting, including searched exit capacity
and complete costs, then protected activated mandates and verified Privy
policy/delegation before repeated execution. Site limits remain drafts. No
real credential, delegation, wallet signature, trade or Serve deployment was
provisioned. No current VPS health or live wallet read was performed this turn;
previous live observations must not be treated as current.

No local Cargo/rustc/radar/issuer/signer process remains. Target measured 31.9
GiB with 116.7 GiB free. Previously rejected ignored-output cleanup was not
retried. This handback follow-up changes documentation only; its CI may be
pending while the source above is fully verified.

### Finalized exact-transaction settlement evidence (2026-10-07)

The next caller is the operator `radar settlement-read` command, using a direct
key-free RPC reader in radar-onchain. It supplies historical effects for later
protected accounting; it does not release any outstanding issuer operation.

- [x] Add an explicit wallet/signed-file/minimum-slot/RPC command. Reuse bounded
  file input and host clock, under one RPC-call/twenty-second read budget.
- [x] Require canonical single nonzero-signature legacy framing and writable
  wallet fee payer. Read its account table; no duplicate signer wire decoder or
  network dependency in the signer. Full validity/inclusion remains RPC trust.
- [x] Query getTransaction at finalized commitment in base64; bind exact signed
  bytes, legacy version and transaction slot. Missing/null transactions remain
  unknown. No partial packet or provider detail on read failure.
- [x] Require explicit execution metadata and known fee. Record landed failures
  with their fee; require native balance arrays matching the account count and
  complete token metadata arrays. Reject duplicate/out-of-range token indices,
  missing identities/programs, invalid integer amounts or decimals. Retain raw
  per-account history, optional chain time and host read times; no USD/PnL guess.
- [x] Five reader unit tests, two actual CLI process tests and one command-scope
  test cover success/failure, exact bytes/options, required fields, quantities,
  slot/size/framing boundaries, every truncated prefix, malformed metadata,
  duplicate token entries, unknown transactions and no partial output. Raise
  the test floor by eight. Initial scoped tests passed: 95 onchain and 234 CLI.
- [x] Local scoped all-target Clippy passed after keeping validated signature
  extraction inside the private input parser. No lint suppression added.
- [x] Removing exact-byte equality fails the mismatched-result test. Replacing
  explicit err lookup with missing-as-null lookup fails the required-field test.
  Source restored; all five reader tests and twelve CLI process tests pass.
  Rust 1.99 scoped Clippy/format and all 33 conformance checks passed.
- [x] Inspect the complete staged diffs and push after completed prior CI.
  Full repaired-source CI 37724907667 passed at c1b16c1, including all four
  mutation shards and final gate: 2,460 Rust tests and 352 web tests.

Source CI 37723194570 at 67a6c44 completed every shard: 2,460 Rust tests,
352 web tests and all non-mutation checks passed; shard zero reported two
survivors. The 134-byte accepted boundary was covered only in another crate's
process test, outside the onchain mutation sandbox. Add it to the existing
onchain framing regression; manually applying the exact `<` to `<=` change at
settlement.rs:17:24 now fails that regression. Source restored.

The settlement.rs:23:9 `||` to `&&` change passed all five reader tests when
applied by hand. The zero/high account-count guards are redundant: readonly
unsigned count is always at least zero, and a complete 128-account table cannot
fit the 1,232-byte packet limit. Remove those guards rather than exclude their
mutants. The remaining bounds still reject zero/noncanonical/truncated tables.
No new test, mutation exclusion or lint suppression is added in this repair.

A read-only public mainnet probe found signature
ptNycYpFJ2R571QUNbPK2fXcBXqNL8aC7ojgP6rAm4mDeYsSGy6PPEn5cCitRndvTp2jdRPXMFH2dhi8o282ZB8
at finalized slot 454421787. Its 1,142-byte version-zero envelope was refused by
this command before its own RPC read, as intended for the current legacy lane.
This is live refusal evidence; successful settlement reads are controlled
fixtures. The Solana reference's devnet example signature was unavailable on
public mainnet. No signature/trade was created, no user wallet read or VPS
health/deployment was performed, and no real credential or delegation changed.

The outstanding-operation guard is unchanged. Operation-to-signed-transaction
binding, protected reconciliation and USD exposure/loss accounting are still
needed to permit another issuance. Live price/cost/searched-exit evidence,
activated owner limits and verified Privy policy/delegation remain prerequisites
for autonomous execution. Site limits are still drafts.

**Final handback — verified source c1b16c190455959f870a9b0f076eb135959ace7f:**
CI 37724907667 passed build/tests/lint/fmt/MSRV/web/site/licence/cargo-deny,
all four mutation shards and final gate. Totals: 2,460 Rust and 352 web tests.
The initial source run completed before the repair push; no check was cancelled.
Final local checks passed: 95 onchain and 234 CLI tests, Rust 1.99 scoped
all-target Clippy and formatting. Earlier 33 conformance checks passed, and
full repaired-source CI includes conformance. The exact-byte, absent-status
and minimum-size wrong behaviors fail regressions. Redundant count guards
were removed rather than adding a mutation exclusion. No lint suppression.

The operator can now read finalized historical fees and raw native/token
balance effects bound to exact signed legacy bytes. A missing transaction
remains unknown. Positive reads are controlled fixtures; the live public
version-zero transaction demonstrated refusal only. No user wallet read,
current VPS health check, Serve deployment, credential/delegation change,
wallet signature or trade occurred this increment.

Next: persist the protected operation-to-authorized/signed-transaction binding
and reconcile known finalized effects before releasing claims, with USD
valuation/exposure/loss evidence. Live price bounds, full costs and searched
exit capacity, activated owner limits and verified Privy policy/delegation
remain necessary before autonomous execution. Site limits remain drafts.

No local Cargo/rustc/radar/issuer/signer process remains. Target measured 32.6
GiB with 116.6 GiB free. Previously rejected ignored-output cleanup was not
retried. This final handback follow-up changes documentation only; its CI may
be pending while the source above is fully verified.

### Protected signed-transaction binding (2026-10-08)

The actual caller is the offline operator issuer. Persist its checked canonical
transaction and wallet in the operation's proposal, then accept a signed artifact
only after local signature/message verification. This closes the identity gap
needed by the future settlement reconciler without releasing any claim.

- [x] Add optional execution correlation metadata without changing the digest
  of older events where it is absent. Retain metadata across replay.
- [x] Record signed bytes only in SubmissionUnknown, after a durable append.
  Repeat identical bytes idempotently; reject a conflicting second binding.
  Replay refuses changed identity, authorized bytes or claim metadata.
- [x] Add explicit offline --bind-signed operation/private-file mode. Verify
  configured wallet, exact legacy message, packet bounds, canonical single
  signature framing and strict wallet Ed25519 signature before recording.
- [x] Four new tests cover real issuer processes, restart/idempotence, changed
  signatures and validly signed foreign messages, interrupted writes, malformed
  bindings/replay and packet boundaries. Raise Rust floor from 2063 to 2067.
- [x] Manually bypassing strict signature verification makes the actual-process
  regression fail. Removing exact-message equality also fails on a validly
  re-signed changed message. Restore source after each demonstration.
- [x] Scoped tests/lint/fmt/conformance and complete staged review passed.
  Full source CI 37800981316 passed at 1e2797f, including every mutation shard
  and final gate: 2,465 Rust tests and 352 web tests.

Older operations lacking binding refuse this command; their claims are retained.
The operator's private config/history/files remain the trust boundary, not
authenticated host/RPC provenance. No executor broadcasts, no settlement closes
and no USD exposure/loss is inferred here. Protected settlement accounting,
live price/cost/searched-exit evidence, activated owner limits and verified Privy
policy/delegation remain before autonomous execution. No real wallet/key/Privy
credential, delegation, trade, current wallet read or Serve deployment changed.

Initial source CI 37797820882 at 6267503 completed all shards. Rust tests
(2,464), web tests (352) and other gates passed; shard three reported one
survivor at radar-issuer.rs:490:28, changing the mode guard's `||` to `&&`.
Add a fifth regression requiring the exact command flag and three arguments,
including misspelled flags and too few/many arguments; raise the floor to 2068.
Applying that exact mutation fails the usage-error regression. Source restored;
all 19 issuer process tests and scoped Rust 1.99 Clippy passed after the repair.
No check was cancelled, and no mutation exclusion or lint suppression is added.

**Final handback — verified source 1e2797f786c7fc86d636185a81d67c108089468f:**
CI 37800981316 passed build/tests/lint/fmt/MSRV/web/site/licence/cargo-deny,
all four mutation shards and final gate. Totals: 2,465 Rust and 352 web tests.
Previous documentation CI 37726270806 and initial source CI 37797820882 completed
before subsequent pushes; no check was cancelled. Local verification: 33 journal
tests, 134 signer tests and 33 conformance checks passed before the scope repair;
all 19 issuer process tests and scoped Rust 1.99 Clippy/format passed after it.
Three manual wrong behaviors (signature bypass, exact-message bypass and exact
command-guard mutation) fail regressions; source restored. No suppression or
mutation exclusion was added.

The offline issuer retains protected wallet/authorized bytes before proof output.
Its explicit signed-file command verifies the exact legacy message and wallet
signature, then persists the signed artifact under existing operation ownership.
Replay preserves that binding, while absent/conflicting metadata refuses. Older
operations remain unbound and outstanding. No generic journal method claims to
verify the wallet signature or authenticate its host. No executor broadcasts yet.
Recording signed bytes never releases a reservation or resets portfolio state.

Next: match protected finalized settlement evidence to the recorded artifact,
establish economic effects/fees and reconcile USD valuation, exposure and losses
before allowing another issuance. Live price bounds, complete cost/searched-exit
evidence, activated owner limits and verified Privy policy/delegation remain
necessary for autonomous execution. The new keys/signatures used only temporary
test wallets. No real wallet/key/credential/delegation/trade, live wallet/VPS
health read or Serve deployment occurred. Site limits remain drafts.

No Cargo/rustc/radar/issuer/signer process remains. Target measured 32.9 GiB,
with 114.9 GiB free. Previously rejected ignored-output cleanup was not retried.
This handback follow-up changes documentation only; its CI may be pending while
the source above is fully verified.

### Protected finalized settlement review (2026-10-08)

The caller is the operator issuer's explicit --review-settlement mode, consuming
private settlement-read JSON and the durable authorized/signed binding. Current
settlement semantics cannot close a measured spend below the reservation without
leaving its remainder open. Build the evidence review while retaining claims;
do not bypass that accounting gap or assume USD exposure/loss disappeared.

- [x] Re-verify the recorded artifact's wallet signature/authorized message.
  Bind evidence to outstanding native-SOL operation, wallet, signature, exact
  canonical transaction, finalized slot/context and ordered fresh read times.
- [x] Require native arrays matching the signed account table, complete token
  identities/quantities and distinct in-range indices. Review integer native
  changes, known fees and separate token arrays without guessing USD/PnL.
- [x] Require native-unit reservation and measured fee/net debit within its
  ceiling. Accept exact equality and measured zero fees; preserve native credits
  as historical changes, without labelling them profit.
- [x] Four regressions cover positive/negative integer extremes, required/bad
  fields, slots/times, equality/bounds, token metadata and actual issuer process
  output/refusals. Journal bytes and outstanding claims remain unchanged. Raise
  the Rust floor from 2068 to 2072.
- [x] Removing exact packet-byte binding fails the missing/mismatched evidence
  regression. Removing caller signature verification (with the otherwise-unused
  decoded buffer renamed) fails the actual-process signature refusal regression.
  Restore source after each manual bug demonstration.
- [x] Scoped tests/Clippy/fmt/conformance and complete staged review passed.
  Source 92bde94 passed CI 37807037187, including all four mutation shards
  and final gate: 2,469 Rust and 352 web tests. Verified handback follows.

No live chain/provider call, wallet/key/credential/delegation change, trade or
Serve deployment occurs here. Review is read-only under journal ownership. The
next accounting change must close known completed measured spends correctly,
retain reconciliation evidence and update exposure/loss before allowing another
issuance. Independent live valuation/cost/searched exit evidence, activated
owner limits and verified Privy policy/delegation remain necessary. Site limits
remain drafts and autonomous execution remains off.

### Handback: protected finalized settlement review (2026-10-08)

Source 92bde94cb5b4bb4d8f34d9f3d2cde83d65bb8c2f passed full CI
37807037187 (https://github.com/1xmint/theradar/actions/runs/37807037187):
2,469 Rust and 352 web tests, all four mutation shards and final gate. The
previous documentation run 37803826507 completed successfully before this
source push; no awaited check was cancelled. Local verification passed all
139 signer tests, 33 conformance checks, scoped Rust 1.99 Clippy and format.
The two manually reapplied bugs failed their regressions and were restored.
No new mutation exclusion or lint suppression was added.

The explicit offline issuer review re-verifies the recorded wallet signature
and authorized message, checks protected finalized evidence against the exact
outstanding operation and reports measured historical integer effects/fees.
The actual-process regressions prove successful review and refusals leave the
journal bytes and outstanding claim unchanged. Protected files do not establish
RPC provenance. Token arrays are retained separately; review does not infer
trade notional, missing balances, current holdings, USD value or realised PnL.

Next: add terminal accounting for completed measured spends below a reservation
ceiling, retain reconciliation evidence and account for exposure/loss before
another issuance. Full protected snapshot construction, live valuation/cost and
searched exit evidence, activated owner limits and verified Privy policy and
delegation remain necessary. Autonomous execution is off; site limits are drafts.
No live chain/provider call, real wallet/key/credential/delegation change, trade,
VPS health read or Serve deployment occurred in this increment.

No Cargo/rustc/radar/issuer/signer process remains. Target measured 33.0 GiB,
with 114.8 GiB free. Previously rejected ignored-output cleanup was not retried.
This follow-up changes documentation only; its CI may be pending while the
source above is fully verified.

### Completed measured native spend (2026-10-08)

Add terminal measured-spend vocabulary for the existing journal confirm/reconcile
callers and expose a candidate in the protected issuer review. A partial fill
still holds the remainder; completed spend debits only the measured amount and
releases unused capital. Review itself remains read-only and USD accounting is
not inferred from native balance changes.

- [x] Add Settlement::Completed with matching units and spend at/below the claim.
  Preserve other claims, partial-fill behavior, and known zero/exact ceilings.
- [x] Preflight journal terminal settlement before writing, apply the validated
  portfolio only after successful persistence, and refuse unreheld reservations.
  Replay checks completed spend against unchanged intent/reservation/units.
- [x] Review reports a typed native settlement candidate only for a measured
  debit covering known fees. Credits and debits below the fee remain absent;
  journal bytes and claims remain unchanged, with USD/PnL still unknown.
- [x] All 262 scoped types/journal/signer tests and 33 conformance checks pass.
  Scoped Rust 1.99 Clippy passes. Six new regressions raise the floor to 2078.
  Removing completed remainder release, moving journal writes before preflight,
  rejecting exact fee equality and bypassing replay checks each fail the
  corresponding regression. Restore source after every manual demonstration.
- [x] Format and complete staged review passed. Source 6bcb1f3 passed full
  CI 37813011455: 2,475 Rust and 352 web tests, all four mutation shards
  and final gate. Verified handback follows.

No protected reconcile command is enabled by this increment. Generic journal
callers establish completion; they do not authenticate chain evidence. Next:
retain finalized reconciliation evidence durably and account for exposure/loss
before another issuance. Full protected snapshot construction, live valuation,
cost/searched exit evidence, activated limits and verified Privy policy/delegation
remain necessary. Autonomous execution is off and site settings remain drafts.

### Handback: completed measured spend (2026-10-08)

Source 6bcb1f336832151c2b84c463af96d18bdaa046ec passed full CI
37813011455 (https://github.com/1xmint/theradar/actions/runs/37813011455):
2,475 Rust and 352 web tests, all four mutation shards and final gate. The prior
documentation CI 37810416491 passed before pushing this source; no awaited check
was cancelled. Formatting's job status field remained in_progress despite its
successful conclusion, completion timestamp and completed steps; the overall
workflow completed successfully. Local verification passed 86 types, 36 journal,
140 signer and 33 conformance tests, scoped Rust 1.99 Clippy and format. All four
manually reapplied bugs failed regressions; source restored. No new mutation
exclusion or lint suppression was added.

Completed records a final measured spend, debits only that amount and releases
the unused reservation. PartiallyFilled still retains its remainder. Journal
terminal callers preflight on a portfolio copy before persistence and apply it
only after the write succeeds. Refusals leave history and balances untouched.
Replayed reservations must be reheld before closure. Completed replay checks
unchanged intent/reservation and spend units/ceiling; completed operations do
not rehold or debit fresh balances again. Duplicate completion is idempotent.

The protected review now emits a typed native settlement candidate when a
measured debit covers known fees, including equality and measured zero. Credits
and debits below fees leave it absent. This candidate does not mutate history,
release claims or infer gross spend, USD exposure/loss or realised PnL. Generic
journal callers must establish the outcome themselves. No protected reconcile
command is enabled yet.

Next: retain finalized reconciliation evidence durably, update economic
exposure/loss and prevent issuance from stale accounting. Complete protected
snapshots, live valuation/cost/searched exit evidence, activated owner limits
and verified Privy policy/delegation remain necessary. Autonomous execution
remains off; site settings are drafts. No live chain/provider call, real wallet
signature/key/credential/delegation change, trade, VPS health read or Serve
deployment occurred here.

No Cargo/rustc/radar/issuer/signer process remains. Target measured 33.1 GiB,
with 114.7 GiB free. Previously rejected ignored-output cleanup was not retried.
This follow-up changes documentation only; its CI may be pending while the
source above is fully verified.

### Durable normalized settlement facts and activation path (2026-10-08)

The owner asked how much remains until autonomous activation. Four gates remain;
this is an integration estimate, not a percentage or launch date. Foundation
components exist, but no production executor broadcasts and the issuer still
uses protected operator-provisioned snapshots. Several substantial integrations
remain, rather than a final UI switch.

| Activation gate | Current foundation | Still required |
| --- | --- | --- |
| Durable settlement and economic accounting | Exact signed binding, retained normalized finalized facts, terminal measured native spend | Reconcile USD exposure/loss and prevent stale accounting from authorizing another operation |
| Independent live risk inputs | Wallet, transaction, mint/curve/fee and combined read commands | Build protected live snapshots with conservative prices, current valued inventory, full costs and searched sellable capacity |
| Autonomous execution loop | ChatGPT link, inert proposals, risk kernel, isolated issuer and Privy signer | Connect checked signing, durable broadcast ownership, uncertain-result recovery, settlement, exits and scheduling |
| Activation and live validation | Private wallet UI and draft limits | Activate owner limits, provision isolated keys, verify Privy policy/delegation, confirm funding, deploy and validate a tightly bounded live trade |

This increment's actual caller is the explicit operator issuer recording mode.
It runs the existing review, retains only normalized facts and the signed
artifact in the journal, and leaves the claim SubmissionUnknown. It does not
close a trade, update USD loss, refresh a live snapshot or enable delegation.

- [x] Optional SettlementRecord correlation preserves existing absent-field
  event hashes. Records and getters retain facts across replay and terminal moves.
- [x] Record only for an unknown operation bound to its recorded signed artifact.
  Append before memory/output; identical repeats are idempotent and conflicts
  refuse. Proposal/replay reject wrongly staged or changed facts/operation data.
- [x] Actual --record-settlement shares signature/message/finality review, reads
  once and persists normalized output only. Preserve minimum slot/read window.
  Claims remain outstanding and further issuance remains blocked.
- [x] Five new regressions raise the floor to 2083. Existing actual-process
  checks cover invalid signatures for both modes. New cases cover durable facts,
  write failure, replay, binding/stage/conflict refusal, sole correlation hashes,
  actual recording/idempotence and rejection of unreviewed input-body persistence.
- [x] Reapplying memory-before-append, conflicting replay acceptance and copying
  unreviewed input response fields each fails its regression; restore source.
- [x] All 181 scoped journal/signer tests and 33 conformance checks pass;
  scoped Rust 1.99 Clippy and format pass. Read the complete staged diff.
- [x] Source 7b51c51 passed full CI 37818503009: 2,480 Rust and 352 web
  tests, all four mutation shards and final gate. Verified handback follows.

Next: economic reconciliation of retained evidence and a durable accounting
watermark before another issuance. Unknown USD value/PnL remains unknown. No
live chain/provider call, real signature/key/credential/delegation change, trade,
VPS health read or Serve deployment is performed here. Current funding and live
Privy setup are unverified; site settings remain drafts in the current code path.

### Handback: durable settlement facts and activation map (2026-10-08)

Source 7b51c51f1146d6fa6a574cb51848fad35aa536a8 passed full CI
37818503009 (https://github.com/1xmint/theradar/actions/runs/37818503009):
2,480 Rust and 352 web tests, all four mutation shards and final gate. Prior
handback CI 37815702919 passed before this source push; no awaited check was
cancelled. Local verification passed 40 journal, 141 signer and 33 conformance
tests, scoped Rust 1.99 Clippy and format. The additional terminal-replay case
also passed its scoped regression. Three manually reapplied bugs failed their
regressions and source was restored. No new mutation exclusion or lint
suppression was added.

The operator's --record-settlement mode reviews one protected packet read and
persists only normalized facts with the exact signed artifact. It re-verifies
wallet signature/message and preserves minimum slot/read times. Arbitrary
input response fields are excluded. The journal appends before memory/output,
refuses unbound or non-unknown operations and changed facts, and makes identical
repeats idempotent. Replay checks record stage/identity/immutability and retains
facts through later terminal records. Older absent-field hashes stay unchanged.
Generic journal callers provide verified normalized data themselves; the
journal does not authenticate cryptography, chain origin or economic effects.

Recording leaves claims outstanding and still blocks issuance, including after
restart. Native candidates do not establish gross spend or USD exposure/loss.
Optional chain block time is not retained by this review; no execution-day or
daily-loss inference is made from host read times. Retained historical evidence
does not make a future portfolio snapshot current.

Four activation gates remain, with the durable-facts portion of gate one now
implemented: economic reconciliation/accounting checkpoint; protected live
risk-input construction; execution/recovery/exit loop; and activated owner
limits, isolated keys, verified Privy policy/delegation, funding, deployment and
bounded live validation. Several substantial integrations remain; no percentage
or launch date is claimed. The next implementation is economic accounting of
retained facts with a durable checkpoint before another issuance.

Autonomous execution remains off and site settings remain drafts in the current
code path. No live chain/provider call, real wallet signature/key/credential or
delegation change, trade, VPS health read or Serve deployment occurred. Live
wallet funding and Privy setup remain unverified.

No Cargo/rustc/radar/issuer/signer process remains. Target measured 33.1 GiB,
with 114.5 GiB free. Previously rejected ignored-output cleanup was not retried.
This follow-up changes documentation only; its CI may be pending while the
source above is fully verified.

### Protected snapshot history coverage (2026-10-08)

Actual caller: offline Issuer::issue compares a required private snapshot
accounting_checkpoint with the owned journal's last complete event digest before
any new operation or proof. Empty string denotes only an empty journal. This
coverage assertion does not prove economic correctness of operator-provisioned
figures; USD exposure/loss reconciliation remains open. No live authority changes.

- [x] Expose the existing durable journal head through Journal/OperationLog.
  All events count, including terminal records and refused reservation proposals.
- [x] Require a string checkpoint in protected snapshots; stdin cannot supply it.
  Mismatches refuse without history changes; outstanding claims still refuse.
- [x] Three new regressions cover append failure, idempotence/restart and
  non-operation events; malformed/missing/candidate fields; aborted/completed
  history and unchanged refusals; current coverage preserving daily-loss refusal.
  Existing insufficient-cash test covers refreshing coverage in a running issuer.
- [x] Bypassing issuer comparison and returning an empty journal head each fail
  their targeted regression; both restored. Raise test floor to 2086.
- [x] All 184 scoped tests and 33 conformance tests pass, with Rust 1.99 scoped
  Clippy and format.
- [x] Read staged diff; source 740ec39 passed full CI 37824117332: 2,483 Rust
  and 352 web tests, all four mutation shards and final gate. Prior handback CI
  37821367844 passed before this push; no awaited check was cancelled.

Next: derive economic exposure/loss from retained facts with explicit valuation
and execution-day evidence, then build protected live snapshots. This checkpoint
only rejects stale history association, not incorrect USD figures carrying a
current marker. Autonomous execution remains off, with all four activation
gates still open. No production deployment, real trade or delegation is made.

### Handback: protected accounting history checkpoint (2026-10-08)

Source 740ec394b81abd7f3ee39b738984ada8f9804ce0 passed full CI
37824117332 (https://github.com/1xmint/theradar/actions/runs/37824117332):
2,483 Rust and 352 web tests, all four mutation shards and final gate. Local
verification passed 41 journal, 143 signer and 33 conformance tests, scoped Rust
1.99 Clippy and formatting. Bypassing the issuer comparison and returning an
empty journal checkpoint each failed its targeted regression; source restored.
No new mutation exclusion or lint suppression was added. Prior handback CI
37821367844 completed successfully before the push. No awaited run was cancelled.

Issuer::issue requires a private accounting_checkpoint string equal to the
owned journal's last complete event digest. Empty string is genesis only; missing
or malformed input refuses. Every event counts, including completed operations,
aborts, proposals whose reservation was refused and non-operation records.
Failed appends do not advance the head; repeats and replay preserve it. Tests
exercise stale history refusal in restarted and running issuers, unchanged
history on refusal, candidate exclusion and a current checkpoint still refusing
the supplied daily-loss stop. Outstanding submissions continue to block issuance.

This asserts operator-provisioned history coverage only. It does not verify that
USD exposure or loss figures are correct, authenticate RPC origin, prevent
operator rollback or establish current prices/holdings. Copying the current
digest onto incorrect economic figures remains possible inside the trusted
operator boundary. No protected claim-closing command was added.

Next: economic reconciliation and independent live snapshot construction.
Inspection shows current ExecutionBinding retains wallet/transaction, while
proposal correlation retains mint/receipt rather than a complete reviewed
proposal. Durable creator/action attribution is needed for economic accounting;
an opaque proposal nonce cannot reconstruct it. Retained normalized settlement
review omits optional chain block time, so execution-day evidence is also needed
before daily-loss attribution. Native net delta and authorization ceilings must
not substitute for measured trade notional, valuation or PnL. These are next-step
requirements, not implementations or settled accounting choices.

All four activation gates remain open: economic reconciliation; protected live
risk inputs; execution/recovery/exits/scheduling; and activated owner limits,
isolated keys, verified Privy policy/delegation, funding, deployment and bounded
validation. This closes one coverage safeguard within the first gate. Autonomous
execution remains off and site settings remain drafts. No live provider/chain
read, real wallet signature/key/credential/delegation change, trade, VPS health
read or Serve deployment occurred. Current funding and Privy setup are unverified.

No Cargo/rustc/Radar process remains locally. Target measured 33.1 GiB with
114.5 GiB free. Previously rejected ignored-output cleanup was not retried. This
follow-up records verified source only; its documentation CI may still be pending.

### Reviewed trade attribution and execution-time evidence (2026-10-08)

Actual callers: Issuer::issue stores the typed protected proposal with its
checked transaction before issuance; existing protected settlement review/record
retain normalized optional execution time. These are economic reconciliation
prerequisites, not a live ledger or a source of model signing authority.

- [x] Optional reviewed_proposal in ExecutionBinding preserves absent-field old
  event hashes. Signing updates and terminal replay retain it; replay rejects
  changed, removed or newly invented attribution. No migration guesses fields.
- [x] Store serialized typed Proposal rather than arbitrary input JSON. Actual
  issuer regression verifies all reviewed fields and exclusion of extra bodies.
- [x] Preserve known unsigned block_time_unix_secs as a decimal string, leave
  missing/null time unknown, refuse malformed/negative/future-to-read times.
  Zero/equality and canonical decimal spelling are covered; record persists time
  and refuses a conflicting repeat.
- [x] Three new regressions raise the test floor to 2089. Existing actual signed
  binding and durable terminal tests verify retained proposal/time through replay.
- [x] Reapplied omission of stored proposal, changed-proposal replay acceptance,
  and using read completion as absent execution time each fail; restore source.
- [x] All 187 scoped tests and 33 conformance checks pass; Rust 1.99 scoped
  Clippy and formatting pass. No mutation exclusion or lint suppression added.
- [ ] Read staged diff, commit, wait prior CI, push and verify all shards/gate.

Next: economic reconciliation using measured fills, cost basis and dated valuation
inputs; authorizing proposal notional is not measured fill value. Protected live
snapshots, execution/recovery/exits and activation verification also remain.
Autonomous trading stays off. No real credential, wallet signature, delegation,
trade, live chain/provider read or production deployment occurs in this increment.
