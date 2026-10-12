<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0035 — Private Alpaca paper credential provisioning

**Status:** implementation ready for GitHub verification; account connection pending.

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
- [ ] Inspect GitHub verification and upload the exact verified setup script.
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

GitHub verification pending. No workstation tests, credentials in GitHub,
funded account changes or orders. Parent code remains verified separately by
plan 0034. No claim of profit, runtime integration or unattended activation.
