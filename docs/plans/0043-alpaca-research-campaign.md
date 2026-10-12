<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0043 — Scheduled subscription research attempts

**Status:** Implementation awaiting GitHub; not installed or enabled.

Branch feat/alpaca-research-campaign from 7e51940, stacked on PR 403. Previous
goal turn made concrete progress: verified inert model-proposal source and
regression tests. Parent documentation CI 38284227085 passed all 14 jobs,
inspected before this branch's push. No blocker audit applies.

## Checklist

- [x] Actual worker invokes fixed market-read then model-propose commands.
- [x] Freeze the campaign and record each claimed slot before external calls.
- [x] Retain missed, failed and interrupted-unknown slots without automatic retry.
- [x] Separate broker credential environment from model environment.
- [x] Bound one slot per invocation, storage reservation and subprocess deadlines.
- [x] Add optional hardened service/timer source, no installation or activation.
- [ ] Inspect GitHub tests, targeted mutation reapplications and every job.
- [ ] Deploy a verified artifact and provision a deliberate campaign.
- [ ] Observe real model decisions/failures and score complete cohorts.

## Handback

This is scheduled attempt retention, not a learning result or order supervisor.
The example's start timestamp is deliberately invalid until the operator chooses
a future campaign. Its quantity/costs are explicit hypothetical research values,
not broker capacity/fees or an account mandate. Quote-only input provides a
useful abstention control; it does not supply learned historical signals.

The worker copies the exact campaign bytes and binds begin records to their
SHA-256. Config changes require a separate store. File and directory fsync
precede subprocess calls. Partial records refuse further processing; claimed
slots without terminal records are interrupted_unknown and never retried.
A slot observed at/after its start window is recorded missed without calling
the model. Future slots remain visible. Storage exhaustion is explicit and does
not delete history. One tick processes at most one due slot, so missed-slot
backlogs need multiple ticks. This is not an attestation against owner edits.

Decision publication and terminal publication are separate: a crash between
them remains unknown, not an adopted success. Failed attempts may retain partial
capture/decision files, which must not be treated as terminal decisions. Failures
do not store vendor bodies, keys or subprocess stderr, and do not imply zero
model usage, zero P&L or a Wait recommendation. Cost-aware cohort outcome
evaluation remains unfinished. Host-clock movement and stale inputs remain
possible; the proposal CLI independently rechecks quote age at completion.

The service uses guardian because the existing local bridge authenticates that
UID. It loads only the paper credential file; the worker forwards broker keys
only to the reader and fixed linked-model configuration only to the proposal
process. The model bridge keeps its existing permissions and CLI authentication.
The new units are source only. Reader release/provisioning, public campaign
config readability and runtime ownership need verification before installation.
No broker submission command, web route, signer change, paid data or deployment.
The fully autonomous trading goal remains active and incomplete.
