<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0023 — Account-bound paper equity assets

**Status:** source verified; combined integration verification pending.

Branch: feat/alpaca-asset-observations, independent base integration 6ae333d.
This is independent of order journal/recovery implementations under verification.
Keep their parent fixed while all child checks run. No workstation jobs.

## Objective and checklist

The future paper executor needs evidence about the exact instrument it proposes,
including whether fractional shares can trade. Actual caller: alpaca-assets-read.

- [x] Fixed paper host, GET only, shared redirect-disabled credential transport.
- [x] Pin expected account identity before and after requested asset reads.
- [x] Bind each response symbol, nonempty unique asset ID and equity class.
- [x] Preserve nullable flags/status; neither absence nor false becomes true.
- [x] Reject URL path/query injection, dot segments, duplicates and malformed data.
- [x] Three unit and one process regression, independent floor 2313.
- [x] Full CI 38182759739 passed at ba4e58a04cfaf2d3e90b7dd44c2d523c011588b1,
  including every mutation shard after the naming-lint repair.
- [ ] Integrate with the verified exposure, recovery, journal and setup work.

## Handback

Initial CI 38182530550 lint flagged Asset.asset_class as a redundant struct-name
prefix. Rename the internal field to kind and preserve the JSON class rename.
Every mutation shard passed. Full-run completion and replacement verification
are still required before integration; do not cancel or push over active checks.

Source only; no keys, orders, deposits, deployment, paid data or delegation.
The CLI accepts --account-id and --symbols only, with no live endpoint override.
Only selected observations are returned; unknown response fields are omitted.
Account changes in selected fields remain visible. Missing tradability and
fractional flags remain null. Inactive/nontradable equities are observations,
not execution permission. Unsupported asset classes refuse the entire report.
The endpoint/model were checked against official Alpaca documentation and SDK.

This does not yet verify session eligibility, increments, quote conditions,
complete fresh risk state or mandate. The next executor must consume these
observations through deterministic checks, use retained journal identities and
reconcile fills/cash before freeing claims. Preserve the owner's adaptive risk,
per-option Agent decides, optional daily stop and research chat requirements.
The full unattended paper loop and separately eligible live setup remain open.
