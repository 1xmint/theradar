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
- [ ] Full CI including all four mutation shards at the code commit.

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

**Handback in progress:** direct wallet measurement only. Remaining work includes
trusted endpoint/wallet provisioning, measured market and transaction evidence,
active mandates, settlement/loss reconciliation and independently tested Privy
refusals. Trading remains inactive; no deployment or real signing in this change.

Live command verification used the owner's previously supplied wallet and
`https://api.mainnet-beta.solana.com`, read-only, at Unix time 1791399631. The
node reported 0 native lamports at slot 454310379, no SPL token accounts at
454310380, and no Token-2022 accounts at 454310381. The command correctly emitted
null for common_reported_slot, USD value and realised P&L. These are those three
node responses, not a current valuation or proof of no other asset types.
