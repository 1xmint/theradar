<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0038 — Wallet address history is not complete wallet coverage

**Date:** 2026-10-09.
**Status:** source and protocol inspection; collector not implemented, no live measurement.
**Bears on:** [private setup plan](../plans/0015-private-chatgpt-and-privy-setup.md)
and [issuer policy](../adr/0008-the-signer-holds-its-own-policy.md).

## What is established

The [Solana signature method](https://solana.com/docs/rpc/http/getsignaturesforaddress)
returns transactions referencing the queried address in accountKeys, newest first,
with before/until pagination and commitment options. The
[token account method](https://solana.com/docs/rpc/http/gettokenaccountsbyowner)
returns token accounts owned by an address. These are different query surfaces.
Inference: a transfer naming a token account but not its owner's wallet address
need not appear in a wallet-address signature scan. An account closed before the
current account enumeration also cannot be discovered from that enumeration
alone. A retained historical account universe is therefore a separate requirement.
A token account created and closed between two balance snapshots can be absent
from both. Matching endpoint balances cannot establish absence of intervening
activity or losses.

Existing code confirms the integration gaps:

- `crates/radar-cli/src/wallet_read.rs` collects native and both token program
  observations plus raw account verification. It does not collect transactions.
- `crates/radar-onchain/src/rpc.rs` has signatures_back_to_oldest for dossier
  callers. It does not request finalized commitment or restrict an accounting
  interval. Its SignatureInfo permits an absent err field and does not retain
  confirmationStatus. Its paging result is not accounting coverage evidence.
- `crates/radar-onchain/src/settlement.rs` binds finalized metadata to supplied
  signed bytes, but requires the configured wallet as fee payer. External native
  deposits can have another payer. Do not silently widen this existing contract.
- `crates/radar-signer/src/bin/radar-issuer/native_transfers.rs` already verifies
  supplied supported native transfers, including another payer's deposit. It
  establishes message authorship and arithmetic, not independently fetched
  inclusion or a complete enumeration.

No existing API should be relabeled as a complete wallet scan.

## Next implementation boundary

Add a bounded operator CLI collector, using the existing RpcClient transport and
Budget, with explicit wallet, endpoint and opening/current slot bounds. Keep the
signer offline. The collector must request finalized signature pages, retain
failed transactions, validate signatures, explicit outcomes and finalized status,
and verify descending slots, unique signatures and forward cursor progress.
Filter the requested transaction interval explicitly; a request context bound is
not a replacement for transaction-slot filtering. New activity above the chosen
upper slot must not enter the accounting interval.

Fetch raw base64 transactions with
[getTransaction](https://solana.com/docs/rpc/http/gettransaction). Bind each
response signature and slot to its enumerated entry and preserve unknown or
unsupported activity as unresolved. A missing transaction, provider error or
budget limit is an incomplete read, never an empty wallet. Do not use parsed
transaction vendors or discard a failed transaction's fee. Exact supported
native packets can then enter the existing issuer review; fetching alone does
not classify every transaction.

Report enumeration and transaction-fetch completion separately, with the stop
reason and reviewed interval. A completed address scan describes what the RPC
reported for that address. It cannot establish exhaustive wallet activity,
provider archival coverage, historical token-account discovery, or independently
verified metadata truth. Keep economic reconciliation, portfolio updates and
claim release closed until those separate requirements have evidence.

Required regressions: exact slot boundaries; newer rows across pages; repeated
cursor/signature; out-of-order rows; missing status/outcome; failed executions;
page/call/deadline exhaustion; empty/short/full pages; missing or mismatched raw
transactions; and a token-account transfer whose owner is absent from accountKeys.
These describe the next collector, not tests already present. Durable transfer
retention and idempotent application follow collection and remain unimplemented.
