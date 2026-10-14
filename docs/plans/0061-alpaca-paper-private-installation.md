<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0061 — Combined private paper installation

**Status:** installer source GitHub-verified; extra metadata regressions and VPS installation pending.

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

- Pending GitHub unit suites and exact installation regressions. Fake systemd
  tests prove installer boundaries, not a deployed lifecycle.
- Pending reviewed archive/installer staging and matching VPS hashes.
- One owner terminal sudo invocation is needed for this installation. Existing
  passwordless grants do not authorize root installation; never request or read
  the password in chat.
- After install, use fixed radar-deploy with the verified website artifact and
  its hash/health build checks to apply the supplementary socket group.
- Verify separate process identities, successful paused ticks and authenticated
  site status before explicit paper handover. Verify actual order reconciliation,
  exits and pause; do not force random trades to manufacture evidence.
- Richer historical research, forward comparisons and profit after costs remain
  unproven. Live activation is separate and not authorized by this installer.

## Current handback

The previous goal turn made source/runtime-inspection/verification progress.
This turn implements the combined installer and operational diagnostics.
No new service or permission has been installed. No local tests or jobs ran.

The owner reported fresh login, but the in-app browser still required Google
authentication. The owner explicitly directed browser control to Brave through
the extension instead. Opening Radar there reached the authenticated setup page:
ChatGPT displayed Linked, and the Alpaca panel displayed private control unavailable,
consistent with the controller not being installed. No credentials were retrieved,
entered or moved between browsers; no handover or trade was enabled.

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
All job statuses were inspected after completion. Two further exact metadata
reapplications and direct file/directory ownership/mode checks are being added;
they change only tests/harness, not the verified installer or bundle payload.
Their verification remains pending. No jobs were canceled or run locally.

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
release. These receipts do not prove a VPS install; remote staging hashes must
match before asking for the one-time sudo invocation. The installer code and
payload remain identical to the source verified above while metadata tests run.
