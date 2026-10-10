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
  bounded file input under one six-call, twenty-second budget, including the
  raw token/mint batch when holdings are present. Emit both
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
- [x] Source 54050e0 passed full CI 37839702196: 2,486 Rust and 352 web
  tests, all four mutation shards and final gate. Prior run 37826896344 passed
  before pushing; no awaited check was cancelled. Complete staged diff read.

Next: economic reconciliation using measured fills, cost basis and dated valuation
inputs; authorizing proposal notional is not measured fill value. Protected live
snapshots, execution/recovery/exits and activation verification also remain.
Autonomous trading stays off. No real credential, wallet signature, delegation,
trade, live chain/provider read or production deployment occurs in this increment.

### Handback: durable reviewed attribution and execution time (2026-10-08)

Source 54050e0b61b5964cea41a9ca4424e8ebd6207570 passed full CI
37839702196 (https://github.com/1xmint/theradar/actions/runs/37839702196):
2,486 Rust and 352 web tests, all four mutation shards and final gate. Local
verification passed 42 journal, 145 signer and 33 conformance tests, scoped Rust
1.99 Clippy and format. The final timestamp fixture is based on the packet's
recorded completion rather than a later clock read and passed its actual-process
regression plus Clippy. Three deliberately reapplied bugs failed and were
restored: omitted reviewed proposal, accepting changed attribution at replay,
and using read time for absent execution time. No mutation exclusion or lint
suppression was added. Prior run 37826896344 passed before the source push; no
awaited run was cancelled.

The actual issuer normalizes its typed protected Proposal into ExecutionBinding
before reservation/proof output. Arbitrary input fields are excluded. Context
retains creator/action, market, quote, notional and risk-input fields. Signing
updates and terminal replay preserve it; replay rejects replacement, removal or
invented context. Generic journal callers establish correctness. Older missing
attribution stays missing and absent fields preserve old hashes; no migration
infers fields from a nonce or a transaction ceiling.

Protected review/record preserve reported optional execution time as canonical
unsigned decimal text. Missing/null remains unknown; malformed, negative or
future-to-read-completion time refuses. Zero and completion equality are measured
values. Known time survives record/replay, and a changed timestamp conflicts on
repeat. Previously stored reviews are not rewritten, including reviews without
the new field; changed normalized shapes conflict. This is protected operator
provenance, not cryptographic proof of chain time or independently calculated
execution-day accounting. Read times never fill an absent execution timestamp.

Next: economic reconciliation with measured fills, cost basis and dated valuation
inputs, feeding exposure and daily-loss state with durable/idempotent accounting.
Reviewed proposal notional is an authorization input, not measured fill value;
native net delta is not gross trade value or PnL. Unknown attribution/time/prices
must remain unknown when later reconciliation needs them. No protected command
closes a claim or derives USD state yet. Protected live risk-input construction,
execution/recovery/exits/scheduling, and activation of owner limits, isolated
keys, verified Privy delegation/policy, funding, deployment and bounded validation
remain. All four activation gates remain open; this closes attribution/time
prerequisites within the accounting gate.

Autonomous trading remains off and site settings remain drafts. No live provider
or chain read, real wallet signature/key/credential/delegation change, trade,
VPS health read or Serve deployment occurred. Current funding and Privy setup
are unverified. Tests use fixture signatures only.

No Cargo/rustc/Radar process remains locally. Target measured 33.1 GiB, with
64.2 GiB free. Previously rejected ignored-output cleanup was not retried. This
handback is a local documentation commit to include with the next source push;
no redundant notes-only CI run is triggered. Source and behavior documentation
are already pushed in PR 334 and fully verified.

### Protected historical native valuation review (2026-10-08)

The actual caller is `radar-issuer --review-valuation <operation-id>
<private-price-file>`. It re-verifies the retained wallet signature, requires
an outstanding bound SOL operation and normalized settlement, and prices known
wallet net debit and network fee using a protected dated SOL price. Both time
and slot ages are checked against execution context; future, stale, missing,
foreign or zero prices refuse. Missing execution time stays unknown and refuses.
Integer arithmetic rounds costs upward and refuses overflow; the output keeps
the older valuation watermark. Price provenance remains operator provisioning.

Wallet net debit can include rent, tips or refunds. Trade notional, position
cost basis and realised PnL remain null. This read-only command does not record
valuation, close claims, update exposure/loss or enable signing/delegation.
Existing history, startup configuration and private-file controls still apply.

- [x] Add three arithmetic/binding/freshness unit tests and an actual-process
  valuation regression; raise the test floor from 2,089 to 2,093.
- [x] Reapply floor rounding, accepting future time via saturating subtraction,
  and bypassing signature verification: each targeted test fails. Restore all
  source afterward. The signature fixture is otherwise valid for valuation.
- [x] Final scoped verification passes: 149 signer and 33 conformance tests;
  Rust 1.99 Clippy and formatting pass. No lint suppression or exclusion added.
- [x] Source a902e09 passed full CI 37847565387: 2,490 Rust and 352 web
  tests, all four mutation shards and final gate. Initial run completed before
  pushing its equality-regression repair; no awaited run was cancelled.

Next: measured fills, cost basis and durable economic reconciliation feeding
USD exposure and daily-loss state. Protected live risk inputs, the execution/
recovery/exit loop and activation of owner limits, isolated keys, verified Privy
policy/delegation, funding and bounded validation remain. All four activation
gates remain open. Autonomous trading stays off; no live read, real credential,
wallet signature, trade or production deployment occurs in this increment.

CI 37844323424 found two missing equality regressions at valuation.rs:81:17
(`<` to `<=` for the intent slot) and :96:14 (`>` to `>=` for reservation
coverage). The existing arithmetic test now accepts execution exactly at intent
slot with debit exactly equal to reservation. Applying each exact mutation by
hand fails this regression; source restored. No exclusion added. Await all
remaining shards and the gate before pushing this test repair (completed).

### Handback: protected historical native valuation (2026-10-08)

Source a902e092a4509be3781fe89c7effb503ecba5c82 passed full CI
37847565387 (https://github.com/1xmint/theradar/actions/runs/37847565387):
2,490 Rust and 352 web tests, all four mutation shards and final gate. Local
verification passed 149 signer and 33 conformance tests, scoped Rust 1.99 Clippy
and format. Deliberately reapplied floor rounding, future price acceptance and
signature-verification bypass each fail their regression. Initial CI found two
missing equality cases, now covered; the exact reported comparisons fail when
reapplied manually. Runtime source was restored. No mutation exclusions or lint
suppression added. All initial shards/gate completed before the repair push.

The actual protected `--review-valuation` command binds retained settlement to
an outstanding SOL operation and re-verifies its wallet signature. Historical
SOL price input is private operator provisioning, not authenticated market data.
It must be positive, correctly typed, no later than execution in either slot or
time and within configured age bounds. Missing execution time refuses. Debit
and network fee USD conversions use upward-rounded integer arithmetic and
refuse overflow. The older price watermark is retained. Repeated reviews leave
history bytes unchanged; outstanding claims still block new issuance.

Measured wallet net debit includes possible rent, tips and refunds. It is not
gross swap value or PnL. Trade notional, cost basis and realised PnL remain null;
no portfolio/loss state is updated and no operation is reconciled or claim
released. Valuation is not yet durable economic accounting or a future sizing
upper price. Next derive measured fills and acquisition cost, then durable
idempotent exposure/loss reconciliation. Independent live risk-input snapshots,
execution/recovery/exits/scheduling and activation of owner limits, isolated
keys, verified Privy policy/delegation, funding, deployment and bounded validation
remain. All four activation gates remain open; several integrations remain.

Autonomous trading stays off and site limits remain drafts. No live provider or
chain read, real wallet signature/key/credential/delegation change, trade, VPS
health read or Serve deployment occurred. Current funding and Privy setup remain
unverified. Tests use fixture signatures only. No local watcher remains. This
verification handback is committed locally to include with the next source push,
avoiding a redundant documentation-only CI run; verified source is in PR 334.

No Cargo/rustc/Radar process remains locally. Target measured 33.1 GiB, with
63.2 GiB free. Previously rejected ignored-output cleanup was not retried.

### Retained measured net token acquisition (2026-10-08)

The actual protected settlement review/record commands derive optional
`wallet_token_acquisition` from paired pre/post token metadata for the retained
buy proposal's mint and configured wallet. Successful native-SOL buy context,
both sides of every included account, matching owner/mint/program/decimals,
usable units and overflow-free totals are required. Internal transfers cancel;
only a positive aggregate change supplies known net acquired units. Failed
execution, absent/invalid context, missing metadata (including a newly created
ATA's pre balance), changed identity, inconsistent units or zero/negative gain
remain null. Other wallets are excluded. No new dependency or command added.

This is measured net acquisition under operator evidence, not authenticated
gross venue fill or attribution of every transfer. Token-2022 extensions are
not interpreted. Cost basis, USD exposure/loss and PnL remain unknown. Existing
record/replay retains the field; repeat shape conflicts remain fail-closed and
older reviews are not rewritten. The claim stays outstanding.

- [x] Three unit regressions cover paired aggregation, unknown/changed metadata,
  buy context, inconsistent units, decimal bounds and overflow; one actual CLI
  regression covers review, durable/idempotent recording, replay and blocked
  issuance. Raise test floor from 2,093 to 2,097.
- [x] Reapply accepting failed execution, skipping pre-owner checks and
  saturating post totals: each acquisition regression fails; restore source.
- [x] Scoped verification passes: 153 signer and 33 conformance tests, Rust
  1.99 Clippy and formatting. Complete staged diff read. Prior CI completed.
- [x] Source c3f903d passed CI 37852448900: 2,494 Rust and 352 web tests,
  all four mutation shards and final gate. Prior CI passed before pushing;
  no awaited run was cancelled.

Next: establish acquisition costs and gross fill attribution, then durable
idempotent USD exposure/loss reconciliation. Protected live risk inputs,
execution/recovery/exits/scheduling and activation/funding verification remain.
All four activation gates remain open. Autonomous trading remains off. No live
provider/chain read, real key/signature/delegation/trade or deployment occurs.

### Handback: retained measured net token acquisition (2026-10-08)

Source c3f903d05d3b26cdf3c465e48faacc4eda207a8c passed full CI
37852448900 (https://github.com/1xmint/theradar/actions/runs/37852448900):
2,494 Rust and 352 web tests, all four mutation shards and final gate. Local
verification passed 153 signer and 33 conformance tests, Rust 1.99 scoped Clippy
and formatting. Accepting failed execution, skipping pre-owner checks and
saturating post totals each failed when deliberately reapplied; source restored.
No mutation exclusion, lint suppression or dependency added. Staged diff read;
prior CI 37847565387 completed successfully before this source push.

The existing protected review/record commands now derive and durably retain
positive net token acquisition for successful reviewed native-SOL buys only
when both metadata sides match wallet/mint/index/program/usable decimals across
all included accounts. Internal transfers cancel; foreign wallets are excluded.
Empty/unpaired/changed metadata, failed or unknown context, unusable units,
overflow and nonpositive totals stay null. Missing pre metadata for a new ATA
is not zero. Record repeats remain idempotent, replay retains the report and
the outstanding claim still blocks issuance. Older reports are not backfilled;
changed normalized repeat shapes conflict. Provenance remains operator evidence,
not independent authenticated chain origin or gross venue-fill attribution.

Next establish acquisition costs, separate swap consideration from rent/tips/
fees and handle newly created accounts with explicit evidence, then implement
durable idempotent economic reconciliation for exposure and daily loss. Current
USD/PnL/cost-basis fields remain unknown. Protected live risk inputs, execution/
recovery/exits/scheduling and activation of owner limits, isolated keys, verified
Privy policy/delegation, funding, deployment and bounded validation remain.
All four activation gates remain open; several substantial integrations remain.

Autonomous trading remains off; site limits remain drafts. No live provider or
chain read, real wallet key/signature/credential/delegation change, trade, VPS
health read or Serve deployment occurred. Current funding/Privy setup remains
unverified. Fixture signatures only. No local watcher remains. This verified
handback is committed locally for inclusion with the next source push; no
redundant documentation-only CI run. Verified source is pushed in PR 334.

No Cargo/rustc/Radar process remains locally. Target measured 33.1 GiB with
61.1 GiB free. Previously rejected ignored-output cleanup was not retried.

### Protected acquisition cost breakdown review (2026-10-08)

Actual caller: existing `radar-issuer --review-valuation` with optional private
price-file `acquisition_costs`. Missing/null leaves notional and basis unknown;
a supplied incomplete/malformed/foreign breakdown refuses. It binds operation,
exact signed artifact, wallet, reviewed successful native-SOL buy, mint, token
program, usable decimals and known positive acquired units. Explicit swap/rent/
tip integers and retained network fee must sum exactly to wallet debit without
overflow; swap is positive and other cash flows must be explicitly absent.
Both under- and over-accounted outlay refuse. Historical price/slot/time,
reservation, private-file and wallet-signature checks remain in force.

Bookkeeping basis capitalizes swap plus fee plus tip; rent is priced separately
without claiming recoverability or a realised loss. Swap alone supplies trade
notional. Existing costs round upward. This is operator-reviewed classification
and arithmetic consistency, not authenticated gross fill or independently
collected economics. A balanced false split cannot be detected within this trust
boundary. PnL stays null; no history/portfolio/loss/claim or authority update.
Cost results are read-only and not yet durable; no new command or dependency.

- [x] Three unit regressions cover complete/absent costs, integer pricing,
  exact accounting, invalid/missing components, artifact/context/quantity/unit
  bindings and decimal bounds. One actual-process regression verifies repeated
  read-only review, refusals and retained outstanding claims. Raise test floor
  from 2,097 to 2,101.
- [x] Reapply accepting underreported outlay, capitalizing rent and bypassing
  signed-artifact binding; the complete issuer unit suite catches each. Restore
  source. An initial narrow name filter omitted the imbalance test; rerunning
  the full issuer unit suite proves the actual regressions.
- [x] Final scoped verification passes: 157 signer and 33 conformance tests,
  Rust 1.99 Clippy and formatting. The final matched-identity/zero-swap boundary
  fixture passes its targeted regression and Clippy.
- [x] Source 72b86d6 passed full CI 37860126670: 2,498 Rust and 352 web
  tests, all four mutation shards and final gate. Staged diff read; prior CI
  passed before pushing. No awaited run was cancelled.

Next retain reviewed acquisition costs durably and implement idempotent economic
reconciliation, including inventory/cost basis, exposure and daily loss. Gross
fill attribution, new-account evidence and independent live component collection
remain. Protected live risk inputs, execution/recovery/exits/scheduling and
activation of limits, isolated keys, verified Privy policy/delegation, funding,
deployment and bounded validation remain. All four gates remain open; autonomy
stays off. No live read, real key/signature/delegation/trade or deployment occurs.

### Handback: protected acquisition cost breakdown (2026-10-08)

Source 72b86d68c7d1a614a72b8af839eb067d5faa988e passed full CI
37860126670 (https://github.com/1xmint/theradar/actions/runs/37860126670):
2,498 Rust and 352 web tests, all four mutation shards and final gate. Local
verification passed 157 signer and 33 conformance tests, Rust 1.99 scoped Clippy
and formatting. The final matched-identity/zero-swap fixture passed its targeted
test and Clippy. Reapplied underreported outlay, rent capitalization and omitted
signed-artifact binding each failed the complete issuer unit suite; restored.
The first name-filtered run omitted the imbalance test; the full suite proves
the regressions. No dependency, mutation exclusion or lint suppression added.
Complete staged diff read; prior CI 37852448900 passed before the source push.

Actual caller remains the protected `--review-valuation` command. Its private
price input now accepts an optional complete acquisition-cost object bound to
operation, exact signed artifact, wallet, reviewed successful native-SOL buy,
mint/program/usable decimals and positive measured acquired quantity. Checked
swap plus retained network fee plus tip plus rent must equal wallet debit
exactly. Swap must be positive and other cash flows explicitly absent. Foreign,
incomplete, unbalanced or overflowing costs refuse the whole review; absent/null
costs keep trade notional/basis unknown. Historical price/signature/reservation
checks remain. Upward-rounded trade notional prices swap alone; position basis
capitalizes swap, network fee and tip. Rent is separately priced without a
recoverability assertion. Fees are already in basis; do not double-expense them.

This verifies binding and arithmetic of operator-reviewed classification, not
independent gross-fill or component provenance. A balanced false split remains
inside the provisioning trust boundary. The result is read-only, not yet durable
cost accounting, tax basis or a live sizing oracle. Repeated actual-process
reviews/refusals preserve history; PnL stays null, portfolio/loss state is not
updated and the outstanding claim still blocks issuance.

Next durably retain reviewed acquisition costs and implement idempotent inventory/
cost-basis, exposure and daily-loss reconciliation. Gross fill attribution,
new-account evidence and independent component collection remain. Protected live
risk inputs, execution/recovery/exits/scheduling and activation of owner limits,
isolated keys, verified Privy policy/delegation, funding, deployment and bounded
validation remain. All four activation gates remain open; several substantial
integrations remain before live autonomy.

Autonomous trading stays off and site limits remain drafts. No live provider or
chain read, real wallet key/signature/credential/delegation change, trade, VPS
health read or Serve deployment occurred. Current funding/Privy setup remains
unverified; fixture signatures only. No local watcher remains. Verification
notes are committed locally to include with the next source push, avoiding a
redundant documentation-only CI run. Verified source is pushed in PR 334.

No Cargo/rustc/Radar process remains locally. Target measured 33.1 GiB with
59.5 GiB free. Previously rejected ignored-output cleanup was not retried.


### Durable reviewed acquisition valuation (2026-10-08)

Actual caller: new protected `radar-issuer --record-valuation` command. It runs
one input read through the existing signature/message/historical-price/full-cost
review and refuses cash-only valuation. The journal retains exact prior
SettlementRecord plus normalized review under the existing lock. It requires
SubmissionUnknown and exact prior facts/unchanged metadata; proposal injection
or a combined first settlement/valuation replay event refuses. Append precedes
memory; identical repeats do not append or advance the checkpoint, while changed
costs or prices refuse even if rounded USD basis agrees. Terminal replay retains
the record. Older hashes and history are unchanged when the field is absent.

This is durable operator-reviewed evidence, not economic reconciliation or
independent provenance. Raw input declarations/provider bodies are not stored.
No portfolio, inventory, exposure, loss, PnL, claim or signing authority changes.
Outstanding claims continue to block issuance after restart.

- [x] Five new regressions cover hash coverage, append failure, exact binding,
  repeat/conflict/restart/terminal replay, injected or misstaged records and the
  actual protected CLI. Existing signature and cash-only process regressions
  also exercise recording. Test floor rises from 2,101 to 2,106.
- [x] Reapply memory-before-append, accepting changed runtime/replay valuations
  and recording cash-only reviews: each relevant regression fails. Source
  restored; no mutation exclusion, lint suppression or dependency added.
- [x] Final scoped tests pass: 46 journal, 158 signer and 33 conformance tests;
  Rust 1.99 scoped Clippy and formatting pass. Staged diff reviewed before
  committing; full CI verification remains below.
- [x] Source 4c10b2b passed full CI 37863760052: 2,503 Rust and 352 web
  tests, all four mutation shards and final gate. Prior CI 37860126670 completed
  successfully before pushing; no awaited run was cancelled.

Next implement idempotent economic reconciliation using the retained costs:
inventory/cost basis, exposure and daily loss. Gross fill attribution,
new-account evidence and independent live collection remain. Protected live risk
inputs, execution/recovery/exits/scheduling and activation of owner limits,
isolated keys, verified Privy policy/delegation, funding, deployment and bounded
validation remain. All four activation gates remain open. Autonomous trading
stays off; no live read, real key/signature/delegation/trade or deployment occurs.


### Next economic reconciliation acceptance (planned)

Keep the protected issuer as the caller. Consume complete retained operation
history, including terminal records; replay-derived acquisition lots must apply
an operation once and bind its wallet, mint/program/units and reviewed creator.
Missing opening inventory, attribution, cost or history coverage must remain
unknown or refuse, never become a flat book. Retained acquisition basis is
historical bookkeeping cost, not current liquidation value.

Before claim closure can permit repeated issuance, establish matching inventory
and risk-state coverage. Failed execution fees and realised exit results require
separate complete accounting, execution-day attribution and failure ordering;
acquisition basis alone supplies no daily-loss result. Keep rent separate and
avoid expensing a fee already capitalized into basis. The acceptance matrix must
include duplicate/restart application, incomplete or conflicting records,
external wallet changes, overflow, unsupported acquisition/disposal effects and
disk failure. Reconciliation must not create capital or clear an uncertain claim.
These are planned checks, not implemented or verified behavior in this increment.


### Handback: durable reviewed acquisition valuation (2026-10-08)

Source 4c10b2b8fd844fbad69d7fd049830cdff310a333 passed full CI
37863760052 (https://github.com/1xmint/theradar/actions/runs/37863760052):
2,503 Rust and 352 web tests, all four mutation shards and final gate. Local
verification passed 46 journal, 158 signer and 33 conformance tests, Rust 1.99
scoped Clippy and formatting. Reapplied memory-before-append, accepting changed
runtime/replay valuations and cash-only recording each failed its regression;
correct source restored. No dependency, mutation exclusion or lint suppression
added. Complete staged diff read; prior CI 37860126670 passed before pushing.

Protected --record-valuation now reads once through the existing strict wallet
signature, authorized-message, historical-price and complete-cost review, then
retains exact prior settlement plus normalized valuation. Cash-only input refuses.
The journal requires unknown-submission stage and exact preceding facts. Append
precedes memory; repeats are idempotent, changed normalized prices/costs refuse,
including equal rounded basis, and replay rejects injected/misstaged/changed
records. Terminal replay preserves retained valuations. The checkpoint covers
new records; absent optional fields preserve older hashes without backfill.
Generic callers establish economic correctness; protected operator provenance
remains the actual trust boundary. Raw inputs/provider bodies are not retained.

This closes durable cost retention, not economic reconciliation. Inventory,
exposure and daily loss are not updated, PnL remains unknown, and the claim still
blocks another issuance. Next implement idempotent economic reconciliation under
the acceptance checks above. Gross-fill attribution, new-account evidence and
independent live collection remain. Protected live risk inputs, execution/
recovery/exits/scheduling and activation of owner limits, isolated keys, verified
Privy policy/delegation, funding, deployment and bounded validation remain.
All four major activation gates remain open; no launch percentage/date is claimed.

Autonomous trading stays off and site limits remain drafts. No live provider or
chain read, real wallet key/signature/credential/delegation change, trade, VPS
health read or Serve deployment occurred. Current funding/Privy setup remains
unverified; fixture signatures only. No local watcher remains. This verified
handback is committed locally to include with the next source push, avoiding a
redundant documentation-only CI run. Verified source is pushed in PR 334.

No Cargo/rustc/Radar process remains locally. Target measured 33.1 GiB with
59.5 GiB free. Previously rejected ignored-output cleanup was not retried.


### Replay-derived acquisition history (2026-10-08)

Actual caller: `radar-issuer --review-acquisitions` over the owned journal.
New OperationLog.entries exposes every retained operation once, including
terminal entries, in deterministic digest order rather than execution chronology.
The report rebuilds native-SOL acquisition lots from complete retained costs,
re-verifies wallet signatures/exact messages and redoes the historical valuation;
all normalized fields must agree and retained facts must name that signed
transaction signature. Unsubmitted operations are listed separately.
Terminal acquisitions require Completed spend equal to measured debit. Missing,
changed, unsupported or duplicate-artifact acquisitions refuse the whole report.
Mint/program/unit identity is consistent across creator groups; aggregation of
measured units, basis and rent is checked, with the oldest price watermark.

Acquisition history remains distinct from complete wallet inventory and current
risk state. Empty lots do not mean flat inventory. Output names the journal
checkpoint but keeps inventory completeness false and exposure/daily loss null.
No history/portfolio/claim/authority changes. Opening inventory, external changes,
failed fees, disposals and current pricing require later reconciliation.

- [x] Six new regressions cover exact multi-lot/mint/creator aggregation, units,
  overflow, terminal replay/repeat, valid distinct signed trades, missing/changed/
  duplicate/invalid evidence and empty/unsubmitted history through the actual
  protected process. Raise test floor from 2,106 to 2,112.
- [x] Reapply taking newest price watermark, accepting duplicate artifacts,
  accepting changed normalized valuation and dropping terminal operations; each
  relevant regression fails. A final diff review added retained-fact signature
  association; removing it also fails the distinct-artifact refusal regression.
  Source restored. No mutation exclusion, lint suppression or dependency added.
  The distinct-trade fixture initially used
  the issuer seed; strict signature verification refused it. Corrected to its
  fixture wallet seed and the positive two-trade case passes.
- [x] Final scoped checks pass: 46 journal, 164 signer and 33 conformance
  tests, Rust 1.99 scoped Clippy and formatting. Complete staged diff reviewed
  before committing; full CI follows below.
- [x] Repair source 6ca7507 passes full CI 37868617053: 2,509 Rust and
  352 web tests, all four mutation shards and final gate. Initial CI completed
  before pushing its narrowly verified test repair; no awaited run cancelled.

Next reconcile these historical lots with protected opening/current wallet
inventory and integrate coverage with economic risk state before claim closure.
Failed execution fees, disposals/day/failure ordering, external changes and
independent current USD valuation remain. Four major activation areas remain:
economic reconciliation, protected live risk inputs, execution/recovery/exits/
scheduling and limits/isolated keys/verified Privy policy/delegation/funding/
deployment/bounded validation. Autonomous trading stays off. No live provider or
chain read, real key/signature/delegation/trade or Serve deployment occurs.


### Acquisition-history CI repair (2026-10-08)

Initial source 7c140e0 passes 2,509 Rust and 352 web tests, but CI 37866521416
shard 0 reports the exact survivor at operation.rs:484:9: replace
OperationLog.entries with std::iter::empty(). Its downstream issuer regression
catches dropped history, but mutation testing of radar-journal runs that crate's
own suite. Extend the existing terminal valuation replay regression to require
its exact retained operation/state from entries even with no outstanding claims.
Reapply that exact body replacement: the journal regression fails; restore the
source and the regression passes. No new test function/floor change, exclusion
or lint suppression. Wait for every initial CI job before pushing this repair.


### Next wallet reconciliation input: inspected gap (2026-10-08)

Source inspection of wallet_read.rs and radar-onchain RPC token-account parsing
shows that wallet-read emits mint/raw amount/decimals under separate classic and
Token-2022 bank contexts. The RPC parser checks the parsed token owner against
the requested wallet, but its retained TokenAccount lacks the account address,
actual account program owner and state/extension semantics. The CLI therefore
cannot distinguish repeated copies of one account from multiple accounts for
one mint. Identical reported slots still do not establish an atomic snapshot.

Before using this packet for reconciled risk inventory, retain and validate
account identity and program/state metadata, reject duplicate account identities
and unsupported/unspendable semantics, and preserve separate slots/read windows.
Then establish protected opening/current inventory coverage for acquisition lots;
unexplained holdings or external changes must remain unknown or refuse. This is
an inspected source gap and planned work, not a live wallet measurement or an
implemented change. Acquisition-history output remains explicitly incomplete.


### Handback: replay-derived acquisition history (2026-10-08)

Source 6ca75076480636516f3740bd2544009128a97856 passes full CI
37868617053 (https://github.com/1xmint/theradar/actions/runs/37868617053):
2,509 Rust and 352 web tests, all four mutation shards and final gate. Initial
CI 37866521416 passed every other shard but reported the exact journal iterator
survivor; its full run completed before the repair push. The repaired shard now
reports 142 caught and 54 unviable of 196 mutants, with none missed. No cancelled
awaited run, mutation exclusion, lint suppression or dependency added.

Local checks passed 46 journal, 164 signer and 33 conformance tests, Rust 1.99
scoped Clippy and formatting. Final signature association cases and Clippy passed.
Reapplied newest-price selection, duplicate artifact acceptance, changed valuation
acceptance, dropping terminal entries and ignoring retained-fact signature each
failed a running regression. Replacing entries at the exact reported
operation.rs:484:9 with std::iter::empty() failed the extended journal terminal
replay regression; restoring source passed that test, scoped journal Clippy and
formatting. The CI-repair documentation passed all 33 conformance checks. Full
staged diffs were read before source and repair commits.

Actual caller --review-acquisitions now rebuilds one historical native-SOL buy
lot per submitted operation from retained complete valuation. It re-verifies the
wallet signature and exact message, checks the signature named by retained facts,
and recomputes the entire historical valuation. Terminal lots need Completed
spend equal to measured debit. Missing/changed/unsupported reviews or the same
signed artifact under different operation IDs refuse the whole report. Every
retained operation is visible once, including completed entries; unsubmitted
operations are listed separately. Groups preserve mint/program/units/creator,
checked acquired quantity, cost basis, separate rent and oldest price watermark.
Repeat and restart do not rewrite history, release claims or update portfolio.

Historical acquisition bookkeeping is implemented; complete wallet inventory,
current USD exposure and daily loss are still unknown. The next inspected input
gap above must close before reconciling opening/current wallet balances with
these lots. External changes, failed fees, disposals/day/failure ordering and
independent current pricing remain. Four major activation areas remain: economic
reconciliation, protected live risk inputs, execution/recovery/exits/scheduling,
and activation of owner limits, isolated keys, verified Privy policy/delegation,
funding, deployment and bounded validation. No percentage or launch date claimed.

Autonomous trading stays off and site limits remain drafts. No live provider or
chain read, real wallet key/signature/credential/delegation change, trade, VPS
health read or Serve deployment occurred. Current funding/Privy setup remains
unverified; fixture signatures only. No local watcher remains. Verified source
is pushed in PR 334. This handback is committed locally for inclusion with the
next source push, avoiding a redundant documentation-only full CI run.

No local Cargo/rustc/Radar process remains. Target measured 33.1 GiB with
59.4 GiB free. Previously rejected ignored-output cleanup was not retried.

### Token-account evidence identity and state (2026-10-08)

The actual wallet-read caller now retains each token account address, owner
program and reported state. The shared RPC parser requires valid addresses,
program equality to the requested filter and known account states, retaining
its existing parsed wallet-owner check. Duplicate identities within a program
refuse that read. Wallet-read refuses duplicates across the two program reads,
even at different slots. Distinct accounts holding one mint remain separate.
Frozen and uninitialized balances remain reported holdings; spendable is null.

This closes account identity/program/state retention, not extension or
spendability validation. It deliberately does not convert frozen balances into
zero or initialized into safe. Mint restrictions, account extensions, delegates
and wrapping semantics still need a protected inventory reader before any risk
state is inferred. Individual finalized slots, host read windows and unknown
USD/P&L remain. Matching slots do not establish an atomic snapshot. Version 1
adds account metadata without changing existing quantity/slot fields.

The response field shape was verified against Solana's getTokenAccountsByOwner
reference, not a live wallet capture. Existing Serve wallet fixture metadata
was updated because it calls the shared reader; its public response shape and
pricing semantics are unchanged. No risk input, claim release, signing authority
or deployment is added. Activation remains closed pending economic
reconciliation, live risk inputs, execution/recovery/exits/scheduling, and owner
limits/keys/Privy policy/delegation/funding/deployment/bounded validation.

Local verification passed below; two regression tests added and MIN_TESTS raised
by two. Tests cover same-mint distinct accounts, malformed/missing identity and
metadata, wrong program, duplicate identity within/across reads, and retained
frozen holdings with unknown spendability.

Local verification: cargo +stable-x86_64-pc-windows-gnullvm test -p
radar-onchain passed 78 unit and 18 integration tests; -p radar-cli wallet_read
passed five unit tests; -p radar-cli --test wallet_read_process passed all 12
actual-command tests; -p radar-serve --test
a_wallets_positions_are_read_and_priced_by_radar passed all 12 site tests;
-p repo-conformance passed 33 checks. Rust 1.99 scoped Clippy for onchain/CLI/
Serve all targets with warnings denied, formatting and git diff --check passed.
Individually disabling within-read duplicate validation, owner-program matching
and state validation failed the running RPC regression. Disabling cross-program
duplicate validation failed the CLI regression. All restored-source tests passed.
An initial duplicate mutation removed HashSet type inference and was unviable;
the reapplied guard-preserving variant failed the running test. No mutation
exclusions, lint suppressions or dependencies added. Full CI passed source
95f10cb in run 37871441513; the verified handback below records the result.

### Next inspected inventory evidence step (2026-10-08)

The shared jsonParsed reader still does not verify extensions, delegates or
mint restrictions, and accepts a u128 amount although an individual raw token
account stores u64. This is read-only evidence, not risk inventory. Inspection
found existing RpcClient::accounts in crates/radar-onchain/src/rpc.rs returns
base64-decoded owned accounts under one reported finalized context. Existing
TokenAccount::parse and MintAccount::parse in crates/radar-pumpfun/src/token.rs
check the raw program layouts and refuse unsupported extensions. Reuse these
actual callers/parsers for a protected inventory read, rather than adding
another parser or assuming initialized means spendable.

Retain account/mint identities, independently parsed units/state/program and
individual contexts/read windows. Refuse missing, mismatched or unsupported
raw metadata instead of interpreting it as zero or safe. Establish protected
opening/current coverage before matching inventory to acquisition lots;
external transfers, unexplained holdings and absent disposal/fee accounting
cannot become a complete portfolio. Enumeration and a subsequent account read
are still distinct bank observations; one context for the second call does not
prove complete wallet enumeration or atomicity with native balance/history.

### Handback: token-account evidence identity (2026-10-08)

Verified source 95f10cbda2ce166445e6a65bf6eb81ff3c702b96 is pushed in PR 334.
Full CI 37871441513 completed successfully on that exact source: 2,511 Rust and
352 web tests, build/lint/MSRV/site/dependency/format/licence checks, every
mutation shard and final gate. The last shard tested 197 mutants in 25 minutes,
116 caught and 81 unviable, none missed. Every awaited job completed; no repair
push, cancellation, exclusion, lint suppression or dependency added.

Local checks and manual regressions are recorded above. Full staged source
diff was read before commit. The actual wallet-read caller now retains token
account identities/program/state and refuses duplicate accounts within and
across program reads. Frozen/uninitialized amounts remain holdings with unknown
spendability. Separate same-mint accounts remain separate. Individual slots,
exact quantities, host read windows and unknown USD/P&L remain unchanged.

Next: the inspected raw account/mint verification step above, then protected
opening/current inventory coverage and reconciliation against retained
acquisition lots. Unexplained holdings, external transfers, disposal/failed fee
accounting, current exposure and daily loss remain unresolved. Four major
activation areas remain: economic reconciliation, protected live risk inputs,
execution/recovery/exits/scheduling, and owner limits/isolated keys/verified Privy
policy/delegation/funding/deployment/bounded validation. No percentage or launch
date claimed. Historical acquisition bookkeeping is not current inventory.

Autonomous trading remains off and site limits remain drafts. No live chain
read, wallet key/signature/credential/delegation change, trade, VPS health read
or Serve deployment occurred. Funding/Privy setup remains unverified. The CI
watch finished and no local Cargo/rustc/Radar process remains. Target measured
34.9 GiB with 57.6 GiB free after local checks; previously rejected ignored-output
cleanup was not retried. This verified handback is committed locally for the
next source push, avoiding a redundant documentation-only full CI run.

### Raw token/mint verification for operator evidence (2026-10-08)

The actual wallet-read and combined evidence-read caller now invoke
crates/radar-onchain/src/wallet_inventory.rs after both token listings. One
getMultipleAccounts batch reads the sorted union of token account and mint
addresses, at most 100 distinct addresses, under one finalized context. This
limit follows Solana's getMultipleAccounts reference. Raw context must not
precede native balance or either listing. Duplicate identities refuse before
batching. The shared RPC boundary checks response count/request order; the
wallet verifier consumes that result rather than duplicating the same guard.

Existing raw token/mint parsers check program layouts and supported extensions.
Raw programs, wallet/mint identity, u64 amounts, account state and mint decimals
must match the listings; noncanonical mint initialization refuses. Same-mint
amount totals use checked u64 addition. Ordinary tokens cannot exceed observed
supply; the classic native-mint exception below preserves wrapped SOL.
Missing/changed/unsupported raw metadata refuses the whole output. This is
locally decoded provider evidence, not independent chain authentication.

The version 1 packet adds raw_token_verification with its separate context,
quantities/units, mint supply, mint authority activity, freeze authority,
delegation and wrapping reserve. Frozen/uninitialized holdings are retained;
spendability stays unknown. Wrapping reserve is not additional native cash.
Empty enumeration skips the raw RPC batch and does not prove complete inventory.
Matching reported slots still do not establish atomic enumeration/native/history
coverage; common_reported_slot includes the raw batch when one is present.

Wallet-read now has a four-call budget and evidence-read a six-call budget,
both sharing a twenty-second deadline. Empty token lists still use three/five
actual calls. The collector prints neither packet after a raw-read failure.
No issuer snapshot, portfolio, cost basis, claim, loss or authority is changed.

Five raw-verifier regressions and one real CLI-process regression were added;
MIN_TESTS raised by six. Tests exercise frozen/delegated/wrapped holdings,
active mint/freeze authority, supported and unsupported Token-2022 layouts,
missing/malformed/changed ownership/identity/state/amount/units, canonical mint
initialization, supply/overflow, all context floors, exact 100-address boundary,
duplicate accounts, incomplete empty inventory, successful wallet/collector
outputs and raw failures with no partial packet or provider detail. Existing
CLI fixtures now use amounts/mint ownership/supply possible under raw layouts.
Local verification passed below; full source CI is pending. Activation remains closed.

Local proof: cargo +stable-x86_64-pc-windows-gnullvm test -p radar-onchain
passed 82 unit and 18 integration tests; final -p radar-cli passed 223 unit and
13 process tests; -p repo-conformance passed all 33 checks. Rust 1.99 scoped
onchain/CLI all-target Clippy with warnings denied and final formatting passed.
Disabling each of the eleven context/program/identity/quantity/state/unit/
initialization/supply/batch-limit checks failed running regressions. Replacing
checked addition with saturating addition and disabling duplicate identity
refusal each failed its regression. Source was restored and final raw tests,
Clippy and full CLI tests passed. No new dependency, mutation exclusion or lint
suppression. Await every CI job and final gate before any repair push or final
handback. Full staged diff must be read before source commit.

Pre-push correction (2026-10-09): the first generic supply guard would have
refused valid wrapped SOL. The existing pumpswap_reserves.json capture records
the classic So111 mint under SPL Token at 82 bytes, nine decimals and supply
zero. Correct the exception by binding canonical mint AND program, requiring
its native-account flag, nine decimals, zero supply and absent mint/freeze
authority. Arbitrary native flags, missing wrapping flags, changed native
metadata and other wrapping identities refuse. Native reserve remains separate
from native cash. A new regression uses the actual captured mint bytes and a
synthetic wallet-owned account; this is not a live wallet measurement. The new
test also checks listing/mint agreement cannot redefine native decimals.

Final local proof after the wrapping correction (2026-10-09): onchain passed
83 unit and 18 integration tests (101 total); CLI passed 223 unit and 13 process
tests (236 total). Reapplying the original unconditional supply bound and seven
individual native identity/program/flag/metadata mistakes failed running tests.
Source was restored; scoped Rust 1.99 all-target Clippy with warnings denied and
formatting passed. LEARNINGS 51 records the pre-push correction. Full CI remains
pending, with autonomy closed.

### Verified raw wallet evidence handback (2026-10-09)

Source b8623e3892c2895c5010cba3a9391da744a32d64 passed full CI 37882564346:
https://github.com/1xmint/theradar/actions/runs/37882564346 . Every job completed
successfully, including build, lint, formatting, MSRV, site, dependencies,
licence headers, 2,517 Rust and 352 web tests, all four mutation shards and the
final gate. Shards 0/1/2/3 tested 208/208/208/205 mutants respectively, with
150/132/169/165 caught and 58/76/39/40 unviable; none missed. The final shard ran
21 minutes. No repair push, cancellation, new dependency, mutation exclusion or
lint suppression. Local conformance passed all 33 checks after LEARNINGS 51.

The actual wallet-read and combined evidence-read paths now require raw token
and mint checks for listed holdings. Captured classic native mint semantics
preserve nonzero wrapped SOL despite its zero supply. This establishes locally
decoded metadata consistency under the provider trust assumption, not complete
inventory, spendability or independent chain authentication.

Next: protected opening/current inventory coverage and reconciliation against
retained acquisition lots. Unexplained holdings, external transfers, disposals,
failed fees, current USD exposure and daily loss remain unresolved. Four major
activation areas remain: economic reconciliation; protected live risk inputs;
execution/recovery/exits/scheduling; owner limits, isolated keys, verified Privy
policy/delegation, funding, deployment and bounded validation. No percentage or
launch date is claimed. The model may propose trades; deterministic risk checks
and the separate signer must enforce the owner's limits before funds can move.

Autonomous trading remains off; site limits remain drafts. No live chain read,
credential/key/signature/delegation change, trade, VPS health read or Serve
deployment occurred. Funding and current Privy policy remain unverified. The CI
watch finished and no local Cargo/rustc/Radar process remained at inspection.
Target measured 35.1 GiB with 57.4 GiB free; previously rejected ignored-output
cleanup was not retried. This verified handback is committed locally for the
next source push, avoiding a redundant documentation-only full CI run.

### Protected current holdings comparison (2026-10-09)

Add the actual radar-issuer --review-inventory caller over its configured
private Snapshot. Reuse existing wallet balance/context/time validation and
replay-derived acquisition signature/cost validation under the journal lock.
Require current snapshot wallet/time and the exact journal checkpoint, account
set equality across listings and normalized raw verification, known states,
wallet ownership, raw context within every enumeration/review bound, and token
listings not preceding retained acquisitions. Unsupported/missing/changed
inputs refuse the whole report. Stdin cannot supply the snapshot or history.

Compare checked observed and acquired u64 totals over the union of mints; same
mint program/units must agree across accounts and lots. Report exact units,
unexplained excess and unaccounted reduction. Missing token observations do not
silently discard acquired units; unrecorded observed mints remain unexplained.
Frozen/uninitialized holdings remain quantities with unknown spendability.
Native cash and wrapped token quantities remain separate. Historical basis and
creator groups are retained as history, not current values. Only selected
wallet read fields are reported; unrelated packet data is omitted.

This is normalized protected operator evidence, not independent chain truth or
a second raw binary verifier. Empty observations/history and exact matches
still report incomplete inventory, unknown opening inventory/exposure/loss and
no economic reconciliation. No journal write, portfolio update, operation
closure, reservation release or signing-path integration. Autonomy remains off.

Three unit regressions plus two actual process regressions exercise exact
matches/excess/reductions, multiple accounts/lots, observed/acquired-only mints,
zero/empty observations, checked overflow, mismatched identities/units/states,
duplicates/missing arrays, raw context bounds, acquisition ordering, protected
wallet/checkpoint/time, repeat/restart, no partial output and unchanged history.
Test floor increased by five. Scoped local checks and manual behavior mutation
verification passed below; await full source CI before verified handback.

Local proof: cargo +stable-x86_64-pc-windows-gnullvm test -p radar-signer
passed all 169 tests (72 library, 20 issuer unit, 35 issuer process, 31 signer
process and 11 Privy oracle regressions). Rust 1.99 all-target signer Clippy with
warnings denied and formatting passed. Individually reapplying 17 arithmetic,
identity/state/set/context/comparison mistakes failed the running unit tests;
five protected wallet/checkpoint/snapshot-age/read-validation/context-selection
mistakes failed the actual process regression. Original source was restored,
scoped inventory tests and Clippy/formatting passed again. No new dependency,
mutation exclusion or lint suppression. Full CI remains pending. This review
is not accepted as risk state by the signing path.

All 33 repo-conformance checks passed after staging the new module and docs.

### Verified protected inventory comparison handback (2026-10-09)

Source 95ecce31b86a56fe3b2cafba2af87b11b0dd4d7f passed full CI 37885067007:
https://github.com/1xmint/theradar/actions/runs/37885067007 . All jobs completed
successfully: 2,522 Rust and 352 web tests, build, lint, format, MSRV, dependency,
licence and site checks, all four mutation shards and final gate. Shards
0/1/2/3 tested 217/217/217/216 mutants, caught 158/140/179/170 with
59/77/38/46 unviable; none missed. The last shard ran 25 minutes. No repair
push or cancellation. Scoped local signer tests, Rust 1.99 Clippy/formatting,
33 conformance checks and all 22 manual behavior mutations passed as recorded
above. No dependency, mutation exclusion or lint suppression was added.

The actual protected --review-inventory caller compares normalized wallet-read
evidence and revalidated retained acquisitions against the exact journal
checkpoint. Exact matches, unexplained excess and unaccounted reductions remain
read-only comparison outputs. Complete inventory, opening holdings, exposure
and daily loss remain unknown. Existing outstanding claims still block issuance.
The signing path does not consume this report. The protected operator file is
the trust boundary; no independent live adapter or chain truth is established.

Next: a protected opening inventory baseline and transfer/disposal/failed-fee
coverage, followed by current valuation, exposure/loss and idempotent economic
reconciliation. All four major activation areas remain: economic reconciliation;
protected live risk inputs; execution/recovery/exits/scheduling; owner limits,
isolated keys, verified Privy policy/delegation, funding, deployment and bounded
validation. No percentage or launch date claimed.

Autonomous trading remains off; site limits remain drafts. No live chain read,
real credential/key/signature/delegation change, trade, VPS health read or Serve
deployment occurred. Funding/current Privy policy remain unverified. The CI
watch finished and no Cargo/rustc/Radar process remained at final inspection.
Target measured 35.1 GiB with 57.3 GiB free; previously rejected ignored-output
cleanup was not retried. This verified handback is committed locally for the
next source push, avoiding a redundant documentation-only full CI run.

### Immutable opening inventory baseline (2026-10-09)

Add actual radar-issuer --record-opening-inventory over the configured private
genesis snapshot. Reuse protected normalized wallet/raw evidence checks;
retain typed wallet, native cash, grouped token quantities/programs/units,
individual slots and read windows. Opening cost basis remains unknown, not zero.
No raw operator extras, key, provider credential or model field is retained.

The owned OperationLog records this only before all other history. Identical
repeats are no-ops, conflicting replacements refuse, persistence precedes memory
and the checkpoint advances. Replay rejects missing, duplicate, late, wrong
stage/outcome or mixed opening records, even if their hash chain is intact.
The optional correlation field preserves old digests; the new inventory stage
requires the updated reader. Existing histories remain without a baseline;
never discard/recreate history to provision one. Migration is unresolved.

Inventory review consumes the retained baseline with configured wallet, sane
read bounds, current token/native observations not preceding opening contexts,
and acquisition execution slots strictly after the opening context. Duplicate
opening mints refuse. Include native context in the highest opening slot even
for empty token enumeration; a unit regression covers that pre-push correction.
Existing checked arithmetic and program/units checks compare the union of
opening quantities plus retained buys with current observations. Report opening,
acquired, expected and observed units and exact excess/reduction; historical
acquisition costs remain separate. Exact matches/empty quantities still cannot
establish complete inventory, USD exposure/loss or cost basis for opening assets.
No existing claim/portfolio/authority changes or live activation.

Three journal persistence/replay regressions, one opening-bound unit regression
and two protected process regressions were added; test floor increased by six.
Tests cover immutable/idempotent genesis, restart and checkpoint retention,
failed writes, intact but invalid replay, protected normalization, opening-plus-
buy composition, unknown costs/coverage, unchanged history and blocked claims.
Local verification passed as recorded below. Await full source CI before
verified handback. Transfer/disposal/failed-fee coverage, current
valuation/exposure/loss and idempotent economic reconciliation remain next.

Manual proof: seven journal stage/genesis/immutability/replay mistakes, ten
opening identity/window/context/order/duplicate mistakes and two actual
opening-plus-buy composition mistakes each failed running tests. Moving the
memory update ahead of the write and permitting mixed opening correlations also
failed running persistence/replay tests (21 variants total). Original source
restored; scoped opening tests and Rust 1.99 journal/signer Clippy passed.
Nine actual CLI audit unit tests passed; its new inventory stage label is
formatting plumbing, not new economic behavior. Final CLI all-target Clippy was
repeated after confirming the earlier parent sessions had finished; LEARNINGS
52 records the local scheduling mistake. Formatting passed. Final full scoped
suites and conformance passed below; whole source CI remains pending.

Final local proof: 49 journal and 172 signer tests passed after the last mixed
replay case, followed by all 33 repository conformance checks. Scoped Rust 1.99
all-target Clippy for journal/signer and CLI passed against restored source;
formatting and nine CLI audit unit tests passed. All 21 deliberate behavior
mutations were rejected by running regressions. Six new tests; no dependency,
mutation exclusion or lint suppression. Full source CI is pending.

### Verified opening inventory handback (2026-10-09)

Source 941e5c13c5a2723ee7d3cbde5aefc26cbbd9c080 passed full CI 37888812914:
https://github.com/1xmint/theradar/actions/runs/37888812914 . Every job completed
successfully: 2,528 Rust and 352 web tests, build, lint, format, MSRV, dependency,
licence and site checks, all four mutation shards and final gate. Shards
0/1/2/3 tested 231/231/231/228 mutants, caught 167/153/191/179 with
64/78/40/49 unviable; none missed. The last shard's mutation step ran 22 minutes.
No repair push or cancellation. Local verification and all 21 manual behavior
mutations passed as recorded above. No dependency, exclusion or suppression.

The protected issuer can persist immutable normalized opening quantities before
other history, and compare opening holdings plus retained buys with current
protected observations. Opening cost basis, complete economic coverage,
exposure and daily loss remain unknown. Existing outstanding claims still block
issuance. Ordinary issuance does not consume the baseline or comparison report.
The operator file remains the trust boundary; independent live inputs are absent.
Existing histories cannot be reset to invent genesis; migration is unresolved.

Next: protected transfer/disposal/failed-fee coverage, then current valuation,
exposure/loss and idempotent economic reconciliation. Four major activation
areas remain: economic reconciliation; protected live risk inputs; execution,
recovery, exits and scheduling; owner limits, isolated keys, verified Privy
policy/delegation, funding, deployment and bounded validation. No percentage or
launch date claimed. Autonomous trading remains off; site limits remain drafts.

No live chain read, real credential/key/signature/delegation change, trade,
VPS health read or Serve deployment occurred. Funding/current Privy policy
remain unverified. The CI watch finished; no Cargo/rustc/Radar process remained
at local inspection. Target measured 35.1 GiB with 57.1 GiB free; previously
rejected ignored-output cleanup was not retried. This verified handback is
committed locally for the next source push, avoiding redundant documentation-only
full CI.

### Protected failed-execution fee classification (2026-10-09)

Extend actual --review-valuation and --record-valuation to classify and retain
failed execution costs when measured native debit equals network fee, distinct
native accounts show only the wallet fee deduction, all other balances and
signed deltas agree, paired token identities/units/quantities are unchanged,
and acquisition metadata is null. Reuse wallet signature/exact message and
historical price bounds with upward integer rounding. No operator fee-only
assertion is accepted. Zero is retained only when measured. Empty token lists
are not complete inventory. Raw/unrelated provider fields are not retained.

Retain classified fee costs through existing immutable valuation storage;
repeat/restart is idempotent, changed prices refuse even at equal rounded USD.
Successful review shape remains unchanged for older acquisition comparison.
Daily loss and realised PnL stay unknown, claims outstanding, portfolio unchanged.
The acquisition-only history reader still refuses mixed submitted failed costs;
whole-history integration, transfers/disposals, valuation/exposure/loss and
idempotent economic reconciliation remain required before live activation.

Three new regressions cover fee-only native/token classification including
identity, ordering, missing evidence, measured zero and refusals, and actual
protected-process retention/replay/repeats/changed prices/unchanged claims.
The test floor increases by three. Local checks and full CI pending below.

Local proof: all 175 signer tests and 33 conformance checks passed, plus scoped
Rust 1.99 all-target Clippy and formatting. All 18 manually reapplied native,
token, outcome, rounding and record-eligibility mistakes failed running
regressions; restored source passed. An initial duplicate-guard removal did
not compile because it removed the set's type inference; the viable variant
retained insertion and disabled only its refusal, and failed the regression.
No dependency, mutation exclusion or lint suppression. Whole source CI pending.

### Verified failed-execution fee handback (2026-10-09)

Source 1b88f0b3dcd0f305121325b7d221ff0d65b81ec9 passed full CI 37936444798:
https://github.com/1xmint/theradar/actions/runs/37936444798 . Every job completed
successfully: 2,531 Rust and 352 web tests, build, lint, format, MSRV, dependency,
licence and site checks, all four mutation shards and final gate. Each shard
tested 240 mutants; shards 0/1/2/3 caught 175/163/197/190 with 65/77/43/50
unviable and none missed. The last mutation step ran 25 minutes. No repair push
or cancellation. All local scoped checks and 18 manual behavior variants passed
as recorded above. No dependency, exclusion or lint suppression.

The actual protected valuation review and record paths can retain historical
failed-execution fee costs, only with fee-only native effects and unchanged
paired token metadata. These records are durable, immutable and idempotent under
the existing journal ownership rules. They do not close operations or release
claims. Successful acquisition review shape remains compatible. The acquisition-
only history reader still refuses submitted failed costs, rather than omitting
them. Operator-provisioned evidence remains the trust boundary.

Next: mixed acquisition/failed-fee history review with exact retained input
revalidation and artifact uniqueness, followed by transfer/disposal coverage,
current valuation, exposure/loss and idempotent economic reconciliation. All four
activation areas remain: economic reconciliation; protected live risk inputs;
execution/recovery/exits/scheduling; owner limits, isolated keys, verified Privy
policy/delegation, funding, deployment and bounded validation. No percentage or
launch date claimed. Autonomous trading stays off; site limits remain drafts.

No live chain read, real credential/key/signature/delegation change, trade, VPS
health read or Serve deployment occurred. Funding/current Privy policy remain
unverified. CI watch transport timed out; direct run inspection subsequently
verified every job and exact source SHA, and the old host cell is stale. No
Cargo/rustc/Radar process remained at local inspection. Target measured 35.1 GiB
with 57.1 GiB free; previously rejected ignored-output cleanup was not retried.
This handback is committed locally for the next source push, avoiding redundant
documentation-only full CI.

### Mixed retained acquisition and failed-fee history (2026-10-09)

Extend --review-acquisitions to revalidate retained successful buys and classified
failed-execution fees through the same configured-wallet signature/exact message,
facts signature/operation association, full normalized historical price/cost
recomputation and terminal debit checks. A shared signed-artifact set refuses
reusing a transaction across either category. Failed fees remain separate from
token lots, creator basis and rent; checked native/USD totals cover only recorded
failed fees. Empty totals are zero recorded costs, not zero wallet loss.

--review-inventory carries the entire checked history and requires both token
listing contexts and native balance to be no earlier than every retained failed
execution. It compares token lots unchanged; native cash reconciliation and
failed-fee ordering against the opening baseline remain unresolved. Reports are
read-only, stable across restart/repeat and retain completed costs once. Daily
loss/exposure and complete coverage remain unknown; live activation stays off.

Three new regressions cover exact/empty/overflow totals and actual mixed-history
repeat/restart/terminal behavior, separate fee/token accounting, missing/changed/
misassociated/duplicate retained evidence, inconsistent terminal costs and stale
native/token contexts. Existing fee-only process regression now verifies history
integration. Test floor increased by three. Synthetic generic journal fixtures
have no live signing effect; fee reservations fit the fixture's capital.

Local proof: 178 signer tests passed after restoring source. All 11 manually
reapplied bugs failed running regressions: facts operation/signature association,
full review equality, artifact uniqueness, terminal debit consistency, category
classification, fee omission, each checked total overflow and independent native/
token observation floors. Scoped Rust 1.99 lint/format and 33 conformance checks
are pending below, followed by whole source CI. No dependency, mutation exclusion
or lint suppression.

Final local gate: Rust 1.99 scoped all-target Clippy and formatting passed,
followed by all 33 conformance checks. Whole source CI remains pending.

Initial source 6e6af9d: CI 37948602727 shard 3 failed its unmodified baseline
at issuer_process.rs:977:68 with WouldBlock reacquiring the terminal fixture's
journal lock. Holder identity was not captured. Keep the existing owned handle
through that fixture transition; production locking and refusals are unchanged.
LEARNINGS 53 records the evidence and limits. Scoped regression/lint passed;
await every initial job before repair push, then full Linux CI verification.

Repair local proof: all 178 signer tests, scoped Rust 1.99 all-target Clippy,
formatting and all 33 conformance checks passed. Disabling terminal debit
consistency still fails the terminal case after its fixture ownership repair.
Production source is unchanged by this repair. Whole initial CI is still awaited.

First fixture repair f24e968 CI 37951658678: ordinary tests and shard 1 baseline
hit the same WouldBlock at issuer_process.rs:845:10 in the invalid-evidence copy
helper. That repair was incomplete. Borrow the existing owned handle for the
copy helper as well; no production change, retry or weaker refusal. LEARNINGS
53 is corrected with the repeated failure. Await all repair-run jobs before the
next push; verify full Linux CI rather than claiming Windows proves resolution.

Full fixture repair local proof: 178 signer tests passed. Reapplying facts
signature association, facts operation association and terminal consistency
failures individually still fails the mixed-history regression. Restored source,
scoped Rust 1.99 Clippy/formatting and 33 conformance checks passed. The same
owner is borrowed through both transitions; no production change or retry.

### Verified mixed-history handback (2026-10-09)

Source b3679745de588e6812a9b79ce38960aa69121520 passed full CI 37952931702:
https://github.com/1xmint/theradar/actions/runs/37952931702 . Every job completed
successfully: 2,534 Rust and 352 web tests, build, lint, format, MSRV, dependency,
licence and site checks, all four mutation shards and final gate. Shards
0/1/2/3 tested 243/243/243/242 mutants, caught 178/164/201/191 with
65/79/42/51 unviable and none missed. The last mutation step ran 23 minutes.
Both earlier runs completed fully before their repairs were pushed; none was
cancelled. Initial 6e6af9d passed ordinary tests but shard 3's baseline hit the
terminal-fixture reopen. First repair f24e968 still hit the copy-helper reopen
in ordinary tests and shard 1 baseline. Complete repair keeps both transitions
under their existing owner; production locking/refusals remain unchanged.
Holder identity at the earlier contention points remains unknown. LEARNINGS 53
records both failures and the limit of the inference. Complete Linux CI now
passes; this is no claim of a deterministic local reproduction of the window.

The protected history path recomputes both acquisition and failed-fee valuation,
checks exact configured-wallet signature/message, facts operation/signature and
terminal debit, and refuses duplicate signed artifacts across categories.
Recorded failed fees remain separate from token quantities/basis/rent. Checked
native/USD fee totals cover retained costs only. Inventory review carries this
history and refuses native/token contexts predating retained failed executions.
Reports are read-only, stable across replay/repeat and retain terminal costs once.
Local 178 signer tests, Rust 1.99 scoped all-target Clippy/formatting and 33
conformance checks passed; all 11 manual behavior variants failed regressions.
Facts association and terminal consistency faults still fail after the complete
fixture repair. No dependency, mutation exclusion or lint suppression.

Next: transfer/disposal and complete cash-flow coverage, including failed-fee
ordering against opening inventory, then current valuation, exposure/loss and
idempotent economic reconciliation. Four activation areas remain: economic
reconciliation; protected live risk inputs; execution/recovery/exits/scheduling;
owner limits, isolated keys, verified Privy policy/delegation, funding, deployment
and bounded validation. No percentage or launch date claimed. Autonomous trading
remains off; site limits remain drafts. Complete inventory/exposure/daily loss
remain unknown; outstanding claims still block issuance.

No live chain read, real credential/key/signature/delegation change, trade, VPS
health read or Serve deployment occurred. Funding/current Privy policy remain
unverified. CI watches ended; no Cargo/rustc/Radar process remained at local
inspection. Target measured 35.1 GiB with 58.3 GiB free; previously rejected
ignored-output cleanup was not retried. This handback is committed locally for
the next source push, avoiding redundant documentation-only full CI.


### Protected net disposal measurement (2026-10-09)

Settlement review/record now measures positive paired wallet/mint net decreases
for successful reviewed SOL Reduce/Exit contexts. Shared acquisition aggregation
preserves strict identity, units, complete pairs and checked totals. Internal
transfers cancel; unknown disposal is omitted to preserve old review shapes.
Immutable repeat/replay keeps claims outstanding. Synthetic process fixtures
exercise the protected reader and retention, not live accepted exit transactions
or exit issuance. Closed accounts with absent post metadata stay unknown.
Sale proceeds, transfer attribution, basis allocation and realised PnL remain
unknown. Acquisition/inventory coverage still refuses incomplete disposal economics.

Local proof: 181 signer tests and 33 conformance checks passed; restored issuer
unit/process tests, Rust 1.99 scoped all-target Clippy and formatting passed. Six
manual faults (outcome/action/quote/direction/positive quantity/retention omission)
failed the disposal regressions. A first synthetic fixture attempted a second
250,005,000-lamport reservation against 49,995,000 free; corrected its fee-sized
intent to 5,000. Production refusal was correct and unchanged. No live chain,
key, delegation, trade, deployment or numeric owner limit change. Full CI is
required before handback. Next: complete disposal/transfer cash-flow coverage,
opening/failed-fee ordering, current valuation and idempotent economic closure.


### Verified disposal handback (2026-10-09)

Source 8c06bd905e078827f5adc170c3423750dcf0b36e passed full CI 37957176724:
https://github.com/1xmint/theradar/actions/runs/37957176724 . Every job succeeded,
including 2,537 Rust and 352 web tests, build, lint, formatting, MSRV, dependency,
licence and site checks, all four mutation shards and the final gate. Shards
0/1/2/3 each tested 248 mutants (992 total), caught 182/168/193/197 with
66/80/55/51 unviable and none missed. The longest mutation step ran 26 minutes.
No check was cancelled or pushed over. Verified with gh run view and completed
job logs through gh api. Local cargo test -p radar-signer -p repo-conformance
passed 181 signer and 33 conformance tests; restored issuer unit/process tests,
Rust 1.99 scoped all-target Clippy and formatting passed. Six manually reapplied
logic faults failed their regressions; all were restored before source commit.

Protected settlement review/record now retains measured net disposal for reviewed
successful SOL Reduce/Exit contexts. Paired identity/units and checked totals are
shared with acquisition review. Synthetic fixtures prove protected process
retention, repeat/replay and immutable conflict refusal, not live exit issuance
or accepted sale transactions. Unknown disposal is omitted, preserving older
buy/failed review shapes. No proceeds, transfer attribution, allocated basis,
realised PnL, portfolio update or claim release is inferred.

Next: complete disposal/transfer cash flows, opening/failed-fee ordering, current
valuation/exposure/loss and idempotent economic reconciliation. Four activation
areas remain: economic reconciliation; independently constructed live risk
inputs; execution/exits/recovery/scheduling; owner numeric limits, isolated keys,
verified Privy policy/delegation, funding, deployment and bounded validation.
No percentage or activation date claimed. Live autonomy remains off and site
limits remain drafts. Outstanding claims still block issuance; complete wallet
inventory, current exposure and daily loss remain unknown.

No live chain read, real credential/key/signature/delegation change, trade, VPS
read or deployment occurred. Funding/current Privy policy remain unverified.
CI watch completed; no Cargo/rustc/Radar process remained at final inspection.
Target measured 35.1 GiB and free disk 58.2 GiB. Previously rejected cleanup was
not retried. This verified handback is committed locally for the next source
push, avoiding a redundant documentation-only full CI run.


### Protected sale proceeds breakdown (2026-10-09)

The existing protected valuation review/record caller now accepts a typed sale
cash-flow breakdown for successful reviewed SOL Reduce/Exit operations. Require
exact operation, wallet, signed artifact, mint/program/units and measured positive
net disposal, and an explicit absence-of-other-cash-flows assertion. Checked gross
plus rent refund minus fee/tip/rent paid must balance the measured signed wallet
effect; each side fits u64. Credits, zero effects and fee-dominated debits are
supported without inventing proceeds from wallet credit alone. Credits round USD
down, costs up; signed net trade proceeds exclude rent. Mixed acquisition/sale
classification refuses. No token basis allocation, realised PnL, risk update,
claim release or live issuance. Acquisition/inventory history still refuses
incomplete sale economics. Operator attribution is not independent chain origin.

Synthetic protected-process fixtures cover exact review, credits/zero/debits,
normalization, immutable changed-economics refusal and repeat/replay retention.
Shared historical price checks remain before classification. Existing disposal
fixture is factored without changing production locking or adding reacquire
windows while a child runs. Local proof: restored-source 186 signer tests (five added), 33 conformance
checks, Rust 1.99 scoped all-target Clippy and formatting passed; 16 manual logic faults caught: operation/artifact,
action/quote/units, disposed/gross zero, other flows, exact equation, credit/debit
rounding, rent separation, three overflow paths and combined classification.
No dependency, lint suppression or mutation exclusion. Full CI is required.

No live chain read, credential/key/delegation change, trade, owner-limit change,
VPS read or deployment. Next: basis allocation and disposal-history accounting,
complete transfer/cash-flow coverage, opening/failed-fee ordering and idempotent
economic reconciliation; then independent live inputs and execution/recovery.
Autonomy remains off; current funding and Privy policy remain unverified.


Sale follow-up audit: the new classification branch returned before debit-only
valuation's reservation bound checks. Protected settlement recording already
checks these bounds, but sale valuation must also refuse inconsistent retained
facts introduced through a generic journal caller. Explicitly pass recorded
reservation to sale review and reject excessive network fee or net wallet debit.
Credit/zero and valid fee-dominated cases remain supported. Direct and dispatcher
regressions cover exact boundaries; reapply fee/debit/dispatcher faults before
handback. Initial source 4595042 CI remains awaited in full before repair push.
No live activation or production deployment. Two tests added for this repair.


Reservation repair local proof: 188 signer tests, 33 conformance checks,
Rust 1.99 scoped all-target Clippy and formatting passed. Three additional
manual faults (fee bound, debit bound, reservation forwarding) failed their
regressions; restored source passed. LEARNINGS 54 records the source review
finding and the limits of its evidence. Initial CI is still awaited.


### Verified sale proceeds handback (2026-10-09)

Source 3e0e4f82d63fccba34f1c0be35ee6f75e48d95e8 passed all jobs in CI
37965634236: https://github.com/1xmint/theradar/actions/runs/37965634236 .
2,544 Rust and 352 web tests passed, as did build, lint, formatting, MSRV,
dependency, licence and site checks, all four mutation shards and final gate.
Shards 0/1/2/3 tested 262/262/262/261 mutants (1,047 total), caught
196/178/208/208 with 66/84/54/53 unviable and none missed. The longest mutation
step ran 24 minutes. Verified with gh run view and completed job logs via gh api.
Initial 459504270032aa08d7581a9103163d5b511c0ac4 also passed whole CI
37962107913: 2,542 Rust/352 web tests; shards tested 260/260/260/258,
caught 194/176/206/205 with 66/84/54/53 unviable, none missed, final gate green.
That run completed fully before the repair push; neither run was cancelled.

The follow-up reservation finding was a source review finding while initial CI
ran, not a live execution failure. Initial tests passed without those new
regressions. LEARNINGS 54 records the missing fee/net-debit reservation checks
and explicit dispatcher forwarding repair. Repaired source passed 188 signer
and 33 conformance tests, Rust 1.99 scoped all-target Clippy and formatting.
All 19 manually reapplied logic faults failed their regressions; overflow faults
still fail after factoring their unit fixture. Source was restored before push.
No dependency, lint suppression or mutation exclusion was added.

Protected review/record now retains exact bound operator sale cash flows. Gross
plus rent refund minus fee/tip/rent paid must match the measured signed wallet
effect, with checked native totals and fee/net debit within the recorded claim.
Credit/zero/fee-dominated effects are supported. Credits round USD down, costs
up; signed net trade proceeds exclude separately priced rent paid/refunded.
Mixed acquisition/sale classification and changed immutable economics refuse.
Synthetic process fixtures exercise exact artifacts, reading, durable repeats
and replay; they do not establish a network-accepted venue sale. No disposed
basis, realised PnL, complete cash-flow coverage, risk update or claim release
is inferred. Acquisition/inventory history still refuses incomplete sales.

Next: basis allocation and disposal-history accounting, complete transfer/cash
flows, opening/failed-fee ordering, current valuation/exposure/loss and idempotent
economic reconciliation. Four activation areas remain: economic reconciliation;
independent live risk inputs; execution/exits/recovery/scheduling; owner numeric
limits, isolated keys, verified Privy policy/delegation, funding, deployment and
bounded validation. No percentage or activation date claimed. Autonomy is off;
site limits remain drafts and outstanding claims still block issuance.

No live chain read, real credential/key/signature/delegation change, trade,
owner-limit change, VPS read or deployment occurred. Funding/current Privy policy
remain unverified. CI watches completed; no Cargo/rustc/issuer/signer process
remained at final inspection. Target measured 35.1 GiB; free disk 58.2 GiB.
Previously rejected cleanup was not retried. This verified handback is committed
locally for the next source push, avoiding redundant documentation-only full CI.


### Revalidated mixed sale history (2026-10-09)

The existing owned-journal acquisition-history caller now reconstructs and
revalidates complete retained sale proceeds alongside buys and failed fees.
The entire normalized review must match; shared signed-artifact deduplication
spans all categories. Sales retain reviewed creator, exact proceeds and execution/
price watermarks separately, without changing acquisition totals. Every sale
terminal state refuses because native Completed spend does not reconcile sale
credits or disposed basis. Nonempty sales explicitly refuse inventory comparison
until disposal quantities and basis can be reconciled. No realised PnL or daily
loss is inferred and outstanding claims remain held.

Two synthetic process regressions cover durable mixed buy/sale history, repeat/
replay without writes, exact disposal/proceeds, missing or changed classification,
altered normalized amounts, duplicate signed artifacts and terminal sales.
Fresh checkpoint and snapshot contexts prove the inventory refusal is the sale
guard. Six manual logic faults failed these regressions: missing sale input,
omitted sales output, terminal bypass, normalization bypass, artifact dedupe
bypass and inventory-sale bypass. Source was restored. Plumbing-only factoring
is not separately mutated. Restored-source 190 signer tests and 33 conformance
checks passed, along with Rust 1.99 scoped all-target Clippy and formatting.
Whole CI remains pending. Test floor raised by two.

Next: allocate disposed token basis and reconcile chronological sale quantities,
then complete cash-flow/transfer coverage, current valuation/exposure/loss and
idempotent application. Four activation areas remain: economic reconciliation;
independent live risk inputs; execution/exits/recovery/scheduling; owner limits,
isolated keys, verified Privy policy/delegation, funding/deployment and validation.
Autonomy remains off. No live chain read, credential, key, delegation, trade,
owner-limit, VPS or deployment change. Current funding/policy remain unverified.


Mixed-sale initial CI 37973753478 failed mutation shard 3's unmodified
baseline at issuer_process.rs:330:63 in the existing sale fixture's disposal
constructor (WouldBlock reacquiring operations.jsonl.lock). Both new mixed-sale
regressions passed in that baseline. Return the signed source fixture's known
owner through a shared helper, and keep it for disposal construction. Production
locking, test parallelism and refusal semantics stay unchanged. LEARNINGS 53
records this additional occurrence; the actual lock holder was not captured.
Repair local proof: 190 signer tests and 33 conformance checks passed, along
with Rust 1.99 scoped all-target Clippy and formatting. Fixture-only ownership
plumbing is not separately mutated; the six behavior faults already failed
their regressions with restored production source. Await every initial job
before repair push; whole repaired Linux CI remains required.


### Verified mixed sale history handback (2026-10-09)

Repaired source ba80fe2c8e28661cbd9970815bbe088e55ccea62 passed every job
in CI 37976776508: https://github.com/1xmint/theradar/actions/runs/37976776508 .
2,546 Rust and 352 web tests passed, along with build, lint, formatting, MSRV,
dependency, licence/site checks, all four mutation shards and the final gate.
Shards 0/1/2/3 tested 263/263/263/262 mutants (1,051 total), caught
197/179/209/207 with 66/84/54/55 unviable (792 caught, 259 unviable),
none missed. The last mutation step ran 25 minutes. Verified by gh run view
and completed job logs through gh api.

Initial source 3b99e8b74c7a915b971f5916f02e98c17f768eb4 completed CI
37973753478: 2,546 Rust/352 web tests and all ordinary jobs passed, plus
mutation shards 0/1/2 (263 each, 197/179/209 caught, 66/84/54 unviable).
Shard 3 failed its unmodified baseline at issuer_process.rs:330:63 with
WouldBlock in the existing disposal fixture's immediate journal reopen. Keep
the known source owner through disposal construction. LEARNINGS 53 records
the observed failure and the unknown lock-holder identity. Every initial job
and final gate completed before the repair push; neither run was cancelled.

The existing protected history caller now reconstructs complete normalized
sale inputs and revalidates their exact signed binding and full valuation.
Sales retain exact proceeds, attribution and execution/price watermarks in a
separate array. Shared artifact deduplication spans buys, sales and failed fees.
Changed/missing economics and every sale terminal state refuse. The inventory
caller explicitly refuses nonempty sales until disposed quantity and basis
reconciliation exist. Repeat/replay writes nothing and leaves both claims held.
190 local signer tests, 33 conformance checks, Rust 1.99 scoped Clippy and
formatting passed. Six manual logic faults failed the regressions; source was
restored. No dependency, lint suppression or mutation exclusion was added.
Synthetic transactions exercise the protected process, not network-accepted
venue execution or independently authenticated cash-flow origin.

Next: allocate disposed token basis and reconcile chronological sale quantities;
then complete transfer/cash-flow coverage, opening/failed-fee ordering, current
valuation/exposure/loss and idempotent economic application. Four activation
areas remain: economic reconciliation; independent live risk inputs; execution/
exits/recovery/scheduling; owner numeric limits, isolated keys, verified Privy
policy/delegation, funding, deployment and bounded validation. No percentage
or activation date is claimed. Autonomy remains off; site limits are drafts,
current funding/policy remain unverified and outstanding claims block issuance.

No live chain read, real credential/key/signature/delegation change, trade,
owner-limit change, VPS read or deployment occurred. Final inspection found
no Cargo/rustc/issuer/signer processes. Target measured 35.1 GiB, disk free
60.1 GiB. No rejected cleanup was retried. Both CI watches completed. This
verified handback is committed locally for the next source push, avoiding a
redundant documentation-only full CI run.


### Recorded FIFO disposal basis and inventory (2026-10-09)

The existing owned-journal history reader now calls the FIFO basis allocator
on revalidated buys/sales. Require a retained opening snapshot and zero opening
quantity for every sold mint; absent/positive unknown opening purchase costs
leave accounting null. Shared opening identity/read/unique-mint bounds apply.
Every trade follows the highest opening context; same-mint slot ties refuse.
Only older same-mint lots can supply a sale. Partial basis rounds micro-USD up
and exact remaining basis/units stay with each lot; large products use u128,
checked allocation sums and signed net proceeds minus basis refuse overflow.
Preserve original acquisition/creator attribution for each sale allocation.

The report describes recorded trades only, with remaining lots and per-sale
recorded trade PnL. Rent and failed fees stay separate; complete external flows,
current exposure and daily wallet loss remain unknown. Inventory comparison
uses known remaining quantities and preserves acquired/disposed counts; native
and both token observations must cover every sale. Without known FIFO basis,
nonempty sales still refuse comparison. No journal/portfolio update or claim
release; outstanding operations still block issuance.

Six regressions added: four allocator tests and two actual issuer-process tests.
They cover FIFO chronology, crossing lots, partial rounding conservation, full
exit, zero/large basis, overflow, absent/positive opening quantity, baseline and
trade units, ties, oversales, exact repeated history, remaining inventory and
independent native/token observation floors. All 17 manual logic faults failed
these regressions: missing/unknown opening, reverse FIFO, opening boundary,
changed units, tied slots, zero acquisition, rounding, quantity/basis remainders,
disposal decrement, oversale acceptance, PnL sign, token/native floors, original
quantity substitution and altered disposed count. Source restored. Fixture
parameter/module wiring is plumbing and not separately mutated. Test floor +6.
Restored-source 196 signer tests and 33 conformance checks passed, with Rust
1.99 scoped all-target Clippy and formatting. Whole CI remains pending.
No dependencies, lint suppressions or mutation exclusions added.

Next: complete transfer/cash-flow coverage and native cash reconciliation,
opening/failed-fee ordering, current valuation/exposure/loss and idempotent
application. Four activation areas remain: economic reconciliation; independent
live risk inputs; execution/exits/recovery/scheduling; owner numeric limits,
isolated keys, verified Privy policy/delegation, funding/deployment/validation.
Autonomy remains off; no live chain read, credential/key/delegation change,
trade, owner-limit change, VPS access or deployment. Funding/policy unverified.


FIFO boundary follow-up: initial CI 37981032392 shard 3 reported two missed
mutations at inventory.rs:180:17 and :180:35 (> to >=). The production bounds
already accept equality, but the process regression exercised only newer and
older snapshots. Add successful exact-sale-slot comparison alongside the newer
case. Reapply each mutation at its exact reported position: both fail the
extended process regression, then restore source. LEARNINGS 55 records the gap.
The manual fault total is now 19. Restored 196 signer tests, 33 conformance
checks, scoped Rust 1.99 Clippy and formatting passed. Await every initial job
before repair push; whole repaired CI remains pending. No live change.


### Verified FIFO disposal accounting handback (2026-10-09)

Source caa96f0ada7ed69898ded0ab24d291c06ca71073 passed every job in
CI 37983976833: https://github.com/1xmint/theradar/actions/runs/37983976833 .
2,552 Rust and 352 web tests passed, with build, lint, formatting, MSRV,
dependency, licence/site checks, all four mutation shards and the final gate.
Shards 0/1/2/3 tested 272/272/272/270 mutants (1,086 total), caught
206/183/221/208 with 66/89/51/62 unviable (818 caught, 268 unviable),
none missed. The final mutation step ran 25 minutes. Proof read with gh run view
and completed job logs through gh api; head SHA matches the repaired source.

Initial source f5115048a73213a4c0842a87839e916888e4c90b completed CI
37981032392: 2,552 Rust tests and every ordinary check passed, as did mutation
shards 0/1/2 (272 each; 206/183/221 caught and 66/89/51 unviable). Shard 3
reported two missed > to >= mutations at inventory.rs:180:17 and :180:35,
with 270 tested, 206 caught and 62 unviable. The original code accepted exact
sale-slot observations correctly, but its process test omitted equality.
Extend that test to accept exact-slot and later observations, keeping independent
older native/classic/Token-2022 refusals. Both reported mutants were reapplied
at their exact positions and failed. LEARNINGS 55 records this test gap.
Every initial job and final gate completed before the repair push; neither run
was cancelled. Repaired whole CI caught both previously missed mutations.

The existing owned history reader now allocates recorded sales against FIFO
buy lots and retains exact remaining quantities/basis. Opening wallet/read/
unique-mint checks are shared with inventory. Require an opening snapshot and
zero opening quantities for sold mints; unknown opening costs stay unknown.
Trade slots must follow the opening read; same-mint ties and sales exceeding
preceding lots refuse. Partial allocation rounds micro-USD upward, uses wide
products and conserves exact remainder across later sales. Checked sums and
signed net proceeds minus basis refuse overflow. Each allocation preserves its
acquisition operation and creator. The inventory caller compares the remaining
quantities, separately preserving original acquired and recorded disposed counts,
and requires native/token observations at or after every recorded sale.

This remains operator-provisioned recorded-trade bookkeeping. Per-sale recorded
trade PnL excludes separately retained rent/failed fees and does not establish
complete wallet realised PnL or daily loss. External transfers/cash flows, native
cash reconciliation, current valuation/exposure/loss and idempotent application
remain unfinished. Reading/replay writes nothing; claims stay outstanding and
block issuance. 196 local signer tests, 33 conformance checks, Rust 1.99 scoped
all-target Clippy and formatting passed after restoration. All 19 manually
reapplied logic faults failed their regressions. Six tests added; no dependency,
lint suppression or mutation exclusion. Synthetic process fixtures do not
establish network-accepted venue execution or independent evidence provenance.

Next: complete transfer/cash-flow coverage and native cash reconciliation,
opening/failed-fee ordering, current valuation/exposure/loss and idempotent
application. Four activation areas remain: economic reconciliation; independent
live risk inputs; execution/exits/recovery/scheduling; owner numeric limits,
isolated keys, verified Privy policy/delegation, funding/deployment/validation.
Autonomy remains off. No percentage or activation date claimed. Site limits
remain drafts; current funding and Privy policy are unverified.

No live chain read, real credential/key/signature/delegation change, trade,
owner-limit change, VPS read or deployment occurred. No local Cargo/rustc/
issuer/signer process remained at inspection. Target measured 35.1 GiB; disk
free 57.1 GiB. No rejected cleanup was retried. Both CI watches completed.
This verified handback is committed locally for the next source push, avoiding
a redundant documentation-only full CI run.


### Recorded native cash comparison increment (2026-10-09)

Continue after the verified FIFO handback. Add derived recorded native effects
for every replay-validated buy, sale and failed-fee operation, preserving existing
signed-artifact deduplication. Validate exactly one wallet effect and agreement
between pre/post native balances and retained signed deltas. No durable schema
or record hash changes. The existing inventory reader compares opening native
cash plus ordered effects against every transaction pre-balance and the current
native read. Missing opening remains null; ambiguous same-slot executions,
executions at/before the highest opening context, future executions and any
intermediate balance outside u64 refuse. Equality at the current slot is valid.

Retain both signed discrepancies and separate final/transaction-anchor match
flags. Never absorb an unexplained flow by resetting the projection. A final
match cannot hide a prior gap or establish complete transfer coverage. Keep
external cash flows incomplete, all economic completion/update/release flags
false, and exposure/daily loss unknown. Process regressions cover exact retained
buy/sale/failed-fee projection, repeat reads and unchanged journal/claims. Four
unit tests cover signs, sorting, equality, gaps, zero and wide balances,
intermediate under/overflow, missing opening and inconsistent wallet effects.
Five new tests raise the Rust floor to 2160.

All 16 manually reapplied cash logic faults failed a regression; source restored.
A first deletion of the same-slot check was unviable due to BTreeSet inference;
repeat retaining the insertion while suppressing refusal failed as required.
Plumbing is exercised by process tests rather than manually mutated. Initial
201 signer tests passed. Formatting returned a session ID before the first test
command started; both parent sessions were then polled to final successful exits.
This was an orchestration mistake under LEARNINGS 52, not evidence that a yielded
formatter had completed. Subsequent Cargo commands must remain strictly serial.
Restored 201 signer tests and 33 conformance checks passed. Rust 1.99 scoped
all-target Clippy passed after changing the test helper to borrow its slice;
formatting passed and all four cash tests passed after that test-only change.
Full CI is pending. Previous CI 37983976833 was read
as completed success at caa96f0ada7ed69898ded0ab24d291c06ca71073 before pushing.

Next: complete transfer/cash-flow coverage, independently sourced current
valuation/exposure/loss and idempotent application; then live risk inputs,
execution/exits/recovery/scheduling and owner limits/keys/Privy policy/delegation/
funding/deployment/validation. Autonomous trading remains off; no activation
percentage or date. No live read, real key/signature/delegation/trade, VPS access,
Serve deployment or owner-limit change in this increment.


### Verified recorded native cash comparison handback (2026-10-09)

Source 8c7881480f556aceaa7049b84f0ba5443df3247a passed every job in
CI 37988123308: https://github.com/1xmint/theradar/actions/runs/37988123308 .
2,557 Rust and 352 web tests passed, with build, lint, formatting, MSRV,
dependency, licence/site checks, all four mutation shards and the final gate.
Shards 0/1/2/3 tested 279/279/279/276 mutants (1,113 total), caught
213/190/223/216 with 66/89/56/60 unviable (842 caught, 271 unviable),
none missed. The slowest mutation step ran 27 minutes. Read completed job logs
through gh api and matched the head SHA through gh run view. Every job completed;
no repair push or cancelled run. PR 334 description updated with this evidence.

The owned history reader derives exact native pre/post/delta effects only after
full retained binding/valuation replay and cross-category artifact deduplication.
The existing inventory reader projects opening cash through recorded buys,
sales and failed fees in slot order, compares every transaction pre-balance and
the current native read, and preserves signed unexplained discrepancies. A final
match cannot hide an earlier anchor gap. Missing opening stays null; same-slot
ordering, execution at/before the highest opening context, future native effects
and intermediate balance under/overflow refuse. Exact current-slot equality is
accepted. Coverage remains recorded operations only, not complete transfers or
independent chain provenance. No risk-state update, daily-loss conclusion,
operation closure or claim release. All 16 manually reapplied logic faults
failed their regressions and were restored. 201 local signer tests, 33
conformance checks, Rust 1.99 scoped all-target Clippy and formatting passed;
the four cash unit tests passed after the final test-helper borrowing change.
Five tests added, floor raised to 2160. No dependency, lint suppression, mutation
exclusion or durable-record schema change. Process fixtures remain synthetic.

Next: complete external transfer/cash-flow coverage, current valuation/exposure/
loss and idempotent application. Four activation areas remain: economic
reconciliation; independent live risk inputs; execution/exits/recovery/scheduling;
owner numeric limits, isolated keys, verified Privy policy/delegation, funding,
deployment and bounded validation. Autonomous execution remains off. The site
limits are still drafts; current funding and Privy policy are unverified. No
activation percentage or date is claimed. This increment was not deployed;
last Serve deployment remains 3f38ee0ffaf2d1e349f54b43475f36c010bb335d.

No live chain read, real key/credential/signature/delegation/trade, owner-limit
change, VPS access or deployment occurred. No local Cargo/rustc/issuer/signer
process remained at inspection. Target measured 35.1 GiB; disk free 57.2 GiB.
No rejected cleanup was retried. CI watches ended. This verified handback is
committed locally for the next source push, avoiding redundant doc-only CI.


### Supplied external native transfer increment (2026-10-09)

Continue from verified native cash handback 35dbc4c / source 8c78814. There is
no independent wallet-activity enumeration yet. Add optional protected snapshot
native-transfer evidence to the existing inventory reader: exact signed legacy
plain System transfers, verified sender signature, supported writable accounts,
empty token effects and exact all-account balance/fee equations. Include deposits,
withdrawals, self-transfers, multiple instructions and fee-only failures. Require
current read metadata, finalized/configured identity, unique signatures both
within supplied evidence and against retained operations, and slots not beyond
the current native observation. Unsupported activity refuses, never skips.

Combine normalized supplied transfer effects with retained native operation
flows in the existing opening cash comparison. Preserve signature/source in
anchors, exact equality, opening ordering, ambiguous-slot refusal, signed gaps
and intermediate range checks. A supplied deposit can explain a retained-buy
cash gap, but even exact final/anchor matches leave external coverage incomplete.
This is protected operator snapshot input, not independent chain provenance or
a durable transfer journal. No risk-state application, claim release or trading
authority. Seven new tests raise floor to 2167.

Local review caught self-transfer cancellation hiding an impossible intermediate
debit. Require successful amounts to fit the post-fee payer balance after earlier
instructions. LEARNINGS 56 records the correction. All 30 manual logic faults
failed their regressions, including signature bypass, historical/local duplicate
counting, metadata identity, readonly target, unknown programs/opcodes, extra
instruction bytes, signed balance/fee math, intermediate self-transfer bounds
and omission from the inventory cash projection. Restore all source after each.
Initial 208 signer tests passed before that additional debit guard; restored
208 signer tests, 33 conformance checks and formatting passed after restoration.
Whole CI is pending. Scoped Rust 1.99 all-target Clippy passed after
using explicit PathBuf defaults in the unit fixture. No dependency, lint
suppression or mutation exclusion. No live read, real signature/key/delegation/
trade, owner limit change, VPS access or deployment. Autonomous execution remains
off. Next remains complete activity enumeration/coverage, durable reconciliation
and current exposure/loss, independent live risk inputs, execution/recovery and
owner limits/Privy policy/delegation/funding/deployment validation.


### Native-transfer initial CI baseline lock failure (2026-10-09)

CI 37993754272 for 6d62b4ba3948891756d01fc02adbd3d378629482 failed
mutation shard 1's unmodified baseline before mutation results. The mixed
invalid-fee history regression panicked opening the journal at
issuer_process.rs:1482:60 with WouldBlock. The inventory fixture had released
its known owner immediately before append_failed_history reopened it. Preserve
that handle through shared owned-fixture and failed-history helpers for both
mixed valid/invalid setups. The actual valuation subprocess still runs after
releasing ownership and must finish before reacquiring it. Production locking,
nonblocking refusal and all assertions remain unchanged. No retry, sleep, lock
file deletion or test serialization. LEARNINGS 53 extended; actual contending
holder unknown. Restored 208 signer tests, 33 conformance checks, scoped
Rust 1.99 all-target Clippy and formatting passed. Repaired full CI is pending.
Await all initial jobs/final gate before repair push; do not cancel the run.
No live change or autonomous activation.


While initial CI continued, extend the native-transfer unit regression to sign
unsupported signer/writable headers and correctly framed short/extra instruction
account lists. All three additional exact logic faults fail these tests (33
manual faults total); restore source. All five native-transfer unit tests,
scoped Rust 1.99 all-target Clippy and formatting pass after the test extension.
Production transfer logic is unchanged by this follow-up. Repaired full CI
remains required; wait for every initial job before pushing.


### Native-transfer exact survivor follow-up (2026-10-09)

Initial CI 37993754272 fully completed failure before any repair push.
2,564 Rust tests and ordinary jobs passed. Shards 0/2 passed with 296 each,
228/246 caught and 68/50 unviable. Shard 1 failed its unmodified baseline on the
fixture lock described above, producing no mutation counts. Shard 3 tested 293,
with 2 missed, 226 caught and 65 unviable. Exact survivors at native_transfers.rs:
32:58 and 38:9 changed || to &&. Production guards were correct. The new signed
unsupported-header cases added while CI ran catch the first. Make duplicate
account metadata satisfy the later balance equation so the identity guard alone
must refuse it. Both survivors were reapplied at their exact positions and failed
native-transfer regressions; restore source. Manual fault total now 35.
LEARNINGS 57 records the masked guards. No production logic, dependency, lint
suppression or mutation exclusion changed. Restored 208 signer tests, 33
conformance checks, scoped Rust 1.99 all-target Clippy and formatting passed.
Repaired whole CI pending. No cancelled CI job or live change.


### Native-transfer second baseline fixture repair (2026-10-09)

Repair 3baa4ccaa4ff2bdf5792b0d5a460a491ef86d066 CI 37995345383 failed
shard 1's unmodified baseline at issuer_process.rs:1321:60, in the native-cash
regression's immediate journal reopen after inventory_fixture_opening. Preserve
that known owner through the first checkpoint read and return the failed-history
owner for the second read. All four immediate sale-history setups now receive
their existing inventory owner too. The first repair was incomplete; actual
contending holder remains unknown. Necessary subprocess boundaries still release
ownership and await completed output before a later open. Production locking and
all assertions are unchanged; no retry, sleep, deletion or serialization. This
is fixture plumbing, with no changed production logic to mutate. Existing 35
manual logic faults remain the behavior evidence. Restored 208 signer tests,
33 conformance checks and scoped Rust 1.99 all-target Clippy passed locally.
Whole repaired Linux CI remains required. Await every current job and final gate
before pushing this additional repair. No live change or autonomous activation.


### Native-transfer wire-size survivor repair (2026-10-09)

CI 37995345383 fully completed before the next repair push: all ordinary jobs
and shards 0/2 passed (296 each; 228/246 caught and 68/50 unviable). Shard 1
failed the unmodified fixture baseline described above. Shard 3 tested 293 with
1 missed, 227 caught and 65 unviable. The new survivor at native_transfers.rs:
32:28 changed the first OR to AND. The oversized test appended invalid trailing
bytes, masking this guard through a later decoder refusal. Replace it with a
correctly signed, decodable 1249-byte message containing another complete zero
transfer. Reapplying the exact :32:28 mutation fails this strengthened regression;
restore production source. Manual logic fault total is now 36. LEARNINGS 57
extended; production behavior and mutation exclusions unchanged. Full repaired
CI remains required. No live wallet, deployment or autonomous activation.

Restored source passed all 208 signer tests and 33 conformance checks, scoped
Rust 1.99 all-target Clippy, and formatting after normalizing the edited file's
newline style. The expected manual mutant failure was at the oversized-message
refusal assertion. No production guard changed. Both prepared repairs will be
pushed together now that every job of CI 37995345383 has completed.


### Handback: native-transfer repairs submitted (2026-10-09)

Source 1438626dda4422372e83ca5dcf85c913f9e28219 includes the prepared
journal-ownership fixture repair 55c42fd and the wire-size regression repair.
Both were pushed only after every job/final gate of CI 37995345383 completed.
CI 37999235384 is queued at handback; full repaired Linux evidence is pending,
not a claimed pass. PR 334 records that status. Local restored evidence remains
208 signer tests, 33 conformance checks, scoped Rust 1.99 all-target Clippy and
formatting; all 36 manual behavior faults caught, source restored. Working tree
was clean after source push. No active local Cargo/rustc process was found at
prior inspection. No production deployment, owner limit, key, delegation or real
trade changed. Autonomous execution remains off.

Next: inspect every job and final gate of CI 37999235384 before any further
push. Read failed baselines or exact mutation positions if present; do not cancel
or bypass the run. On green, close this increment's verification record, then
continue complete wallet activity coverage/durable economic reconciliation. Live
risk inputs, execution/recovery/exits/scheduling, and activated owner limits plus
verified Privy policy/delegation/funding/deployment validation still remain.


### Handback: native-transfer CI runtime limit and coverage inspection (2026-10-09)

CI 37999235384 attempt 1 completed at source
1438626dda4422372e83ca5dcf85c913f9e28219. All ordinary jobs passed:
2,564 Rust tests, 352 web tests, build, lint, format, MSRV, dependencies, licence
and site. Shards 0/2/3 passed: 296/296/293 tested, 228/246/228 caught and
68/50/65 unviable (885 total, 702 caught, 183 unviable, none missed).
Shard 1's unmodified baseline passed (103s build, 11s test), then its job
exceeded the configured 30-minute maximum. The check-run annotation explicitly
says the maximum execution time was exceeded. It was cancelled by that limit;
no agent cancellation or new push caused it. No mutation totals or survivors
were reported for that shard. The aggregate failed; full verification remains
incomplete. Disk telemetry still showed 76GB free on the runner. No evidence
establishes a wedged test or resource exhaustion.

Rerun only the failed shard with gh run rerun --job against the same source,
preserving the passed checks. Attempt 2 is running; shard-1 job 114061977905
and its dependent final gate remain required. No workflow deadline, mutation
exclusion or production code was changed. If this attempt also reaches the
limit, inspect its logs and discuss the CI runtime/sharding trade-off before
changing the gate or substituting local verification. Do not cancel or push
over the active rerun.

Independent source/protocol inspection is recorded in
[research 0038](../research/0038-wallet-address-history-is-not-complete-wallet-coverage.md).
The existing dossier pager neither explicitly requests finalized commitment nor
provides an accounting interval. Wallet-address signatures and current token
account enumeration do not establish historical token-account coverage. The next
bounded operator collector must preserve failures, cursor/order integrity, exact
raw transaction bindings and explicit incomplete-read reasons. The collector is
not implemented. All 33 conformance checks passed after saving the research as
UTF-8; its initial Windows default encoding was rejected and corrected.

Documentation commits remain local for the next source push. At inspection,
no local Cargo/rustc process was active; target was 35.17GiB with 56.91GiB free
on C. No cleanup was attempted. No real wallet read, key, signing, delegation,
owner limit, deployment or trade changed. Autonomous execution remains off.
Next: read every rerun job/final gate, then close transfer verification on green
and implement the bounded finalized activity collector. Other activation gates
remain live risk inputs, durable economic/risk-state application, execution/
recovery/exits/scheduling and owner limits/verified Privy authority/funding/
deployment validation.


### Bounded finalized address collector, isolated increment (2026-10-09)

While source 1438626 CI 37999235384 attempt 2 remains in flight, implement the
independent read-only collector on feat/wallet-activity-read in the sibling
radar-wallet-activity worktree. It uses only the existing RpcClient/Budget and
does not alter the pending signer source, its fixtures or its workflow. This
collector is finishable offline and cannot authorize capital regardless of the
pending mutation result. No new push interrupts that run.

The operator command wallet-activity-read requires wallet, RPC endpoint and an
exclusive after/inclusive through slot interval. Fetch finalized signature pages
of 32 with a context floor at the upper bound; validate explicit status/outcome,
canonical signature identity, nonincreasing slots and global duplicate/cursor
integrity. Ignore entries above the upper slot and stop at the lower boundary
or provider-reported exhaustion, with page/call/deadline stop reasons. Preserve
failed executions. Fetch base64 raw transactions and bind their first wire
signature, exact slot and outcome to enumeration. Check packet/signature extent
and integer fee; retain raw metadata explicitly as untrusted unresolved input.
No full message decoding, local signature verification or address-membership
proof is claimed. Multi-signature/versioned bytes remain unresolved.

Report signature scan and transaction fetch completion separately. Missing
transactions/provider errors/bounds produce incomplete evidence with retained
entries; malformed or inconsistent rows refuse. The CLI Budget is 36 calls,
three pages and 20 seconds. Wallet coverage, economic reconciliation, portfolio
updates and claim release remain false. No issuer snapshot is installed and no
journal is written. ADR 0008 and research 0038 document these limits. Historical
token accounts, independent metadata provenance, classification and durable
idempotent application remain unfinished.

Eight tests added; floor raised from 2167 to 2175. Restored verification passed
238 CLI tests (224 unit, 14 process), 107 on-chain tests (89 unit, 18 process),
33 conformance checks, scoped Rust 1.99 all-target Clippy and formatting. All 27 manually applied logic
faults failed their regressions: exact interval/packet/signature boundaries,
identity/outcome/fee checks, finalized status, order, duplicate/cursor checks,
full/oversized pages, completion flags, query commitment/context and page budget.
The first local harness was interrupted after five caught faults, with the
sixth temporary guard mutation still present. Restore that guard, complete all
22 remaining faults and verify exact restoration against a saved baseline. No
fault is retained. Normalize newline style with rustfmt after restoration.
No mutation exclusion, dependency or lint suppression added. Full CI for this
collector remains required after the pending run completes. No live read, key,
signing, delegation, owner-limit change, trade or deployment. Autonomy stays off.


### Handback: collector locally integrated, prior CI still running (2026-10-09)

Collector source 025b6c286c53cc8815cab2ad53b4fb87dc60074a is committed on
feat/wallet-activity-read and fast-forwarded locally into
fix/wallet-signin-diagnostics after its isolated checks completed. The sibling
radar-wallet-activity worktree remains clean on its source branch. Local source
is now available in the primary checkout; no remote push or deployment occurred.
At that source, 345 scoped tests (238 CLI + 107 on-chain), 33 conformance checks,
Rust 1.99 scoped all-target Clippy and formatting passed. All 27 manual faults
were caught and restored. The collector itself has not run in GitHub CI.

Prior signer-source CI 37999235384 attempt 2 still runs shard-1 job
114061977905 at handback. It verifies 1438626, not the new collector source.
Wait for its completed shard/final gate before the next push. If it passes,
record exact totals, then push the integrated collector and inspect all of its
CI jobs. If it also exceeds 30 minutes, inspect its annotation/log and resolve
the CI runtime/sharding trade-off instead of repeated blind reruns or dropping
coverage. The repository requires discussion when CI cannot complete the check.
No timeout, matrix, exclusions or checks changed in this increment.

After collector CI, next is local message classification plus historical account
coverage and durable transfer retention/idempotent economic application. Address
collection alone does not establish wallet coverage, loss/exposure or release
claims. Live risk inputs, execution/recovery/exits/scheduling and owner limits
plus verified Privy authority/funding/deployment validation remain. Autonomous
execution stays off. No local Cargo/rustc process was active at final inspection;
C had 61,074,026,496 bytes free. No file cleanup was attempted.


### Read-only mainnet measurement after collector integration (2026-10-09)

At local source 780011b, run the compiled debug wallet-activity-read against
https://api.mainnet-beta.solana.com for the configured owner wallet, after slot
0 through finalized slot 455038266. The command succeeded at host Unix second
1791587961: zero enumerated signatures and zero fetched transactions; scan stop
provider_history_exhausted, scan/fetch finished true. Wallet coverage and economic
reconciliation remain false. This measures the public provider's empty address
history response; it does not validate classification of a real transaction or
prove exhaustive historical token-account coverage.

A separate wallet-read succeeded during host seconds 1791587973..1791587974.
Native balance was exactly zero lamports at finalized reported slot 455038327;
legacy Token and Token-2022 account lists were empty at reported slot 455038328.
The reads had no common reported slot, inventory_complete remained false, and
USD value/realized PnL were unknown. Do not relabel this as complete accounting.
Both JSON outputs are saved in the workstation temporary directory with the
radar-owner-wallet-activity-20261009 and radar-owner-wallet-balance-20261009 names.
No paid RPC, model call, key access, signature, submission or deployment occurred.
The measured wallet still needs funding before any bounded live validation.

Prior source CI attempt 2 was still running at the most recent check. No push
occurred. The next source publication still requires its completed jobs and final
gate; collector CI remains pending. All four activation areas in the preceding
handback remain open. Autonomous execution is off.


### Collected native-transfer adapter, isolated increment (2026-10-09)

On feat/wallet-activity-read, independently of pending source 1438626 CI, connect
optional protected wallet_activity evidence to inventory native cash comparison.
Reuse the native-transfer decoder/signature/effect checks rather than introducing
a second classifier. Reject competing supplied/collected packets, incomplete
scans/fetches, invalid intervals/newer contexts, missing or mismatched entries,
wire identities and raw metadata. Normalize raw u64 native balances only; absent
token effects and unsupported transactions refuse. Existing freshness, wallet
identity, duplicate history and exact fee checks remain. No network, keys,
journal writes, portfolio updates or coverage assertions added. ADR 0008 records
the contract. Two tests added; test floor 2175 to 2177.

Initial fault reapplication found that the equal-bound interval guard could be
weakened without failing the tests: the intended empty-packet assertion had not
been inserted after formatting changed its textual anchor. Add that actual
boundary regression, restore production source, and rerun all fault checks.
Full restored tests, scoped lint and conformance remain required before commit.
No push, deployment or autonomous activation occurred.


Restored adapter verification passed 210 signer tests (72 library, 45 issuer,
51 issuer process, 31 signer process, 11 Privy signature), 33 conformance checks,
Rust 1.99 scoped all-target Clippy and formatting. All 14 manual faults caught:
interval/context equality, row lower/upper bounds, entry extent, coverage and
completion guards, enumeration slot/signature/outcome, verified wire signature,
metadata outcome and fee. Source restored byte-for-byte after the harness;
subsequent changes only improved Clippy assertion diagnostics and documentation.
No mutation exclusion or dependency added. Full collector/adapter CI is pending.

Prior source 1438626 CI 37999235384 attempt 2 completed successfully, including
all ordinary jobs, all four mutation shards and final aggregate. The rerun shard
1 baseline passed in 90s build + 9s test; 296 mutants tested in 28m, 202 caught
and 94 unviable. Combined with preserved shards 0/2/3, 1,181 mutants were tested:
904 caught, 277 unviable, none missed. Ordinary evidence remains 2,564 Rust and
352 web tests. Aggregate job 114069639327 logs shards: success. No workflow
runtime/matrix/exclusion changed and no agent cancelled either attempt. Every
job is now complete, permitting the next source push.

Next: integrate this adapter locally with the collector, push both and read the
new full CI. Then continue historical wallet-account coverage and durable,
idempotent economic application; live risk inputs, execution/exits/recovery and
owner activation requirements still remain. The most recent live wallet read
returned zero SOL. Autonomous execution stays off; no deployment occurred.


### Handback: collector and adapter published for CI (2026-10-09)

Source 6d46e4e09a13789899a7b4ad45a5220e579d019d is integrated into
fix/wallet-signin-diagnostics and pushed to PR 334 after every job/final gate of
prior CI 37999235384 completed successfully. The PR description now describes
the final combined implementation and separates old green evidence from new
pending CI. New CI 38004462702 runs at that exact source: formatting, site,
dependencies and licence checks have passed; remaining ordinary jobs and all
mutation shards were running at the final inspection. No pass is claimed for
the new full run. No further push until every job and final gate completes.
Read exact failed baselines or survivor positions before repairing if needed.

Local adapter evidence is 210 signer tests, 33 conformance checks, scoped Rust
1.99 all-target Clippy/formatting and all 14 logic faults caught/restored. The
collector's prior 345 scoped tests and 27 fault checks are unchanged. Both
worktrees were clean after source publication. No local Cargo/rustc process was
active. Target measured 37,800,603,438 bytes, C free 61,024,964,608 bytes; no
cleanup attempted. The native sibling worktree remains available on its source
branch. This handback is a local documentation commit for the next source push.

Next integration is durable external-transfer retention and idempotent economic
application alongside historical wallet-account coverage. These still cannot be
inferred from an address-history scan or this protected operator comparison.
Independent live risk inputs, execution/exits/recovery/scheduling and activated
owner limits/isolated keys/verified Privy policy/delegation/funding/deployment
validation remain required. No new live read, deployment, key, delegation,
owner limit or trade in the adapter increment. Autonomous execution stays off.


### Durable external-transfer evidence, isolated increment (2026-10-09)

While source 6d46e4e CI 38004462702 runs, work on the independent local
feat/native-transfer-retention branch in the sibling worktree. Add immutable
NativeTransferRecord storage to the existing owned journal and a protected
--record-native-transfers caller. Canonical signed identity, exact normalized
whitelisted facts/review, idempotent repeats, persistence-before-memory and
operation collision checks survive replay. Re-verify retained facts/signatures
and combine identical current/retained transfers once in inventory cash review.
No risk state, reservation, claims, execution authority or completeness changes.
Seven tests added; floor 2177 to 2184. ADR 0008 records the contract.

The initial intact-chain missing-record test hit Journal::NoCorrelation before
replay; give it an unrelated correlation so it reaches the intended association
refusal. A temporary module insertion matched a word in a doc comment instead
of the import; repair the anchor before compilation. No such edit is retained.
Scoped restored verification and manual guard faults remain required.

Pending CI's ordinary tests failed in existing protected_disposal_measurement_
survives_replay_without_releasing_capital at issuer_process.rs:385:13: the child
reported history unavailable. This generic message does not establish the
underlying lock or replay error. Other ordinary jobs passed; mutation jobs still
run. No push/cancellation, retry workaround or production locking change made.
Inspect completed baselines/survivors before a repair; do not assert a cause not
reported by the evidence.


Restored verification passed 504 scoped tests: CLI 238, journal 54 and signer
212, with one pre-existing ignored journal test unchanged. Rust 1.99 all-target
scoped Clippy passed after extracting fixture helpers to satisfy its function
length limit without suppressions. Formatting passed. The affected native-
transfer tests were rerun after that helper-only refactor. Conformance remains
to be checked after staging the new storage module.

All 26 manual logic faults caught and restored: wire count/extent/canonical
identity, evidence/signature binding, immutable and replay conflicts, outcome/
stage/operation associations, operation collisions in both directions, retained
wallet/artifact/review/context/conflicts, checkpoint/freshness/wallet guards and
advanced-record counting. The first artifact fault survived because an existing
record's immutability refusal masked it; add fresh-insert artifact/signature
cases and catch the exact guard removals. LEARNINGS 57 extended. Two harness
substring-uniqueness assertions stopped before applying ambiguous faults;
restrict them to the intended closure/function, finish the checks, and restore
all four production files. No fault, dependency, exclusion or lint suppression
is retained. Full new-source GitHub CI is still required.


All 33 conformance checks passed after staging the storage module. Prior source
6d46e4e CI 38004462702 attempt 1 fully completed: every ordinary job except
tests passed; all four mutation shards and final gate passed. Shards 0/1/2/3
tested 315/315/315/312 mutants, caught 242/219/265/246, unviable 73/96/50/66:
1,257 tested, 972 caught, 285 unviable, none missed. Every unmodified mutation
baseline passed (60s+2s, 93s+8s, 10s+2s, 55s+4s build/test). Final gate logs
shards: success. Ordinary test failure remains the disposal process's generic
history unavailable refusal, without its underlying I/O or replay cause.

Rerun only failed tests job 114069938853 once, after all jobs completed, to
measure repeatability on unchanged source. Attempt 2 is pending; no source push
or cancellation caused it. Do not treat a rerun as an established root-cause
repair. If it repeats, improve evidence and diagnose the exact failure before
another attempt. New retention code still needs its own full CI after this run
completes; no source publication or deployment in this increment.


### Handback: transfer retention locally integrated; old test rerun pending (2026-10-09)

Source 6ad039ff0f69bf447e05a6cdebce5044754a8b33 is committed on
feat/native-transfer-retention and fast-forwarded locally into
fix/wallet-signin-diagnostics. Both worktrees were clean after integration. The
remote remains 6d46e4e. This new retention source has not been pushed or run in
GitHub CI. Local proof: 504 scoped tests, 33 conformance checks, scoped all-target
Rust 1.99 Clippy/formatting and 26 manual faults caught/restored. One previously
ignored journal test is unchanged. Fixture helper refactoring passed affected
native-transfer tests after the full scoped run.

Prior source 6d46e4e CI 38004462702 attempt 2 still runs only tests job
114108474129; all preserved checks including mutation shards/final gate are
successful. Initial ordinary disposal-process failure was history unavailable,
not an established root cause. PR 334 records exact initial mutation totals,
the single rerun and that local retention is unpublished. Do not push over this
run. On green, record its completed full-source result before publishing retention
and inspecting new full CI. On repeat failure, diagnose the concrete underlying
history error before another rerun or repair. No retries/locking relaxations,
CI workflow changes or exclusions added.

No local Cargo/rustc process remained at inspection. Target measured
38,461,382,283 bytes; C free 59,374,178,304 bytes. Measurement process finished.
No cleanup was attempted. No live read, key, limit, delegation, deployment or
trade in this increment. Autonomous execution remains off.

Durable transfer evidence/comparison now exists locally; historical account
coverage and independent metadata provenance still do not. Next is verified
complete economic reconciliation/idempotent portfolio application, followed by
independently constructed live risk inputs, execution/exits/recovery/scheduling,
and owner limits plus isolated keys/verified Privy policy/delegation/funding/
deployment validation. Retention does not authorize spending or release claims.


### History diagnostics and completed prior CI (2026-10-09)

CI 38004462702 attempt 2 completed successfully at source 6d46e4e: all
ordinary jobs, four mutation shards and final gate passed. The rerun tests job
114108474129 reports 2,574 passing Rust tests; web passed 352. Mutation totals
remain 1,257 tested, 972 caught and 285 unviable, none missed. Only the failed
tests job was rerun once on unchanged source. The original generic history
failure did not repeat; its underlying cause remains unknown, not repaired.

The isolated fix/issuer-history-diagnostics follow-up replaces the generic
history-open refusal with stable ownership, I/O, journal, integrity and replay
categories without paths or OS details. It does not retry, relax locking or
change authority. Two regressions cover safe category mapping and actual child
process refusals for held locks, malformed history and invalid operation replay.
All 214 signer tests passed, scoped Rust 1.99 all-target Clippy passed, and five
manual logic faults caught/restored the ownership guard and I/O/journal/
integrity/replay categories. An initial harness assertion stopped before the
I/O fault because formatting split the match arm; scope to the category literal
and completed all four remaining faults. No fault is retained. Formatting and
all 33 conformance checks passed before commit. Retention source 6ad039f and this
follow-up still require new-source full CI. No deployment or live authority.


### Handback: retention and diagnostics published; fresh CI pending (2026-10-09)

Source f3e7f596d69feb0e9ac1bef6cba0babea9095e48 is fast-forwarded from the
isolated diagnostics worktree into fix/wallet-signin-diagnostics and pushed to
PR 334, including retention source 6ad039f. Previous CI 38004462702 fully
completed successfully before this push; no running check was cancelled.
PR description now reflects durable evidence, safe history diagnostics and the
remaining portfolio/coverage gaps. New full CI 38017286781 is in progress at
f3e7f59, not yet verified. Inspect every ordinary job, all four mutation shards
and final gate before another push; do not claim this source green yet.

Local proof for diagnostics: 214 signer tests, 33 conformance checks, scoped
all-target Rust 1.99 Clippy, formatting and five manual faults caught/restored.
Retention proof remains 504 scoped tests, 33 conformance checks and 26 faults.
The old generic history failure remains unexplained; one unchanged-source
rerun passed. New categories improve evidence for a repeat, not root-cause proof.

This handback is a local documentation commit, deliberately not pushed over the
new CI run. No live read, secrets, limits, keys, delegation, deployment or trade
in this increment. Autonomous execution remains off. Next work: inspect new CI,
then complete economic reconciliation and idempotent portfolio application;
independent live risk inputs and execution/exits/recovery/scheduling remain.
Activation also requires owner money limits, isolated keys, verified Privy
policy/delegation, funding and bounded deployment validation.


### Opening account identity retention and comparison (2026-10-10)

Prior source f3e7f59 passed every job and final gate in CI 38017286781:
2,583 Rust tests and 352 web tests. All four mutation baselines passed;
shards 0/1/2/3 tested 333/333/333/332, caught 253/241/276/264 and marked
80/92/57/68 unviable. Total 1,331 tested, 1,034 caught, 297 unviable, none
missed. Final gate reports shards: success. No rerun or workflow change.

Inspection found opening inventory retains mint totals but drops the account
identities needed to keep historical scan targets. New opening records now
persist normalized account address/mint/program/units/state/quantity rows from
validated raw/listed observations. Older records omit this optional field and
report unknown account history; they are not silently upgraded. Protected
review validates opening uniqueness/state/grouping and exact mint totals, then
reports the deterministic union of opening/current accounts. Missing rows keep
null quantities; changed identity/units refuse. Equal mint totals no longer hide
migration between known accounts. Arbitrary provider fields do not persist.

This is known endpoint account evidence only. It does not identify accounts
created and closed between observations, prove complete history or update risk
state, release claims or grant authority. No network/delegation/deployment/trade.
Five new regressions cover old serialization/known-empty durable replay,
account migration/unknowns/conflicts/group corruption and actual protected
opening/review process replay without writing during comparison. Verification
and manual logic faults are recorded below before publication.


Local restored proof: 273 scoped tests (55 journal, 218 signer), with the one
pre-existing ignored journal test unchanged. Rust 1.99 scoped all-target Clippy
and formatting passed after collapsing a conditional and extracting the
observed-inventory helper to satisfy the existing length check. No suppression.
Sixteen manual logic faults caught/restored: account/state uniqueness, grouped
program/units/sums/mint uniqueness/totals, both sides of the address union,
reused mint/program/units, unknown-opening presence, retained-history flag,
opening validation caller and account capture caller. JSON formatting and
unchanged transport plumbing are not separately mutated. Initial duplicate-
check removal was unviable due to inferred BTreeSet type; preserve insertion
while bypassing its result and catch the semantic fault. An opening-file
harness newline conversion and a multiline-pattern assertion stopped before
valid fault execution; byte-preserving writes and scoped pattern fixed the
harness. All production sources restored. All 33 conformance checks passed
after staging the new module. New-source full CI still required.


### Handback: opening accounts published; new full CI pending (2026-10-10)

Source 255337cdaae621a5e044aa54fbd3a042e916c5a0 is pushed to
fix/wallet-signin-diagnostics and PR 334. Prior f3e7f59 full CI 38017286781
was successful before the push, including all four mutation shards and final
gate. New CI 38023346308 runs at 255337c: initial site/deny/licence/fmt passed;
other ordinary checks and four mutation shards remain in progress. Inspect all
completed jobs and final gate before another push. No CI cancellation, timeout,
workflow, dependency, exclusion or authority change. PR description is current.

Local evidence is 273 scoped tests, 33 conformance checks, scoped all-target
Rust 1.99 Clippy/formatting and 16 faults caught/restored. The protected migration
process was rerun successfully after the final capture fault was restored.
This handback commit stays local while CI runs. No Cargo/rustc processes remained
at inspection. No live reads, secrets, money limits, keys, delegation, deployment
or trading in this increment; autonomous execution remains off.

Next: inspect CI, then consume known opening/current account targets in bounded
activity collection and resolve historical account discovery, independent
metadata provenance and economic reconciliation before idempotent risk-state
application. Independent live risk inputs, execution/exits/recovery/scheduling
and owner limits/isolated keys/verified Privy policy/delegation/funding/bounded
deployment validation remain activation requirements. Opening/current account
union is not exhaustive wallet history and must not soften those refusals.


### Known-account bounded activity collection (2026-10-10)

CI 38023346308 still runs at 255337c, with ordinary checks successful and mutation
shards pending at the latest inspection. To avoid cancelling it, use isolated
feat/known-account-activity in the native sibling worktree, based on primary
540c152. No push while that verification is in flight.

Add optional --inventory-review <path> to wallet-activity-read. It validates the
configured wallet and inventory-comparison shape, bounds file reads to one MiB,
and accepts at most fifteen unique account addresses distinct from the wallet,
including retained accounts absent from current observations. Labels are not
authenticated provenance and output explicitly leaves ownership unverified.
Read-only on-chain collection scans sorted unique wallet/known accounts under
one shared 108-call/16-page/20-second budget. Per-address stops stay explicit;
identical cross-address signatures/transactions merge, conflicts refuse. Global
scan/fetch completion requires every address, never one successful scan.

Seven regressions cover merged duplicates/query identity/failed transactions,
slot/outcome/metadata conflicts, shared budget exhaustion and unavailable fetches,
exact target/file-size bounds, unsupported review shapes, and an actual CLI
process that fetches a missing-current account's history. The new known-address
coverage label is deliberately unsupported by the existing single-address native
adapter. This preserves unknown activity, grants no authority, and does not
establish ownership, exhaustive history or economic reconciliation. No live
network measurement, secrets, deployment, delegation or trade in this increment.


Restored local checks passed 352 scoped tests (CLI 241, on-chain 111), Rust 1.99
scoped all-target Clippy and formatting. All sixteen manual logic faults caught
and restored: wallet target inclusion, target maximum, cross-address conflict,
all-target scan/fetch completion, exact file-size and token-count bounds,
duplicate wallet/account targets, authority/wallet/coverage/history/portfolio
shape guards and actual CLI dispatch. The fetch-completion fault has a dedicated
case where one scan fetch fails and another has an empty completed read.
Unchanged transport plumbing and JSON formatting are not separately mutated.
A first compile found a missing String borrow in targets; corrected before
successful tests. A multiline test-insertion assertion stopped before changing
source; use the function boundary to insert the intended case. No fault,
dependency, suppression or exclusion retained. All 33 conformance checks passed
after staging; new-source full CI remains required. Prior-run logs are not yet
available through gh while that run is in progress; do not report a test total
from the empty/error log response.


### Handback: known-account collector locally integrated; prior CI pending (2026-10-10)

Source 51ff9b9 is committed on feat/known-account-activity in the native sibling
worktree and fast-forwarded locally into fix/wallet-signin-diagnostics. It is
not pushed and has no full CI yet. Remote PR 334 still ends at 255337c.
CI 38023346308 at that prior source has all ordinary jobs and mutation shards
0/2 successful; shards 1/3 still run. No cancellation, retry or workflow change.
Full logs were unavailable through gh during the run, so no new prior-source
Rust/mutation totals are claimed. Inspect all completed shard logs, unmodified
baselines and final gate before the next push. Do not push over a running check.

Local proof: 352 scoped CLI/on-chain tests, 33 conformance checks, scoped
all-target Rust 1.99 Clippy/formatting and sixteen manual faults caught/restored.
Both worktrees were clean after the source commit and local integration, before
this handback. No Cargo/rustc process remained at inspection. No cleanup, live
measurement, keys, limits, delegation, deployment or trade. Autonomous execution
remains off. This handback also stays local while prior CI runs.

On prior-source green, publish the completed collector follow-up and inspect
its fresh full CI. On a failure, diagnose the exact reported check before another
push. Then bridge known-address evidence into local token-effect review without
claiming target ownership or exhaustive coverage. Historical account discovery,
independent metadata provenance, economic reconciliation/idempotent risk-state
application, live risk inputs and execution/exits/recovery remain unfinished;
owner limits and verified Privy delegation/funding/deployment are still needed
before autonomous activation. The unsupported new coverage label must not be
relabeled to make the existing native adapter accept it.


### Protected supplied account-activity authorship review (2026-10-10)

Prior CI 38023346308 at 255337c still runs its final mutation shard 1 at the
latest inspection; other ordinary checks and shards 0/2/3 are successful. Keep
the unpublished collector source 51ff9b9 and this independent follow-up local.
New work is isolated on feat/account-activity-review in the native sibling,
based on primary b9bc418. No push or CI cancellation.

Add --review-account-activity to the offline protected issuer. It reads the
configured snapshot, binds wallet/checkpoint/freshness, validates current account
observations and opening read order, and requires the supplied known-address
target set to match the endpoint account union. Rows bind one-to-one to supplied
enumeration with exact reported signature/slot/outcome and interval. Supported
static messages decode locally and verify every required signature, including
multiple signers. Reporting targets must be nonempty/unique/known and actually
named in signed static accounts. Header/wire count mismatch, duplicate accounts,
unsupported lookup tables or forged signatures refuse.

This is cryptographic authorship and static query membership, not financial
effect classification or complete collection verification. The command can
review valid supplied rows from an incomplete collector packet, without asserting
that no rows are missing. Provider metadata, outcomes/slots, inclusion, ownership
and economics remain unverified; output labels slots/outcomes as reported and
does not forward arbitrary metadata. No journal write, claim release or authority.
Three regressions cover all signers and exact bytes, malformed/duplicate/bounded
supplied rows, and actual issuer process snapshot/target/expiry/opening guards
without changing history. Manual faults and final scoped verification follow.

Restored verification passed 221 signer tests, scoped all-target Rust 1.99
Clippy and workspace formatting. Thirty-five deliberate logic faults were
caught and restored: nineteen unit faults cover wire bounds, every signature,
signer/account consistency, reporting membership and one-to-one row/interval
binding; sixteen actual-process faults cover snapshot identity/checkpoint/time,
native balance binding, opening order, packet identity, exact target bounds and
read expiry. Transport/printing plumbing is not separately mutated. No fault,
dependency, suppression or exclusion retained. Test helpers keep process cases
within the existing function-size lint without weakening assertions.

### Handback: supplied account-activity review complete locally (2026-10-10)

The review is on feat/account-activity-review, based on primary b9bc418, in the
native sibling checkout. The source commit includes this block; recover its hash
from git log. It and collector 51ff9b9 are unpublished while CI 38023346308 at
255337c still runs shard 1. Other ordinary checks and shards 0/2/3 are successful;
full shard logs/final gate remain unverified. No push, cancellation or rerun.
After successful conformance and commit, fast-forward primary locally. Inspect
all prior CI logs and the final gate before publishing the completed follow-ups.
A running check must finish before the next push.

Autonomous trading stays off. This review writes no history and gives no signing
authority. Next implementation is local token-effect classification with explicit
unknown cases, followed by complete economic reconciliation and idempotent risk
state. Independent live risk inputs, execution/exits/recovery, owner limits,
isolated keys and verified Privy policy/delegation/funding/deployment remain.
No live read, keys, delegation, deployment or trade occurred in this increment.

All 33 repo-conformance checks passed after staging, and git diff --cached
--check passed. The full signer suite is 72 library + 52 issuer unit + 55 issuer
process + 31 signer process + 11 Privy boundary tests = 221. Earlier collector
proof remains 352 CLI/on-chain tests; these sources were unchanged here.

### Integration handback: account review 0a17265 (2026-10-10)

Fast-forwarded fix/wallet-signin-diagnostics locally to source 0a17265 after
reading the entire staged diff and passing 221 signer tests, 33 conformance
checks, scoped all-target Clippy, formatting and 35 restored manual faults.
Both checkouts were clean at integration; no Cargo/rustc process remained.
C: had 59,418,595,328 bytes free; no cleanup was needed or performed.
At 04:43 UTC, prior CI 38023346308 remained in progress on shard 1; no source
push or deployment. Collector 51ff9b9 and review 0a17265 still need fresh full CI.
Read every prior shard baseline/result and the final gate before pushing these
completed commits. Autonomous execution remains off; next work and outstanding
activation requirements are in the source handback above.

### CI timeout diagnosis and unchanged rerun (2026-10-10)

CI 38023346308 attempt 1 at 255337c completed cancelled, not green. The shard 1
check annotation explicitly says it exceeded 30m0s; its log printed a passing
unmutated baseline and 339 tested / 243 caught / 96 unviable before cancellation.
The final gate failed with shards: cancelled. No newer push caused this stop.
Other shards passed: 0 = 339 / 258 caught / 81 unviable; 2 = 339 / 284 / 55;
3 = 338 / 264 / 74. These are observed summaries, not a substitute for a passing
gate. Ordinary checks passed, including 2588 Rust tests, 8 private-setup Python
tests and 352 web tests. No survivor was reported in the inspected summaries.

Requested one unchanged job rerun with gh run rerun 38023346308 --job
114128872692. Attempt 2 is in progress; shard 1 job is 114134402505. No timeout,
workflow, exclusion, source or assertion changed to make CI pass. Keep the
completed unpublished commits local while this run is active. If this timeout
recurs, report it and agree the CI tradeoff before changing the gate.

### Signed top-level token transfer intents (2026-10-10)

Independent work on feat/signed-token-intents in the sibling checkout is based
on primary 6f36d94. The caller is the protected account-activity review after
all wire signatures and static reporting membership have passed. Add ordered
instruction intent rows for exact canonical legacy SPL Token TransferChecked:
program identity, opcode 12, ten bytes, four accounts and a required-signing
single authority. Extract source/mint/destination/authority, unsigned little-endian
raw requested amount and requested decimals. These are signed requests, not
executed effects or authenticated mint/account ownership. Root classification
stays unresolved. Other programs, Token-2022, multisig forms, unsupported extents
and instructions remain explicit ordered unresolved rows, never disappear.

Reference checked: the official SPL Token interface packing and account-order
implementation at https://github.com/solana-program/token/blob/main/interface/src/instruction.rs.
No new dependency. Two unit regressions and the existing actual protected issuer
process cover exact extraction, unsupported forms, mixed instruction ordering,
maximum integer precision and reported-failed transactions without history writes.
Final restored checks and manual faults follow.

Restored proof: 223 signer tests passed, scoped all-target Rust 1.99 Clippy and
workspace formatting passed. Sixteen deliberate faults were caught/restored:
five program/extent/opcode/signer guards, amount endianness, four account roles,
decimals, recognized instruction order, execution/ownership flags, unknown-form
classification and actual protected issuer dispatch. The recognized-order fault
initially survived because only one recognized row was tested; added a second
recognized row, then re-applied that exact fault and confirmed failure. Fixed two
initial test-style Clippy findings without suppressions. Printing and iterator
plumbing were not separately mutated. No dependency or exclusion changes.

### Handback: signed token intent decoding complete locally (2026-10-10)

This source commit on feat/signed-token-intents includes the handback; find its
hash in git log. After staged conformance and full diff review, integrate locally
into fix/wallet-signin-diagnostics. Prior CI 38023346308 attempt 2 remains in
progress at last inspection; no push while it runs. The completed collector,
signature review and this intent decoder all still need fresh full CI. On repeated
CI timeout, agree the tradeoff with the owner before changing the check.

No live read, keys, funding, delegation, deployment or trade. Autonomous remains
off. Next is binding supported token intents to exact historical balance effects,
with unsupported programs/CPI and ownership provenance remaining unknown; then
complete coverage/reconciliation and idempotent risk-state application. Live risk
inputs, execution/exits/recovery and owner limits/Privy deployment are still open.

All 33 staged repo-conformance checks and git diff --cached --check passed.

Integration: fast-forwarded primary fix/wallet-signin-diagnostics to f0c0e9a.
Both checkouts were clean after integration, no Cargo/rustc processes remained,
and C: had 59,410,722,816 bytes free. Latest CI inspection still showed attempt 2
shard 1 running; no push or deployment. Preserve these local commits and inspect
the prior completed rerun before publishing them. No further CI polling or local
build remains running for this handback.

### Provider-reported token balance consistency (2026-10-10)

Independent work on feat/token-balance-consistency is based on primary a6f2944
in the sibling checkout while prior CI 38023346308 attempt 2 runs. Caller is
protected account-activity review after signature/membership checks, using its
locally decoded TransferChecked intents. No signing or journal write is added.

For nonempty all-supported intents and explicitly empty innerInstructions,
compare reported outcome/error, fee identity and exact native fee-only changes.
Parse token balances with bounded unique static account indices and exact u64
amounts, mint/program/owner/decimal identities. Require paired account sets and
stable identities. Apply ordered signed debits/credits with intermediate checked
arithmetic; self-transfer must fund the debit. Reported failed execution requires
unchanged token balances. Every supplied token row, including unrelated rows,
must match the expected final balance. Account creation/closure, unknown CPI,
unsupported instructions, incomplete identities or unexplained changes remain
unresolved without discarding the valid signature/membership review.

Output says consistent_with_signed_transfer_intents only for this bounded
provider-data comparison. Execution effects, ownership, independent metadata
provenance and portfolio updates remain unverified/false; root classification
stays unresolved. The comparison does not prove runtime account writability,
historical owner authority, inclusion or exhaustive history. It is not risk-state
application. Official RPC metadata shapes checked at
https://solana.com/docs/rpc/json-structures.

Two new unit regressions cover cumulative and failed transfers, intermediate
self-transfer debit/overflow, paired identities/index/extent failures and unknown
activity. The actual protected issuer process checks matching failed-transfer
balances and a changed balance returning unresolved while retaining verified
signatures and leaving history untouched. Final proof and faults follow.

Restored checks passed 225 signer tests, scoped all-target Rust 1.99 Clippy and
workspace formatting. Twenty-six deliberate faults were caught/restored: token
index bounds/duplicates, exact native extents, payer/fee arithmetic and equality,
empty/unsupported intents, inner activity, succeeded/failed outcome branches,
fee binding, token extent, signed mint/program/units, debit/credit arithmetic,
stable post mint/owner/program/units, final token equality, change sign and actual
protected issuer effect-review dispatch. Unchanged JSON/iterator plumbing was
not separately mutated. No dependency, exclusion or suppression was added.
The expanded negative test exceeded the function-size lint; extracted paired
identity/index cases into a borrowed helper and reran lint and full scoped tests.

### Handback: reported token balance consistency complete locally (2026-10-10)

This source commit on feat/token-balance-consistency includes the handback; find
its hash in git log. After staged conformance/full diff review, integrate primary
locally. Prior CI 38023346308 attempt 2 still ran at last inspection; do not push
over it. The unpublished collector/signature/intent/consistency sources need
fresh full CI together after prior-run completion. Inspect every shard baseline,
summary and final gate; a repeated timeout needs an owner CI tradeoff decision.

Autonomous execution stays off. No journal or risk state was changed by this
review, and no live read, key provisioning, funding, delegation, deployment or
trade occurred. Next is establishing complete economic coverage and durable,
idempotent risk-state application; provider-data consistency cannot establish
independent metadata truth. Independent live risk inputs, execution/exits/recovery
and owner limits/Privy policy/delegation/funding/deployment remain unfinished.

All 33 staged repo-conformance checks and git diff --cached --check passed.

Integration handback: fast-forwarded primary fix/wallet-signin-diagnostics to
5a65043. Both checkouts were clean after integration. No Cargo/rustc process
remained; C: had 59,382,648,832 bytes free. CI attempt 2 shard 1 remained running
at final inspection. No push, deployment or authority change. Resume by checking
the existing rerun, then publish the completed follow-ups only after it finishes
and its results are inspected; these new sources have local proof, not full CI.

### Operational readiness inspection and handback (2026-10-10)

At around 05:00 UTC, public health was ok at deployed Serve
3f38ee0ffaf2d1e349f54b43475f36c010bb335d, agent configured true/provider codex,
last call never, policyClosed true; /automation returned HTTP 302 unauthenticated.
This proves public reachability, not authenticated setup, live inference or
signing. trading=true is the configured component field, not autonomous authority.

SSH guardian-vps-tail timed out. Read-only local Tailscale status showed Running,
no health warnings and self online. The exact SSH target peer 100.105.198.39 is
clawguard, offline, last seen 2026-10-07T17:22:48.1Z. No host configuration,
service restart or connectivity change was attempted. Asked the owner to check
Tailscale; answer is pending. Public reachability makes this a private access
problem, not evidence that the whole VPS is down. Current host provisioning and
wallet funding remain unknown.

CI 38023346308 attempt 2 at 255337c still runs shard 1 (114134402505, started
04:45:40 UTC); other listed jobs are successful. Do not push over that run or
claim new-source CI. No local build, background waiter, trade or deployment is
running. Completed code through 5a65043 is integrated locally and clean before
this documentation update. STATE now puts the verified limited inspection above
its explicitly historical September snapshot.

Next: inspect the existing rerun on completion; if successful, read every shard
baseline/summary/final gate and publish the completed collector/review changes for
fresh full CI. A repeated timeout requires an owner decision before changing the
gate. Restore VPS private connectivity before any host inspection/deployment.
Then finish economic coverage and durable idempotent risk-state application,
independent live risk inputs and execution/exits/recovery. Owner numeric limits,
Privy policy/delegation, isolated key setup, funding and bounded live validation
still precede activation; the public health check does not close those gates.

### Connectivity restored; repeated CI timeout handback (2026-10-10)

With owner approval, extended clawguard's expired Tailscale key through the
admin console and completed the existing account's SSH check. The console now
reports Connected and key expiry six months away. A fresh BatchMode SSH to
guardian-vps-tail returned hostname clawguard and active tailscaled. Public
Radar /health still returns ok at 3f38ee0ffaf2d1e349f54b43475f36c010bb335d.
No deployment or trading-authority change occurred. An exploratory check of
systemctl radar and port 8080 used unverified service/port names; their negative
results do not establish a Radar outage. The public health response is positive.

CI 38023346308 attempt 2 is complete, cancelled. Check-run annotations for
114134402505 explicitly report the maximum execution time of 30m0s exceeded;
its unmutated baseline passed (105s build, 16s test), but no final mutation
summary was observed. Other ordinary checks and shards 0, 2 and 3 passed; the
required mutants gate failed. This repeats attempt 1's timeout, so AGENTS
sections 6 and 8 require an owner decision. Recommended change: increase only
the mutation job timeout from 30 to 60 minutes, preserving all shards, test
coverage and required-gate behavior. Asked owner; no workflow change, push,
rerun or substitute broad local run performed. Completed sources through
5a65043 remain local, awaiting fresh full CI before deployment. Autonomous
execution remains off and the earlier economic/risk/execution/owner setup
gaps remain open. Resume with the owner's timeout decision.

### Owner-approved GitHub verification handback (2026-10-10)

Owner approved increasing the mutation job timeout to 60 minutes and instructed
that tests and jobs run on GitHub. Recorded that preference in AGENTS section 6;
it overrides the earlier local test-loop guidance. Changed only the mutation
job's timeout in CI; all four shards, baseline checks and required final gate
remain enabled. No local Cargo, test or build job was run for this update.
Prior CI attempt 2 is complete, so publish the accumulated completed sources
with this workflow change for fresh full CI on the actual branch head. Inspect
all job results, each shard's baseline and mutation summary, and the final gate
before treating these sources as verified or deploying. Do not push over the
new run. Autonomous execution remains off pending the previously recorded
economic coverage, risk-state, live input, execution and owner setup work.

### Known token endpoint comparison handback (2026-10-10)

Separate branch feat/account-balance-reconciliation starts at cea2e21 while
primary CI 38063256175 runs. The protected account-activity command now calls a
read-only token reconciliation helper after its signature, target and per-row
effect checks. It compares retained opening/current known account identities
and quantities with an ordered chain of supplied token effects. Endpoint token
read slots must agree exactly with the history interval. Collection flags must
report completion; new/disappeared accounts, unsupported effects, owner/unit
changes, pre-balance gaps, unexplained final balances and duplicate transaction
slots stay unresolved. Signature lexical order cannot prove intra-slot order.
Reported ownership accompanies existing token effect output so an unknown
wallet-owned account cannot silently disappear from this comparison.

This is consistency of supplied known-account token data, not native cash
reconciliation, independent metadata provenance, exhaustive historical account
discovery, durable portfolio application or trading authority. Root coverage and
economic completion flags stay false. The helper adds no journal write or key.
Three unit regressions and the existing protected-process test cover the new
comparison/dispatch; unit test floor increases by three. Source formatted locally
as an editing step; no local tests, build, lint or mutation jobs run. Fresh
GitHub CI is required and has not yet verified this change. Publish this branch
separately so primary's running mutation jobs are not cancelled.

Primary CI ordinary build, lint, format, web, MSRV, licence and dependency jobs
passed; tests failed in protected_cost_record_is_durable_idempotent_and_refuses_changed_economics
at issuer_process.rs:1021, repeated --record-valuation assertion. Its child stderr
is absent from that assertion; cause remains unknown. All mutation shards were
still running at inspection. Do not call primary green or deploy it. Inspect
final shard baselines/summaries and new-branch test results; investigate the
valuation failure without removing its idempotency assertion. Autonomous
execution remains off; earlier activation requirements still apply.

### Journal owner lifetime repair handback (2026-10-10)

Previous goal turn made progress: PR 335 at aaf69ab added known token endpoint
comparison; GitHub run 38063799478 is live. Primary run 38063256175 also remains
live. Primary tests failed a repeated valuation assertion with no child stderr.
Follow-up tests failed repo-conformance because the added owner preference made
AGENTS 415 lines against its 410-line ceiling. Removed obsolete local-job
guidance, keeping the GitHub preference and the existing ceiling intact.

Separate fix/journal-owner-release branch at cea2e21 addresses the documented
descriptor-lifetime mechanism: a private guard explicitly unlocks on drop in the
acquiring process, including failed replay. A different PID's inherited wrapper
does not explicitly unlock its parent. The new Unix test holds a cloned file
descriptor across log drop and verifies refusal while live, release on drop,
foreign-PID destructor behavior and continued ownership after a stale duplicate
closes. Add child stderr to the exact repeated valuation assertion so recurrence
reports its reason. No lock retry, serialization or owner-check bypass added.
The actual holder in the original CI failure remains unknown; this is not a
claim that the failure's cause has been captured or that CI is now green.

No local test, build, lint or mutation job run. Source formatting and staged
diff review only; fresh GitHub CI must verify the repair and exercise mutations.
Publish separately over primary so neither existing live run is cancelled.
After all runs finish, inspect actual test failures, every mutation baseline,
summary and final gate, fix any survivors, and combine independently verified
branches. Unit floor here is 2206; combining PR 335's three additional tests
requires 2209. No deployment or trade; autonomous execution remains off.

### Combined accounting and owner repair (2026-10-10)

PR 335 run 38063799478 completed: all mutation baselines and final gate passed;
50 mutants tested (42 caught, 8 unviable), no missed. All ordinary jobs passed
except tests, which failed AGENTS line-count conformance (415 versus 410).
PR 336 run 38064184192 passed its mutation gate and three nonempty shard
baselines (the fourth had zero mutants), but lint
refused the regression's access to the underscored owner field; ordinary jobs
were still running. Neither run establishes full success. Primary run
38063256175 remains live; do not cancel or push onto its branch.

Merged PR 336's sources into PR 335 locally, preserving both plan sections and
setting the combined unit floor to 2209. The regression now exercises the private
owner guard directly rather than accessing OperationLog's underscored field.
Drop calls a separate release method so GitHub's mutation testing can exercise
removing the release body, not only inverting its PID check. Staged source/diff
review and source formatting only locally; no local test/build/lint/mutation
job. Publish the combined PR 335 head for fresh full GitHub CI, because its
previous run is terminal, leaving PR 334 and PR 336 live jobs untouched. Neither
economic coverage nor live signing is complete; autonomous execution stays off.

### Combined GitHub proof and live readiness handback (2026-10-10)

Previous goal turn made progress by publishing the owner repair. This turn
resolved its lint issue, combined it with token reconciliation and obtained
GitHub proof at 76fdf8ed34d2a7ac1d34407038f4e8b20b61d3cb. Run 38064406463
passed every job: 2606 tests, build, lint, formatting, MSRV, web, site, licence,
dependency checks and required final mutation gate. All four mutation baselines
passed; shard summaries total 54 (43 caught, 11 unviable), none missed. The
duplicated-descriptor owner regression and original valuation idempotency test
passed in the ordinary test job. This establishes these tested outcomes, not
the identity of the original contending lock holder. PR 335 description records
the proof; PR 336 is closed because its sources and correction are included.
Fast-forwarded local primary to 76fdf8e. No primary push over live CI.

Read-only SSH inspection shows radar-serve, radar-codex, radar-follow and
radar-market-tape active. Serve runs as guardian with ProtectHome read-only,
NoNewPrivileges yes, WorkingDirectory /home/guardian/radar and writable path
/home/guardian/radar/data. radar-signer.socket is inactive; executable tests for
/usr/local/bin/radar-signer and /usr/local/bin/radar-issuer failed. This does not
inspect every custom install path or prove configuration/funding. No secrets
were read, no service restarted and no authority/delegation/trade changed.

Primary run 38063256175 at cea2e21 remains in progress: mutation shard 0 passed,
shards 1, 2 and 3 still running at last inspection. Ordinary tests already failed
on old source, including valuation idempotency. Leave this exact run intact;
once terminal, inspect all baselines, summaries and failure annotations, then
publish integrated primary for fresh full-branch CI at its actual head. The
stacked PR mutation scope covers new changes, not the full unpublished base
delta against main. Do not use its green check to claim that broader scope.
All test/build/lint/mutation jobs remain on GitHub. Next required work remains
economic coverage and durable idempotent portfolio/risk application, independent
live risk inputs, execution/exits/recovery/scheduling, and owner limits,
isolated issuer/signer deployment, Privy policy/delegation and funding validation.
Autonomous execution remains off; keep the full goal active.

### Retained risk bounds at issuance (2026-10-10)

Added an issuer caller guard after the pure kernel accepts and before any
authorization or new operation is persisted. It reuses acquisitions::review to
recheck retained signatures and economic inputs, then refuses a supplied risk
state below recorded remaining cost basis (total and creator), current UTC-day
gross disposal losses plus failed network fees, or the ordered trailing failure
count. Gains do not offset incurred loss stops; the failure streak survives a
UTC day boundary and resets on a recorded success. These are conservative
recorded bounds, not current market exposure or complete wallet risk. Missing
opening basis, unknown disposal basis, ambiguous event ordering, future records
and arithmetic overflow refuse. A completed journal entry without signed and
valued economics also refuses even after refreshing its checkpoint.

Added boundary/understatement/refusal unit coverage and a process regression
that completes a retained buy, refreshes the checkpoint, and proves flat or
unattributed state cannot issue or change history. Recorded basis and creator
exposure permit the next issue in this synthetic fixture. Unit floor is 2212.
Local work is source review, formatting and diff checks only; GitHub must still
verify tests, lint and mutation coverage. Publish a separate stacked branch over
PR 335, leaving primary run 38063256175 intact. That older run now reports seven
survivors in account_activity::transaction (signature extent arithmetic and
account-count boundary); its last shard is still running. Inspect the complete
run before fixing and publishing integrated primary. No deployment, signing
authority or trade enabled. Live risk input/reconciliation, execution and exits,
owner numeric limits, isolated deployment, delegation and funding remain.

PR 337 first run 38065430606 at dd762b0 completed with build, lint, formatting,
MSRV, web, site and ancillary jobs passing. Tests and all four mutation
baselines failed the same two older fixtures at issuer_process.rs:849: a newly
requested authorization over nonzero opening holdings now correctly refuses.
Both new risk unit tests and the new completed-buy process regression passed
in that baseline; no mutation coverage was established. Corrected the fixture
to assert the actual issuer refusal, then seed generic historical journal
records for the existing read/replay tests. No production guard was weakened.
Fresh GitHub verification is required after this fixture-only correction.

Older primary run 38063256175 is now terminal. All four baselines passed and
1511 mutants completed within the approved 60-minute limit: 1182 caught,
322 unviable, seven missed. No timeout. Its ordinary test failure is the
valuation-idempotency assertion already corrected and passing on PR 335 at
76fdf8e. The seven account-activity survivors are addressed on separate PR 338
at 70ec786; fresh proof pending. Integrated primary needs fresh full-branch CI
after these fixes, since the stacked mutation scopes do not cover the full base.

PR 337 correction run 38065674814 at 285b7f7 passed every ordinary job and all
four mutation baselines. Of 69 mutants, 52 were caught, 14 unviable and three
missed. Added the missing buy-only/no-opening refusal to distinguish || from
&& at risk_floor.rs:151:57. Changed the sale-loss timestamp fixture so seconds
modulo one day cannot coincidentally equal the UTC day number, covering the /
to % survivor at :121:64. The :121:16 < to <= mutant is equivalent: zero PnL
negates/converts/adds as zero. Replaced the comparison with i128::is_negative,
preserving its exact meaning, and asserted break-even PnL contributes no loss.
No broad mutation exclusions added. New GitHub proof required. Integrated
primary 62cfeb8 was published before these results; run 38065819742 is live and
must finish intact before integrating this follow-up and refreshing its checks.

### Account activity mutation boundary follow-up (2026-10-10)

Older full-branch run 38063256175 shard 3 completed with seven survivors at
account_activity.rs:15:56, :15:73, :15:60, :15:65 and :21:35. Removed redundant
minimum signature-length arithmetic: tx::decode already bounds-checks every
signature and message field before any slicing. Kept the packet-size and
nonzero-signature restrictions. Added a valid single-signer/single-account
transaction (the missing equality boundary) and refusal for every truncated
prefix. This addresses the actual reported checks without broad mutation
exclusions. Unit floor is 2210 on this separate branch over PR 335; verification
is pending GitHub Actions, with no local test/build/lint/mutation jobs. No live
deployment or authority changed. PR 337 separately contains the retained-risk
guard; its first GitHub baseline found two older nonzero-opening fixtures that
must be constructed as historical journal entries, not newly authorized trades.

PR 338 run 38065557483 passed each nonempty mutation baseline but reported one
new survivor at account_activity.rs:15:20, changing packet length >1232 to
>=1232. Added a correctly signed, decodable packet of exactly 1232 bytes with an
opaque instruction; trailing bytes would not test this boundary because the
decoder refuses them independently. Ordinary jobs were still finishing at last
inspection. Wait for terminal status before publishing this test correction;
all verification remains on GitHub. No exclusions or production guard changes.

### Integrated primary verification handback (2026-10-10)

Primary now integrates PR 335's proven owner/token sources, PR 337's risk guard
and historical fixture correction (285b7f7), and PR 338's packet-boundary fix
(f186b11). Resolved only plan append conflicts and combined unit floor to 2213.
Reviewed the full staged changes before committing. PR 338 first run finished:
every ordinary job passed, only its packet-equality mutant and final gate failed;
the exact-boundary correction is published for fresh GitHub verification.
The risk correction's run 38065674814 is also live. Publish integrated primary
now that its prior full-branch run is terminal and all failure logs/annotations
were inspected. Fresh full-branch GitHub tests and all four mutation shards must
verify this combined source; stacked checks alone cannot prove that scope.
Do not push over these live runs. No local test/build/lint/mutation jobs,
deployment or trades. Continue with any reported failures, then complete live
risk coverage, durable application/recovery, execution/exits, limits, signer
isolation, delegation and funding before turning autonomous execution on.

GitHub marked PRs 335, 337 and 338 merged when their commits reached the stacked
base branch; that does not merge PR 334 into main or establish full CI success.
PR 338 run 38065790380 at f186b11 is now fully green, including all jobs and
the final mutation gate. The two nonempty mutation shards each passed baseline
and caught two mutants; two shards had none. Risk follow-up 7a67a7a is on draft
PR 339. Its initial plan append conflict prevented CI from starting; merged
primary 62cfeb8 into the follow-up, preserving both plan sections and combined
floor 2213, so GitHub can verify it. Keep primary run 38065819742 intact. Next:
inspect PR 339 results, fix any failures, then integrate the correction and
refresh full primary CI only after its current run finishes. Autonomous off.

### Per-option agent control owner decision and implementation (2026-10-10)

Josh reaffirmed that the site owns the choices and clarified three independent
Agent decides checkboxes: unchecked uses the owner's manual value and permits
agent trading within it; checked lets the agent choose that option from wallet
balance. Mixed choices are required. Stopped the uncommitted single-button
implementation and replaced it with this flow. Recorded the superseding owner
decision in design 0017; do not request these numbers in chat again.

The site now records each field's choice independently, preserves dormant manual
values for unchecking, and permits saving all three agent choices with no manual
numbers. Saving requests autonomous trade selection. Server validation requires
positive fixed-precision values only for manual fields, with manual trade/loss
bounded by manual capital when both are specified. A dynamic choice is unknown
until the live agent/verified wallet adapter resolves it; it is not zero, an
unlimited numeric policy, or signing authority. Existing preferences default to
manual per option. Wallet/identity binding and inactive execution are retained.
Added two Rust regressions for all eight mode combinations, manual constraints,
durability and legacy parsing; web checks cover mixed/all-agent/manual restore,
save failure and legacy data. Unit floor 2215. Verification is GitHub-only;
publish this independent branch while the primary mutation run finishes.

Risk follow-up f39d8d7 passed every job in run 38066090016: 2610 tests and all
four mutation baselines; seven mutants (six caught, one unviable), none missed.
Fast-forwarded local primary to it, without pushing over full-branch run
38065819742, which remains live. PR 339 records the follow-up; full primary
verification must still incorporate it after that current run finishes. No live
deployment, policy grant or trade. Future agent-selected numbers must become
explicit wallet-derived bounds enforced by the independent kernel/signer, with
durable loss/recovery state; the model cannot directly write signing authority.

### Per-option verification handback (2026-10-10)

At 26e0aa3, PR 340 run 38066782544 passed every GitHub job and final mutation
gate. Inspected each mutation baseline and summary: four baselines passed, ten
mutants tested (four caught, six unviable), none missed. GitHub web job reported
356 passing tests across 27 files; Rust job reported 2612 passing tests. No local
test/build/lint/mutation jobs ran. Fast-forwarded local primary to this source;
do not publish over primary run 38065819742, still in progress at inspection.
Next inspect that run's terminal results, publish the integrated primary for
full-scope verification, then deploy only the verified artifact through the
fixed deployment procedure. These controls save preferences; autonomous signing
and trading remain off pending live risk inputs and execution integration.

### Issuer release artifact handback (2026-10-10)

Read-only preflight confirmed Radar at its documented 127.0.0.1:8402 endpoint:
build 3f38ee0, healthy, policyClosed true, Codex configured with no calls yet.
radar-signer.socket remains inactive; serve retains NoNewPrivileges and read-only
home protection. An initial probe at unrelated port 8090 was not Radar evidence.

The release workflow builds every radar-signer package binary, including the
issuer, but omitted radar-issuer from both BUILD-INFO hashes and upload paths.
Include it in both so installation can use a traceable GitHub artifact. Corrected
the workflow's server deployment example to the fixed radar-deploy procedure;
that procedure does not install or activate the separate issuer/signer. Verify
the release build and its artifact on GitHub before claiming this deployment gap
closed. No server files, service settings, delegation or funds changed.

PR 342 source 9e43e11 passed every CI job in run 38067700927. Its diff against
the verified per-option branch contains only release YAML and plan prose, so
the four mutation jobs have no changed Rust behavior to mutate. Fast-forwarded
local primary to 9e43e11; release run 38067699398 remains live and artifact
inclusion/hash verification is still pending. Do not publish over primary run
38065819742: shard 1 remains live. Shard 2 passed baseline and tested 405 mutants
(354 caught, 51 unviable); shard 3 passed baseline and tested 402 (324 caught,
75 unviable, three missed). Its exact survivors are the same risk_floor.rs
121:16 < to <=, 121:64 / to %, and 151:57 || to && already corrected and verified
at f39d8d7 in PR 339. No additional speculative risk changes needed.

PR 341 correction 7643dd8 run 38067797787 now passes lint, four baselines and
all mutation shards (25 mutants: 20 caught, five unviable), with build/tests
still live at inspection. Inspect terminal results before integration and
publish the combined primary only after its existing full run finishes. No
local test/build/lint/mutation jobs or deployment occurred in this handback.

### Mixed recorded/external wallet activity (2026-10-10)

Found an execution-path gap: native transfer capture rejects a collected swap
even when that exact transaction has already been verified and valued in the
owned journal. Acquisition review now exposes its replayed settlement records.
Collected activity recognizes those exact signed bytes, signature, slot,
outcome, fee and every native/token balance before leaving the existing trade
in history and collecting only external plain transfers. Duplicate recorded
rows and changed metadata refuse; manually supplied transfers keep their prior
duplicate refusal. Unknown activity remains unsupported, and address history
completion still does not assert complete wallet coverage or release capital.

Added two unit regressions for mixed known/unknown activity and exact metadata
matching, and a process regression exercising --record-native-transfers against
a real replayed recorded swap with repeated reads and conflicting token data.
Unit floor 2217. Publish on an independent branch based on verified per-option
controls while primary run 38065819742 finishes; all test/build/lint/mutation
verification stays on GitHub. No production deployment, signing grant or trade.

PR 341 first run 38067543478 at b1f0d12 is terminal: every ordinary job except
lint passed, as did all four mutation baselines and final gate. Twenty-five
mutants tested (twenty caught, five unviable), none missed. Lint found capture
over its line limit and an empty assertion style issue. Simplified capture to a
direct loop with normal error propagation and changed the assertion to show the
actual length. No lint suppression or mutation exclusion; publish for fresh
GitHub proof now that the prior run finished. Primary run 38065819742 remains
live. Separate PR 342 includes the issuer in release hashes/artifacts; release
run 38067699398 is live and has not installed anything on the VPS.

PR 341 correction run 38067797787 at 7643dd8 is now fully green: every job,
2615 Rust tests, all four baselines and the final mutation gate passed. Merged
this verified source into local primary with only appended plan sections in
conflict; preserved both. Full-scope primary verification is still required
after its existing live run finishes. Release artifact proof remains pending.

### Issuer artifact proof (2026-10-10)

Release run 38067699398 succeeded at 9e43e1140f03e0c754adb33d5209a3b5503531a0.
Downloaded artifact 11676061580, radar-linux-x86_64, with gh run download into
the sibling radar-release-38067699398 directory. BUILD-INFO names that exact
commit. Get-FileHash SHA256 verified all six listed binaries, including the
1,935,552-byte radar-issuer with hash
4af9566650d0028fdf5b1cddd008581809037feeffc663de378e55b38b1b45ce.
Packaging omission is now closed by actual artifact evidence. The artifact
predates mixed-activity integration 164d334 and must not be represented as its
release. Primary run 38065819742 shard 1 remains confirmed live at inspection;
wait for its terminal output before publishing the integrated source and
requesting its full CI/release build. Nothing was installed or activated.

### Full primary run completion and publication (2026-10-10)

Run 38065819742 at 62cfeb8 is terminal, with no cancellation or timeout. Every
ordinary job passed, including 2610 Rust tests. All four mutation baselines
passed; 1617 mutants tested: 1274 caught, 340 unviable and the three known
risk-floor survivors in shard 3. Shard 1 finished successfully after 37 minutes
with 405 mutants (280 caught, 125 unviable). Inspected both failed job logs and
annotations; the final gate correctly reports shard failure. The exact three
survivors were corrected and verified in f39d8d7, already integrated locally.

Publish the combined primary now: per-option controls, corrected retained risk,
recorded activity recognition and issuer release packaging all have their scoped
GitHub proof. Request fresh full-scope CI against main and a release build of
this integrated commit. Leave both intact to terminal results. Do not deploy
the older 9e43e11 artifact as this source or enable autonomous execution: live
risk construction, complete economic reconciliation/durable application,
execution/exits/recovery, isolated service installation, Privy delegation and
funding verification remain. Owner choices stay on the website per option.

### Integrated release verification and design discussion (2026-10-10)

Release run 38068470125 succeeded at
ac2f87a821a507d6b05f2665c6c78c796d6bfa23. Downloaded radar-linux-x86_64 with
gh run download into sibling radar-release-38068470125. BUILD-INFO names that
exact commit; Get-FileHash SHA256 verified all six binaries. radar-issuer is
1,950,624 bytes, hash
8d22b49509d446a3d8254a6493c4edabd23f62ecde14d29b52a7e99aaf9cdd71.
Full CI 38068474113 has passed every ordinary job; all four mutation shards
remain live at inspection. Do not publish over that run or claim its final
gate passed. The artifact has not been deployed. Radar health on port 8402
still reports build 3f38ee0, policyClosed true and no Codex calls;
radar-signer.socket remains inactive.

Owner raised a direction question: adaptive risk management instead of a
mandatory daily loss stop, plus conversational research assistance. Stopped
implementation and discussed the recommendation in chat: optional daily cap,
adaptive allocation and sizing, independent operational controls, measured
performance and researched user tips under an existing mandate. This is a
recommendation awaiting the owner's response, not a settled product decision;
no policy, UI or trading behavior was changed to implement it. Continue
artifact/CI verification independently, and settle the discussion before
recording new product doctrine or implementing the risk-panel redesign.

### Approved adaptive-risk direction and optional cap drafts (2026-10-10)

Josh accepted the recommendation and requested it in the checklist. Recorded
the decision in design 0017 and the executable-work checklist in plan 0011:
adaptive risk by default, optional daily cap, current portfolio/risk explanations,
conversational research leads, measured results and reviewed tool improvements.
The prior discussion is settled; implementation resumed with optional cap
preferences. New site drafts disable the daily cap explicitly. Older persisted
preferences missing daily_loss_enabled keep their existing cap enabled. The
separate Agent decides choice and saved manual value survive disabling,
re-enabling and refresh. Enabled manual caps retain numeric validation;
disabling the cap does not bypass capital/per-trade validation. These settings
remain drafts; live Policy construction and adaptive behavior remain open.

Added Rust legacy/validation/restart and web toggle/save/refresh regressions;
unit floor 2218. Verify on GitHub in an independent branch while primary full
CI 38068474113 remains live. No local jobs, deployment or signing activation.

Collector/issuer bridge PR 343 at 2031e2e passed every job in GitHub run
38068950676, including all four baselines and final mutation gate. Logs show
twelve mutants, nine caught and three unviable, none missed. Its verified source
is ready for integration after the primary's current full run finishes.

### Combined known-address history ingestion (2026-10-10)

The actual wallet-activity-read --inventory-review collector emits
provider_reported_known_address_history. Native transfer capture previously
accepted only the wallet-only history variant, preventing the combined packet
from reaching --record-native-transfers even for an exact retained swap or
plain native transfer. Accept both collection variants. Combined-query packets
must have unique, bounded targets including the wallet; reuse account activity
row verification to bind each enumerated reporting address to a queried target
and to the signed message before recognizing retained operations or classifying
external native transfers. These targets describe queries, not verified account
ownership. No complete wallet-coverage assertion or economic state release is
introduced, and unsupported token/opaque external activity still refuses.

Added a unit regression for recipient-account queries, exact sixteen-target
boundary, forged membership, malformed targets, incomplete collection and
changed native effects. Extended the actual issuer process's retained-swap
regression through both packet variants and repeated combined reads without
journal mutation. Unit floor 2218. Run all verification on GitHub in an
independent stacked PR based on published primary ac2f87a, leaving full primary
CI 38068474113 intact. No deployment, signing authority or live trade performed.
The owner's adaptive-risk-panel discussion remains unsettled and independent
of this collector/issuer integration fix.

### Local integration of verified wallet controls (2026-10-10)

Fast-forwarded local primary to PR 344 source 2b80d25, fully verified by
GitHub run 38069237983, and merged PR 343 source 2031e2e, fully verified by
38068950676. Only appended plan history conflicted; retained both sections.
The adaptive-risk discussion is settled as recorded in design 0017 and plan
0011. Combined unit floor 2219 includes both new regressions. Do not publish
over still-live primary run 38068474113. Corrected kernel PR 345 run
38069809249 has passed lint and all four mutation shards but other jobs remain
live; do not call it fully verified or integrate until terminal inspection.
No deployment or autonomous activation occurred.

### Optional daily cap at the actual risk consumer (2026-10-10)

PR 344 at 2b80d25 passed every GitHub job in run 38069237983, including web,
Rust tests, all four mutation shards and the final gate. Draft preferences now
represent whether the owner enabled a cap, but the actual kernel still required
a number. Policy.max_daily_loss now uses Option<MicroUsd>: existing serialized
numeric caps remain numerically identical, explicit null disables only that
check, and an omitted field defaults to Some(0), preserving refusal rather than
silently disabling protection. Policy::CLOSED remains closed. No enormous-dollar
sentinel or model-dependent exception is used.

The private offline issuer consumes this Policy directly. Added actual issuer
process cases for explicit null, numeric and omitted caps, plus halt with no cap;
refused cases retain no outstanding reservation. Kernel regression covers the
exact loss boundary and enabled zero cap, and verifies that no-cap policies still
enforce halt, failures, position sizing, stale inputs and measurable exits.
Property generators now exercise both enabled and disabled caps. Updated typed
test fixtures/doc example without changing their serialized numeric caps. Unit
floor 2219. Verify on GitHub in a separate stacked PR; do not publish over primary
CI 38068474113 while it remains live.

This closes the Policy representation gap only after verification. Converting
the owner's draft choices and adaptive proposals into an authorized live policy
still requires the independent mandate and verified wallet inputs. Existing
action-aware reduction work, accounting, execution/recovery and delegation
remain necessary; no live service, policy file, authority or funds changed.

PR 345 first run 38069556295 at d7d506c exposed a serde-default helper lint
and one equivalent survivor. Lint rejected a function always returning Some;
read the value from Policy::CLOSED instead, keeping a single default source.
Shard 3 applied policy.rs:139:5's exact Some(Default::default()) replacement
and passed its baseline and mutated tests. MicroUsd derives Default over u64,
so this equals Some(MicroUsd::ZERO). Added only that exact replacement to the
documented equivalence list; None replacements and the daily-loss boundary
remain tested. All four baselines passed: four mutants, two caught, one unviable
and the equivalent survivor. Other jobs passed except lint; tests remain live
at inspection. Do not push this correction until the current run is terminal.

### Verified kernel integration (2026-10-10)

Corrected PR 345 source e5bc317 passed every job in run 38069809249. Three
nonempty mutation baselines passed; three mutants tested, two caught and one
unviable. Shard 3 correctly had no mutants after excluding only the identical
Some(Default::default()) replacement; the final gate passed. Integrated this
source locally with PR 343/344. Only appended plan history conflicted; retained
all sections. Combined unit floor 2220. Complete integrated GitHub verification
and a fresh release are still needed before deployment. Primary run 38068474113
remains live; do not overwrite it. Autonomous execution remains off.


### Protected native-operation completion (2026-10-10)

Previous goal turn made concrete progress: integrated verified PR 343/344/345
source on 57c2757 and dispatched GitHub integration/release. Those runs are now
terminal green: CI 38070118800 passed every job and the final mutation gate;
release 38070117408 built all six binaries. Downloaded artifacts and independently
matched every SHA256 against BUILD-INFO.txt at exactly 57c2757. Primary full-scope
CI 38068474113 remains live at inspection; leave it intact.

Actual next caller: radar-issuer --reconcile-operation <operation-id>. Recheck
retained exact signed transaction/normalized economic facts, one unknown
operation, zero-token opening basis, fresh wallet/journal checkpoint, raw token
quantities, native balance and transaction anchors. Also compare each retained
buy's pre/post wallet mint totals with preceding retained quantities. Current
net quantities alone can conceal an unexplained earlier token change; a new
process regression demonstrates that refusal. Existing zero-opening fixtures
now use consistent zero pre-token balances and fifteen acquired units. Unknown
opening basis and unsupported sales remain refused.

Every external native effect used for cash comparison must already be durable.
Rehold the original claim against verified pre-execution native cash, then use
OperationLog::reconcile to validate and write completion before releasing the
unused reservation. Retained lots/basis/rent/failed fees replay on restart; a
response-loss retry performs no second write, and subsequent issuance still
requires a new protected checkpoint and risk state covering those effects.

Added six actual-process regressions covering success, retries, next-issuance
risk floors, failed network fees, stale/foreign/mismatched inputs, missing or
nonzero opening basis, multiple unknown operations, external-flow retention and
hidden token gaps. Unit floor remains 2220 (new integration tests only).
Verification must run on GitHub in an independent branch based on integrated
controls. No local jobs or live signing. This does not close complete live wallet
coverage, sale completion, dropped-transaction recovery, delegation, adaptive
reasoning, research chat or live supervision. Goal remains fully autonomous
trading, not this offline milestone.


### Verified controls deployment and reconciliation corrections (2026-10-10)

Full-scope primary CI 38068474113 at ac2f87a is now terminal green, with
1,633 mutants tested: 1,288 caught, 345 unviable, none missed. All ordinary jobs
and the final mutation gate passed. Integrated controls CI 38070118800 at
57c2757 is also fully green. All six release 38070117408 hashes matched.
Copied only verified radar-serve to /tmp/radar-serve.new and used the fixed
sudo radar-deploy procedure. Health on 8402 now reports exact build 57c2757,
ok, policyClosed=true, configured Codex with last_call=never; signer socket
remains inactive. This deployment exposes optional daily-cap drafts; it grants
no signing authority and does not deploy the pending reconciliation command.

PR 347 source 90d3d37 run 38077204642 completed with failures. Every mutation
baseline and the actual process tests passed, but three mutations survived:
reconciliation.rs:43:63 &&->|| in owner/mint filtering, 84:43 ||->&& in the
sale/asset guard, and 91:61 ||->&& in state/count checking. Lint also reported
the new CLI run body exceeding 100 lines and one empty-vector assertion.
Move the CLI dispatch into its existing mode handler and use a diagnostic array
comparison. Add same-mint counterparty rows to consistent zero-opening fixtures
so foreign ownership cannot satisfy a wallet anchor. Make the concurrent
unknown-operation fixture's snapshot and sequential cash effects consistent;
its refusal must depend on the intended count gate, not an earlier stale
checkpoint/cash mismatch. Remove the redundant asset guard: the only supplied
portfolio holding is native SOL, and rehold already refuses any other asset.
The sale guard remains explicit. No mutation exclusions or check weakening.
Reverify the corrections on GitHub after the previous run is terminal.


Corrected-source run 38077523601 at ed85334 found a test compilation error,
not a new behavioral result: serde_json::Value's multiple PartialEq types make
the suggested empty array RHS ambiguous (E0282, issuer_process.rs:237). Compare
the JSON array directly to json!([]). Mutation baselines could not build, so
none of this run's mutation outcomes count as verification. Preserve the run
until terminal, then publish this test-only correction and reverify all checks.


### Verified native completion integration (2026-10-10)

Previous goal turn made source and deployment progress. Current turn verified
corrected PR 347 at 8acc176: GitHub run 38077732628 passed every job, all four
baselines and the final mutation gate. Thirty-three mutants tested, twenty-seven
caught and six unviable, none missed. The six actual-process regressions passed
and the first run's ownership/count survivors are now caught without exclusions.
Fast-forwarded the combined local branch to that verified source. Publish its
combined CI and fresh six-binary Linux release without changing any live signing
configuration. Deployed server remains verified 57c2757 with closed policy and
inactive signer.

Next execution work must distinguish actual acquisition from reduction in the
signer and bound what an exit spends. Existing pipeline always requests build_buy;
signer checks native outgoing spend but does not bind a known venue trade's side
to Authorization.action, and its sale token quantity is explicitly unbounded.
Inspect and close that concrete mismatch before enabling live exits. This is
required execution work, not a replacement objective or readiness claim. Live
snapshot/reconciliation coverage, adaptive supervisor/chat, isolated delegation
and canary/recovery verification remain open.

### Native spend units implementation (2026-10-10)

Previous goal turn made progress correcting an erroneous process-test nonce
expectation and publishing GitHub verification. Current turn observed direction
run 38078931338 at 5dc2cbe fully green. Work continues on a separate branch from
verified integrated 63e66e6 so physical-unit work cannot cancel or depend on the
direction test run. Preserve and integrate that verified direction separately.

Actual callers are radar_signer::check and radar-issuer::prepare. Explicit
protected native policy separates chain units from USD exposure. Issuer refuses
missing/zero native configuration, converts the authorized USD amount with the
protected SOL upper price, clamps to its native cap and proves the resulting
max_lamports. Signer independently clamps in native units; missing old fields
retain the prior restriction and do not silently gain authority. The model
cannot write either protected policy. Fee reservation stays additional.

Added actual Privy process cases for a $50 / $200-per-SOL conversion, exact native
boundary, independent signer/caller limits, USD overauthorization, zero and
legacy fallback, and proof tampering. An actual issuer-process curve buy verifies
250m lamports under $50 authorization, smaller protected cap, refusals beyond
either cap, retained proof and absence of reservations after refusal. Startup
refuses null/zero caps. A wire regression covers explicit native units and old
policy compatibility. Unit floor 2221 on this branch (direction integration adds
its separate test). GitHub verification required; no local jobs or live changes.

Still open: live complete portfolio/evidence adapter, token-bounded exits and
mint/account roles, routing, adaptive supervisor/chat, isolated deployment and
end-to-end recovery/canary evidence. Goal remains fully autonomous trading.

### Native policy regression correction (2026-10-10)

Run 38079356230 at 7a8fe94 found an old reservation test whose deliberately tiny
SOL price now hits the native cap before reaching its intended insufficient-cash
guard. In that test only, set the native ceiling above available cash so wallet
reservation remains the isolated guard. New exact-cap and converted-amount
regressions remain unchanged. Lint also requires simplifying the startup guard
to is_none_or(limit == 0). New curve-buy and native signer cases passed in the
completed mutation baseline log, but that baseline failed at the old test; do
not count any mutation verification from this failed run. Preserve the current
run until terminal before publishing the correction. Protocol comments now
state separate USD/native checks and the authenticated Privy conversion exactly.

### Signer verifies known curve trade direction (2026-10-10)

Native completion PR 347 at 8acc176 passed all jobs in GitHub run
38077732628, with four baselines and thirty-three mutants: twenty-seven caught,
six unviable, none missed. Fast-forwarded combined source and published handoff
63e66e6; combined CI 38078028706 and release 38078025956 are live. Leave those
runs intact. This branch is based independently on verified controls 57c2757.

Actual caller: radar_signer::check, used by both local and isolated Privy signing
processes. It decoded buy sizes but skipped all sales and never checked a known
venue trade's side against Authorization.action. Decode every known curve trade,
including sell argument extents. Require Buy for acquisitions and Reduce/Exit
for sales; refuse mixed directions within one message. Keep every native spend,
policy, expiry, ownership and nonce check. A trusted issuer proof cannot waive
the decoded direction. Update the previous large-sale regression to use actual
Exit authority; its minimum incoming SOL still does not count as outgoing spend.

Added one unit matrix across all six trade variants and all three actions,
mixed messages and every truncated sale argument extent, plus an actual Privy
process regression with valid re-attested proofs. A verified issuer attempt
consumes its nonce even on refusal; corrected direction needs fresh authority
and duplicate success refuses.
Unit floor 2221. Verify on GitHub; no local jobs or live signing.

This closes only a concrete side mismatch after verification. Sale token bounds,
per-instruction mint/account roles, routing, live snapshot accounting and exit
completion remain open, alongside the adaptive supervisor/chat and delegation.
It does not claim a generic arbitrary-program semantic check or enable exits.
The goal remains fully autonomous trading.


Direction PR 348 initially had no CI run because GitHub reported conflicting
appended ADR/plan sections against integrated 63e66e6. Merged that verified
base and retained both accounting and direction sections; no production-source
conflict. Combined CI 38078028706 and release 38078025956 at 63e66e6 are now
fully green; downloaded all six binaries and matched every release SHA256.
The root-owned issuer still needs protected installation/configuration; fixed
radar-deploy updates only Serve, so no issuer installation was implied.
Publish the conflict resolution to allow PR 348's actual GitHub checks to run.

Also re-inspected verify.rs lamport_ceiling: it still reads the USD notional as
lamports and takes the minimum with the actual lamport conversion. The source
explicitly documents this as blocking real sizing. A separate physical spend
ceiling with explicit native units, independently enforced in the signer and
bound to issuer proof, must replace that unit substitution before activation.
Do not fix it by widening an untrusted caller bound or silently trusting a price
inside the signer. Preserve the owner's USD-facing choices and closed defaults.
This is an implementation requirement to resolve next, not new authority.

### Direction process regression correction (2026-10-10)

Run 38078622823 at 7073f4e completed with a failing new process regression,
including mutation baselines. Build, lint, format and unit direction checks
passed. The regression incorrectly expected a refused, issuer-verified attempt
to leave its nonce reusable. Actual handle_privy deliberately claims before
key use/checks and never rolls back: even interrupted or rejected attempts need
fresh issuer authority. Preserve that protection. Assert corrected bytes with
the old nonce refuse for reuse, then re-attest a fresh nonce and require success.
No production behavior changed in this correction. Verify on GitHub before
integrating direction enforcement; autonomous signing remains closed.

Merged verified trade-direction source 5dc2cbe into the native-unit branch while
its original run finishes. Preserve both process regressions and ADR/plan
sections. Combined unit floor 2222. The merge requires a fresh GitHub run with
the reservation/lint correction; do not infer combined verification from the
separate direction run. No production configuration or service was changed.

### Enforceable token-debit bound for exits (2026-10-10)

Previous goal turn corrected native-policy regression/lint failures and combined
verified direction enforcement. Current turn observed combined run 38079626824
at aa1ea1c fully green. This branch began independently at verified direction
5dc2cbe so source work did not cancel that run.

Actual caller: radar_signer::check in the key-holding local and Privy processes.
Authorization now carries optional max_token_debit_raw. All known curve sales
share that single explicit bound; checked aggregate arithmetic refuses overflow,
missing bound or excess quantity. Incoming SOL is never treated as outgoing
tokens. The existing direction regression supplies a valid quantity so it still
isolates side enforcement; large-exit regression explicitly bounds its tokens.

Added unit coverage for both sale variants and Reduce/Exit, zero/exact/missing
bounds, individually-small but collectively-excess sales and u64 overflow. The
actual Privy process checks the same cases with valid proofs and detects tampered
token authority. A risk wire regression shows old kernel output retains its wire
shape and invents no raw-token authority. No local jobs or live changes. GitHub
verification required; do not claim that token/account roles or live exits work.

Next: merge the now-verified native-unit source before combined verification;
then derive exit authority from protected holdings and reviewed execution data,
bind mint/account roles and route reductions correctly. Current kernel and issuer
leave token authority unset and issuer stays buy-only. Complete live portfolio
coverage, recovery, adaptive supervisor/chat and isolated deployment still remain.
Goal remains fully autonomous trading; no live activation is claimed.

Merged verified native-unit source aa1ea1c (all GitHub checks green in
38079626824) into the token-bound branch before publication. Retained both
physical-unit and token-quantity checks and all three actual-process regressions.
Combined minimum test floor 2225. Fresh combined GitHub verification is required;
separate native verification does not prove this new token-bound behavior.

### Executor direction and handback (2026-10-10)

Started feat/executor-trade-direction from verified token/native/direction
source a6d1a9d while role-binding PR 351 runs GitHub CI 38081131378. This routing
change is independent of the new signer-role implementation; neither run is
cancelled or treated as verification of the other.

Actual caller: radar_exec::execute. It previously always called build_buy even
for Reduce/Exit. Add required Routing::build_sell and dispatch by authorization
action. Sales require positive explicit raw-token authority and use that amount;
missing/zero authority refuses before routing, signing or sending. Native bounds
stay separate. Production Jupiter router continues to refuse in both directions.
Updated every Routing implementation and added two executor regressions proving
exact side/asset/wallet/amount, preserved native bound, and early refusal without
any routing/key/send calls. Extended the router refusal regression to both sides.
Unit test floor 2227 on this independent branch. No local jobs or live changes.

Publish this branch for GitHub tests/build/lint/mutation verification. Integrate
with role binding only after both actual runs pass; combined floor will be 2228.
This does not implement sell construction, protected holdings/exit authority,
portfolio evidence, sale reconciliation/recovery or adaptive supervision/chat.
Goal remains fully autonomous trading; production authority remains closed.

Role-binding run 38081131378 at 2768670 completed fully green, including the
actual-process capture regression and all mutation shards/final gate. Twelve
mutants: eight caught, four unviable, none missed. No live changes followed.
Executor run 38081286620 at e5d6fe1 reported a lint failure: the local name routed
is too similar to router. Rename it built without changing behavior. Let all
remaining jobs finish before publishing the correction; require a fresh full run.

### Captured curve trade roles and handback (2026-10-10)

Verified token-bound source a6d1a9d: GitHub CI 38080077978 completed successfully,
including tests/build/lint and all mutation shards/final gate. Thirteen mutants:
eight caught and five unviable, none missed. No local verification jobs ran.

Current branch feat/signer-trade-account-roles began from that verified source.
Actual caller remains radar_signer::check in the key-holding processes. Added
independent per-instruction mint/trader binding for all six known curve trade
variants and explicit wrapped-SOL quote binding for v2. Test fixtures retain six
successful public mainnet RPC captures with signatures, slots, source versions,
argument bytes, instruction accounts and reported mint/owner token anchors.
The old and v2 layouts differ; a global mint-membership check did not bind roles.
The actual Privy process regression re-attests substituted/missing roles, mixed
messages and a wallet that is not a required signer. Existing synthetic trade
fixtures now supply the correct mint/trader roles rather than masking new checks.

Capture investigation also found the old buy_exact_sol_in fixture transaction
failed before invoking the trade. A different successful transaction anchors the
new role fixture; old fixture provenance is not silently rewritten. RPC metadata
is provider-reported layout evidence, not an independent historical ownership
proof. Reconstructed legacy tests are not claimed as executed mainnet messages.

GitHub verification is required for this new branch. No jobs ran locally and no
production service, protected policy or signing authority changed. The smart-panel
checklist remains in plan 0011 and design 0017: adaptive risk, optional daily cap,
each option independently agent-selected, live portfolio/reasons, research chat,
independent pause/revoke and measured net outcomes. These remain activation work.

Next: inspect the fresh GitHub results, then integrate verified source. Protected
exit authority/holdings, token-account and curve derivations, bidirectional routing,
live wallet/portfolio evidence, sale completion and recovery, supervisor/chat,
protected deployment and owner site handover still block autonomous activation.
The goal remains fully autonomous trading; it is not enabled yet.

### Verified role source integrated before corrected executor run

Merged verified role-binding source 2768670 into the executor correction locally.
Only appended ADR/plan text and the test floor conflicted; retained both sections
and set the combined floor to 2228. Rather than running the lint-only correction
separately and then another merge run, publish this combined source after the
existing executor run is terminal. Role behavior is already verified; executor
behavior and the combined result still require the fresh full GitHub run. This
supersedes the earlier sequencing note, not any activation gate. No live change.


### Exact-SOL output units and handback (2026-10-10)

Previous goal turn made source progress by combining verified role checks and the
executor correction. This turn verified CI 38081547094 at f90b058 fully green:
17 mutants, nine caught and eight unviable, none missed; tests/build/lint and all
other jobs passed. PR 352 now records those results. Nothing new was deployed.

Started fix/curve-minimum-output-units from that verified combined source.
Source inspection found the direct builder mislabeled min_tokens_out as
slippage_bps. First-party pump interface at 2293f9a and successful captured
transaction 3Dk1fn5... confirm native input then raw-token floor. Research 0039
records the evidence, including the old failed-packet caveat. Rename the field
and add Trade::exact_sol_buy, used by the existing builder-to-signer integration,
to calculate a nonzero raw floor from a fee-inclusive quote and tolerance.
Added capture-backed arguments and boundary/encoding regressions. Floor 2230.
No local tests/build/lint/mutation jobs or live signing/deployment occurred.

Publish for fresh GitHub verification. This corrects construction units, not
quote freshness/provenance or independent authorized minimum-output enforcement.
Those, live holdings/exit authority, supported sell construction and reconciliation,
portfolio accounting/recovery, adaptive supervisor/research chat and protected
site handover/deployment still remain. Goal stays fully autonomous trading.

Exact-SOL run 38081913177 at fe1da36 found a lint naming collision in the new
capture test (captures/captured). Rename the payload packet. All mutation shards
passed; let remaining jobs finish before publishing the correction. Require a
fresh full GitHub run rather than treating a naming-only fix as already verified.

The same initial output-unit run also found the new LEARNINGS entry lacked the
required recurrence-check line and index row. Added both, naming its actual
argument/conversion regressions; adjusted the index count. The trading tests
passed in that run, but repository conformance is a required test and the run
remains failed. Publish this documentation fix with the naming correction only
after the old run finishes. No production behavior changed in either correction.

### Integrated verified signer and executor source (2026-10-10)

Fast-forwarded feat/integrated-autonomous-controls from 63e66e6 to verified
f90b058. This includes direction, explicit native units, aggregate token-debit
bounds, captured mint/trader/quote roles and correct executor buy/sell dispatch.
Their combined CI 38081547094 passed every job, with 17 mutants (nine caught,
eight unviable). Earlier accounting and optional-cap controls remain included.
No production authority or service was changed. Test floor 2228.

Request fresh broad integrated GitHub CI and a release-linux artifact before
protected installation work. Fixed radar-deploy updates Serve only; do not imply
that downloading an issuer/signer binary installs or activates those services.
Exact-SOL builder correction is separate PR 353; initial run 38081913177 has a
naming-only lint correction committed as 72f296a, awaiting terminal old run before
publication. Do not integrate it as verified yet. Authorized output floors, live
holdings/exit issuance and reconciliation, portfolio inputs, adaptive supervisor/
research chat, recovery and protected site handover remain goal work.


### Issuer-bound output protection and handback (2026-10-10)

Previous goal turn made source progress on the builder-unit correction and
verified-source integration. This turn inspected initial PR 353 failures, fixed
its LEARNINGS metadata and published the correction. GitHub retargeted PR 353
after its executor base was integrated: it now conflicts only in appended plan
text against integrated 33cbe0f, so it has no fresh run until resolved. Do not
mistake missing checks for a running verification handle.

Started feat/signer-output-floor from verified f90b058 and fast-forwarded its
base to integrated 33cbe0f (documentation-only difference). Integrated CI
38082045246 and release 38082041644 have both now passed. No live change.

Actual caller: radar_signer::check and the protected issuer prepare path. Added
optional min_output_raw authority, checked aggregate encoded trade-output floors
and refusal for zero/insufficient/overflow guarantees and unsupported suffixes.
Current first-party IDL includes partial-fill options, so do not assume the first
two u64 fields alone guarantee the named output with arbitrary trailing arguments.
The protected issuer requires a positive floor for known curve trades, drawn from
its private snapshot rather than stdin. Legacy absent library/wire authority
retains no output guarantee; kernel decisions invent no quote. Existing native
issuer regression now supplies a protected floor and still isolates native units.

Four actual-process regressions cover all variants/units, split floors/overflow,
proof tampering, foreign/nontrade discriminators, optional suffixes, and issuer
refusal before reservation. Extended wire compatibility; floor 2232 on this branch
(and 2234 once the two builder regressions are integrated). No local jobs or live
signing/deployment. Publish for GitHub tests/build/lint/mutation verification.

Next: resolve PR 353's retargeted-base conflict, inspect both fresh runs, then
combine verified source. Download/check the verified integrated release for later
protected installation. Live quote provenance/slippage policy, holdings/exit
issuance/construction/reconciliation, portfolio accounting/recovery, adaptive
supervisor/chat and protected website handover remain before goal completion.

Output-bound initial CI 38082644735 at 6df0d00 found two issuer-helper lint
findings (boolean spelling and method closure), corrected without changing its
meaning. Shard 0 reported the exact execution_output.rs:8:13 && to || mutant
survived: existing issuer tests had no foreign-program trade payload or known
curve nontrade. Added both accepted legacy no-output-promise cases in the actual
issuer process; that mutant wrongly requires a curve floor for them. This is
classification coverage, not proof of arbitrary program execution semantics.
Floor now 2233; combined builder floor will be 2235. Let the old run finish before
publishing; require the fresh GitHub mutation run to prove the exact guard.
Integrated CI/release 38082045246/38082041644 passed, and every one of the six
downloaded binary SHA256 hashes matched BUILD-INFO.txt at 33cbe0f. No live change.

### Verified builder and output-regression publication handback (2026-10-10)

This goal turn progressed verification and publication, not live activation.
Builder PR 353 at f18f368 passed full GitHub CI 38082687122: every job green,
including 13 mutants (11 caught, two unviable, none missed). Its PR description
now records exact evidence. It is ready to integrate once the parallel output
branch has finished against its existing base.

Output initial CI 38082644735 completed: tests/build/MSRV passed, with only the
previously identified lint and classification-mutation failures. Published the
committed f2fa79e correction after that run was terminal. Fresh GitHub CI
38082995334 is in progress; lint and the formerly failing shard 0 now pass.
Do not claim full success or push over this run. PR 354 records the pending run.
No local tests/build/lint/mutation jobs, live service or wallet changes occurred.

Next: inspect terminal CI 38082995334 and its exact mutation results; integrate
verified builder/output source, resolving appended plan/test-floor differences,
then require full combined GitHub CI/release. Continue protected holdings-derived
exit authority and actual bidirectional construction/reconciliation, live marked
portfolio/evidence, adaptive supervisor/research chat, recovery and protected
site handover. Existing checklist decisions remain settled; do not ask for chat
limits again. Autonomous trading remains off and the goal remains active.


### Corrected builder verification after base retarget

GitHub marked executor/role/token PRs integrated when their exact source reached
the integration branch, and retargeted PR 353 to that branch. Its correction
push c963b24 had no CI run because appended plan notes conflicted with 33cbe0f.
Merged that verified documentation base, preserving both handbacks; no source
conflict or trading behavior change. Fresh GitHub verification is required.
Issuer-output enforcement is independently published as PR 354 at 6df0d00 and
not yet integrated or verified. Integrated 33cbe0f CI/release both passed; no
production authority or service changed. Goal remains fully autonomous trading.


### Combined output protections and diagnostic gate (2026-10-10)

Prior turn made progress publishing the output correction and verifying the
builder. This turn observed CI 38082995334 at f2fa79e fully green: 32 mutants,
19 caught and 13 unviable, none missed. However shard 0 had eight unviable and
zero caught, with an explicit cargo-mutants warning. Therefore the exact original
execution_output.rs:8:13 guard regression is not yet proven caught; a green
summary alone does not meet that acceptance criterion. The workflow retained no
mutation logs, so the compiler cause is presently unknown.

Merged verified builder f18f368 into the output branch, retaining both appended
plan histories and setting the combined test floor to 2235. Added per-shard
GitHub diagnostic artifacts (outcomes, logs and diffs) using the already-pinned
upload action, without changing mutation selection or weakening any check.
Publish this combined source for full GitHub CI and inspect the exact guard's
outcome/compiler log before treating output protection verification as complete.
No local verification jobs or live wallet/service/authority changes occurred.

Next actionable execution gap confirmed by source: issuer issue remains buy-only
and reconciliation apply rejects sales, while the executor dispatches sells.
Holdings-derived token authority, sale completion/credits and durable recovery
must be connected before autonomous position management. Actual supported route
construction, live marked portfolio/evidence, adaptive supervisor/chat and
protected website handover remain. Goal active; autonomous trading stays off.

### Output protections verified and integrated (2026-10-10)

Combined source 67c3e9e passed every job in GitHub CI 38083298087, including
45 mutations: 30 caught, 15 unviable, none missed. Downloaded the four diagnostic
artifacts and inspected outcomes plus the exact original guard's diff and log.
execution_output.rs:8:13 AND-to-OR is CaughtMutant: its build succeeded and the
issuer_distinguishes_nontrades_and_foreign_program_trade_bytes process test
failed at its intended refusal-versus-issued assertion. The earlier all-unviable
shard summary did not establish this; the current exact evidence does. Other
unviable cases are not mislabeled caught. No exclusions or local jobs were used.

Fast-forwarded feat/integrated-autonomous-controls from 33cbe0f to this verified
combined source, keeping builder and protected output authority together. Test
floor 2235. Publish this handback and request fresh broad integrated GitHub CI
and release before any installation. No production or authority change occurred.

Next implementation: complete sales with retained exact native credits and
outgoing costs, FIFO token/basis anchors, atomic durable completion and restart
idempotence, then protected holdings-derived exit issuance. A native debit-only
Settlement cannot by itself represent returned sale cash; do not disguise a
credit as an unsigned negative debit or erase the fee. Actual routing, live
portfolio/evidence, supervisor/research chat and protected site handover remain.
Goal remains active and autonomous trading is off.

### Sale cash completion implementation and handback (2026-10-10)

Previous goal turn progressed by proving the exact output guard caught and
integrating the verified builder/signer source. This turn observed broad
integrated CI 38083559773 and release 38083555818 at 896009c fully green.
Release download/hash verification remains to do; nothing new was deployed.

Started feat/sale-cash-completion from 896009c. Actual callers are Portfolio::settle,
OperationLog reconciliation/replay and the protected issuer reconcile-operation
consumer. Added CompletedCashFlow with separate same-asset spent/received units;
credits cannot hide spending beyond the claim. Both balance sides and range are
validated before mutation; the journal persists before applying the copy. Replay
checks the original intent/reservation and both units. Existing wire variants
retain their meaning.

The issuer derives the exact cash sides from reverified retained sale valuation,
checks pre/post cash, chronological buy/sale token anchors and FIFO/current
inventory before completion, and replays matching terminal sales exactly once.
New process regressions cover buy then partial sale, fees/proceeds, remaining
basis, restart/retry idempotence and next-issuance risk/checkpoint refusal. Hidden
sale token gaps and incoming-credit-funded overspending remain refused with the
claim and journal unchanged. Added portfolio atomicity/other-claim/unit/overflow
regressions and extended journal tampered-terminal replay cases. Test floor 2239.

Publish for GitHub tests/build/lint/mutation verification; no local jobs ran.
This closes an offline economic completion gap, not holdings-derived exit
issuance, actual route construction, live evidence/portfolio, dropped recovery,
adaptive supervisor/research chat or protected site handover/deployment. Keep
those goal items open and live signing closed. Inspect this branch's exact CI
results next, then continue the protected exit path. Goal remains active.

Initial sale CI 38084015922 at 2be1777 found journal replay exceeded the lint
function-length threshold. Extracted its unchanged completed-record validation
into a focused helper. Mutation shard 1 found reconciliation.rs:63:50 OR-to-AND
survived: checking both token endpoints duplicated the already reverified exact
delta. Removed the redundant end check instead of adding a test for an impossible
single-endpoint inconsistency. Chronological starting quantities plus the retained
verified delta still anchor both ends, and the balanced hidden-gap regression
remains. No exclusion or relaxed check. Shortened the enlarged sale fixture helper.

Integrated 896009c release 38083555818 has now been downloaded; all six SHA256
hashes matched BUILD-INFO.txt. Its CI 38083559773 also passed. Nothing deployed.
Let the initial sale run finish, inspect remaining results, then publish this
correction and require a fresh full GitHub run. Goal active; live signing closed.

Sale correction CI 38084228108 at 94f3a01 passed every mutation shard: 30
mutants, 16 caught and 14 unviable, none missed. Downloaded diagnostics confirm
the real issuer completion paths and credit-unit check are caught, while compiler
refusals remain labeled unviable. Lint then reached two new test assignments and
required their trailing semicolons; corrected both. Wait for the remaining test
job before publication, then require a fresh complete run. No local jobs or live
changes. Protected exit issuance remains the next feature after this gate.
