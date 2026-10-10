<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0039 — Exact-SOL buys have a token floor, not a basis-point argument

**Date:** 2026-10-10.
**Status:** first-party interface and successful capture inspected; builder unit
correction verified at f18f368 by GitHub CI 38082687122 (all jobs passed; 13
mutants, 11 caught and two unviable). Combined issuer/signer verification pending.
**Bears on:** [direct construction](../adr/0009-radar-builds-its-own-pump-fun-swaps.md)
and [private setup](../plans/0015-private-chatgpt-and-privy-setup.md).

## Correction

Trade::BuyExactSolIn called its second u64 slippage_bps. Its comment inferred
basis points because the old packet carried 500. That inference was wrong.
The current [first-party interface](https://github.com/pump-fun/pump-public-docs/blob/2293f9a66c654e9fe82dc5e8f4618538f24bb35f/idl/pump.json)
names the arguments spendable_sol_in and min_tokens_out. A value of 500 means
500 raw tokens, not a 5% tolerance. Renaming the field makes a caller using the
old interpretation fail compilation instead of silently producing a weak floor.

Successful transaction
`3Dk1fn5HT3TMoo3jn2chJyrV7fJx8v697FrN4NHFeStJzLLiraUjYbrreEg9REFGrnvDvmM6b6PB7PfiBfaEEFqu`,
slot 440625732, has 654301622 native input and 2202113837114 as its second amount.
Its instruction payload and mint/owner anchors are retained in
[the public capture fixture](../../crates/radar-signer/tests/fixtures/pumpfun_trade_roles.json).
Finalized getTransaction reported err:null. The second amount's scale contradicts
the basis-point interpretation; the first-party interface supplies its name.
RPC metadata remains provider-reported, not an independently verified inclusion
proof or a guarantee of present-day execution.

The new regression compares the discriminator and two arguments against that
successful packet. It does not claim the builder's trailing track-volume flag
was in the capture: the successful packet is 24 bytes and omits trailing flags.
The old 500 packet came from a transaction that failed before this trade ran;
its existing byte test pins an observed packet, not runtime acceptance.

## Construction behavior

Trade::exact_sol_buy takes native input, a fee-inclusive token quote and a
basis-point tolerance. It computes floor(quoted_tokens * (10000-bps) / 10000)
using wide arithmetic and encodes that raw-token floor. It refuses zero native
spend, invalid tolerance and a zero resulting floor. The existing builder-to-
signer integration now calls this helper with an explicitly synthetic quote.
GitHub regressions cover rounding, extreme values, encoded quantity and refusals.

This does not authenticate quotes, make the public enum impossible to construct
with a zero floor, enforce a minimum output in issuer authority, update optional
argument encodings, or prove live venue support. Those remain execution work.
No transaction was signed or submitted and no production authority was changed.
