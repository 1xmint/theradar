// SPDX-License-Identifier: Apache-2.0
//! Direct wallet evidence with optional independently read native SOL valuation.
//! Enumeration and raw token verification are not an atomic wallet snapshot.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use radar_onchain::rpc::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID, TokenAccountsRead};
use radar_onchain::{Budget, RpcClient, sol_price};
use radar_types::{Address, Slot};
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
    inventory(rpc, wallet, budget).map(|(evidence, _)| evidence)
}

fn inventory(
    rpc: &RpcClient,
    wallet: Address,
    budget: &mut Budget,
) -> Result<(Value, radar_onchain::rpc::Balance), String> {
    let balance = rpc
        .balance(budget, &wallet)
        .map_err(|_| "native SOL read failed")?;
    let tokens = rpc
        .token_accounts_by_owner(budget, &wallet, TOKEN_PROGRAM_ID)
        .map_err(|_| "SPL token read failed")?;
    let extended = rpc
        .token_accounts_by_owner(budget, &wallet, TOKEN_2022_PROGRAM_ID)
        .map_err(|_| "Token-2022 read failed")?;
    let verification = radar_onchain::wallet_inventory::read(
        rpc,
        budget,
        wallet,
        balance.slot,
        &tokens,
        &extended,
    )?;
    let tokens = token_read(tokens)?;
    let extended = token_read(extended)?;
    let slot = balance.slot.get().to_string();
    let coherent = (tokens["slot"] == slot
        && extended["slot"] == slot
        && (verification["slot"].is_null() || verification["slot"] == slot))
        .then_some(&slot);
    Ok((
        json!({
            "version":1, "wallet":wallet.to_string(), "commitment":"finalized",
            "native_sol":{"slot":slot, "raw_amount":balance.lamports.to_string(), "decimals":9},
            "token_program":tokens, "token_2022":extended, "common_reported_slot":coherent,
            "raw_token_verification":verification,
            "usd_value":null, "realised_pnl":null, "authority":"read_only"
        }),
        balance,
    ))
}

// Native SOL only: token marks, cost basis, pending claims and operating
// reserves are not inferred from this band. Confidence is not an exit guarantee.
fn native_value(lamports: u64, lower: u64, upper: u64) -> Result<Value, String> {
    let lower = u128::from(lamports) * u128::from(lower) / 1_000_000_000;
    let upper = (u128::from(lamports) * u128::from(upper)).div_ceil(1_000_000_000);
    let lower = u64::try_from(lower).map_err(|_| "native SOL value exceeds dollar range")?;
    let upper = u64::try_from(upper).map_err(|_| "native SOL value exceeds dollar range")?;
    Ok(json!({"lower_micro_usd":lower,"upper_micro_usd":upper}))
}

fn valued(
    rpc: &RpcClient,
    wallet: Address,
    budget: &mut Budget,
    bounds: &sol_price::Bounds,
) -> Result<Value, String> {
    let (mut evidence, balance) = inventory(rpc, wallet, budget)?;
    if balance.slot < bounds.minimum_slot {
        return Err("native balance precedes valuation context floor".into());
    }
    let observation = sol_price::read(rpc, budget, bounds)?;
    // Require a later bank read, without requiring an oracle posting in the
    // same slot as the wallet (the feed has its own update cadence).
    if observation.context_slot < balance.slot {
        return Err("oracle bank context precedes native balance".into());
    }
    evidence["native_sol"]["usd_value"] = native_value(
        balance.lamports,
        observation.lower_micro_usd_per_sol,
        observation.upper_micro_usd_per_sol,
    )?;
    evidence["sol_price"] = json!({"source":"pyth_receiver_full","account":sol_price::ACCOUNT,
        "observation":observation});
    evidence["wallet_inventory_complete"] = json!(false);
    evidence["usable_capital_micro_usd"] = Value::Null;
    evidence["execution_guaranteed"] = json!(false);
    Ok(evidence)
}

