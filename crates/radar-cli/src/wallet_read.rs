// SPDX-License-Identifier: Apache-2.0
//! Direct wallet evidence, before any valuation or kernel portfolio is inferred.
//! Three reads are not an atomic snapshot; their individual slots survive output.

use std::{
    collections::HashSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use radar_onchain::rpc::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID, TokenAccountsRead};
use radar_onchain::{Budget, RpcClient};
use radar_types::Address;
use serde_json::{Value, json};

fn token_read(value: TokenAccountsRead) -> Result<Value, String> {
    let slot = value.slot.ok_or("token read has no context slot")?;
    let accounts: Vec<Value> = value
        .accounts
        .into_iter()
        .map(|account| {
            json!({"address":account.address.to_string(), "program":account.program.to_string(),
                "state":account.state, "mint":account.mint.to_string(),
                "raw_amount":account.amount.to_string(), "decimals":account.decimals,
                "spendable":null})
        })
        .collect();
    // Keep accounts separate: duplicated mints need verified common decimals
    // before summing. No UI float or dollar valuation enters this evidence.
    Ok(json!({"slot":slot.get().to_string(), "accounts":accounts}))
}

pub(super) fn read(rpc: &RpcClient, wallet: Address, budget: &mut Budget) -> Result<Value, String> {
    let balance = rpc
        .balance(budget, &wallet)
        .map_err(|_| "native SOL read failed")?;
    let tokens = rpc
        .token_accounts_by_owner(budget, &wallet, TOKEN_PROGRAM_ID)
        .map_err(|_| "SPL token read failed")?;
    let extended = rpc
        .token_accounts_by_owner(budget, &wallet, TOKEN_2022_PROGRAM_ID)
        .map_err(|_| "Token-2022 read failed")?;
    let mut seen = HashSet::new();
    for account in tokens.accounts.iter().chain(&extended.accounts) {
        if !seen.insert(account.address) {
            return Err("token account repeated across program reads".into());
        }
    }
    let tokens = token_read(tokens)?;
    let extended = token_read(extended)?;
    let slot = balance.slot.get().to_string();
    let coherent = (tokens["slot"] == slot && extended["slot"] == slot).then_some(&slot);
    Ok(json!({
        "version":1, "wallet":wallet.to_string(), "commitment":"finalized",
        "native_sol":{"slot":slot, "raw_amount":balance.lamports.to_string(), "decimals":9},
        "token_program":tokens, "token_2022":extended, "common_reported_slot":coherent,
        "usd_value":null, "realised_pnl":null, "authority":"read_only"
    }))
}

pub(super) fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| "host clock unavailable".into())
}

