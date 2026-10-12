<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0042 — Linked subscription equity research proposals

**Status:** In progress; no live model run or trading authority.

Branch feat/alpaca-model-proposal from d83f8f6, stacked on PR 402. Previous goal
turn completed GitHub-verified forward decision recording and matched scoring.

## Checklist

- [x] Add actual CLI calling the configured subscription provider with fenced
  quote observations and strict Enter/Wait JSON parsing.
- [x] Keep quantity, horizon and cost assumptions outside model control.
- [x] Validate quote again at answer time, then use shared decision publication.
- [x] Require explicit unlimited subscription allowance and per-call adequacy;
  finite daily budgets need a shared durable meter, not a per-process reset.
- [x] Add real command/provider subprocess tests with deterministic GitHub stubs.
- [ ] Inspect all GitHub jobs and mutation diagnostics.

Initial code a43bd5a CI 38282204500 finished with 11 successful jobs, formatting
failure and a mutation survivor (shard 3 and aggregate failed). Tests passed
2816 Rust and 27 Python tests, with 32 targeted Python mutations caught.
All four Rust artifacts inspected: 22 caught, 12 compiler-unviable, one missed,
no timeouts. Unviable reasons were unused/dead code under denied warnings,
missing Default implementations (Request, Reply, Answer), and a trait-associated
Default call without a concrete provider implementation. These are not catches.

The survivor was alpaca_propose.rs:89:49, || changed to && in the experiment
guard. Later rejection hid an unnecessary model call. The actual-process test
now requires each invalid field to refuse before any stdin capture appears,
plus checks horizon overflow. GitHub's tests recipe reapplies that exact guard
mutation (now line 105 after formatting), requires the executed test to fail,
and restores source in finally. It also reapplies the omitted-system-framing
bug against a focused provider subprocess test. No workstation jobs ran.

## Handback

Official OpenAI [non-interactive mode](https://learn.chatgpt.com/docs/non-interactive-mode)
confirms exec stdin/final-stdout and saved CLI authentication. Reuse the existing
provider and isolated bridge; no credential handling or extra CLI flags needed.
No API-backed app or new paid API path. Model output remains inert research data.
Local timestamps do not attest the model's information set. Cohort scheduling,
failed-attempt retention and held-out evaluation remain to be integrated.
The autonomous-trading goal remains active; this is not a completion claim.

Inspection found Codex::ask sent Request::render without Request::system. The
adapter now prefixes the trusted framing on stdin before question and fenced
evidence; API-role semantics are not claimed for CLI stdin. The actual process
test inspects what the subprocess receives, including all six fence markers.
No bridge permissions or CLI authentication flow changed. Parent documentation
run 38281339062 passed all 14 jobs, inspected before this branch's push.

The decision keeps an optional model-observation cutoff, bounded by capture
availability and post-answer decision time. Old manual records may omit it.
Reusing a stale capture refuses at answer completion under the explicit quote
age bound; it never creates a historical pre-answer trade. A production producer
will need frequent fresh captures and complete failed-attempt records. The model
is prompted to use only supplied data; its information set is not OS-attested.
