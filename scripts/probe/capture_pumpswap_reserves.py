# SPDX-License-Identifier: Apache-2.0
"""Capture what a PumpSwap pool holds: the pool, both vaults and both mints, at one slot.

Sibling of `capture_pumpswap_pools.py`, and shaped by the one thing that script
could not do. That one read each account with its own `getAccountInfo`, so its ten
pools carry ten different slots. Reserves cannot be captured that way: a base
balance from slot N and a quote balance from slot N+40 is a ratio that never
existed on the chain, and it would look exactly like a price.

So every read here is **one `getMultipleAccounts` call**, which the RPC answers
with a single `context.slot` covering every account in it. Five accounts go in
together -- the pool, its two vaults, and the two mints -- and the slot that comes
back is the slot all five were read at. That is the atomicity the parser is
allowed to assume, and it is why the fixture records one slot per read rather than
one per account.

The vault addresses are not guessed: they are read out of the pool account at
offsets 139 and 171, which `capture_pumpswap_pools.py` and research 0033
established. A first `getAccountInfo` discovers them; the pool is then read again
inside the atomic call, and it is that second copy the fixture stores.

Two things this captures that no single pool can show:

* **The two token programs differ within a pool and swap sides between pools.**
  Each vault's owner program is recorded, because it -- not any field of `Pool` --
  is what says whether the account is classic SPL Token or Token-2022.
* **Token-2022 extensions are real and they are not all harmless.** The pools'
  own vaults carry `ImmutableOwner`, which changes nothing about a balance. The
  `specimens` list holds mints that carry the ones that do: a transfer fee, a
  transfer hook, a permanent delegate, confidential transfers. They are captured
  so the parser's refusals are asserted against bytes mainnet actually holds
  rather than against a reading of the specification.

Read-only. `getAccountInfo` and `getMultipleAccounts` against the public endpoint,
no credential, nothing signed and nothing submitted.

Writes crates/radar-pumpfun/tests/fixtures/pumpswap_reserves.json.
"""

import base64
import json
import os
import sys
import time
import urllib.request

RPC = os.environ.get("RADAR_RPC", "https://api.mainnet-beta.solana.com")
PAMM = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA"
OUT = os.path.join("crates", "radar-pumpfun", "tests", "fixtures", "pumpswap_reserves.json")

B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

# Four pools out of the ten `capture_pumpswap_pools.py` holds, chosen so that no
# assumption about either side survives: the base program, the quote program and
# the quote asset all vary, and one pool is the shortest length that exists.
POOLS = [
    (
        "C4mLt6fs2dL2W1oovZAT9QpM3tpL6CA7DZ8hqHU9Ldqb",
        "Token-2022 base against a classic SPL wrapped-SOL quote. The pool behind "
        "the buy research 0028 priced, so its vault addresses are ones a transaction "
        "the network accepted actually named.",
    ),
    (
        "6xsdRpzd53b7LsLHjNppa7fZzJu1xW3s1jAV8X79gPvd",
        "The mirror image: classic SPL wrapped-SOL base against a Token-2022 quote. "
        "Neither program belongs to a side.",
    ),
    (
        "13bkcX5JGKaaj35brGP9ZUJtG1iUAbQVVqYDnUFL1B9",
        "Both sides classic SPL, and a 211-byte pool -- the shortest that exists, "
        "with no coin_creator and no virtual_quote_reserves.",
    ),
    (
        "82zcJ16FYLuqbjxbdHKbD3F7YigdhBe6YHTTvsErNHB",
        "Quotes in USDC, which has six decimals against wrapped SOL's nine. A quote "
        "reserve read as lamports here is wrong by a factor of a thousand.",
    ),
]

# Mints and accounts captured for their extensions rather than for their pools.
# Every one of these is refused by the parser, and the point of capturing them is
# that the refusal is asserted against mainnet bytes.
SPECIMENS = [
    (
        "CKfatsPMUf8SkiURsDXs7eK6GWb4Jsd6UDbs7twMCWxo",
        "mint",
        "A Token-2022 mint whose only extension is TransferFeeConfig. The cleanest "
        "refusal there is: nothing else about this mint is unusual, and a balance in "
        "it still cannot be moved without losing a fee the parser does not model.",
    ),
    (
        "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo",
        "mint",
        "Eight extensions on one mint, four of which change what a balance is worth "
        "or who may move it: a permanent delegate, a transfer fee, a transfer hook "
        "and confidential transfers. Proof that extensions arrive in bundles and "
        "that finding one harmless one says nothing about the next.",
    ),
]


