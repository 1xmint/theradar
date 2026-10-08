// SPDX-License-Identifier: Apache-2.0
//! Finalized exact-transaction evidence for the operator settlement reader.
//! No journal transition, signing, sending or USD accounting is inferred here.

use radar_types::{Address, Signature, b64};
use serde_json::{Value, json};

use crate::{Budget, RpcClient, preflight::MAX_TRANSACTION_BYTES};

fn signed_transaction(
    bytes: &[u8],
    wallet: Address,
) -> Result<(Signature, Vec<Address>), &'static str> {
    // One canonical signature, legacy header, writable fee payer. Full message
    // validity and execution are established by the exact finalized RPC result.
    if bytes.len() > MAX_TRANSACTION_BYTES
        || bytes.len() < 134
        || bytes[0] != 1
        || bytes[1..65].iter().all(|byte| *byte == 0)
        || bytes[65] != 1
        || bytes[66] != 0
        || bytes[68] == 0
        || bytes[68] >= 128
        || bytes[67] >= bytes[68]
    {
        return Err("settlement-read needs a single signed legacy transaction");
    }
    let end = 69 + usize::from(bytes[68]) * 32;
    if bytes.len() < end + 33 {
        return Err("settlement transaction accounts are truncated");
    }
    let accounts: Vec<Address> = bytes[69..end]
        .as_chunks::<32>()
        .0
        .iter()
        .copied()
        .map(Address::new)
        .collect();
    if accounts[0] != wallet {
        return Err("settlement wallet is not the transaction fee payer");
    }
    Ok((
        Signature::new(bytes[1..65].try_into().expect("signature extent")),
        accounts,
    ))
}

fn balances(meta: &Value, field: &str, count: usize) -> Result<Vec<String>, &'static str> {
    let values = meta[field]
        .as_array()
        .ok_or("settlement balances are missing")?;
    if values.len() != count {
        return Err("settlement balances do not match transaction accounts");
    }
    values
        .iter()
        .map(|value| {
            value
                .as_u64()
                .map(|value| value.to_string())
                .ok_or("invalid settlement balance")
        })
        .collect()
}

fn token_balances(meta: &Value, field: &str, count: usize) -> Result<Vec<Value>, &'static str> {
    let mut seen = std::collections::BTreeSet::new();
    meta[field].as_array().ok_or("settlement token balances are missing")?
        .iter().map(|value| {
            let index = value["accountIndex"].as_u64()
                .filter(|index| *index < count as u64).ok_or("invalid settlement token index")?;
            if !seen.insert(index) {
                return Err("duplicate settlement token index");
            }
            let address = |field: &str| -> Result<Address, &'static str> {
                value[field].as_str().and_then(|value| value.parse().ok())
                    .ok_or("missing or invalid settlement token identity")
            };
            let mint = address("mint")?;
            let owner = address("owner")?;
            let program = address("programId")?;
            if program.to_string() != crate::rpc::TOKEN_PROGRAM_ID
                && program.to_string() != crate::rpc::TOKEN_2022_PROGRAM_ID {
                return Err("unsupported settlement token program");
            }
            let amount = value["uiTokenAmount"]["amount"].as_str()
                .and_then(|value| value.parse::<u64>().ok()).ok_or("invalid settlement token amount")?;
            let decimals = value["uiTokenAmount"]["decimals"].as_u64()
                .and_then(|value| u8::try_from(value).ok()).ok_or("invalid settlement token decimals")?;
            Ok(json!({"account_index":index,"mint":mint.to_string(),"owner":owner.to_string(),
                "program_id":program.to_string(),"raw_amount":amount.to_string(),"decimals":decimals}))
        }).collect()
}

