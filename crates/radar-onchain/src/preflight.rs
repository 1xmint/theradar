// SPDX-License-Identifier: Apache-2.0
//! Exact-byte read evidence for `radar transaction-read`. No signing or sending.
//! Only the single-zero-signature legacy envelope is inspected locally; the RPC
//! validates the message. The independent signer must still decode its content.

use radar_types::b64;
use serde_json::{Value, json};

use crate::{Budget, RpcClient};

/// Solana's serialized transaction packet limit; also bounds operator input.
pub const MAX_TRANSACTION_BYTES: usize = 1232;

fn slot(result: &Value, minimum: u64) -> Result<u64, &'static str> {
    result["context"]["slot"]
        .as_u64()
        .filter(|slot| *slot >= minimum)
        .ok_or("transaction read has missing or stale context")
}

impl RpcClient {
    /// Simulates the supplied unsigned legacy bytes without blockhash replacement,
    /// then prices their exact message. Each response retains its own bank slot.
    ///
    /// # Errors
    /// Unsupported envelopes, failed/missing simulation or fee, stale/malformed
    /// context, transport/node failure or exhausted read budget. Provider details
    /// are deliberately absent from these operator-facing errors.
    pub fn transaction_preflight(
        &self,
        bytes: &[u8],
        minimum_slot: u64,
        budget: &mut Budget,
    ) -> Result<Value, &'static str> {
        // Canonical shortvec(1), exactly one zero signature, legacy header with
        // one required signer. This is framing, not instruction authorization.
        if bytes.len() > MAX_TRANSACTION_BYTES
            || bytes.len() <= 65
            || bytes[0] != 1
            || bytes[1..65].iter().any(|byte| *byte != 0)
            || bytes[65] != 1
        {
            return Err("transaction-read needs a single unsigned legacy transaction");
        }
        let transaction = b64::encode(bytes);
        let message = b64::encode(&bytes[65..]);
        let simulation: Value = self
            .call(
                budget,
                "simulateTransaction",
                &json!([transaction, {"encoding":"base64", "commitment":"finalized",
                    "sigVerify":false, "replaceRecentBlockhash":false,
                    "minContextSlot":minimum_slot}]),
            )
            .map_err(|_| "transaction simulation RPC failed")?;
        let simulation_slot = slot(&simulation, minimum_slot)?;
        // Indexing a missing key gives Null: use get to require explicit err:null.
        if simulation["value"].get("err") != Some(&Value::Null) {
            return Err("transaction simulation failed or omitted its result");
        }
        if simulation["value"]
            .get("replacementBlockhash")
            .is_some_and(|value| !value.is_null())
        {
            return Err("transaction simulation replaced the blockhash");
        }
        let units = match simulation["value"].get("unitsConsumed") {
            None | Some(Value::Null) => None,
            Some(value) => Some(value.as_u64().ok_or("invalid simulation units")?),
        };
        let fee: Value = self
            .call(
                budget,
                "getFeeForMessage",
                &json!([message, {"commitment":"finalized", "minContextSlot":minimum_slot}]),
            )
            .map_err(|_| "transaction fee RPC failed")?;
        let fee_slot = slot(&fee, minimum_slot)?;
        let lamports = fee["value"].as_u64().ok_or("transaction fee is unknown")?;
        Ok(json!({
            "version":1, "authority":"read_only", "commitment":"finalized",
            "minimum_context_slot":minimum_slot.to_string(),
            "transaction_base64":transaction, "message_base64":message,
            "simulation":{"slot":simulation_slot.to_string(), "err":null,
                "signature_verified":false, "blockhash_replaced":false,
                "units_consumed":units.map(|value| value.to_string())},
            "network_fee":{"slot":fee_slot.to_string(), "lamports":lamports.to_string()},
            "rent_and_other_instruction_costs":null, "usd_value":null,
            "execution_guaranteed":false
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::Transport;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct Fixture {
        answers: Mutex<VecDeque<Value>>,
        calls: Arc<Mutex<Vec<Value>>>,
    }
    impl Transport for Fixture {
        fn post(&self, _: &str, body: String) -> Result<String, String> {
            self.calls
                .lock()
                .expect("calls")
                .push(serde_json::from_str(&body).expect("request"));
            self.answers
                .lock()
                .expect("answers")
                .pop_front()
                .map(|value| value.to_string())
                .ok_or("private transport detail".into())
        }
    }
    fn bytes() -> Vec<u8> {
        let mut bytes = vec![0; 100];
        bytes[0] = 1;
        bytes[65] = 1;
        bytes[99] = 7;
        bytes
    }
    fn simulation() -> Value {
        json!({"result":{"context":{"slot":40},"value":{"err":null,"unitsConsumed":u64::MAX}}})
    }
    fn fee() -> Value {
        json!({"result":{"context":{"slot":41},"value":u64::MAX}})
    }
    fn client(answers: Vec<Value>) -> (RpcClient, Arc<Mutex<Vec<Value>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            RpcClient::with_transport(
                "http://fixture",
                Box::new(Fixture {
                    answers: Mutex::new(answers.into()),
                    calls: Arc::clone(&calls),
                }),
            ),
            calls,
        )
    }
    fn read(answers: Vec<Value>, bytes: &[u8]) -> Result<Value, &'static str> {
        client(answers).0.transaction_preflight(
            bytes,
            40,
            &mut Budget::new(2, 0, Duration::from_secs(20)),
        )
    }

    #[test]
    fn exact_bytes_options_slots_and_integer_costs_survive_both_reads() {
        let (rpc, calls) = client(vec![simulation(), fee()]);
        let bytes = bytes();
        let output = rpc
            .transaction_preflight(&bytes, 40, &mut Budget::new(2, 0, Duration::from_secs(20)))
            .expect("read");
        assert_eq!(
            output,
            json!({"version":1,"authority":"read_only","commitment":"finalized",
            "minimum_context_slot":"40", "transaction_base64":b64::encode(&bytes),
            "message_base64":b64::encode(&bytes[65..]),
            "simulation":{"slot":"40","err":null,"signature_verified":false,
                "blockhash_replaced":false,"units_consumed":u64::MAX.to_string()},
            "network_fee":{"slot":"41","lamports":u64::MAX.to_string()},
            "rent_and_other_instruction_costs":null,"usd_value":null,"execution_guaranteed":false})
        );
        let calls = calls.lock().expect("calls");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["method"], "simulateTransaction");
        assert_eq!(
            calls[0]["params"],
            json!([b64::encode(&bytes),{"encoding":"base64","commitment":"finalized",
            "sigVerify":false,"replaceRecentBlockhash":false,"minContextSlot":40}])
        );
        assert_eq!(calls[1]["method"], "getFeeForMessage");
        assert_eq!(
            calls[1]["params"],
            json!([b64::encode(&bytes[65..]),{"commitment":"finalized","minContextSlot":40}])
        );
    }

