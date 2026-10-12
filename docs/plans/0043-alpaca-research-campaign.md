<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0043 — Scheduled subscription research attempts

**Status:** GitHub-verified scheduled attempt worker; not installed or enabled.

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
- [x] Inspect GitHub tests, targeted mutation reapplications and every job:
  99085c7, CI 38285036998.
- [ ] Deploy a verified artifact and provision a deliberate campaign.
- [ ] Observe real model decisions/failures and score complete cohorts.

## Handback

Code proof: 99085c7fa4ee7ed2320e0c129974c4a316d86917, run 38285036998
passed all 14 jobs. Tests job 114931253179 passed 2817 Rust and 36 Python tests
(nine new campaign tests). All 12 campaign mutations and 32 existing Python
mutations were caught, along with both subscription regression reapplications.
Both pairs of systemd units verified. The four Rust shard logs explicitly say
the diff changes no Rust source; these are not Rust mutation catches. No mutation
exclusions added. Draft PR 404 is attached and stacked on PR 403.

Release-linux run 38285575372 passed, including every build/upload step. Its
BUILD-INFO commit matches the code proof above. Downloaded CLI SHA-256 matches
12efd7e4fd958138840dc0056f84fe6ba5690cb7de01ce02065131e6cb2112d0.
Staged at /tmp/radar-research.new on the VPS; independent SSH hash matches,
13,661,032 bytes, guardian:guardian 0644. This is a staging file, not installed
or executable, and no root-owned binary/service changed. The old recorder remains
active. Next provisioning should isolate the research binary from the existing
hash-pinned recorder rather than silently invalidating that installer's hash.
No production web restart or recorder replacement. This handback is documentation
only; later documentation CI is separate from code proof.

Read-only SSH inspection confirmed the existing model socket is active/running,
radar-agent:guardian mode 0660; its client wrapper is root:root 0755. Both
/usr/local/lib/radar-paper and /etc/radar are root:root 0755. The existing market
recorder timer remains active. No credentials were read, no model call made.

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
