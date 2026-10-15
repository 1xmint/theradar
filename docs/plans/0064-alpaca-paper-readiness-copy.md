<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0064 — Accurate paper readiness copy

**Status:** copy and existing rendering expectation updated; GitHub verification and deployment pending.

Branch feat/alpaca-paper-readiness-copy starts from verified PR 423 source
70aabc9. It is independent of the protected upgrade in PR 424, which is currently
under verification. No upgrade payload or reader pin is changed here.

The setup footer previously asserted that the supervisor still needed
installation and that execution was disabled, regardless of runtime or owner
handover. October 15 fixed-helper inspection proves a supervisor is installed
and running paused. Use a neutral readiness heading and describe the separate
operational verification needed before unattended trading. Keep the distinction
between owner permission and evidence of a running agent, and keep Solana
settings as separate drafts. This static copy does not claim a healthy lifecycle,
active trading, learned edge or profitability.

Update the existing render expectation for the renamed heading. No new test is
needed for this reversible copy change; existing frontend checks run on GitHub.
No workstation tests/jobs ran. The current production website is unchanged.
After verification, build a website artifact on GitHub and deploy only through
fixed radar-deploy with hash and health-build checks. Verify the authenticated
rendering in Brave. The broader autonomous-trading goal remains active.