    #[test]
    fn unsupported_envelopes_refuse_before_any_rpc() {
        let mut candidates = vec![vec![]];
        for size in [65, MAX_TRANSACTION_BYTES + 1, MAX_TRANSACTION_BYTES + 2] {
            let mut bad = bytes();
            bad.resize(size, 0);
            candidates.push(bad);
        }
        for (index, value) in [
            (0, 0),
            (0, 2),
            (0, 129),
            (1, 1),
            (64, 1),
            (65, 0),
            (65, 2),
            (65, 128),
        ] {
            let mut bad = bytes();
            bad[index] = value;
            candidates.push(bad);
        }
        for bad in candidates {
            let (rpc, calls) = client(vec![]);
            assert_eq!(
                rpc.transaction_preflight(
                    &bad,
                    40,
                    &mut Budget::new(2, 0, Duration::from_secs(20))
                ),
                Err("transaction-read needs a single unsigned legacy transaction")
            );
            assert_eq!(*calls.lock().expect("calls"), Vec::<Value>::new());
        }
        // Local framing only; the real RPC must still validate these messages.
        for size in [66, MAX_TRANSACTION_BYTES] {
            let mut accepted = bytes();
            accepted.resize(size, 0);
            assert!(read(vec![simulation(), fee()], &accepted).is_ok());
        }
    }

