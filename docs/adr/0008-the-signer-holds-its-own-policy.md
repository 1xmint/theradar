<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0008 — The signer holds its own policy, and clamps against it unconditionally

**Date:** 2026-08-31
**Status:** accepted
**Decides:** what the signer trusts about the authorisation it is handed, and
therefore what it protects against.

## Context

[LEARNINGS](../../LEARNINGS.md) 23 records the finding this ADR answers. The
signer does not verify that the `Authorization` it receives came from the kernel:
there is no MAC on it. At the original decision, its `nonce` — a content hash of
the proposal and the state it was judged against — was never checked. The dated
October update below adds Privy process reuse checks, not issuer authentication.

So its guarantee is *the transaction matches the authorisation the caller
supplied.* Against an executor **bug**, that is complete, and it is what the
check was built for. Against a **compromised caller**, it is not: such a caller
writes its own authorisation, with its own mint, its own ceiling and its own
expiry, and the signer checks the transaction faithfully against those.

That gap became load-bearing when
[ADR 0007](0007-the-privy-authorization-key-lives-in-the-signer-process.md) moved
the Privy authorization key into this process specifically to survive a
compromise of `radar-serve`.

### The obvious fix is worse than it looks

ADR 0007's amendment suggested the signer could hold the `Policy` and **run the
kernel itself**, taking a proposal rather than a finished authorisation.
`radar-risk` is already one of its dependencies, so it is cheap.

On inspection it is the wrong shape, and it is worth writing down why, because it
is the intuitive answer.

`evaluate(proposal, state, policy)` is a function of three things, and the signer
would hold only one of them. The caller would still supply the **portfolio
state** — how much is already deployed, what today's realised loss is, what is
held per creator. Every portfolio-scoped limit in the policy is evaluated against
that state, so a caller that understates it gets those limits to pass. The
kernel's answer is exactly as trustworthy as the state it was given.

The result would *look* far stronger — the signer runs the risk kernel! — while
protecting against very little more than today. That is the failure mode this
repository keeps recording, and building it would be a new instance of it.

## Decision

**The signer loads a `Policy` of its own and clamps every authorisation against
it, unconditionally.** No caller-supplied value can widen anything.

Unconditional is the operative word. A clamp does not depend on state the caller
supplies, so unlike a kernel re-run it cannot be defeated by lying about
anything. It answers a narrower question than the kernel does, and it answers it
without trusting the questioner.

Four checks, each on a value the caller currently controls outright:

1. **Autonomy.** If the signer's policy cannot self-authorise
   (`Observe`, `Alert`, `Approve`), nothing is signed. This is what makes
   `Policy::CLOSED` *closed at the signer* rather than closed only in the process
   that decides. Today `Policy::CLOSED` is enforced in one place, and that place
   is upstream of the key.
2. **Notional.** An authorisation may not exceed `policy.max_position`. Refused
   rather than silently clamped: a caller asking for more than the operator's
   policy allows is either a bug or an attack, and quietly serving it a smaller
   number hides both.
3. **Canary.** Under `Autonomy::Canary`, `policy.max_canary` is the ceiling
   instead. The dust round trip is the one thing that level exists to permit, and
   it must not inherit the larger bound.
4. **Lifetime.** An authorisation's window may not exceed
   `policy.max_input_staleness` slots from now. Expiry is the only thing making a
   grant temporary, and a caller currently chooses it.

The policy is **loaded once, at start**, from a file — the same rule the
allowlist already follows, for the same reason stated there: *a signer that
re-reads its rules per request is a signer whose rules can be changed by whoever
can write that file while it runs.*

Rule 8 applies: **no policy file means no signing.** Not a default policy, and
not the caller's judgement.

## What this does not fix, stated plainly

**A compromised caller can still repeat.** Each authorisation is bounded by
`max_position`, and nothing here bounds how many of them arrive. The
portfolio-scoped limits — `max_deployed`, `max_daily_loss`, `max_per_creator` —
remain unenforceable in this process, for the same reason the kernel re-run was
rejected: they are functions of state the signer does not hold.

The fix for that is for the signer to hold its **own** accounting: it sees every
authorisation it grants, so it can accumulate them and enforce a daily ceiling on
its own numbers rather than on the caller's. That is a real design and it is
deliberately not in this ADR, because it needs durable state that survives a
restart, and adding both at once produces a change too large to review carefully
in the one process whose virtue is that it is small enough to read completely.