fn check_window(started: u64, completed: u64, maximum_age: u64) -> Result<(), String> {
    if completed
        .checked_sub(started)
        .is_none_or(|age| age > maximum_age)
    {
        return Err("wallet valuation read window is stale or clock moved backwards".into());
    }
    Ok(())
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
    let valuation = if args.iter().any(|arg| arg == "--value-sol") {
        Some(sol_price::Bounds {
            minimum_slot: Slot(
                crate::flag(args, "--min-slot")
                    .and_then(|v| v.parse().ok())
                    .ok_or("wallet-read --value-sol needs --min-slot <u64>")?,
            ),
            maximum_age_secs: crate::flag(args, "--max-age-secs")
                .and_then(|v| v.parse().ok())
                .ok_or("wallet-read --value-sol needs --max-age-secs <u64>")?,
            maximum_confidence_bps: crate::flag(args, "--max-confidence-bps")
                .and_then(|v| v.parse::<u32>().ok())
                .filter(|v| *v <= 10_000)
                .ok_or("wallet-read --value-sol needs --max-confidence-bps <0..10000>")?,
        })
    } else {
        None
    };
    let started = now()?;
    // Three inventory calls, at most one raw batch, and an optional oracle read.
    let mut budget = Budget::new(5, 0, Duration::from_secs(20));
    let rpc = RpcClient::new(endpoint);
    let mut evidence = match &valuation {
        Some(bounds) => valued(&rpc, wallet, &mut budget, bounds)?,
        None => read(&rpc, wallet, &mut budget)?,
    };
    let completed = now()?;
    if let Some(bounds) = valuation {
        check_window(started, completed, bounds.maximum_age_secs)?;
    }
    evidence["read_started_at_unix_secs"] = json!(started);
    evidence["read_completed_at_unix_secs"] = json!(completed);
    println!("{evidence}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_onchain::rpc::Transport;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    #[test]
    fn native_value_rounds_outward_without_floats_or_saturation() {
        for (lamports, low, high, expected_low, expected_high) in [
            (0, 2, 3, 0_u64, 0_u64),
            (1, 999_999_999, 1_000_000_001, 0, 2),
            (1_000_000_000, 2, 3, 2, 3),
            (3_000_000_001, 2, 3, 6, 10),
            (u64::MAX, 1, 2, 18_446_744_073, 36_893_488_148),
        ] {
            assert_eq!(
                native_value(lamports, low, high).unwrap(),
                json!({"lower_micro_usd":expected_low,"upper_micro_usd":expected_high})
            );
        }
        for lower in [0, u64::MAX] {
            assert!(native_value(u64::MAX, lower, u64::MAX).is_err());
        }
    }

    #[test]
    fn valuation_window_accepts_exact_age_and_refuses_stale_or_backwards_clock() {
        for (start, end, maximum) in [(100, 100, 0), (100, 101, 1), (100, 120, 20)] {
            assert_eq!(check_window(start, end, maximum), Ok(()));
        }
        for (start, end, maximum) in [(100, 101, 0), (100, 121, 20), (100, 99, u64::MAX)] {
            assert!(check_window(start, end, maximum).is_err());
        }
    }

    #[test]
    fn native_valuation_requires_explicit_bounds_before_network_reads() {
        let wallet = wallet().to_string();
        let args = [
            "--wallet",
            &wallet,
            "--rpc",
            "http://127.0.0.1:1",
            "--value-sol",
            "--min-slot",
            "50",
            "--max-age-secs",
            "60",
            "--max-confidence-bps",
            "100",
        ];
        for length in [5, 7, 9] {
            let args: Vec<_> = args[..length].iter().map(|v| (*v).to_owned()).collect();
            assert!(
                run(&args)
                    .unwrap_err()
                    .contains("wallet-read --value-sol needs")
            );
        }
        for (index, invalid) in [(6, "bad"), (8, "bad"), (10, "10001")] {
            let mut damaged = args;
            damaged[index] = invalid;
            let args: Vec<_> = damaged.iter().map(|v| (*v).to_owned()).collect();
            assert!(
                run(&args)
                    .unwrap_err()
                    .contains("wallet-read --value-sol needs")
            );
        }
    }

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
        let mint = if program == TOKEN_PROGRAM_ID {
            0x22
        } else {
            0x33
        };
        json!({"pubkey":Address::new([identity;32]).to_string(),"account":{"owner":program,
            "data":{"parsed":{"info":{"mint":Address::new([mint;32]).to_string(),
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
        Budget::new(4, 0, Duration::from_secs(20))
    }

    fn raw(slot: u64, values: &[(bool, u8, u64, u8)]) -> String {
        let accounts: Vec<_> = values
            .iter()
            .map(|&(mint, identity, amount, state)| {
                let extended = identity == 0x33 || identity == 3;
                let mut bytes = vec![0; if mint { 82 } else { 165 }];
                if mint {
                    bytes[36..44].copy_from_slice(&amount.to_le_bytes());
                    bytes[44] = 6;
                    bytes[45] = 1;
                } else {
                    bytes[..32].copy_from_slice(&[if extended { 0x33 } else { 0x22 }; 32]);
                    bytes[32..64].copy_from_slice(&[0x55; 32]);
                    bytes[64..72].copy_from_slice(&amount.to_le_bytes());
                    bytes[108] = state;
                }
                json!({"owner":if extended {TOKEN_2022_PROGRAM_ID} else {TOKEN_PROGRAM_ID},
                "data":[radar_types::b64::encode(&bytes),"base64"]})
            })
            .collect();
        token(json!({"slot":slot}), json!(accounts))
    }

    #[test]
    fn exact_quantities_and_each_program_slot_survive_finalized_reads() {
        let (rpc, calls) = client(vec![
            balance(),
            token(
                json!({"slot":40}),
                json!([
                    account("18446744073709551614", TOKEN_PROGRAM_ID, 1),
                    account("1", TOKEN_PROGRAM_ID, 2)
                ]),
            ),
            token(
                json!({"slot":40}),
                json!([account("2", TOKEN_2022_PROGRAM_ID, 3)]),
            ),
            raw(
                40,
                &[
                    (false, 1, u64::MAX - 1, 1),
                    (false, 2, 1, 1),
                    (false, 3, 2, 1),
                    (true, 0x22, u64::MAX, 1),
                    (true, 0x33, 2, 1),
                ],
            ),
        ]);
        let output = read(&rpc, wallet(), &mut budget()).expect("complete");
        assert_eq!(output["native_sol"]["raw_amount"], u64::MAX.to_string());
        assert_eq!(output["native_sol"]["decimals"], 9);
        assert_eq!(
            output["token_program"]["accounts"][0]["raw_amount"],
            "18446744073709551614"
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
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[3]["method"], "getMultipleAccounts");
        assert_eq!(
            calls[3]["params"],
            json!([
            ([1u8,2,3,0x22,0x33].map(|byte| Address::new([byte;32]).to_string())),
            {"encoding":"base64","commitment":"finalized"}])
        );
        assert_eq!(output["raw_token_verification"]["slot"], "40");
        assert_eq!(
            output["raw_token_verification"]["inventory_complete"],
            false
        );
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
            Err("duplicate raw token account identity".into())
        );
        let (rpc, _) = client(vec![
            balance(),
            token(json!({"slot":40}), json!([frozen])),
            token(json!({"slot":40}), json!([])),
            raw(41, &[(false, 1, 10, 2), (true, 0x22, 10, 1)]),
        ]);
        let output =
            read(&rpc, wallet(), &mut budget()).expect("frozen holding, no spendability claim");
        assert_eq!(output["token_program"]["accounts"][0]["state"], "frozen");
        assert_eq!(output["token_program"]["accounts"][0]["raw_amount"], "10");
        assert!(output["token_program"]["accounts"][0]["spendable"].is_null());
        assert_eq!(
            output["raw_token_verification"]["accounts"][0]["state"],
            "frozen"
        );
        assert_eq!(output["raw_token_verification"]["slot"], "41");
        assert!(output["common_reported_slot"].is_null());
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