def b58encode(raw):
    num = int.from_bytes(raw, "big")
    out = ""
    while num:
        num, rem = divmod(num, 58)
        out = B58[rem] + out
    return "1" * (len(raw) - len(raw.lstrip(b"\0"))) + out


def rpc(method, params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(RPC, data=body, headers={"content-type": "application/json"})
    for attempt in range(6):
        try:
            with urllib.request.urlopen(req, timeout=120) as response:
                out = json.load(response)
            if "error" in out:
                print("rpc error:", out["error"], file=sys.stderr)
                time.sleep(4 * (attempt + 1))
                continue
            return out["result"]
        except Exception as err:  # noqa: BLE001 -- a probe, and the retry is the handling
            print("retry:", err, file=sys.stderr)
            time.sleep(4 * (attempt + 1))
    raise SystemExit(f"{method} gave up after six attempts")


def multiple(addresses):
    """One call, one slot. This is the whole point of the script."""
    result = rpc("getMultipleAccounts", [addresses, {"encoding": "base64"}])
    slot = result["context"]["slot"]
    values = result["value"]
    if len(values) != len(addresses):
        raise SystemExit(f"asked for {len(addresses)} accounts and got {len(values)}")
    return slot, values


def main():
    reads = []
    for address, note in POOLS:
        # Discovery only. Which accounts to ask for; never a value the fixture keeps.
        found = rpc("getAccountInfo", [address, {"encoding": "base64"}])["value"]
        if found is None:
            raise SystemExit(f"{address} does not exist")
        if found["owner"] != PAMM:
            raise SystemExit(f"{address} is not owned by PumpSwap but by {found['owner']}")
        raw = base64.b64decode(found["data"][0])
        roles = [
            ("pool", address),
            ("base_mint", b58encode(raw[43:75])),
            ("quote_mint", b58encode(raw[75:107])),
            ("base_vault", b58encode(raw[139:171])),
            ("quote_vault", b58encode(raw[171:203])),
        ]
        time.sleep(3)

        slot, values = multiple([a for _, a in roles])
        accounts = []
        for (role, addr), value in zip(roles, values):
            if value is None:
                raise SystemExit(f"{addr} ({role} of {address}) does not exist")
            accounts.append(
                {
                    "role": role,
                    "address": addr,
                    "owner": value["owner"],
                    "len": len(base64.b64decode(value["data"][0])),
                    "data_b64": value["data"][0],
                }
            )
        reads.append({"pool": address, "slot": slot, "note": note, "accounts": accounts})
        print(f"{address} slot={slot}", file=sys.stderr)
        time.sleep(3)

    specimens = []
    for address, kind, note in SPECIMENS:
        slot, values = multiple([address])
        value = values[0]
        if value is None:
            raise SystemExit(f"{address} does not exist")
        specimens.append(
            {
                "address": address,
                "kind": kind,
                "owner": value["owner"],
                "slot": slot,
                "note": note,
                "len": len(base64.b64decode(value["data"][0])),
                "data_b64": value["data"][0],
            }
        )
        print(f"specimen {address} slot={slot}", file=sys.stderr)
        time.sleep(3)

    out = {
        "source": f"Solana mainnet via {RPC} getMultipleAccounts, base64",
        "endpoint": RPC,
        "captured": time.strftime("%Y-%m-%d"),
        "why": (
            "What a PumpSwap pool holds, read so that the numbers describe one instant. "
            "Each entry in `reads` is a single getMultipleAccounts call carrying the pool, "
            "both vaults and both mints, so the one `slot` recorded is the slot every "
            "account in it was read at -- reserves captured one getAccountInfo at a time "
            "would be a ratio that never existed. The vault owner programs are recorded "
            "because they, and not any field of the Pool account, are what say whether a "
            "vault is classic SPL Token or Token-2022. `specimens` are mints captured for "
            "their Token-2022 extensions alone: the parser refuses each of them by name, "
            "and these are the bytes that refusal is asserted against. Stored as base64, "
            "which is what the RPC returns and what radar_types::b64 reads."
        ),
        "reads": reads,
        "specimens": specimens,
    }
    with open(OUT, "w", encoding="utf-8") as handle:
        json.dump(out, handle, indent=1)
        handle.write("\n")
    print(f"wrote {OUT}: {len(reads)} reads, {len(specimens)} specimens", file=sys.stderr)


if __name__ == "__main__":
    main()