**It is a precondition for customer capital, and it is written down as one here**
so that it is a scheduled piece of work rather than a paragraph someone remembers
later.

Until it exists, the bound on a compromised caller is: `max_position` per
transaction, times however many transactions it can get signed, with
**Privy's policy engine as the independent backstop** — which
[ADR 0005](0005-customers-keep-custody-and-grant-radar-a-bounded-signer.md)
precondition 1 still requires be verified by making it refuse.

## Consequences

- `radar-signer` gains `RADAR_SIGNER_POLICY`, and refuses to sign without it.
  Absent is a refusal, not a permissive default.
- The deployment gains a third file for the signer. Its contents are an operator
  decision about money, which is the point of it being a file rather than a
  constant.
- `radar-exec` and `radar-serve` are unaffected: the protocol does not change.
  The clamp is invisible to a caller that was already inside the policy, and a
  refusal to one that was not — which is the correct signal in both directions.
- A test suite that asserts the clamp must include the case where the caller's
  authorisation is *wider* than the policy, since that is the only case the
  change is about.

## Private Privy wallet binding — 2026-10-07

The private trader now has an explicit `RADAR_SIGNER_MODE=privy` startup mode.
It loads no local Solana private key and refuses local signing requests. A Privy
authorization key in either mode requires trusted startup configuration for the
app ID, wallet ID and Solana address. `privy::authorise` accepts this `WalletScope`
instead of a caller-selected address, and binds the signed request to HTTP POST,
the exact wallet RPC URL, matching app header, `signTransaction` and base64
encoding. An optional chain type must be Solana. It cannot authorize server-side
sign-and-send, arbitrary message signing or wallet administration.

This closes request destination/operation substitution, not the remaining issuer,
nonce, portfolio accounting or trusted-clock gaps above. The existing executor
composition test uses this bound scope. No production signer or delegation is
enabled by this source change; `Policy::SHIPPED` remains closed.

## Privy single-attempt guard — 2026-10-07

The Privy binary now requires `RADAR_SIGNER_NONCE_DIR`, an existing signer-owned
private directory on a persistent local filesystem. It exclusively creates a
SHA-256-named tombstone for the nonce, syncs that file and (on Unix) its directory,
then invokes the checked signing method. Concurrent processes cannot create the
same tombstone; existing markers refuse across process restarts and key rotation.
Empty nonces, missing state and persistence failures refuse. Nonce text is never
used as a path. The zero-byte marker is sufficient: existence means consumed,
regardless of its contents, and no signature is stored there.

The decision deliberately trades transparent retry for refusal: an interrupted,
rejected or ambiguously completed attempt stays consumed. Reconcile before
requesting a new authorization. Never clear state to retry. The systemd unit
permits writes only to this state directory; it does not automatically recreate
missing nonce state. Protect and preserve that directory, including backups;
deletion or rollback by an administrator can reset replay protection.

This is at-most-one signing attempt per caller-supplied nonce in the Privy
**process**, not an authenticated issuer or a network replay guarantee. It does
not prevent the executor from resending an already obtained signature to Privy,
forging fresh nonces or lying about time/state. `privy::authorise` remains a
checked-signing library method without this process state. The local lane is
unchanged. Authenticated issuance, trusted expiry and durable portfolio/submission
accounting remain incomplete. Live delegation and execution stay inactive.

## Privy issuer verification and independent expiry — 2026-10-07

The Privy process now requires a base58 Ed25519 public key from startup
configuration (`RADAR_SIGNER_ISSUER_PUBLIC_KEY`) and a positive maximum intent
lifetime (`RADAR_SIGNER_MAX_INTENT_LIFETIME_SECS`). Neither defaults. Requests
must carry a proof with Unix issue/expiry seconds and a base64 Ed25519 signature.
Older callers without that proof refuse. No issuer private key is loaded into
this signer, Serve or the executor.

The exact v1 transcript is `attestation::payload`: the domain
`radar/privy-intent/v1`, authorization, complete typed Privy request, wallet,
caller slot and lamport bound, and issue/expiry times. The existing canonical
JSON subset encodes integers exactly; external clients must preserve u64 values,
not round them through JavaScript numbers. Strict Ed25519 verification rejects
another issuer or changes to the transcript. Wallet scope, transaction decoding,
own policy and allowlist remain separate mandatory checks.