    #[test]
    fn absent_failed_replaced_or_malformed_simulation_never_produces_evidence() {
        let mut missing = simulation();
        missing["result"]["value"]
            .as_object_mut()
            .expect("value")
            .remove("err");
        let mut bad = vec![
            missing,
            json!({"error":{"message":"private node detail"}}),
            Value::Null,
        ];
        for value in [
            json!(false),
            json!("failure"),
            json!({"InstructionError":[0,"failure"]}),
        ] {
            let mut answer = simulation();
            answer["result"]["value"]["err"] = value;
            bad.push(answer);
        }
        let mut replaced = simulation();
        replaced["result"]["value"]["replacementBlockhash"] = json!({"blockhash":"replacement"});
        bad.push(replaced);
        for answer in bad {
            let (rpc, calls) = client(vec![answer]);
            let error = rpc
                .transaction_preflight(
                    &bytes(),
                    40,
                    &mut Budget::new(2, 0, Duration::from_secs(20)),
                )
                .expect_err("refuse");
            assert!(!error.contains("private"));
            assert_eq!(calls.lock().expect("calls").len(), 1);
        }
    }

    #[test]
    fn both_contexts_must_meet_the_requested_slot_and_fee_must_be_known() {
        for index in 0..2 {
            for context in [
                Value::Null,
                json!({}),
                json!({"slot":39}),
                json!({"slot":-1}),
                json!({"slot":"40"}),
            ] {
                let mut answers = vec![simulation(), fee()];
                answers[index]["result"]["context"] = context;
                assert!(read(answers, &bytes()).is_err());
            }
        }
        for bad in [
            json!({"result":{"context":{"slot":41}}}),
            json!({"result":{"context":{"slot":41},"value":null}}),
            json!({"result":{"context":{"slot":41},"value":"5000"}}),
            json!({"error":{"message":"private fee detail"}}),
        ] {
            assert!(
                !read(vec![simulation(), bad], &bytes())
                    .expect_err("unknown fee")
                    .contains("private")
            );
        }
        let mut zero = fee();
        zero["result"]["value"] = json!(0);
        assert_eq!(
            read(vec![simulation(), zero], &bytes()).expect("measured zero")["network_fee"]["lamports"],
            "0"
        );
    }

    #[test]
    fn optional_units_are_unknown_and_bad_units_or_budget_refuse() {
        for units in [None, Some(Value::Null), Some(json!(0))] {
            let mut answer = simulation();
            let value = answer["result"]["value"].as_object_mut().expect("value");
            value.remove("unitsConsumed");
            if let Some(units) = &units {
                value.insert("unitsConsumed".into(), units.clone());
            }
            value.insert("replacementBlockhash".into(), Value::Null);
            let output = read(vec![answer, fee()], &bytes()).expect("optional");
            assert_eq!(
                output["simulation"]["units_consumed"],
                if units == Some(json!(0)) {
                    json!("0")
                } else {
                    Value::Null
                }
            );
        }
        for units in [json!(-1), json!("0"), json!(false)] {
            let mut answer = simulation();
            answer["result"]["value"]["unitsConsumed"] = units;
            assert_eq!(
                read(vec![answer], &bytes()),
                Err("invalid simulation units")
            );
        }
        let (rpc, _) = client(vec![simulation(), fee()]);
        for calls in [0, 1] {
            assert!(
                rpc.transaction_preflight(
                    &bytes(),
                    40,
                    &mut Budget::new(calls, 0, Duration::from_secs(20))
                )
                .is_err()
            );
        }
    }
}