impl RpcClient {
    /// Reads finalized metadata for exactly the supplied signed legacy bytes.
    ///
    /// # Errors
    /// Unsupported input, wrong fee payer, missing/stale/mismatched transaction,
    /// missing/malformed metadata or exhausted budget. Absence never releases a claim.
    pub fn settlement_evidence(
        &self,
        bytes: &[u8],
        wallet: Address,
        minimum_slot: u64,
        budget: &mut Budget,
    ) -> Result<Value, &'static str> {
        let (signature, accounts) = signed_transaction(bytes, wallet)?;
        let raw: Value = self
            .call(
                budget,
                "getTransaction",
                &json!([signature.to_string(),
            {"encoding":"base64","commitment":"finalized","maxSupportedTransactionVersion":0}]),
            )
            .map_err(|_| "settlement transaction unavailable at finalized commitment")?;
        let slot = raw["slot"]
            .as_u64()
            .filter(|slot| *slot >= minimum_slot)
            .ok_or("settlement transaction has missing or stale slot")?;
        let transaction = raw["transaction"]
            .as_array()
            .ok_or("missing settlement transaction bytes")?;
        if raw["version"] != "legacy"
            || transaction.len() != 2
            || transaction[0] != b64::encode(bytes)
            || transaction[1] != "base64"
        {
            return Err("settlement result does not bind exact legacy transaction bytes");
        }
        let meta = &raw["meta"];
        let success = match meta.get("err") {
            Some(Value::Null) => true,
            Some(Value::String(_) | Value::Object(_)) => false,
            _ => return Err("settlement execution result is missing or malformed"),
        };
        let fee = meta["fee"]
            .as_u64()
            .ok_or("settlement transaction fee is unknown")?;
        let pre = balances(meta, "preBalances", accounts.len())?;
        let post = balances(meta, "postBalances", accounts.len())?;
        let tokens_before = token_balances(meta, "preTokenBalances", accounts.len())?;
        let tokens_after = token_balances(meta, "postTokenBalances", accounts.len())?;
        let block_time = match raw.get("blockTime") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_i64()
                    .ok_or("invalid settlement block time")?
                    .to_string(),
            ),
        };
        Ok(
            json!({"version":1,"authority":"read_only","commitment":"finalized",
            "wallet":wallet.to_string(),"signature":signature.to_string(),"slot":slot.to_string(),
            "minimum_slot":minimum_slot.to_string(),"transaction_base64":b64::encode(bytes),
            "outcome":if success {"succeeded"} else {"failed"},"network_fee_lamports":fee.to_string(),
            "account_keys":accounts.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "pre_balances_lamports":pre,"post_balances_lamports":post,
            "pre_token_balances":tokens_before,"post_token_balances":tokens_after,
            "block_time_unix_secs":block_time,"usd_value":null,"realised_pnl":null,
            "signature_verified_locally":false,"operation_reconciled":false}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::Transport;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct Fixture {
        answer: Value,
        calls: Arc<Mutex<Vec<Value>>>,
    }
    impl Transport for Fixture {
        fn post(&self, _: &str, body: String) -> Result<String, String> {
            self.calls
                .lock()
                .expect("calls")
                .push(serde_json::from_str(&body).expect("request"));
            Ok(self.answer.to_string())
        }
    }
    fn bytes() -> Vec<u8> {
        let mut bytes = vec![1];
        bytes.extend_from_slice(&[0xAB; 64]);
        bytes.extend_from_slice(&[1, 0, 0, 2]);
        bytes.extend_from_slice(&[0x55; 32]);
        bytes.extend_from_slice(&[0x44; 32]);
        bytes.extend_from_slice(&[0xAA; 32]);
        bytes.push(0);
        bytes
    }
    fn token() -> Value {
        json!({"accountIndex":1,"mint":Address::new([0x22;32]).to_string(),
            "owner":Address::new([0x55;32]).to_string(),"programId":crate::rpc::TOKEN_PROGRAM_ID,
            "uiTokenAmount":{"amount":u64::MAX.to_string(),"decimals":6,"uiAmount":null}})
    }
    fn result(bytes: &[u8]) -> Value {
        json!({"slot":50,"version":"legacy","transaction":[b64::encode(bytes),"base64"],
            "blockTime":null,"meta":{"err":null,"fee":5000,"preBalances":[u64::MAX,0],
                "postBalances":[u64::MAX-5000,0],"preTokenBalances":[],"postTokenBalances":[token()]}})
    }
    fn client(answer: Value) -> (RpcClient, Arc<Mutex<Vec<Value>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            RpcClient::with_transport(
                "http://fixture",
                Box::new(Fixture {
                    answer,
                    calls: Arc::clone(&calls),
                }),
            ),
            calls,
        )
    }
    fn read(raw: &Value, bytes: &[u8]) -> Result<Value, &'static str> {
        client(json!({"result":raw})).0.settlement_evidence(
            bytes,
            Address::new([0x55; 32]),
            50,
            &mut Budget::new(1, 0, Duration::from_secs(20)),
        )
    }

    #[test]
    fn finalized_exact_bytes_and_integer_metadata_survive_success_and_fee_paying_failure() {
        let bytes = bytes();
        for (err, outcome, fee, program) in [
            (Value::Null, "succeeded", 0, crate::rpc::TOKEN_PROGRAM_ID),
            (
                json!({"InstructionError":[0,1]}),
                "failed",
                5000,
                crate::rpc::TOKEN_2022_PROGRAM_ID,
            ),
        ] {
            let mut raw = result(&bytes);
            raw["meta"]["err"] = err;
            raw["meta"]["fee"] = json!(fee);
            raw["meta"]["postTokenBalances"][0]["programId"] = json!(program);
            raw["blockTime"] = json!(1_791_000_000);
            let (rpc, calls) = client(json!({"result":raw}));
            let output = rpc
                .settlement_evidence(
                    &bytes,
                    Address::new([0x55; 32]),
                    50,
                    &mut Budget::new(1, 0, Duration::from_secs(20)),
                )
                .expect("read");
            assert_eq!(output["outcome"], outcome);
            assert_eq!(output["network_fee_lamports"], fee.to_string());
            assert_eq!(output["transaction_base64"], b64::encode(&bytes));
            assert_eq!(output["slot"], "50");
            assert_eq!(output["pre_balances_lamports"][0], u64::MAX.to_string());
            assert_eq!(
                output["post_balances_lamports"][0],
                (u64::MAX - 5000).to_string()
            );
            assert_eq!(
                output["post_token_balances"][0],
                json!({"account_index":1,
                "mint":Address::new([0x22;32]).to_string(),"owner":Address::new([0x55;32]).to_string(),
                "program_id":program,"raw_amount":u64::MAX.to_string(),"decimals":6})
            );
            assert_eq!(output["pre_token_balances"], json!([]));
            assert_eq!(
                output["account_keys"],
                json!([
                    Address::new([0x55; 32]).to_string(),
                    Address::new([0x44; 32]).to_string()
                ])
            );
            assert_eq!(output["block_time_unix_secs"], "1791000000");
            assert_eq!(output["operation_reconciled"], false);
            assert_eq!(output["signature_verified_locally"], false);
            assert!(output["usd_value"].is_null());
            assert!(output["realised_pnl"].is_null());
            assert_eq!(
                *calls.lock().expect("calls"),
                vec![json!({"jsonrpc":"2.0","id":1,
                "method":"getTransaction","params":[Signature::new([0xAB;64]).to_string(),
                    {"encoding":"base64","commitment":"finalized","maxSupportedTransactionVersion":0}]})]
            );
        }
        assert!(
            read(&result(&bytes), &bytes).expect("unknown clock")["block_time_unix_secs"].is_null()
        );
    }

    #[test]
    fn required_metadata_never_becomes_success_zero_fees_or_empty_holdings() {
        let bytes = bytes();
        for pointer in [
            "/slot",
            "/version",
            "/transaction",
            "/meta",
            "/meta/err",
            "/meta/fee",
            "/meta/preBalances",
            "/meta/postBalances",
            "/meta/preTokenBalances",
            "/meta/postTokenBalances",
            "/meta/postTokenBalances/0/accountIndex",
            "/meta/postTokenBalances/0/mint",
            "/meta/postTokenBalances/0/owner",
            "/meta/postTokenBalances/0/programId",
            "/meta/postTokenBalances/0/uiTokenAmount/amount",
            "/meta/postTokenBalances/0/uiTokenAmount/decimals",
        ] {
            let mut raw = result(&bytes);
            let (parent, key) = pointer.rsplit_once('/').expect("pointer");
            raw.pointer_mut(parent)
                .expect("parent")
                .as_object_mut()
                .expect("object")
                .remove(key);
            assert!(read(&raw, &bytes).is_err(), "missing {pointer}");
        }
    }

    #[test]
    fn mismatched_stale_malformed_and_ambiguous_results_refuse() {
        let bytes = bytes();
        for (pointer, value) in [
            ("/slot", json!(49)),
            ("/slot", json!("50")),
            ("/version", json!(0)),
            ("/transaction/0", json!(b64::encode(&[1; 166]))),
            ("/transaction/1", json!("base58")),
            (
                "/transaction",
                json!([b64::encode(&bytes), "base64", "extra"]),
            ),
            ("/meta/err", json!(false)),
            ("/meta/fee", Value::Null),
            ("/meta/fee", json!("5000")),
            ("/meta/preBalances", json!([0])),
            ("/meta/postBalances", json!([0, 0, 0])),
            ("/meta/preBalances/0", json!(-1)),
            ("/meta/postBalances/0", json!("0")),
            ("/meta/preTokenBalances", Value::Null),
            ("/meta/postTokenBalances", json!({})),
            ("/meta/postTokenBalances/0/accountIndex", json!(2)),
            ("/meta/postTokenBalances/0/mint", json!("invalid")),
            (
                "/meta/postTokenBalances/0/programId",
                json!(Address::SYSTEM_PROGRAM.to_string()),
            ),
            (
                "/meta/postTokenBalances/0/uiTokenAmount/amount",
                json!("18446744073709551616"),
            ),
            ("/meta/postTokenBalances/0/uiTokenAmount/amount", json!(0)),
            (
                "/meta/postTokenBalances/0/uiTokenAmount/decimals",
                json!(256),
            ),
            ("/meta/postTokenBalances", json!([token(), token()])),
            ("/blockTime", json!("unknown")),
        ] {
            let mut raw = result(&bytes);
            *raw.pointer_mut(pointer).expect("field") = value;
            assert!(read(&raw, &bytes).is_err(), "bad {pointer}");
        }
        assert!(read(&Value::Null, &bytes).is_err());
    }

    #[test]
    fn unsupported_truncated_unsigned_or_foreign_wallet_inputs_refuse_before_rpc() {
        let original = bytes();
        let mut inputs: Vec<_> = (0..original.len())
            .map(|size| original[..size].to_vec())
            .collect();
        for (at, value) in [
            (0, 2),
            (65, 0x80),
            (65, 2),
            (66, 1),
            (67, 2),
            (68, 0),
            (68, 128),
            (69, 0),
        ] {
            let mut bytes = original.clone();
            bytes[at] = value;
            inputs.push(bytes);
        }
        let mut unsigned = original.clone();
        unsigned[1..65].fill(0);
        inputs.push(unsigned);
        let mut oversized = original.clone();
        oversized.resize(MAX_TRANSACTION_BYTES + 1, 0);
        inputs.push(oversized);
        for bytes in inputs {
            let (rpc, calls) = client(json!({"result":result(&original)}));
            assert!(
                rpc.settlement_evidence(
                    &bytes,
                    Address::new([0x55; 32]),
                    50,
                    &mut Budget::new(1, 0, Duration::from_secs(20))
                )
                .is_err()
            );
            assert_eq!(*calls.lock().expect("calls"), Vec::<Value>::new());
        }
        let mut maximum = original;
        maximum.resize(MAX_TRANSACTION_BYTES, 0);
        assert!(read(&result(&maximum), &maximum).is_ok());
    }

    #[test]
    fn exhausted_budget_and_provider_failures_return_no_provider_details() {
        let bytes = bytes();
        let (rpc, calls) = client(json!({"result":result(&bytes)}));
        assert!(
            rpc.settlement_evidence(
                &bytes,
                Address::new([0x55; 32]),
                50,
                &mut Budget::new(0, 0, Duration::from_secs(20))
            )
            .is_err()
        );
        assert_eq!(*calls.lock().expect("calls"), Vec::<Value>::new());
        let error = client(json!({"error":{"message":"private provider detail"}}))
            .0
            .settlement_evidence(
                &bytes,
                Address::new([0x55; 32]),
                50,
                &mut Budget::new(1, 0, Duration::from_secs(20)),
            )
            .expect_err("refuse");
        assert!(!error.contains("private provider detail"));
    }
}
