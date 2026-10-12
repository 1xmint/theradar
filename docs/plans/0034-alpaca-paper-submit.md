<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0034 — Protected paper submission after deterministic risk

**Status:** implementation ready for GitHub verification; activation closed.

Branch: feat/alpaca-paper-submit, based on 08943b5. Previous goal turn was
verified progress on equity risk and the actual preflight caller. Parent run
`gh run view 38209966608 --json status,conclusion,jobs` passed all 14 jobs.

## Checklist

- [x] Add actual private alpaca-paper-submit CLI caller using the same directly
  collected state as preflight, with existing journal ownership. Never deserialize
  state or authorization from an operator/model evidence file.
- [x] Evaluate deterministic risk, serialize exact decimal order fields, decode
  the actual request and check it against opaque authorization before reserving.
  Persist the immutable reservation and SubmissionUnknown before any effect.
- [x] Reload protected owner policy and host time after durable writes, recheck
  original authorization and evaluate the latest policy over aged provider facts.
  Do not refresh stale provider timestamps merely because policy was reloaded.
- [x] Fixed paper POST endpoint, existing redirect-disabled credential agent,
  no live-host/configurable-host path. Sanitize transport and parse errors.
  Bind response payload to retained intent; a response is not reconciled fills.
- [x] Add payload precision, unknown-evidence, expiry/revocation, uncertainty,
  restart/no-retry and response mismatch tests. Actual collector-to-submit test
  proves unknown costs/capacity prevent any reservation or network effect.
- [ ] Inspect GitHub tests/build/lint/mutation diagnostics and repair findings.
- [ ] Establish actual account/feed capacity, conditions, session times, order
  increments and costs. Add service isolation/authenticated supervisor transport,
  durable owner handover/revocation, paper cancellation/recovery and site controls.
  Run forward learning/evaluation before considering eligible real-money trading.

## Handback

No account credentials, actual orders, deposits, deployment or activation.
`radar alpaca-paper-submit --journal <existing-private-file> --intent <typed-intent>
--policy <private-policy> --max-pages <positive-u16>` is a private operator
process, not a web route or authenticated execution daemon. Protected policy,
credentials and journal paths must be provisioned outside model/web access.
The actual collector still supplies unknown capacity and costs, so risk refuses
before reservation/POST. This does not provide an enablement shortcut.

Once verified inputs are implemented, the path makes one first submission only.
Existing client IDs refuse; ambiguous/malformed/mismatched responses retain all
claims. Even final policy/clock refusal after the uncertainty write remains
conservative uncertainty. No automatic retry, release or inference of nonreceipt.
Recovery/cancellation must be completed before unattended activation.

The wire request has simple limit/day/regular-hours fields and exact nanoshares
and micro-USD digits. Fields follow the official
[create-order endpoint](https://docs.alpaca.markets/us/reference/postorder), which
documents HTTP 200. Response payload matching reuses the recovery validator.
The final check bounds freshness of already collected state; it is not an atomic
broker snapshot or a guarantee against changes between checking and execution.
Preflight reports now name oldest_input_unix_ms explicitly; it can precede host
read start when exchange quote time is older. No Solana-slot clock substitution.
