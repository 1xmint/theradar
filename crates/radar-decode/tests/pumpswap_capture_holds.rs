// SPDX-License-Identifier: Apache-2.0
//! A successful mainnet CPI, rather than a discriminator derived from a name.

use radar_decode::{Decoded, Instruction, Program, decode, pumpswap};
use serde_json::Value;
use sha2::{Digest, Sha256};

const RAW: &[u8] = include_bytes!("fixtures/pumpswap_buy_exact_quote_in_raw.json");

#[test]
fn the_captured_successful_pumpswap_buy_decodes_in_its_actual_program() {
    assert_eq!(
        format!("{:x}", Sha256::digest(RAW)),
        "53b1766fe02e92cb536c96533c8de84fa25070f8c7dd67176f43e66a2f8ecbc2",
        "retain the complete raw RPC response"
    );
    let capture: Value = serde_json::from_slice(RAW).expect("raw RPC JSON");
    let result = &capture["result"];
    assert_eq!(result["slot"], 455_527_070);
    assert_eq!(result["version"], 1);
    assert!(result["meta"]["err"].is_null());
    assert_eq!(
        result["transaction"]["signatures"][0],
        "4hioC89h8bGN84NubErash1iPQWTUi5DpMU4PsjEYz6NRGnVdGAkuAtEEPKSihuAAP3qXYqmBPb6oRbyekVpV2uA"
    );
    let keys = result["transaction"]["message"]["accountKeys"]
        .as_array()
        .expect("captured static account keys");
    let group = result["meta"]["innerInstructions"]
        .as_array()
        .expect("captured CPIs")
        .iter()
        .find(|group| group["index"] == 3)
        .expect("outer instruction 3");
    let trade = group["instructions"]
        .as_array()
        .expect("inner instructions")
        .iter()
        .find(|instruction| instruction["data"] == "2Nk86asu1ki8KjCzwAE2VBvAkWWSxXSXBs6")
        .expect("captured trade, distinct from the self-CPI event");
    let index = usize::try_from(trade["programIdIndex"].as_u64().expect("program index"))
        .expect("index fits usize");
    let address = keys[index]
        .as_str()
        .expect("program address")
        .parse()
        .expect("address");
    let program = Program::at(&address).expect("known program");
    assert_eq!(program, Program::PumpSwap);
    let data = bs58::decode(trade["data"].as_str().expect("instruction data"))
        .into_vec()
        .expect("base58 instruction bytes");
    assert_eq!(data.len(), 25);
    assert_eq!(
        trade["accounts"].as_array().expect("account roles").len(),
        26
    );
    assert_eq!(
        decode(program, &data),
        Decoded::Known(Instruction::PumpSwap(
            pumpswap::Instruction::BuyExactQuoteIn
        ))
    );
}
