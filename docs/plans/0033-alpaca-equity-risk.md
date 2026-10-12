<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0033 — Pure equity risk and the protected paper preflight caller

**Status:** implementation ready for GitHub verification; activation closed.

Branch: feat/alpaca-equity-risk, based on 8d7790e. All 14 jobs in
`gh run view 38207446630 --json status,conclusion,jobs` passed before branching.
Previous goal turn made verified progress on durable share/cash accounting.

## Checklist

- [x] Add pure equity policy/proposal/state decisions in the existing risk crate,
  with Unix milliseconds and explicit nanoshares/micro-USD, never Solana slots.
  Check handover/pause, account/identity, time, regular session, marketable limit
  distance/spread/capacity, cost/fee evidence, cash, shares and allocation bounds.
- [x] Optional daily loss stop affects entries; owned-share reductions still
  require independent fee, liquidity, cash and share checks. Missing facts refuse.
  Opaque in-process authorization binds the exact order and validity interval;
  there is no deserialize/constructor or credential input to model reasoning.
- [x] Actual private alpaca-order-preflight caller owns the journal, advances its
  existing cash/share checkpoint, joins bracketed pending orders and positions,
  verifies owner/account/asset fields, collects IEX quote and a final account,
  then calls the pure kernel. Every provider URL is constructed on fixed hosts.
  Redirects remain disabled. No order mutation/submission is present.
- [x] Count retained liabilities once: subtract claims from cash, then bound by
  broker buying power/equity; do not subtract them twice from buying power.
  Retained sells reserve owned shares; other open orders or changed progress refuse.
- [x] Add kernel boundary/unknown/overflow/payload/expiry/reduction tests and actual
  caller tests for fresh normalized reads, local liabilities, pending identities,
  restrictions and private input bounds.
- [ ] Run/inspect full GitHub tests, build, lint and mutation diagnostics; repair
  real findings and record precise evidence before claiming verification.
- [ ] Resolve actual account/feed capacity units, conditions, session timestamps,
  price/quantity increments and fee/cost adequacy, then pass verified facts.
  Isolated authenticated paper executor, supervisor, owner site controls and
  forward learning/outcome evaluation remain required. Real-money deployment
  requires separate eligible live setup and measured performance evidence.

## Handback

No account keys, actual orders, deposits, deployment or activation. Owner account
creation remains pending; independent code work continued. GitHub verification
pending. Preflight emits a report, never a portable signing/execution token. It
does not reserve the proposed order or authorize retries. It may advance a valid
observation checkpoint before a later risk refusal; existing order claims remain.
Capacity units/quote conditions and cost/fee adequacy have not been verified on
this account, so their state inputs remain unknown and execution stays refused.
Optional daily-loss measurements also remain unknown. Preserve the owner's
manual/Agent decides controls and optional loss stop when resolving effective
policy from the site; do not request replacement numeric limits in chat.
The kernel is a bound checker over protected caller facts, not a provenance
verifier. A separate executor must authenticate its caller, recheck exact payload,
time, owner revocation, current broker state and durable client-ID reuse before
releasing effects. This increment supplies its risk boundary, not those effects.
