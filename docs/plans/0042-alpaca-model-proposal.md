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
