# SPDX-License-Identifier: Apache-2.0
"""Capture PumpSwap `Pool` accounts from mainnet, one of every length that exists.

The vendor's README and IDL propose a field order for `Pool`. This repository has
twice caught the same vendor's references being incomplete about this same program
family (LEARNINGS 25), so the layout is settled by bytes rather than by prose.

Two things a single capture cannot tell you, and this script is shaped around both:

* **A field that reads zero in one pool is not a constant.** So more than one pool,
  and deliberately including a pool whose quote mint is not SOL.
* **The account has more than one length on mainnet.** A census over every account
  carrying the `Pool` discriminator found eight: 211, 243, 244, 245, 261, 270, 300
  and 301 bytes. Five of those are exactly the cumulative field boundaries of the
  documented layout, which is what proves the field widths -- 245 + 16 = 261 and
  there is no 253, so `virtual_quote_reserves` is sixteen bytes wide, not eight.
  One pool of each length is captured so the parser's ladder is asserted against
  every shape the chain actually holds.

Also records, per pool, the *owner program of each mint*. The two token programs
are not fields of `Pool` -- they are accounts an instruction carries -- and they
are neither equal to each other nor constant across pools, so the capture records
which each pool uses rather than letting anything assume.

Read-only. `getAccountInfo` and `getProgramAccounts` against the public endpoint,
no credential, nothing signed and nothing submitted.

Writes crates/radar-pumpfun/tests/fixtures/pumpswap_pools.json.
"""

import base64
import json
import os
import sys
import time
import urllib.request

RPC = os.environ.get("RADAR_RPC", "https://api.mainnet-beta.solana.com")
PAMM = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA"
OUT = os.path.join("crates", "radar-pumpfun", "tests", "fixtures", "pumpswap_pools.json")

B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

# Chosen by hand from a census of every `Pool`-discriminator account, so that
# every length on the chain is represented and no field is asserted from a pool
# where it happens to read zero. The note is what each one is here to prove.
POOLS = [
    (
        "C4mLt6fs2dL2W1oovZAT9QpM3tpL6CA7DZ8hqHU9Ldqb",
        "The pool behind the buy research 0028 priced. Its base, quote, and both "
        "vault addresses are the ones that transaction carried, so this capture "
        "ties the field order to a transaction the network accepted.",
    ),
    (
        "6xsdRpzd53b7LsLHjNppa7fZzJu1xW3s1jAV8X79gPvd",
        "Base is wrapped SOL and quote is not, which is the reverse of every other "
        "pool here: neither side is a constant. Its virtual_quote_reserves is zero "
        "where its neighbours' is not.",
    ),
    (
        "GNUcWTRcY94cmP9HgxH8gMNRPg1PaDcukmWp15hA3Wfk",
        "A third 301-byte pool, whose virtual_quote_reserves differs from the "
        "first's -- the field is read, not a constant.",
    ),
    ("13bkcX5JGKaaj35brGP9ZUJtG1iUAbQVVqYDnUFL1B9", "211 bytes: through lp_supply, and no further."),
    ("114hoiDuak8RVc8JCQgX7hPrEuqaoUwKLeQrKE7ESFa", "243 bytes: coin_creator present, the flags absent."),
    ("11RWS7x2FrV847pEyz4QuPskgfAacMR42H7jrCfod1F", "244 bytes: is_mayhem_mode present, is_cashback_coin absent."),
    ("12Tmc1bcYnY3dJc8Gh8awuH77FLQvMema2Qmy8aF1nK", "245 bytes: both flags present, virtual_quote_reserves absent."),
    ("1ED6iUwkgnqLBcASfPUcSKitowQdGqq52M6TLyohTpF", "261 bytes: every documented field, no padding. Its index is 1, not 0."),
    (
        "82zcJ16FYLuqbjxbdHKbD3F7YigdhBe6YHTTvsErNHB",
        "270 bytes, and its quote mint is USDC. The rarest shape on the chain (268 "
        "accounts) and the one that settles whether a non-SOL quote pool exists.",
    ),
    ("118Kjv7HGmoVEnXRupC43AaB2aaAyitKHaW1Kdz1SJp", "300 bytes: the documented fields and 39 bytes of padding."),
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
                time.sleep(3 * (attempt + 1))
                continue
            return out["result"]
        except Exception as err:  # noqa: BLE001 -- a probe, and the retry is the handling
            print("retry:", err, file=sys.stderr)
            time.sleep(3 * (attempt + 1))
    raise SystemExit(f"{method} gave up after six attempts")


def account(address):
    result = rpc("getAccountInfo", [address, {"encoding": "base64"}])
    value = (result or {}).get("value")
    if value is None:
        raise SystemExit(f"{address} does not exist")
    return base64.b64decode(value["data"][0]), value["owner"], result["context"]["slot"]


def main():
    captured = []
    for address, note in POOLS:
        raw, owner, slot = account(address)
        if owner != PAMM:
            raise SystemExit(f"{address} is not owned by PumpSwap but by {owner}")
        base_mint = b58encode(raw[43:75])
        quote_mint = b58encode(raw[75:107])
        time.sleep(2)
        mints = rpc(
            "getMultipleAccounts",
            [[base_mint, quote_mint], {"encoding": "base64", "dataSlice": {"offset": 0, "length": 0}}],
        )
        time.sleep(2)
        captured.append(
            {
                "address": address,
                "owner": owner,
                "slot": slot,
                "len": len(raw),
                "note": note,
                "base_mint": base_mint,
                "quote_mint": quote_mint,
                "base_token_program": mints["value"][0]["owner"],
                "quote_token_program": mints["value"][1]["owner"],
                "data_hex": raw.hex(),
            }
        )
        print(f"{address} len={len(raw)} slot={slot}", file=sys.stderr)

    out = {
        "source": f"Solana mainnet via {RPC} getAccountInfo, base64",
        "captured": time.strftime("%Y-%m-%d"),
        "why": (
            "The PumpSwap `Pool` account, read from the chain rather than from the "
            "vendor's README. One pool of every length the account has on mainnet -- "
            "211, 243, 244, 245, 261, 270, 300 and 301 bytes -- because five of those "
            "are the cumulative field boundaries of the documented layout and together "
            "they prove the field widths that no single capture can. base_token_program "
            "and quote_token_program are NOT fields of this account; they are the owner "
            "programs of the two mints, recorded here because they differ from each "
            "other within a pool and swap roles between pools. Stored as hex so the "
            "test needs no base64 decoder."
        ),
        "pools": captured,
    }
    with open(OUT, "w", encoding="utf-8") as handle:
        json.dump(out, handle, indent=1)
        handle.write("\n")
    print(f"wrote {OUT}: {len(captured)} pools", file=sys.stderr)


if __name__ == "__main__":
    main()
