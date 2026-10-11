// SPDX-License-Identifier: Apache-2.0
//! Compare inline messages with captured instruction metas, not another encoder.

use radar_exec::assemble::{assemble, assemble_legacy};
use radar_exec::route::RouteError;
use radar_types::Address;
use serde_json::{Value, json};

const TAKER: &str = "CjfBjFVBs6QRvRTpMdKTBxZ7PZuJvHXWQKGRvR7wFbdz";
const SMALL: &str = include_str!("../fixtures/jupiter-build-usdc-sol.json");
const LARGE: &str = include_str!("../fixtures/jupiter-build-sol-usdc.json");

fn instructions(body: &Value) -> Vec<&Value> {
    body["computeBudgetInstructions"]
        .as_array()
        .unwrap()
        .iter()
        .chain(body["setupInstructions"].as_array().unwrap())
        .chain(std::iter::once(&body["swapInstruction"]))
        .chain((!body["cleanupInstruction"].is_null()).then_some(&body["cleanupInstruction"]))
        .chain(body["otherInstructions"].as_array().unwrap())
        .collect()
}

#[test]
fn captured_legacy_message_preserves_every_instruction_account_and_global_privilege() {
    let body: Value = serde_json::from_str(SMALL).unwrap();
    let built = assemble_legacy(SMALL, TAKER.parse().unwrap()).unwrap();
    let decoded = radar_signer::decode(&built.wire).unwrap();
    assert!(!decoded.versioned);
    assert_eq!(decoded.required_signatures, 1);
    assert_eq!(
        decoded.accounts[0],
        *TAKER.parse::<Address>().unwrap().as_bytes()
    );
    assert_eq!(built.fee_payer.to_string(), TAKER);
    assert_eq!(
        built.last_valid_block_height,
        body["blockhashWithMetadata"]["lastValidBlockHeight"]
            .as_u64()
            .unwrap()
    );
    let blockhash: Vec<u8> =
        serde_json::from_value(body["blockhashWithMetadata"]["blockhash"].clone()).unwrap();
    assert_eq!(decoded.recent_blockhash.as_slice(), blockhash);
    assert_eq!(&built.wire[..65], &[&[1_u8][..], &[0_u8; 64]].concat());
    assert_eq!(built.wire[66], 0, "payer is writable");
    let expected = instructions(&body);
    assert_eq!(decoded.instructions.len(), expected.len());
    let readonly_start = decoded.accounts.len() - usize::from(built.wire[67]);
    for (actual, ix) in decoded.instructions.iter().zip(&expected) {
        assert_eq!(
            actual.program_id,
            *ix["programId"]
                .as_str()
                .unwrap()
                .parse::<Address>()
                .unwrap()
                .as_bytes()
        );
        assert_eq!(
            actual.data,
            radar_types::b64::decode(ix["data"].as_str().unwrap()).unwrap()
        );
        let metas = ix["accounts"].as_array().unwrap();
        let accounts: Vec<_> = metas
            .iter()
            .map(|meta| {
                *meta["pubkey"]
                    .as_str()
                    .unwrap()
                    .parse::<Address>()
                    .unwrap()
                    .as_bytes()
            })
            .collect();
        assert_eq!(actual.accounts, accounts);
    }
    for (index, address) in decoded.accounts.iter().enumerate() {
        let address = Address::new(*address).to_string();
        let writable = address == TAKER
            || expected.iter().any(|ix| {
                ix["accounts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|meta| meta["pubkey"] == address && meta["isWritable"] == true)
            });
        assert_eq!(index < readonly_start, writable, "{address}");
        let flags = decoded.privileges(index).unwrap();
        assert_eq!(
            flags.writable, writable,
            "decoded global privilege {address}"
        );
        assert_eq!(flags.signer, address == TAKER, "decoded signer {address}");
    }
}

#[test]
fn inline_packet_limit_refuses_large_capture_without_truncating_it() {
    assert!(assemble(LARGE, TAKER.parse().unwrap()).is_ok());
    let error = assemble_legacy(LARGE, TAKER.parse().unwrap()).unwrap_err();
    assert!(matches!(error, RouteError::Malformed(reason) if reason.contains("packet limit")));
}

#[test]
fn legacy_build_retains_shared_signer_tip_and_fee_refusals() {
    for case in ["signer", "tip", "fee", "data"] {
        let mut body: Value = serde_json::from_str(SMALL).unwrap();
        match case {
            "signer" => body["swapInstruction"]["accounts"][1]["isSigner"] = json!(true),
            "tip" => body["tipInstruction"] = json!({}),
            "fee" => {
                let mut bytes = vec![3];
                bytes.extend(u64::MAX.to_le_bytes());
                body["computeBudgetInstructions"][0]["data"] =
                    json!(radar_types::b64::encode(&bytes));
            }
            _ => body["swapInstruction"]["data"] = json!("not-base64!"),
        }
        assert!(
            assemble_legacy(&body.to_string(), TAKER.parse().unwrap()).is_err(),
            "{case}"
        );
    }
}