The process reads its own host clock, requires issue <= now < expiry and an
interval no larger than the configured maximum. It checks before nonce
reservation and again after persistence, before invoking checked key use.
Unauthenticated or already expired requests do not consume a nonce; once claimed,
an attempt stays consumed even if the second check or transaction check refuses.
Host time is a trust dependency: rollback can extend acceptance. This does not
authenticate the chain head or prevent submission of a signature already returned.

This changes the Privy binary's earlier caller-forgery guarantee, not the local
lane or the checked-signing library by itself. The signature proves provenance
from the configured issuer key. **At this increment there was no isolated issuer that evaluates
the risk kernel against trusted portfolio state and reserves capital atomically.**
Do not give that key to the caller or describe this verifier as proof that the
kernel authorized capital. No real issuer trust anchor, live expiry cap,
delegation or trading is configured by this increment. Durable reservations,
loss/submission accounting and independent Privy refusal tests remain required.

## Offline issuer over separately provisioned evidence — 2026-10-07

The separate `radar-issuer` binary in the signer package now has an actual
operator-facing stdin/stdout entry point. It holds an Ed25519 issuer key, no
Solana wallet key or Privy authorization key, and makes no network calls. It
emits the exact v1 proof the existing Privy signer verifies. The autonomous
executor is not wired to it yet.

Its configured files are authority: active wallet-bound policy with an expiry,
a current independently provisioned snapshot, a pre-existing intact journal,
and a private key file. On Unix configured files must be regular and mode 0600
or stricter; deployment still needs a distinct service identity, protected
directories and Windows ACLs where applicable. The caller supplies only a
proposal and unsigned transaction. Both must exactly match the proposal and
bytes in the provisioned evidence; caller-asserted exit capacity, costs or
creator identity cannot become trusted by passing through the pure kernel.

The snapshot includes wallet identity, counted native SOL, an upper SOL price
in micro-USD, reviewed fee upper bound, observed Unix time and kernel state
(including deployed exposure, daily loss and failures). These are trusted
operator-provisioned inputs, not independently verified live reads by this
binary. The issuer checks wallet/time/price, evaluates the actual risk kernel,
narrows its slot expiry, rounds the USD-to-lamport ceiling down with u128
arithmetic, re-decodes the transaction with existing signer guards, and reserves
the ceiling plus configured fee cushion. Fees must fit that cushion.

Only then does it persist SubmissionUnknown and sign/output the proof. An
outstanding claim blocks all further issuance, including after restart. This
conservative single-flight restriction avoids pretending unimplemented
settlement accounting can provide current exposure or realised loss. No timer,
pipe error or refused second request frees it. Claims correlate to the kernel
nonce and mint. Changed configuration refuses new requests until restart;
already emitted proofs remain bounded by their own expiry and signer policy.

This is an offline issuer prerequisite, not autonomous trading activation.
The site's draft limits are not its authority. A live adapter must independently
read the wallet and market, construct measured evidence, account for fills and
losses, and activate/revoke mandates outside Serve's write authority. Protected
stable history and trusted checkpoints are still required against replacement
and rollback; file mode checks alone do not prove ownership separation. No
real issuer key, expiry/price/fee policy or delegation is provisioned here.

The actual process regressions in
`crates/radar-signer/tests/issuer_process.rs` cover valid kernel-derived issuance,
proof verification, caller evidence substitution, persistent outstanding
claims, missing state, invalid configuration, snapshot failures, risk refusals,
insufficient cash, fee coverage and conversion/expiry bounds. Clock boundaries
are also checked with deterministic arguments in the binary's unit test.

## Direct wallet evidence is not yet issuer state — 2026-10-07

`radar wallet-read --wallet <address> --rpc <URL>` now calls the read-only RPC
client directly for native SOL and both token programs, with finalized
commitment, exact quantity strings and individual context slots. Failed reads
or missing context refuse the whole output. Read start/completion times are
host observations; matching slot numbers do not prove an atomic common-bank
snapshot. USD valuation and realised P&L remain null. Node-reported decimals
and RPC truth still depend on the selected provider.

This operator command is the first live wallet measurement piece, not an
activated mandate, complete kernel state or trusted market adapter. Its output
is not accepted as the issuer Snapshot. Protected endpoint/wallet provisioning,
valuation, measured capacity/fees, deployed exposure and loss reconciliation
remain required. It grants no authority and loads no key.

## One-context curve exit measurement — 2026-10-07

