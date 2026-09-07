<!-- SPDX-License-Identifier: Apache-2.0 -->
# Security

## Reporting a vulnerability

Report privately through
[GitHub's advisory form](https://github.com/hey-vera/radar/security/advisories/new),
which is visible only to the maintainers until an advisory is published. Please
do not open a public issue for anything exploitable.

There is no bounty programme and no service-level commitment. What you will get
is an acknowledgement, a considered reply, and credit in the advisory unless you
would rather not have it.

## What is worth reporting

Radar is a research recorder today and a system that will hold a Solana signing
key tomorrow. That shapes what matters.

**Most valuable:**

- Anything that could make the signer authorise a transaction it did not fully
  read. Its guarantee is stated absolutely — *every account it authorises is one
  it read in the bytes it signed* — so a counterexample is the most serious class
  of bug in this repository. See
  [ADR 0003](docs/adr/0003-legacy-transactions-because-the-signer-must-be-able-to-read-them.md).
- Any path from a reasoning layer to the signer. Only the deterministic risk
  kernel may turn a proposal into an authorization
  ([AGENTS.md](AGENTS.md) rule 1).
- A decoder that can be made to panic, hang, or mis-parse attacker-supplied
  bytes. `radar-decode` and the signer's transaction decoder both read data an
  attacker chooses.
- Anything that would let untrusted content — token metadata, memos, social copy
  — reach a position where it is treated as an instruction (rule 4).
- Supply-chain findings: a dependency, a pinned action, or a build step that
  could put code into the released binary.

**Also worth reporting, lower severity:** a way to make the recorder skip chain
without saying so. A missing slot range is indistinguishable from a quiet market,
which is the failure this project is organised against.

**Not vulnerabilities:** losing money on a trade; a strategy that performs badly;
a threshold you disagree with. Those are research questions, and
[`docs/research/`](docs/research/) is where they are argued.

## Scope

This repository, its released binaries, and its GitHub Actions workflows.

The deployment described in [`deploy/README.md`](deploy/README.md) runs on a host
shared with unrelated services. Please do not test against `radar.heyvera.org`;
report what you have found and it will be reproduced locally.

## What is already true

Stated so you can skip the ground already covered:

- `unsafe_code = "forbid"` across the workspace.
- The shipped policy is `Policy::CLOSED`, which refuses every proposal. No
  capital is deployed and no key is installed.
- The signer is a separate process with no network, no listener, and no method
  that signs arbitrary bytes. It refuses address lookup tables so that every
  account it authorises is one it read.
- No crate that binds a socket depends on the signer crate. Enforced by
  `nothing_that_listens_on_a_network_depends_on_the_signer_crate`, after that
  boundary was found broken.
- `cargo deny` runs on every pull request for advisories, licences and sources.
- Every GitHub Action is pinned to a full commit SHA.

## What is not

Also stated plainly, because a security policy that lists only strengths is a
marketing document:

- **There is no threat model document yet.** It is planned and not written.
- **Fuzz testing is absent.** The decoders are tested on chosen inputs and a
  small deterministic byte sweep, which is not the same thing. Property testing
  is not absent — `radar-risk` carries `proptest` — but it covers the kernel and
  not the decoders, which are where a hostile byte string arrives.
- **The signer trusts the caller's price.** It bounds a swap in lamports now
  (research 0030, C1), and the ceiling it compares against is the
  authorisation's micro-USD notional read *as* lamports, because this process has
  no price feed by design. That fails closed and cannot size a real trade. The
  fix is a lamport-denominated `Policy`, which is a decision about what the
  operator's limit means and belongs in an ADR.
- **There is no rate limit in `radar-serve`.** The analyst's gate limits what the
  X account answers; nothing limits what the HTTP surface serves, and the edge
  cache in front of it is not applying (research 0030, H9).

Three claims that used to stand here were true when written and are not now, and
they are listed rather than quietly deleted:

- *"There is no spend meter in the running system."* There is.
  `radar_analyst::spend::Spend` meters every mention read, model call, reply and
  post; `radar-agent` carries its own ledger; both persist across a restart.
- *"Property and fuzz testing are absent."* Half true, and corrected above.
- *"The public server has no authentication."* `radar_serve::access` decides an
  audience per exact path, and the operator surface is behind Cloudflare Access.
  Everything on the public paths is still intended to be public.
