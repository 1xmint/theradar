<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0065 — Attributed research shared with the paper supervisor

**Status:** controller/supervisor, authenticated HTTP, metered model reply and browser sources implemented; GitHub verification pending; not deployed.

Branch feat/alpaca-paper-research-leads starts from verified upgrade source
1955fc7 (CI 38785330408 all fourteen jobs passed), and merge 71b77f8 preserves
verified/deployed readiness copy 1dcf60e (CI 38785773799 all fourteen passed).
Do not change or overwrite the immutable staged rejection-recovery upgrade:
the owner may execute its previously reviewed command at any time. Its root
application remains pending. This feature is a separate future rollout.

## Actual integration being built

The existing Agent.tsx question box and chat.rs reading assistant remain useful,
but they are not connected to the paper trader. Reuse their provider/rendering
plumbing for a private paper conversation, with the existing paper.rs Operator
identity proof, same-origin/custom-header CSRF checks and bounded private IPC.
Required browser routes are paper research read/append; no research request
changes handover, policy, signing authority or order intents.

The controller now stores an exclusive origin and bounded retained message
history under its existing private directory. User and assistant notes are
attributed to the verified operator; roles and receive timestamps are assigned
by trusted callers/controller, not free text. Nonce reuse with identical content
is idempotent; changed content refuses. Partial/missing origin/state never resets
or adopts old unattributed notes. All source claims stay unverified. URLs are
literal data and are not fetched on ingestion. An assistant reply references
exactly one prior unanswered user question.

The supervisor's live observation now includes up to six notes available at its
watermark. Unknown/corrupt/uninitialized research is explicit unavailable context,
not an assertion of no tips, and does not disable the owner's pause. The model
receives current research once, with historical copies excluded from its prompt
to prevent multiplying a chat across sixteen observations. The frozen audit
observations remain intact. Tips never replace market/account reads or the risk
kernel. No extra broker/program invocation occurs when saving research.

## Limits and unfinished work

Each message is at most 4000 UTF-8 bytes; the initial research file retains up to
128 messages under a 1 MiB read bound. At the bound new research refuses without
silently deleting old content. This is a temporary retention limit requiring a
reviewed archival path; do not describe it as indefinite conversation storage.
The private read returns the full retained thread; model context truncation is
explicit. Messages from future timestamps are excluded, never backdated.

New controller methods and supervisor context parsing are implemented, with
eight controller research tests, two supervisor context tests and twelve exact
reapplications queued in the justfile. The authenticated paper research endpoint
now saves a user message before any model call, assigning role/owner server-side.
A browser nonce binds identical resubmissions, existing replies avoid another
call, and one reply runs at a time per website process. A bounded plain-text
reply uses the shared chat provider/meter and durable ledger, with all messages
inside Request::observing fences. No provider call occurs if its reservation
cannot be persisted. Provider errors return the saved thread with an explicit
unavailable reply; an uncertain controller write stays an HTTP error. The UI
preserves unresolved drafts and reads history before another send, without an
automatic retry or HTML/link interpretation. Three model-boundary tests, four
HTTP/projection tests and five browser tests are added, not yet executed.

This chat's initial reply has no browsing tools and says so. The autonomous
supervisor consumes recent conversation separately with its account/market
observation. External investigation tools, archival beyond 128 messages,
protected rollout and all GitHub verification remain unfinished. Do not publish
this as completed autonomous trading or claim an external link was investigated.
No local tests/builds/jobs ran; no staged VPS or deployed file changed.

Next verify malformed tips/roles/identity/clock/partial writes, lost responses,
metering and prompt framing on GitHub, then prepare a separate coordinated rollout only after observing the
existing owner upgrade. The full autonomous-trading goal remains active.

## Handback — October 15, restored SSH

After the owner completed the fresh Tailscale check, BatchMode SSH succeeded.
The fixed `radar-paper-autonomy-admin inspect` reported successful recent
supervisor ticks at 19:16–19:18 UTC, each paused with execution_authority false;
the timer was waiting and the controller/socket were active. Public installed
hashes still match reader 0c421ea, controller d0bd5ac and supervisor db74d45:
the recovery upgrade is still unapplied. Its three staged hashes were rechecked
and exactly match plan 0063; none of those files was changed or executed.

GitHub run 38788604287 at 997ecb3 completed successfully with all fourteen jobs.
This verifies the readiness-copy documentation branch, not the uncommitted
research implementation. No research tests have run yet. The provider inspection
confirmed that the existing chat has a shared locked agent meter, durable ledger
and bounded read-only investigation, but its trusted system framing is Solana
specific. Paper research must supply its own trusted framing and watermark-bound
context while preserving that metering; routing the new conversation through
the old Solana prompt would misrepresent its capabilities. HTTP/UI/provider
integration was the next implementation step at that checkpoint; its source is
now implemented above and awaiting GitHub verification. Root installation still needs
the owner's already-reviewed terminal command because no installed passwordless
grant permits installation. Do not overwrite that staged command's payload.