pub fn run(args: &[String]) -> Result<(), String> {
    let wallet: Address = crate::flag(args, "--wallet")
        .ok_or("wallet-read needs --wallet <address>")?
        .parse()
        .map_err(|_| "wallet-read needs a valid wallet address")?;
    let endpoint = crate::flag(args, "--rpc")
        .filter(|value| !value.trim().is_empty())
        .ok_or("wallet-read needs an explicit --rpc <URL>")?;
    let started = now()?;
    // Exactly one call per read. This is a read deadline, not a trading policy.
    let mut budget = Budget::new(3, 0, Duration::from_secs(20));
    let mut evidence = read(&RpcClient::new(endpoint), wallet, &mut budget)?;
    evidence["read_started_at_unix_secs"] = json!(started);
    evidence["read_completed_at_unix_secs"] = json!(now()?);
    println!("{evidence}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_onchain::rpc::Transport;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    struct Fixture {
        answers: Mutex<VecDeque<String>>,
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
                .ok_or("unexpected call".into())
        }
    }
    fn wallet() -> Address {
        Address::new([0x55; 32])
    }
    fn token(slot: Value, accounts: Value) -> String {
        let result =
            serde_json::Map::from_iter([("context".into(), slot), ("value".into(), accounts)]);
        json!({"result":result}).to_string()
    }
    fn account(amount: &str, program: &str, identity: u8) -> Value {
        json!({"pubkey":Address::new([identity;32]).to_string(),"account":{"owner":program,
            "data":{"parsed":{"info":{"mint":Address::new([0x22;32]).to_string(),
            "owner":wallet().to_string(), "state":"initialized", "tokenAmount":{"amount":amount, "decimals":6}}}}}})
    }
    fn client(answers: Vec<String>) -> (RpcClient, Arc<Mutex<Vec<Value>>>) {
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
    fn balance() -> String {
        json!({"result":{"context":{"slot":40},"value":u64::MAX}}).to_string()
    }
    fn budget() -> Budget {
        Budget::new(3, 0, Duration::from_secs(20))
    }

    #[test]
    fn exact_quantities_and_each_program_slot_survive_finalized_reads() {
        let (rpc, calls) = client(vec![
            balance(),
            token(
                json!({"slot":40}),
                json!([
                    account("18446744073709551615", TOKEN_PROGRAM_ID, 1),
                    account("1", TOKEN_PROGRAM_ID, 2)
                ]),
            ),
            token(
                json!({"slot":40}),
                json!([account("2", TOKEN_2022_PROGRAM_ID, 3)]),
            ),
        ]);
        let output = read(&rpc, wallet(), &mut budget()).expect("complete");
        assert_eq!(output["native_sol"]["raw_amount"], u64::MAX.to_string());
        assert_eq!(output["native_sol"]["decimals"], 9);
        assert_eq!(
            output["token_program"]["accounts"][0]["raw_amount"],
            "18446744073709551615"
        );
        assert_eq!(
            output["token_program"]["accounts"]
                .as_array()
                .expect("accounts")
                .len(),
            2
        );
        assert_eq!(
            output["token_program"]["accounts"][0]["mint"],
            Address::new([0x22; 32]).to_string()
        );
        assert_eq!(output["token_program"]["accounts"][0]["decimals"], 6);
        for (section, program, identity) in [
            ("token_program", TOKEN_PROGRAM_ID, 1),
            ("token_2022", TOKEN_2022_PROGRAM_ID, 3),
        ] {
            assert_eq!(
                output[section]["accounts"][0]["address"],
                Address::new([identity; 32]).to_string()
            );
            assert_eq!(output[section]["accounts"][0]["program"], program);
            assert_eq!(output[section]["accounts"][0]["state"], "initialized");
            assert!(output[section]["accounts"][0]["spendable"].is_null());
        }
        assert_eq!(output["token_2022"]["accounts"][0]["raw_amount"], "2");
        assert_eq!(output["common_reported_slot"], "40");
        assert_eq!(output["wallet"], wallet().to_string());
        assert_eq!(output["commitment"], "finalized");
        assert_eq!(output["authority"], "read_only");
        assert!(output["usd_value"].is_null());
        assert!(output["realised_pnl"].is_null());
        let calls = calls.lock().expect("calls");
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0]["method"], "getBalance");
        assert_eq!(
            calls[0]["params"],
            json!([wallet().to_string(),{"commitment":"finalized"}])
        );
        for (index, program) in [(1, TOKEN_PROGRAM_ID), (2, TOKEN_2022_PROGRAM_ID)] {
            assert_eq!(calls[index]["method"], "getTokenAccountsByOwner");
            assert_eq!(
                calls[index]["params"],
                json!([wallet().to_string(),{"programId":program},{"encoding":"jsonParsed","commitment":"finalized"}])
            );
        }
    }

    #[test]
    fn repeated_identity_across_program_reads_refuses_and_frozen_balance_survives() {
        let mut frozen = account("10", TOKEN_PROGRAM_ID, 1);
        frozen["account"]["data"]["parsed"]["info"]["state"] = json!("frozen");
        let (rpc, _) = client(vec![
            balance(),
            token(json!({"slot":40}), json!([frozen.clone()])),
            token(
                json!({"slot":41}),
                json!([account("2", TOKEN_2022_PROGRAM_ID, 1)]),
            ),
        ]);
        assert_eq!(
            read(&rpc, wallet(), &mut budget()),
            Err("token account repeated across program reads".into())
        );
        let (rpc, _) = client(vec![
            balance(),
            token(json!({"slot":40}), json!([frozen])),
            token(json!({"slot":40}), json!([])),
        ]);
        let output =
            read(&rpc, wallet(), &mut budget()).expect("frozen holding, no spendability claim");
        assert_eq!(output["token_program"]["accounts"][0]["state"], "frozen");
        assert_eq!(output["token_program"]["accounts"][0]["raw_amount"], "10");
        assert!(output["token_program"]["accounts"][0]["spendable"].is_null());
    }

    #[test]
    fn separate_slots_never_become_an_atomic_snapshot() {
        for (legacy, extended) in [(41, 40), (40, 41), (41, 42)] {
            let (rpc, _) = client(vec![
                balance(),
                token(json!({"slot":legacy}), json!([])),
                token(json!({"slot":extended}), json!([])),
            ]);
            let output = read(&rpc, wallet(), &mut budget()).expect("measured window");
            assert!(output["common_reported_slot"].is_null());
            assert_eq!(output["native_sol"]["slot"], "40");
            assert_eq!(output["token_program"]["slot"], legacy.to_string());
            assert_eq!(output["token_2022"]["slot"], extended.to_string());
        }
    }

    #[test]
    fn missing_failed_or_malformed_reads_never_become_zero_holdings() {
        let good = token(json!({"slot":40}), json!([]));
        for bad in [
            json!({"error":{"message":"secret endpoint detail"}}).to_string(),
            token(Value::Null, json!([])),
            token(json!({"slot":40}), Value::Null),
            token(
                json!({"slot":40}),
                json!([account("not-an-integer", TOKEN_PROGRAM_ID, 1)]),
            ),
        ] {
            for index in 0..3 {
                let mut answers = vec![balance(), good.clone(), good.clone()];
                answers[index] = bad.clone();
                let (rpc, _) = client(answers);
                let error = read(&rpc, wallet(), &mut budget()).expect_err("unknown");
                assert!(!error.contains("secret"));
            }
        }
    }

    #[test]
    fn arguments_refuse_before_network_when_wallet_or_endpoint_is_missing() {
        for (args, expected) in [
            (vec![], "wallet-read needs --wallet <address>"),
            (
                vec!["--wallet", "invalid"],
                "wallet-read needs a valid wallet address",
            ),
            (
                vec!["--wallet", &wallet().to_string()],
                "wallet-read needs an explicit --rpc <URL>",
            ),
            (
                vec!["--wallet", &wallet().to_string(), "--rpc", " "],
                "wallet-read needs an explicit --rpc <URL>",
            ),
        ] {
            assert_eq!(
                run(&args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()),
                Err(expected.into())
            );
        }
    }
}