`radar curve-exit` now reads the mint, derived bonding curve and derived fee
schedule together under one finalized node-reported context. Owners and existing
layouts are mandatory; current authority/initialization/supply and unsupported
extensions cannot be silently defaulted away. Lower mint supply due to holder
burns is allowed. Pricing refuses active mint/freeze authority and gross exits
not covered by real SOL reserves.

The venue fee bound is the maximum total over every observed tier and flat row,
requiring tier coverage from zero. It does not claim the current market-cap tier
or a fee constant. Fee rounding favours cost conservatism. Output is a requested
hypothetical exit for that state, not a future fill, searched exit capacity or
complete trade economics. USD valuation, network fee, wallet ownership and exact
transaction simulation remain unestablished. This component cannot populate a
full issuer Snapshot or activate delegation on its own.

## Captured fee extension — 2026-10-07

The one-context reader now uses the complete `FeeSchedule` parser in
`crates/radar-pumpfun/src/fee_schedule.rs`. It preserves the standard prefix and
reads the captured stable-tier vector and exotic flat fees. Historical all-zero
reserved tails remain absent extensions, not an asserted zero exotic schedule.
Unknown nonzero suffixes and truncated extensions still refuse. Its upper bound
includes every standard/stable row and both flat schedules, with zero-threshold
coverage required for the standard and observed stable schedules. No choice of
token classification or current market-cap tier is inferred. Existing prefix-only
`FeeConfig::parse` callers retain their previous semantics; only this complete
reader claims the expanded bound. Read-only live verification emitted a quote;
it is still incomplete execution evidence and grants no signing authority.

## Component rounding in the cost bound — 2026-10-07

The complete schedule's `charge_upper` evaluates each observed row's component
ceilings separately and chooses the largest cost, rather than charging the row
with the largest total bps. [Pump's published buy formulas](https://github.com/pump-fun/pump-public-docs/blob/8cda1fa30ea658b20909d8aedf002047119388d2/idl/pump.json)
show separate protocol/creator rounding. This is not a claim that sell implements
that exact formula; the hypothetical sell bound covers this rounding as well.
LP/protocol/creator costs are each rounded up, summed in u128 and clamped to the
gross amount. The CLI refuses an exhausted exit. Rate and lamport cost bounds
are separate because different component splits can reverse their ordering.
Existing combined `Fees::charge` callers retain their historical semantics.
Neither fee bound supplies transaction simulation, network fees or future fills.

### Exact transaction reads

`radar transaction-read --transaction <binary-file> --min-slot <N> --rpc <URL>`
now obtains read-only simulation and network-fee evidence. Its caller is the
operator CLI, not Serve or the issuer. Input is bounded to a legacy packet of
1,232 bytes, with canonical one-signature framing and one zero placeholder.
Versioned, multi-signer and already signed envelopes refuse. Local inspection
establishes framing only; the RPC validates the message and the independent
signer still must decode and authorize its instructions.

`simulateTransaction` receives the exact base64 bytes, finalized commitment,
explicit minimum context slot, signature verification disabled and blockhash
replacement disabled. Explicit `err:null` is required; a missing field cannot
be success. A reported replacement blockhash refuses. `getFeeForMessage` then
receives the exact message suffix, with the same commitment and minimum slot.
Both response contexts must independently meet that slot; they are not an
atomic snapshot. Missing/null fees refuse, whereas measured zero remains zero.
Absent compute units remain unknown. Output retains both full base64 strings
for exact equality binding, individual slots and host start/completion times.

No provider errors or program logs are copied into operator output. No partial
JSON is emitted when either read fails. These measurements grant no authority,
do not verify signatures or guarantee later execution, and leave rent, other
instruction costs and USD valuation unknown. An expired original blockhash
refuses rather than simulating a different transaction. The issuer has not yet
been connected to these reads; its live snapshot and reconciliation gaps, and
Privy delegation/policy refusal verification, still keep live execution closed.

The RPC contracts and packet bound were checked against Solana's
[simulation](https://solana.com/docs/rpc/http/simulatetransaction),
[message fee](https://solana.com/docs/rpc/http/getfeeformessage) and
[transaction](https://solana.com/docs/core/transactions) documentation.

## What would reverse this

Nothing foreseeable reverses holding a policy locally. The specific ceilings are
expected to move, and moving them is an operator action against a file rather
than a code change, which is the shape this ADR is choosing.
