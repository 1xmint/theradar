<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0061 — Combined private paper installation

**Status:** installed and independently observed paused; authenticated panel connected; handover and full order lifecycle unverified.

Branch feat/alpaca-paper-install, parent f1a139c. Supervisor implementation
48936c3 passed CI 38476247749; parent documentation CI 38477850089 completed
successfully before this branch is pushed. Do not change the parent while this child is open.

## Concrete installation

The one-time root script accepts a reviewed tar SHA256, validates every member
without extracting paths, and pins the already verified paper stress reader.
It checks existing subscription bridge/client hashes and credential metadata;
it never reads broker environment contents or changes subscription credentials.
Installation refuses existing targets, partial state, foreign/missing genesis,
owner policies and known active research/model calls. The original broker journal
and genesis/lock markers are retained and compared after the initial paused tick.
Unknown setup outcomes require inspection, not deletion or blind reinstallation.

Completed guardian-UID research timers are disabled while their files remain.
The supervisor gets a distinct non-root user and private state directory, with
origin/lock created once under that service identity. Dedicated socket-group and
web/Codex drop-ins are installed together. The fixed subscription wrapper stays
intact; its reviewed bridge upgrade retains original bytes. A task-count check
refuses a known active model call before stopping the old socket/service; it is
not a guarantee against a new concurrent login request during deployment.

The private paper scenario declares one share maximum assumed capacity per
order, $0.01 fee per side and 10 bps slippage per side. These are simulation
assumptions, not measured live liquidity or costs. Owner budget and per-trade
ceilings still come from the site and may be smaller. The initial owner grant
remains absent/disabled. The installer runs one paused observation tick before
enabling scheduling, checks history preservation, and never initializes broker
history, acknowledges opening cash again, submits an order or restarts the web.

Fixed passwordless paper administration adds only inspect/start/stop of the
supervisor timer and selected unit diagnostics. It cannot edit policy, run an
arbitrary command or read arbitrary paths. Timer stop does not cancel an in-flight
tick; the site's owner pause remains the policy revocation control.

## Verification and activation sequence

- GitHub unit suites and all 23 exact installation regressions passed at b8c1315.
  Fake systemd tests prove installer boundaries, not a deployed lifecycle.
- Reviewed archive, reader and installer were staged with matching VPS hashes.
- The owner completed the one-time root installation in their terminal. Its
  report confirms original journal-prefix preservation and disabled handover.
- Fixed radar-deploy applied the website supplementary socket group, with
  matching artifact/installed hashes and health build checks below.
- Separate service identities, successful paused ticks and authenticated site
  status are observed. Before unattended use, verify actual order reconciliation,
  exits and pause; do not force random trades to manufacture evidence.
- Richer historical research, forward comparisons and profit after costs remain
  unproven. Live activation is separate and not authorized by this installer.

## Current handback

The combined private controller and supervisor are installed. The exact
passwordless inspection helper independently reports successful paused ticks,
an active supervisor timer, and an active controller/socket. The supervisor runs
as radar-paper-supervisor with radar-paper-ipc supplementary membership; the
controller runs as radar-paper-execution. No local tests or jobs ran.

After the owner's installation, fixed radar-deploy returned active. Both the
staged and installed website binary hash are
8c5b14087d97df803c1dd8312207be15df7e2f3d8c43cd585259707ef364e7c3;
localhost health reports build 4116592006059769175b0cfda839439a001b03ab and
status ok. Website User/Group remain guardian, MemoryMax remains 805306368,
ProtectHome read-only, ProtectSystem strict and NoNewPrivileges yes. The new
SupplementaryGroups value is radar-paper-ipc.

The owner explicitly directed browser control to Brave through the extension.
After deployment, its authenticated setup page shows ChatGPT Linked, simulated
cash/equity 100000 USD, zero positions, regular session Closed and paper handover
disabled (broker clock 2026-10-13T21:36:08-04:00). Manual/Agent decides controls,
optional daily cap, Save/Enable/Pause are visible. No preferences or handover were
changed, and no credentials were retrieved or moved between browsers. The footer
still says the supervisor needs installation: that copy is now stale and needs
an accurate status distinction; it does not invalidate the independently observed
paused runtime or establish a verified trading lifecycle.

Next blocker found by source inspection: the controller maps all submission CLI
errors to an unknown outcome, while the supervisor retains unresolved attempts
and refuses to proceed when their nonce is absent from the journal. A routine
pre-reservation risk refusal can therefore halt unattended operation. In
crates/radar-cli/src/alpaca_submit.rs, the initial equity verdict precedes reserve,
whereas errors after reserve deliberately retain uncertainty even if no network
call occurred. Add an explicit, provable pre-reservation refusal outcome and its
consumer behavior; never infer safe retry from stderr text, a missing broker ID,
or a post-reservation error. Verify refusals resume fresh reasoning while actual
unknown submissions remain closed. Then verify owner handover, actual paper
entry/exit/reconciliation/cancellation/pause without forcing random trades.
Adaptive learning, forward performance after costs and live activation remain
unfinished; this installation does not establish an edge or authorize live orders.

## First GitHub verification

Draft PR 422, source 2fe67017a1cb02ab94099b04c7a676b1fb038bb6, CI 38492547352
completed with thirteen successful jobs and one failed test job. Python discovery
ran 136 tests; the partial-install recovery case failed because the fixture's
mocked existing-file pin raised AssertionError on a second attempt, whereas the
real boundary raises ValueError. The fixture now preserves the real refusal type.
No installation regression execution is claimed from that failed run; discovery
stopped the recipe before mutation checks. No local tests or jobs ran.

## Verified installer and candidate bundle

Source 615da927ea485ac6bc0ae656da4a253c4203e13d passed all fourteen jobs in
CI 38493019184. Tests job 115622400528 reports 2862 Rust tests, 136 Python tests,
all 21 installation reapplications caught, and successful systemd unit checks.
All job statuses were inspected after completion. Source
b8c1315edeab883a660b9794b0c287a22daf5a24 then passed all fourteen jobs in
CI 38494341124, including the two further metadata reapplications and direct
file/directory ownership/mode checks. Tests job 115626575502 ran 136 Python
tests and caught all 23 installation reapplications, followed by systemd unit
checks. These additions changed only tests/harness and documentation, not the
verified installer or bundle payload. No jobs were canceled or run locally.

Local packaging under workstation TEMP/radar-paper-install-candidate-38493019184
retains eleven exact named payload files. Packaging/listing/hashing ran locally;
no test or application process was executed. Candidate receipts:

- radar-paper-autonomy-bundle.tar: 67584 bytes,
  5189e78d40993f50552cd57fc9ef946a52040be653e7882b842a25dc0417cd50.
- radar-paper-autonomy-reader.new: 13879760 bytes,
  0c421ea6475f400cda75126214725f718ed37cede13fe68d72c5f663016494f9.
- radar-paper-autonomy-setup.py: 14032 bytes,
  81109f976f8b9fcc910d7f314c3baf97a7cb37c8e07fc9502dfa127fb2c21ed0.

The binary independently matches BUILD-INFO from the already verified a66793a
release. All three remote staging hashes matched before the owner's one-time
sudo invocation. The owner then reported private_paper_installed true,
original_prefix_preserved true, handover_enabled false, new_order_authority false,
and website_restart_required true. The independent runtime and subsequent fixed
website deployment checks are recorded in Current handback above. The installer
code and payload remain identical to the verified source above.
