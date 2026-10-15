// SPDX-License-Identifier: Apache-2.0
//! Bind supported Router bytes to the quote; this grants no signing authority.

use serde_json::Value;

use crate::route::{Quote, QuoteRequest, RouteError};

const JUPITER: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
const RAYDIUM: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const EVENT_AUTHORITY: &str = "D8cy77BBepLMngZx6ZukaTff5hCt1HrWyKk3Hnd9oitf";

fn refused() -> RouteError {
    RouteError::Unverifiable("unsupported or inconsistent Router instruction envelope".into())
}

pub(crate) fn review(
    body: &str,
    request: &QuoteRequest,
    quote: &Quote,
    slippage_bps: u32,
) -> Result<(), RouteError> {
    let response: Value = serde_json::from_str(body).map_err(|_| refused())?;
    let ix = &response["swapInstruction"];
    let bytes = ix["data"]
        .as_str()
        .and_then(radar_types::b64::decode)
        .ok_or_else(refused)?;
    // Exactly one full RaydiumV2 hop. No unknown enum payload or suffix is read
    // as padding. The larger public Swap enum does not describe all live routes.
    if ix["programId"] != JUPITER || bytes.len() != 39 {
        return Err(refused());
    }
    if bytes[..8] != [187, 100, 250, 204, 49, 196, 175, 20]
        || bytes[30..] != [1, 0, 0, 0, 105, 16, 39, 0, 1]
    {
        return Err(refused());
    }
    let input = u64::from_le_bytes(bytes[8..16].try_into().map_err(|_| refused())?);
    let output = u64::from_le_bytes(bytes[16..24].try_into().map_err(|_| refused())?);
    let tolerance = u16::from_le_bytes(bytes[24..26].try_into().map_err(|_| refused())?);
    if input != request.amount
        || output != quote.out_amount
        || u32::from(tolerance) != slippage_bps
        || bytes[26..30] != [0; 4]
    {
        return Err(refused());
    }
    let accounts = ix["accounts"].as_array().ok_or_else(refused)?;
    if accounts.len() != 19 {
        return Err(refused());
    }
    let wallet = request.taker.to_string();
    let source = QuoteRequest::wire_mint(request.input).to_string();
    let destination = QuoteRequest::wire_mint(request.output).to_string();
    for (index, expected) in [
        (0, wallet.as_str()),
        (3, source.as_str()),
        (4, destination.as_str()),
        (5, TOKEN),
        (6, TOKEN),
        (7, JUPITER),
        (8, EVENT_AUTHORITY),
        (9, JUPITER),
        (10, RAYDIUM),
        (11, TOKEN),
        (18, wallet.as_str()),
    ] {
        if accounts[index]["pubkey"] != expected {
            return Err(refused());
        }
    }
    if accounts[1]["pubkey"] != accounts[16]["pubkey"]
        || accounts[2]["pubkey"] != accounts[17]["pubkey"]
    {
        return Err(refused());
    }
    let hops = response["routePlan"].as_array().ok_or_else(refused)?;
    if hops.len() != 1 || hops[0]["swapInfo"]["ammKey"] != accounts[12]["pubkey"] {
        return Err(refused());
    }
    // Pool/vault ownership, ATA derivation and global writable/signing privileges
    // still require independent protected state and signer verification.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_types::Asset;
    use serde_json::json;

    const BODY: &str = include_str!("../fixtures/jupiter-build-raydium-usdc-sol-2026-10-11.json");

    #[test]
    fn reviewed_envelope_binds_exact_quote_route_and_account_roles() {
        let original: Value = serde_json::from_str(BODY).unwrap();
        let taker = original["swapInstruction"]["accounts"][0]["pubkey"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let request = QuoteRequest::new(Asset::Usdc, Asset::Sol, 10_000_000, taker);
        let quote = Quote::from_response(BODY, &request, 100).unwrap();
        assert!(review(BODY, &request, &quote, 100).is_ok());
        for index in [0, 8, 16, 24, 26, 28, 30, 34, 35, 37, 38, 39] {
            let mut value = original.clone();
            let mut bytes =
                radar_types::b64::decode(value["swapInstruction"]["data"].as_str().unwrap())
                    .unwrap();
            if index == bytes.len() {
                bytes.push(0);
            } else {
                bytes[index] ^= 1;
            }
            value["swapInstruction"]["data"] = json!(radar_types::b64::encode(&bytes));
            assert!(
                review(&value.to_string(), &request, &quote, 100).is_err(),
                "byte {index}"
            );
        }
        for index in [0, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 16, 17, 18] {
            let mut value = original.clone();
            value["swapInstruction"]["accounts"][index]["pubkey"] =
                json!("11111111111111111111111111111111");
            assert!(
                review(&value.to_string(), &request, &quote, 100).is_err(),
                "account {index}"
            );
        }
        for field in ["programId", "data", "accounts"] {
            let mut value = original.clone();
            value["swapInstruction"][field] = Value::Null;
            assert!(
                review(&value.to_string(), &request, &quote, 100).is_err(),
                "{field}"
            );
        }
    }
}
