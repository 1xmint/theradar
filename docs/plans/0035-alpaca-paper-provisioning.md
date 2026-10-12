<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0035 — Private Alpaca paper credential provisioning

**Status:** GitHub verified and uploaded; private credential entry pending.

Branch: feat/alpaca-paper-provisioning, based on 80258a3. Parent GitHub run
38256428823 passed all 14 jobs; inspected with `gh run view --json jobs`.

## Checklist

- [x] Inspect the owner's actual Alpaca dashboard: paper mode, simulated cash,
  no positions or orders. Generate paper API keys after explicit confirmation.
  Do not read, copy or record their values in model context.
- [x] Add the actual administrator caller for private terminal provisioning.
  Hidden prompts, fixed HTTPS paper account GET, no redirects/proxies, bounded
  response, active USD account with explicitly false restriction flags.
  Root-owned non-writable directory, root-only atomic no-overwrite credential
  file, including refusal of existing symlinks. No service configuration changes.
- [x] Add fake-provider and temporary-file tests to the existing GitHub private
  setup job: account restrictions, transport sanitization, redirects, permissions,
  overwrite/symlink refusal and credential injection.
- [x] Inspect GitHub verification and upload the exact verified setup script.
- [ ] Owner enters paper credentials privately and confirms provisioning.
- [ ] Actual protected Radar reader verifies the account. Trading activation
  remains dependent on the unchecked items in plan 0034 and forward evaluation.

## Handback

The browser's Generate New Keys action was specifically approved by the owner.
Values were not read or exported. Keep the browser's visible keys until the owner
has copied them privately. The default simulated $100,000 cash and $400,000 buying
power are observations, not an authorized experiment allocation or leverage policy.

`deploy/setup-alpaca-paper.py` is an administrator-only provisioning command,
not an executor or website endpoint. It writes /etc/radar/alpaca-paper.env (0600)
only after a fixed paper account verification. It never installs/restarts a
service, emits credentials, submits orders, or enables autonomous trading.
Verification and installation are separate steps; a saved key may later be revoked.

GitHub run 38260583301 at 988643f09ec3803849a26ab99061ec3ea3ee1241
passed all 14 jobs, inspected using `gh run view --json jobs`. The tests log
for job 114849729747 measured 2,804 Rust tests and 12 Python tests, including
all four new SetupTests. All four mutation shards reported no changed Rust
source; those jobs do not mutate this Python implementation. No claim of Python
mutation coverage. No workstation test/build/lint/mutation jobs ran.

The authenticated Tailscale SSH check completed successfully. Read-only server
inspection reported /etc/radar as root-owned, mode 755. Uploaded the verified
script with scp; local Get-FileHash and remote sha256sum both measured
4b23a43f80e779d7c75042de6492dcdae3f08683475f30cad5539c26e8afd44a
for /tmp/radar-alpaca-paper-setup.py. Owner was given the exact private terminal
command and asked for its success/refusal message only. No credentials have
been entered or saved by this agent; runtime verification remains pending.

No credentials in GitHub, funded account changes or orders. Parent code remains
verified separately by plan 0034. No claim of profit, runtime integration or
unattended activation. The evidence update is documentation only; code evidence
above is for 988643f, not an inferred later HEAD.
