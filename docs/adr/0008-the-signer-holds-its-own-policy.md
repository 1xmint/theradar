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

The shared token reader requires each account's public address, actual owner
program matching the requested filter, parsed wallet owner, and a known reported
state (initialized, frozen or uninitialized). Malformed or duplicate account
identities refuse a program read. The CLI also refuses an identity repeated
across the two program reads, even at different slots; distinct accounts of the
same mint remain separate. Account address/program/state survive its version 1
output as additive fields. Frozen/uninitialized balances remain holdings rather
than disappearing. The jsonParsed listing alone does not verify mint restrictions,
account extensions, delegates or native wrapping. The operator command now adds
the raw verification below; spendability remains explicitly null because exit
capacity, transaction feasibility and complete inventory are not established.
The shape is checked
against the [Solana RPC reference](https://solana.com/docs/rpc/http/gettokenaccountsbyowner),
not a live capture of the configured wallet.

For nonempty token listings, wallet-read now uses one additional finalized
getMultipleAccounts batch for every distinct listed token account and mint,
bounded to [100 addresses](https://solana.com/docs/rpc/http/getmultipleaccounts).
Its context must not precede any of the three enumeration contexts. Existing
raw token/mint parsers validate owner programs, layouts and supported extensions.
Raw wallet/mint identity, u64 amount, state and mint decimals must agree with the
listing. Missing/changed data, unsupported extensions or noncanonical mint
initialization refuse the entire output. Same-mint amounts are checked for u64
overflow. Ordinary token totals cannot exceed observed mint supply; the captured
classic wrapped-SOL mint reports zero supply despite nonzero wrapped balances.
That exception requires its canonical mint/program, matching native flag,
nine decimals, zero supply and absent mint/freeze authority. Other wrapping
identities refuse; a native flag cannot exempt an arbitrary mint. Frozen/uninitialized
holdings remain evidence with state, not safe capital.

Version 1 adds raw_token_verification with its own slot, raw quantities/units,
mint supply, mint authority activity, freeze authority, delegation and native
wrapping reserve. Wrapping reserve is not additional native cash. Provider truth
remains a trust assumption; locally decoded bytes are not independent chain
authentication. Empty listings need no extra batch and explicitly retain
inventory_complete=false and unknown raw context. Matching slot numbers still
do not prove atomic enumeration/native/history coverage; the common reported
slot now includes the raw batch when present. Spendability, USD/P&L and portfolio
completeness remain unknown. Wallet-read has a four-call, twenty-second budget.

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
refuses rather than simulating a different transaction. Live snapshot and
reconciliation gaps, and Privy delegation/policy refusal verification, still
keep live execution closed.

The RPC contracts and packet bound were checked against Solana's
[simulation](https://solana.com/docs/rpc/http/simulatetransaction),
[message fee](https://solana.com/docs/rpc/http/getfeeformessage) and
[transaction](https://solana.com/docs/core/transactions) documentation.

### Binding operator read evidence to offline issuance

The private snapshot now requires `transaction_evidence`, containing the JSON
emitted by `radar transaction-read`. The offline issuer is its actual consumer;
candidate stdin cannot supply it. Older snapshots without this field refuse.
No RPC, network-bearing dependency or real credential was added to the issuer.
This remains operator-provisioned evidence: file protection establishes trust,
not cryptographic proof of RPC origin. A live adapter must still own collection.

Before capital reservation, the issuer requires version 1, read-only/finalized
metadata, explicit successful simulation with no blockhash replacement, and
equality with both the exact transaction bytes and the decoded signable message.
The Privy request is encoded from those Checked bytes too. The shared decoder's
legacy leniency is unchanged; an alternate input spelling cannot be forwarded
as a different signing payload, and the proof binds the canonical request.
Signature verification remains explicitly disabled in this unsigned read.
The requested minimum slot cannot precede the proposal's oldest input; both
reported contexts must meet that minimum and cannot exceed the snapshot's
decision slot. Separate contexts remain separate. The measured network fee must
fit the snapshot's reviewed full fee ceiling, which already must fit the mandate
reserve. An unknown fee refuses; measured zero is distinct from unknown.

Host read start/completion must be present, ordered and no later than issuance;
the start must be fresh under the mandate's existing snapshot age bound. Proof
expiry is additionally capped by start plus that bound, with checked arithmetic.
A recent completion cannot refresh an old read. All failures happen before
reservation/proof output. Optional units and unknown rent/USD fields grant no
authority; this does not establish complete costs, portfolio valuation, losses,
settlement or Privy delegation. Live execution remains closed.

`radar evidence-read --wallet <address> --transaction <binary-file> --min-slot <N>
--rpc <URL>` collects both read packets with one explicit endpoint and a shared
six-call, twenty-second budget (the raw token batch is omitted for empty listings).
It reuses the wallet and preflight readers,
preserves each read's slots and host window, and emits `wallet_evidence` plus
`transaction_evidence` only after every call succeeds. A failure prints neither
packet. Its operator caller can use the fields in the protected issuer snapshot;
the output itself is not a complete snapshot and grants no authority. It does
not fill missing USD bounds, kernel portfolio state or other instruction costs.

The private snapshot also requires `wallet_evidence`, containing the JSON from
`radar wallet-read`. Before reserving capital the issuer binds version 1,
read-only/finalized metadata, configured wallet identity, native decimals of
nine, and the exact integer SOL balance to the reviewed snapshot. Native, SPL
and Token-2022 contexts must each lie between the proposal's oldest input slot
and the decision slot; both token account arrays must be present. Independent
contexts remain independent, even when their slot numbers agree.

The same ordered, non-future read-window check applies to both evidence sources.
Proof lifetime follows the oldest read start as well as the existing snapshot,
mandate and intent deadlines. Missing wallet evidence or a balance mismatch
refuses before reservation, and candidate stdin cannot replace the evidence.
The issuer does not infer USD exposure or realised loss from token arrays,
nor authenticate RPC origin from this JSON. Those remain protected operator
facts pending live collection, valuation and settlement reconciliation. The
existing Jupiter SOL-to-USDC quote is a point quote, not the conservative upper
USD price required for a live spending ceiling. This increment does not activate
delegation or change the site's draft limits.

### Finalized settlement reads

`radar settlement-read --wallet <address> --transaction <signed-binary-file>
--min-slot <N> --rpc <URL>` now reads historical finalized evidence for exactly
one signed legacy transaction. The operator CLI is the caller. The read holds
no key, sends nothing and cannot transition a journal operation.

Input is bounded to 1,232 bytes, one canonical nonzero signature and a legacy
header with the configured wallet as writable fee payer. Account keys are read
locally; the RPC remains responsible for full message validity, signature and
execution assertions. The request uses the first signature, base64 encoding,
finalized commitment and maximum supported transaction version zero. The
response must explicitly say legacy, bind the exact canonical transaction
bytes and report a transaction slot at or above the supplied minimum. This is
RPC trust, not a cryptographically authenticated inclusion proof.

Explicit execution metadata is required. A landed failure remains a failure
with a measured fee; a missing/null transaction or absent execution field is
unknown and emits no packet. Native pre/post balance arrays must match the
local account table. Token arrays must be present; their entries require unique
in-range account indices, valid mint/owner identities, SPL or Token-2022 program,
integer-string amounts and byte-sized decimals. Quantities and fees are emitted
as strings. UI floats, provider errors and program logs are not copied.

The packet preserves historical balances, transaction slot, optional chain
block time and the host read window. It does not make those balances current,
infer an economic trade from native movement or turn a missing token entry into
an assumed zero. USD valuation and realised PnL remain unknown; the packet does
not reconcile an operation. A future protected reconciler must bind evidence
to the authorized operation, establish effects and fees, and update exposure
and loss accounting before another issuance. The issuer's outstanding-operation
guard and live delegation gate remain closed.

The method/encoding/metadata contract was checked against Solana's
[getTransaction reference](https://solana.com/docs/rpc/http/gettransaction).
Controlled process tests cover successful and fee-paying failed reads. A live
public mainnet transaction at slot 454421787 used version zero and was refused
by the legacy input guard; this is a verified refusal, not a successful live
settlement read. The reference's example signature was unavailable on the public
mainnet endpoint. No user wallet transaction was signed or broadcast.

## Protected operation-to-transaction binding

The offline issuer now puts an `ExecutionBinding` in the proposed operation's
correlation before reservation/proof output. It contains the configured wallet
and the canonical transaction copied from the checked Privy request. The
operation identity therefore covers those bytes in the existing journal chain.
Absent metadata on older history remains absent; it is never inferred from a
new candidate or current snapshot.

With `RADAR_ISSUER_CONFIG` pointing to the same private operator configuration,
`radar-issuer --bind-signed <operation-id> <private-signed-binary-file>` acquires
the existing journal ownership lock. It accepts only an outstanding operation
with that configured wallet, exact authorized legacy message and one signature,
then verifies the wallet's Ed25519 signature locally. It durably appends the
canonical signed bytes before reporting success. Identical repeats are
idempotent; conflicting bytes refuse. Replay refuses changed wallet, unsigned
message, signed binding or capital intent. A write failure leaves the in-memory
binding unchanged. These are protected-file assertions; the journal does not
authenticate its host or verify cryptography for generic callers.

Recording a signed artifact neither broadcasts it nor settles the operation.
The claim remains SubmissionUnknown, including after restart. A future executor
must call this step before broadcast; no executor does so yet. The protected
reconciler must match finalized evidence to this recorded artifact and establish
fees/economic effects and USD accounting before releasing capital. Limits remain
drafts and live delegation remains closed. Tests use fixture keys only.

## Protected settlement review

`radar-issuer --review-settlement <operation-id> <private-settlement-json-file>`
uses the configured protected journal and a copied `radar settlement-read`
packet. It requires an outstanding native-SOL operation with a recorded signed
artifact, re-verifies that artifact's wallet signature and exact authorized
message, then checks the packet's wallet, signature, canonical transaction,
finalized commitment, slot floor and ordered fresh host read window. Native
arrays must match the signed account table; token metadata must have complete
identities/quantities and distinct in-range account indices. The measured fee
and wallet's net debit must fit the native reservation, including exact equality.

The report preserves signed integer native changes, fee-paying failed execution
and separate pre/post token metadata. It does not infer trade notional, USD
exposure, realised PnL, a zero pre-balance for a new token account or profit from
a native credit. Token program identities are retained as metadata; this step
does not classify token effects economically. Protected files are the trust
boundary, not authenticated RPC or host provenance.

Review leaves the journal and reservation unchanged, including across restart.
This is an operator evidence check before reconciliation, not a reconciler that
closes an operation. A completed transaction can spend less than its reservation;
the existing PartiallyFilled settlement leaves the remainder open.

The report now includes a typed `native_settlement_candidate` only when the
wallet has a measured nonnegative native debit covering the known network fee.
It is `Settlement::Completed` with that debit in lamports; a native credit or a
debit smaller than the fee leaves the candidate absent. Zero debit with a known
zero fee is representable. This candidate neither mutates the journal nor
establishes trade notional, gross spending, USD costs or realised loss.

`Portfolio::settle` accepts Completed as a terminal measured spend within the
outstanding claim's unit/ceiling. It debits only that spend, releases unused
capital and preserves other claims. PartiallyFilled retains its existing
meaning. The journal's confirm/reconcile callers preflight settlement on a
portfolio copy before writing the terminal outcome, and apply that copy only
after the write succeeds. A reserved operation must rehold its claim after
replay before closing. Replay checks Completed against the unchanged recorded
intent/reservation and does not debit balances read fresh after restart.

Generic journal callers remain responsible for establishing the outcome; these
methods do not consume or authenticate finalized chain evidence. The protected
issuer still performs review only. Binding reconciliation evidence durably and
independent USD exposure/loss updates are needed before safely enabling repeated
issuance. Native debit alone cannot reset those limits. Live delegation stays
closed.

## Durable normalized settlement facts

`radar-issuer --record-settlement <operation-id> <private-settlement-json-file>`
runs the same protected signature/message/finality/effect review, then records
only its normalized output with the canonical signed artifact under the existing
journal ownership lock. It reads the input once; arbitrary input fields and
provider bodies are not copied into history. The report now also preserves its
minimum slot and ordered host read window. No raw RPC response is retained.

The journal checks SubmissionUnknown and exact association with its previously
recorded signed artifact. The append precedes the in-memory update and success
output. Identical repeats are idempotent; changed facts refuse, including changed
read windows. Replay rejects unbound, conflicting, wrongly staged or changed
operation metadata, while retained facts survive later terminal records.
Absent optional settlement fields do not change hashes of older events. Generic
journal callers supply verified normalized facts; the journal does not perform
cryptographic, chain or economic verification for them.

Recording evidence leaves the claim outstanding and still blocks issuance.
Historical read times do not authorize a future decision, and a saved review
does not establish USD exposure/loss. The protected issuer still has no command
that closes the claim. Economic reconciliation and independently fresh live
snapshots remain necessary before repeated autonomous execution.

## Protected accounting coverage checkpoint

The private issuer snapshot requires `accounting_checkpoint`, a string equal to
the owned operation journal's last complete event digest. Only an empty journal
uses an empty string. Missing, null or malformed fields refuse. Older snapshots
must be reprovisioned; candidate stdin cannot supply this field. The issuer
compares it before evaluating or recording a new operation, and a mismatch
refuses without changing history or releasing a proof.

The checkpoint includes every journal event, including proposals whose
reservation was refused, pre-submission failures, normalized settlement and valuation records,
terminal settlements and non-operation events. The journal advances its head
only after a successful durable append. Replay restores it from the complete
history. Idempotent repeats do not advance it. Outstanding claims still refuse
before this check; an accounting marker never clears an unknown submission.

This is the protected operator's coverage assertion, not verification that its
USD exposure, creator allocations or daily-loss figures are correct. An operator
copying a current digest onto incorrect figures remains inside the existing
trusted provisioning boundary. The digest does not authenticate chain origin,
establish execution day, prevent operator rollback of the journal or verify
fresh economic reads. The risk kernel still checks the supplied portfolio state.
Economic reconciliation and independently constructed live snapshots remain
required; live delegation stays closed.

## Durable reviewed trade attribution and execution time

New issuer proposals retain the typed, independently reviewed `Proposal` as
normalized JSON inside ExecutionBinding, together with the canonical checked
transaction. It includes creator, action, market, quote, sizing and risk-input
fields. Serialization of the typed proposal excludes arbitrary input fields.
The proposal is recorded before reservation/proof output; signing metadata
updates and terminal replay preserve it. Replay refuses replacing, dropping or
inventing this field in a later execution binding. Generic journal callers
establish the proposal's correctness; it is inert data, not signing authority.
Older bindings deserialize with no reviewed proposal, and an absent optional
field preserves their event hashes. No attribution is inferred or backfilled.

Protected settlement review now retains `block_time_unix_secs` as a canonical
unsigned decimal string when present. Missing or null time remains null, never
the host read time. Malformed, negative or future-to-read-completion timestamps
refuse. Measured zero and equality with read completion are accepted. The
existing record mode stores this normalized field durably; changing it in a
repeat conflicts with the earlier record. Older recorded reviews remain as they
were; a repeat with a different normalized shape also conflicts rather than
rewriting old evidence. This is protected operator evidence, not authenticated chain time or proof
of an execution day independent of that trust boundary.

Attribution and time are prerequisites for later economic reconciliation. The
reviewed notional is an authorization input, not measured fill value. No prices,
cost basis, USD exposure or realised loss are inferred here, and no claim-closing
command or live delegation is enabled.

## Protected historical native valuation review

Settlement review also measures `wallet_token_disposal` for successful reviewed
SOL Reduce/Exit operations. It shares acquisition's paired wallet/mint account
aggregation: both sides must exist, retain owner/mint/program/decimals identity
and consistent valid units, and checked totals must fit u64. Internal wallet
transfers cancel. Only a strictly positive pre-minus-post quantity is measured;
failed executions, buys, missing context, changed metadata or incomplete pairs
leave disposal unknown. The field is omitted when unknown, preserving existing
buy/failed review shapes. Recording retains the measured quantity immutably and
repeats survive replay without releasing or closing claims.

This is a net token decrease, not authenticated sale fill or proceeds. Closed
accounts with absent post metadata remain unknown. No gross fill, transfer
attribution, proceeds, cost-basis allocation or realised PnL is inferred.
Acquisition-history/inventory coverage still refuses disposals lacking complete
economic classification; this reader increment does not enable exit issuance,
economic reconciliation, live signing or broadcast.

`radar-issuer --review-valuation <operation-id> <private-price-file>` prices
retained wallet net debit and network fee without updating the journal or
portfolio. It requires an outstanding SOL operation, its retained normalized
settlement and exact signed artifact, and re-verifies the wallet signature.
The private price JSON accepts `version: 1`, `asset: "sol"`, decimal strings
`micro_usd_per_sol`, `as_of_slot`, `as_of_unix_secs`, and an optional
`acquisition_costs` object described below. Missing required fields,
unknown fields, a zero price or malformed/overflowing integers refuse.

The price file can instead include `sale_proceeds`, handled by
`crates/radar-signer/src/bin/radar-issuer/sale_proceeds.rs` through the same
protected review and durable record modes. Its required fields are version 1,
operation, exact signed_transaction, wallet, mint, token_program, decimals,
net_disposed_raw, gross_proceeds_lamports, tip_lamports, rent_paid_lamports,
rent_refund_lamports and other_cash_flows_absent. The latter must be true; all
amounts are unsigned decimal strings. Acquisition and sale breakdowns cannot
coexist. The same historical price identity, watermarks and staleness checks
apply before either classification. Sale context must be a reviewed successful
SOL Reduce/Exit and match the retained positive net disposal and exact artifact.

Checked gross plus rent refund, minus network fee, tip and rent paid, must equal
the exact signed wallet change. Gross proceeds must be positive; net credits,
zero changes and fee-dominated debits can all balance. Gross proceeds plus refund
and fee plus tip plus rent must individually fit u64. Retained network fee and
any net wallet debit must still fit the operation's recorded reservation.
Credits round USD down;
costs round up without floats. Net trade proceeds deduct network fee and tip,
can be negative, and exclude rent paid/refunded, which remain separate.
This is a protected operator cash-flow assertion, not independent venue fill
provenance. Changed classifications/economics refuse immutable repeats.

Sale review does not assign disposed-token cost basis, calculate realised PnL,
update portfolio risk, release claims or establish complete wallet cash-flow
coverage. Acquisition/inventory history still refuses incomplete sale economics.
Rent refunds are not profits. Missing sale evidence never becomes zero proceeds
or a debit valuation for a net credit. No exit issuance or live trade is enabled.

Both price watermarks must precede or equal reported execution context. Age
must fit the configured slot and seconds budgets; equality is accepted. Missing
execution time refuses rather than using host read time. The output preserves
the older price watermark. Integer arithmetic rounds debit and fee valuations
up to micro-USD and refuses overflow. Native credits, out-of-range effects,
debits above the reservation and fees above the debit refuse.

This is protected operator historical pricing, not an authenticated live oracle
or a conservative upper price for future sizing. Wallet net debit can contain
rent, tips and refunds; it is not gross trade value. Without the separate complete
cost breakdown, trade notional and cost basis remain null; realised PnL remains
null in all cases. No claim is released, no operation is reconciled,
and no USD exposure/loss state, signing authority or live delegation changes.

## Retained measured net token acquisition

Protected settlement review and recording now include optional
`wallet_token_acquisition`. It is derived only for a successful native-SOL buy
with a retained typed reviewed proposal. Both pre and post metadata must exist
for every reported account belonging to that wallet and mint. Account index,
owner, mint, token program and usable decimals must match on both sides; all
included accounts must share program and decimals. Integer totals are checked
for overflow. A positive aggregate post-minus-pre quantity is reported as
decimal strings alongside mint, owner, program, decimals and both totals.

Internal transfers between included accounts cancel. Empty, unpaired or
inconsistent metadata, failed execution, absent/invalid buy context, unusable
decimals, overflow and zero/negative aggregate changes produce null. A newly
created account with absent pre metadata is unknown, not an inferred zero.
Other wallets' accounts are excluded. The result is a measured net quantity
under protected operator provenance, not authenticated gross venue fill,
transfer attribution or interpreted Token-2022 extension behavior. Acquisition
cost, USD value and realised PnL remain unknown; no claim closes or authority
changes. The existing record command persists the normalized field and replay
retains it. Older reviews are not backfilled; repeats with a changed normalized
shape conflict rather than rewriting history.

## Optional protected acquisition cost breakdown

The existing `--review-valuation` price file may include `acquisition_costs`.
Absent or null means cash valuation only, with unknown trade notional/basis.
A supplied object must be complete and have no unknown fields; invalid costs
refuse the entire review. It contains `version: 1`, `operation`, exact canonical
`signed_transaction`, `wallet`, `mint`, `token_program`, numeric `decimals`,
decimal strings `net_acquired_raw`, `swap_lamports`, `rent_lamports`,
`tip_lamports`, and `other_cash_flows_absent: true`.

Costs require a successful native-SOL buy with retained reviewed context and
known token acquisition. Operation, signed artifact, wallet, mint, token program,
usable decimals and positive measured quantity must match. Swap spend must be
positive; explicit measured zero rent/tips are valid. Checked integer addition
of swap, the retained network fee, tip and rent must equal the retained wallet
debit exactly. Neither an under-accounted debit nor a claimed unrelated credit,
refund or absent component is accepted. Price freshness, watermark, signature,
reservation and upward-rounded USD conversion checks still apply.

The bookkeeping convention capitalizes swap consideration plus network fee
and tip into `position_cost_basis_micro_usd`. Rent remains separately priced in
`acquisition_costs.rent_micro_usd`; this does not classify its recoverability or
recognize a realised loss. `trade_notional_micro_usd` prices only the swap.
Network fees are already included in basis and must not later be double-expensed.
Reported known costs carry `authority: "protected_operator_breakdown"`.

This proves binding and arithmetic consistency of private operator-reviewed
components, not their independent origin or the truth of their classification.
A balanced false split remains inside the existing operator trust boundary.
It does not establish gross venue fill attribution, tax basis or a live sizing
oracle. Review remains read-only; the separate record mode below retains its
normalized result. Neither mode changes exposure or daily-loss state. PnL stays
null and the claim remains outstanding. Independent live collection, economic
reconciliation and delegation activation remain required.

## Durable reviewed acquisition valuation

`radar-issuer --record-valuation <operation-id> <private-price-file>` performs
one protected input read through the same valuation review, including wallet
signature, exact authorized message, historical price and classified cost checks.
Complete acquisition costs or the failed-execution fee review below are required;
unclassified cash-only reviews cannot be recorded. It retains a ValuationRecord
containing the exact previously retained SettlementRecord and normalized review;
raw input declarations, arbitrary fields and provider bodies are not stored.
The same journal ownership lock covers review and append.

The journal requires SubmissionUnknown, an exact preexisting settlement record
and unchanged operation metadata. A proposal cannot carry valuation, and a replay
event cannot establish its first settlement and valuation together. Append comes
before the in-memory update. Identical repeats do not append or advance the
accounting checkpoint; any changed normalized cost or price refuses, even when
its rounded USD basis is unchanged. Replay rejects unbound, conflicting or
misstaged records. Retained valuation survives later terminal records; a new
record after completion refuses. Absent optional valuation fields preserve older
event hashes; older history is not backfilled.

Generic journal callers establish economic validity; this storage layer checks
association, stage and immutability, not signatures or economic classification.
The actual protected issuer repeats those signature and cost checks before
recording. Retention does not authenticate operator evidence, update inventory,
cost basis, USD exposure or loss, or release capital. The claim remains
outstanding and blocks another issuance. Economic reconciliation must consume
these retained records idempotently before repeated autonomous execution.

## Protected failed-execution fee costs

The existing valuation review can classify a retained failed execution as a
fee-only cost. The measured wallet debit must equal the retained network fee.
Native effects must be nonempty, name distinct accounts with the configured
wallet first, and show only that fee deduction; all other native balances must
be unchanged. Recorded pre/post amounts and signed deltas must agree. Paired
token metadata must contain the same indices, mint, owner, program, decimals and
integer quantities regardless of enumeration order. Missing pairs or changed
metadata refuse classification. Empty paired lists establish no token changes
in this packet, not complete wallet inventory. Acquisition metadata must remain
null. Measured zero fees remain zero, never inferred from absent fields.

This uses the same verified signed binding and historical SOL price bounds and
upward integer rounding as acquisition valuation. It retains
failed_execution_costs with the measured native fee and its historical USD cost.
No price-file declaration can assert that a failure was fee-only. Successful
reviews keep their previous serialization shape, so earlier retained acquisition
reviews can still be compared exactly. The fee-only classification is consistent
with [Solana's fee rules](https://solana.com/docs/core/fees/fee-structure), which
charge a landed failure; operator evidence remains the trust boundary.

The existing record command stores this classified review through the same
immutable ValuationRecord association, append-before-memory and replay rules.
Repeated records are no-ops; changed price/cost records refuse even if rounding
produces equal USD amounts. This is a historical fee cost, not total realised
PnL, daily loss or economic reconciliation. No operation closes, reservation
releases or portfolio changes. The history reader below revalidates both buys
and submitted failed-fee costs rather than dropping either category. Idempotent
economic reconciliation remains to be implemented. Live delegation
and autonomous execution remain closed.

## Replay-derived acquisition history review

`radar-issuer --review-acquisitions` reads every retained operation under the
journal ownership lock, including completed entries. It reconstructs one
historical acquisition lot, sale-proceeds record or failed-fee record per
submitted operation,
re-verifies its configured wallet signature/exact message, checks the retained
facts name that signature and operation,
and reruns the existing complete historical
valuation from normalized retained inputs. The entire normalized result must
match, including every price, cost, watermark and operation field. The journal
already guarantees exact settlement association; the report adds no duplicate
storage or association rule.

Proposed, Reserved and pre-submission Failed operations are listed separately,
with no inferred acquisition or released claim. Every other operation needs
complete successful native-SOL buy costs, sale proceeds or classified
failed-execution fees.
SubmissionUnknown is supported for all categories. Sales refuse every terminal
state because Completed native spend cannot reconcile credits or disposed basis.
For buys and failed fees, Confirmed/Reconciled are supported only with Completed
native spend equal to
the retained wallet debit. Missing costs, unsupported terminal outcomes or
inconsistent retained reviews refuse the whole report. The same signed artifact
under different operation identities also refuses rather than double-counting.

Lots preserve reviewed creator, mint/program/decimals, measured acquired units,
cost basis, separately priced rent and execution/price watermarks. Aggregation
by mint and creator uses checked integer totals and the oldest price slot.
Failed fees are a separate array with operation, exact native and historical USD
fee cost, execution time/slot and price watermarks. Checked native/USD totals
cover only recorded failed fees; empty history has zero recorded fees, not zero
wallet loss. Failed fees never become token lots, creator basis or rent. Artifact
uniqueness applies across all categories; terminal debit consistency applies to
buys and failed fees. Entire
normalized valuation is recomputed before classification; missing or changed
fee classification refuses even if a public total would be unchanged.
A mint changing program or decimals refuses, including across creators. The
report names the owned journal's current accounting checkpoint and is stable
across replay/repeat; no history or portfolio state is written.

Sales retain the complete revalidated sale-proceeds breakdown, reviewed creator,
operation and execution/price watermarks in a separate `sales` array. Historical
price reconstruction includes the exact retained disposal and all cash-flow
components; changed amounts or classification refuse. Sales do not become buy
lots or alter acquisition totals. The optional FIFO report below allocates
recorded buy basis; complete wallet realised PnL remains unknown.

This is protected operator acquisition history, not verified current wallet
inventory. An empty report does not establish a flat wallet. Opening inventory,
external transfers, disposals, complete failed-execution coverage, current valuations and
complete daily-loss accounting remain unresolved. Output explicitly reports
wallet inventory incomplete and exposure/loss unknown. Existing claims remain
outstanding; no operation is closed or signing authority activated. Historical
basis is bookkeeping acquisition cost, not current liquidation value. Changes
to configured valuation age bounds can refuse older records; history is never
rewritten to accommodate them.

## Recorded FIFO disposal accounting

The existing protected history reader calls the FIFO allocator after exact
valuation replay, before emitting its report. `recorded_disposal_accounting`
is null without a retained opening snapshot or when any sold mint had positive
opening holdings whose purchase costs are unknown. Opening wallet, read bounds,
raw context and unique mint checks are shared with inventory comparison. Program
and decimals must agree with zero opening holdings as well as across trade events.
This is operator-provisioned bookkeeping, with coverage `recorded_trades_only`;
a zero or absent enumerated holding does not establish complete wallet coverage.

Sort recorded buys and sales by execution slot. Every trade must follow the
highest opening read context strictly. Same-mint trades in one slot refuse:
there is no retained transaction index to establish their order. A sale consumes
only preceding same-mint buy lots, oldest first; excess sales refuse instead of
borrowing future buys. Each partial allocation charges basis times disposed units
with upward micro-USD rounding using a wide integer product. Subtract the exact
allocated basis and units from the lot; later sales use that remainder, conserving
all original basis even when earlier partial allocations rounded upward. Allocation
sums and signed net proceeds minus basis refuse overflow. Preserve acquisition
operation and creator attribution for each consumed lot.

Report each sale's allocated basis and signed `recorded_trade_pnl_micro_usd`,
plus original lot metadata and exact remaining units/basis. Net sale proceeds
already deduct fee/tip; rent remains separate and failed fees are not added to
this per-trade PnL. There is no aggregate wallet PnL, daily loss, current exposure,
portfolio update or economic completion. Unknown external transfers and cash-flow
coverage still prevent applying these values to risk. Reading/replay writes
nothing, closes no operation and releases no claim. This FIFO method is a risk
bookkeeping convention, not a tax accounting claim.

## Protected current inventory comparison

`radar-issuer --review-inventory` reads the configured private snapshot once
under the owned journal lock and reuses `--review-acquisitions` validation over
all retained lots. Nonempty sale history requires the known recorded FIFO
report above; otherwise inventory comparison refuses. Use its remaining lot
quantities plus opening holdings for comparison, while preserving original
acquired and recorded disposed quantities in separate fields.
The snapshot must name the configured wallet and exact
current accounting checkpoint and be current by the host clock. Existing
wallet evidence checks bind its native balance, finalized read identity,
context bounds and current read window. Stdin cannot replace these inputs.

Require the wallet-read raw verification section and compare its account set
with both listings by address, mint, program, decimals, state and exact quantity.
Raw account owners must match the wallet. Duplicate identities, unknown states,
changed metadata or unavailable arrays refuse. A nonempty raw context must not
precede native balance or either listing and cannot exceed the snapshot's
reviewed slot. Empty listings require an empty raw set and unknown raw context;
they do not establish a flat wallet. Both token listing contexts must be at
least as recent as every retained acquisition execution slot.
Both token listing contexts and the native read must also be at least as recent
as every reviewed failed-fee execution and recorded sale. Preserve its history
and separate fee
totals in the report without adding fees to token quantities or inferring native
cash reconciliation. Ordering failed fees against an opening baseline, complete
cash-flow coverage and idempotent fee application remain for economic reconciliation.

Sum observed holdings and remaining recorded quantities by mint using checked u64
arithmetic, including frozen/uninitialized quantities and separate accounts.
Mint program/units must agree within and across the two totals. Report the
union of observed and acquired mints with exact acquired/observed units,
unexplained excess and unaccounted reduction. Missing observed/acquired units
are zero only within this comparison, after the corresponding inputs validated;
neither implies complete wallet coverage. Preserve native balance and separate
context/read times, the checked historical lots and their basis/creator groups.
Do not forward unrelated wallet packet fields.

The private file is still the trust boundary. This checks agreement of
operator-provisioned normalized evidence; it does not rerun the raw binary
decoder, authenticate chain truth or construct an independent live snapshot.
Per-account restrictions remain in the original wallet-read evidence and no
spendability is inferred from totals. Wrapped SOL remains a token quantity,
separate from native cash. Exact unit equality cannot account for offsetting
transfers or disposals, opening holdings, failed fees or current valuations.

This command reports differences, not economic reconciliation. Opening inventory
remains unknown unless the genesis record below exists; its cost basis,
complete inventory, current exposure and daily loss remain
unknown even when every quantity matches. It writes no history or portfolio,
closes no operation and releases no reservation. The signing path does not
consume its report or gain authority. Live activation remains closed until the
remaining economic coverage and protected live adapter are implemented.

## Recorded native cash comparison

The acquisition-history caller also derives `recorded_native_cash_flows` from
exact retained settlements after replaying their signed binding and normalized
valuation. Each submitted buy, sale or failed-fee operation contributes once;
the existing cross-category signed-artifact deduplication still applies. Require
one configured-wallet native effect and exact pre/post/delta agreement with the
retained wallet delta. This adds derived output, not a new durable record shape.

The inventory caller exposes `recorded_native_cash_comparison`. Without an
opening inventory it is null. With an opening record, share the existing wallet
and opening-read validation, sort recorded native effects by execution slot,
require every execution strictly after the highest opening context and at or
before the current native read, and refuse same-slot effects because transaction
indices are unavailable. Include failed fees in these bounds. Project cash from
opening lamports plus exact native deltas, checking the u64 balance range after
every operation; a later offset cannot rescue an impossible intermediate balance.

Compare each recorded pre-balance against the projected pre-balance and retain
signed unexplained differences, then compare the current native observation
against the final projection. Never reset the projection to an unexplained
pre-balance. Report final `balance_matches` separately from
`transaction_anchors_match`: offsetting missing flows can leave a final match
while transaction anchors differ. These are discrepancy observations, not
proven transfer classifications. Without supplied native transfers, coverage
remains `recorded_operations_only`;
even both matches do not prove external cash-flow completeness, independently
sourced evidence, realised wallet PnL, current exposure or daily loss. Complete
transfer enumeration and idempotent application remain absent. Review writes
nothing, releases no claims and enables no execution.

## Supplied external native transfers

The protected wallet snapshot may additionally contain `native_transfers`, with
version 1, `read_only` authority, finalized commitment, configured wallet,
current host read start/completion times and a `transactions` array. The existing
inventory reader validates this optional packet using the same read-window
checks and exposes only `reviewed_external_native_transfers`. Missing packet
means no supplied evidence, never proof that no external activity occurred.
This is operator-provisioned snapshot evidence, not an independent RPC adapter
or a durable transfer journal. Unreviewed provider fields are not copied.

Each transaction supplies exact base64 signed bytes, string slot/fee, explicit
succeeded/failed outcome, string native pre/post balance arrays and empty pre/
post token balance arrays. Support only legacy, one writable signer/fee payer,
one readonly unsigned System Program account last, unique static accounts and
plain System Program Transfer instructions from that signer to writable static
accounts. The configured wallet may be payer or recipient. Reject unsupported
programs, token effects, envelopes, missing evidence or extra instruction bytes;
no partial interpretation. Reuse the local transaction decoder and strictly
verify the sender's Ed25519 signature over the exact message. Check every native
account against signed transfers plus the supplied network fee. Successful
transfers must also fund each intermediate payer debit after fees and preceding
transfers, including self-transfers. Failed execution changes only the fee.
Protocol references: the [Foundation transaction guide](https://github.com/solana-foundation/developer-content/blob/main/docs/core/transactions.md)
and [System transfer constructor](https://docs.rs/solana-system-interface/latest/solana_system_interface/instruction/fn.transfer.html).

Normalize signature, slot, wallet native pre/post/delta, transfer delta excluding
its fee, wallet-paid fee, outcome and local signature verification. Deduplicate
signatures both inside the packet and against retained Radar operations; require
execution at or before the native observation, accepting equality. Combine
verified supplied transfers with recorded operations in the existing opening-
cash projection. All executions must still follow the opening context and have
unambiguous slot order; no same-slot transaction indices are available. Preserve
signature/source in transaction anchors. With supplied rows, label coverage
`recorded_operations_and_supplied_native_transfers`. Even a matching final
balance and all anchors leave `external_cash_flows_complete` false. Signatures
prove message authorship, not finalized inclusion or truth/completeness of
operator-provisioned balances, fee and outcome. Unsupported wallet activity,
pagination/coverage, token transfers, independent provenance and durable
idempotent application remain unfinished. No risk update, claim release or
execution authority follows from this review.

## Immutable opening inventory before operations

`radar-issuer --record-opening-inventory` reads the configured private genesis
snapshot once and reuses the protected inventory review with an empty journal
checkpoint and no acquired lots. Native balance, wallet, read windows, context
bounds and raw/listed account metadata are checked by the existing caller.
Only normalized typed wallet/native/token quantities, programs/units, separate
contexts and read times reach an OpeningInventoryRecord. Unrelated operator
packet fields are omitted. No cost basis, valuation or spendability is inferred.

OperationLog owns the write under its existing lock. A first record requires
an empty checkpoint before every operation or other event. Exact repeats do
not append, including after subsequent operations; conflicting records refuse.
Persistence precedes the in-memory update and advances the journal checkpoint.
Inventory-stage replay requires exactly one unmixed opening record at genesis,
Outcome::Ok and no operation entry. Missing/duplicate/late/misstaged/mixed records
refuse intact-chain replay rather than being ignored. Storage enforces these
association rules; the protected caller establishes observation semantics.

The optional correlation field is absent from older serialization/digests;
older histories retain unknown opening inventory and are never backfilled.
The new inventory stage requires this reader. Do not discard or recreate an
existing history to make it eligible for genesis recording. Legacy accounting
migration is not implemented. A protected host can still replace/rollback an
entire journal; this is not off-host checkpoint authentication.

The current inventory review consumes a retained opening record only for the
configured wallet, coherent recorded read bounds and observations at least as
recent as every opening context, including native context for empty token lists.
Every retained acquisition must execute strictly after the opening read's
highest slot. Duplicate opening mints refuse. Existing checked quantity
aggregation and program/unit comparisons now include opening holdings.
Rows retain opening_raw, retained_acquired_raw, expected_raw and observed_raw;
excess/reduction compare observed units with opening plus buys. Historical
acquisition lots and basis remain separate and are never doubled by the baseline.

The baseline is a recorded observation, not a proof that all holdings or cash
flows were captured. Empty or matching inventories still leave complete
inventory, opening cost basis, current exposure and daily loss unknown.
Transfers, disposals, failed fees and current valuation remain unfinished.
Neither command updates a kernel portfolio, closes an operation, releases
capital or grants authority. Ordinary issuance does not yet construct risk
state from the opening record; live activation remains closed.

## Bounded operator address history collection

`radar wallet-activity-read --wallet <address> --after-slot <N> --through-slot <N> --rpc <URL>`
collects provider-reported address history in an explicit exclusive/inclusive
slot interval. Its caller is the operator CLI; the signer remains offline.
Request finalized signature pages of 32 with a context floor at the upper slot,
then explicitly filter transaction slots. Validate finalized status, explicit
success/failure, descending slots and unique canonical signatures across pages.
Retain failed executions. Full pages advance the before cursor; stop at the
lower slot boundary, provider-reported history exhaustion or an explicit bound.
The CLI allows 36 calls, three pages and 20 seconds, using the existing Budget.
Read-start/completion timestamps describe the host collection window.

Fetch raw base64 transactions for enumerated entries at finalized commitment.
Require exact first wire signature, slot and outcome agreement, a bounded packet
and signature extent, and an explicit integer fee. Preserve raw metadata as
untrusted evidence for later classification; never interpret its UI quantities
as accounting values. The collector does not decode full messages, verify
signatures locally, establish query-address membership, or classify supported
transfers. These missing guarantees are explicit in its output. Multi-signature
and versioned messages can be collected as unresolved data, not authorized.

Report signature_scan_finished and transaction_fetch_finished separately with
stop reasons. Provider errors, missing transactions and page/call/deadline bounds
produce incomplete evidence, including already collected entries. Malformed or
inconsistent rows refuse. Empty/short pages mean only provider-reported exhaustion;
no archival completeness or absence of external activity follows. Current token
accounts omit historical/closed accounts, and an owner-wallet scan can omit
transactions naming only its token accounts. See [research 0038](../research/0038-wallet-address-history-is-not-complete-wallet-coverage.md).

Coverage is provider_reported_address_history, wallet_coverage_complete remains
false, and no economic reconciliation, portfolio update, reservation release,
journal write, key, signing or sending occurs. Output is not automatically
installed as an issuer snapshot. Independent metadata provenance, the historical
account universe, transfer classification and durable idempotent application
remain required before the collector can contribute to live reconciliation.

## Collected plain SOL transfers in protected inventory review

The operator may put wallet-activity-read output under wallet_activity in the
protected wallet evidence used by --review-inventory. It is mutually exclusive
with native_transfers. No network or additional signer dependency is introduced.
Require completed signature enumeration and transaction fetching, the expected
address-history coverage label, matching entry/transaction extents and identities,
and an explicit (after, through] interval no newer than the native observation.
Existing wallet identity and host freshness checks apply unchanged.

Normalize only integer raw native balances, exact fee/outcome agreement and raw
token-balance arrays. Reuse the existing plain native transfer decoder, strict
Ed25519 verification and exact balance/fee effect checks; bind the verified wire
signature back to the collected entry. Successful and failed supported transfers
contribute to the same cash comparison. Duplicates against retained operations
still refuse. Any unsupported message, missing effects or incomplete collection
blocks the comparison rather than silently being skipped.

This is a protected operator adapter, not automatic snapshot installation or a
live accounting service. Collection completion never establishes full wallet
coverage. Provider metadata provenance, historical account coverage, durable
transfer retention and idempotent risk-state application remain absent. Inventory
review keeps its existing incomplete/unknown accounting and authority flags.

## Immutable external native-transfer retention

The protected issuer's --record-native-transfers mode captures supported supplied
or collected activity from its private snapshot. Snapshot wallet, host freshness
and exact journal checkpoint must agree before recording. Preserve only canonical
signed bytes, whitelisted integer native/token effects and outcome/fee facts,
the configured wallet and the exact locally verified transfer result. Raw provider
metadata and extra input fields are excluded. Print the resulting checkpoint;
the operator must associate a refreshed snapshot with it before the next record.

OperationLog stores NativeTransferRecord values in its existing owned hash chain,
using a separate native_transfer stage and optional correlation field. Older
absent fields remain absent from serialization and historical digests. Storage
checks a canonical single-signature wire prefix, bounded extent and immutable association;
the protected caller establishes cryptographic and economic correctness. The
implementation is in [native transfer storage](../../crates/radar-journal/src/native_transfers.rs).
Identical repeats append nothing; conflicts refuse. Persistence precedes memory
updates. Each record is durable independently: a failed multi-record command may
leave a verified prefix, which an associated retry can safely recognize. No
batch atomicity, rollback protection or chain inclusion proof is claimed.

Replay validates stage/outcome/correlation and exact identity, deduplicates
identical records and rejects conflicts or signed artifacts also attached to an
operation. Recording and future signed binding/proposal reject that collision in
both directions. No reservation, portfolio or operation transition is made.

Inventory review re-decodes and strictly verifies every retained transfer for
the configured wallet, compares its full normalized result and rejects observations
older than its execution. Combining current and retained identical evidence counts
one signature once; conflicts and duplication with recorded operations refuse.
Cash comparison can therefore still use a transfer after it disappears from the
current input packet. Coverage, daily loss/exposure, economic reconciliation and
risk-state updates remain incomplete. This is durable evidence and comparison,
not live idempotent portfolio application or authority to trade.

## What would reverse this

Nothing foreseeable reverses holding a policy locally. The specific ceilings are
expected to move, and moving them is an operator action against a file rather
than a code change, which is the shape this ADR is choosing.
