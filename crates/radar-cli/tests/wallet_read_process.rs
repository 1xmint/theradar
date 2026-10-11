// SPDX-License-Identifier: Apache-2.0
//! The operator command emits measured evidence only after all reads succeed.

use serde_json::{Value, json};
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

fn sol_price_reads() -> Vec<Value> {
    let mut bytes = radar_types::b64::decode("IvEjY51+9M1gMUcENA3t3zcf1CRyFI8kjp0abRpesqw6zYt/1dayQwHvDYtv2izrpB2hXUCV0do5Kg0vjtDGx7wPTPrIwoC1bf82BJwCAAAALUcYAAAAAAD4////qsDLagAAAACpwMtqAAAAAFi/XZcCAAAAbTwuAAAAAACD0ykbAAAAAAA=").unwrap();
    bytes[93..101].copy_from_slice(&i64::try_from(now()).unwrap().to_le_bytes());
    bytes[125..133].copy_from_slice(&50_u64.to_le_bytes());
    vec![json!({"result":{"context":{"slot":51},"value":[{
        "owner":"rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ",
        "data":[radar_types::b64::encode(&bytes),"base64"]
    }]}})]
}

fn valued_wallet_command(endpoint: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_radar"))
        .args([
            "wallet-read",
            "--wallet",
            &radar_types::Address::new([0x55; 32]).to_string(),
            "--rpc",
            endpoint,
            "--value-sol",
            "--min-slot",
            "50",
            "--max-age-secs",
            "60",
            "--max-confidence-bps",
            "10000",
        ])
        .output()
        .unwrap()
}

fn portfolio_command(endpoint: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_radar"))
        .args([
            "portfolio-read",
            "--wallet",
            &radar_types::Address::new([0x55; 32]).to_string(),
            "--rpc",
            endpoint,
            "--min-slot",
            "50",
            "--max-age-secs",
            "60",
            "--max-confidence-bps",
            "10000",
        ])
        .output()
        .unwrap()
}

fn portfolio_price(context: u64, posted: u64) -> Value {
    let mut response = sol_price_reads().remove(0);
    response["result"]["context"]["slot"] = json!(context);
    let mut bytes =
        radar_types::b64::decode(response["result"]["value"][0]["data"][0].as_str().unwrap())
            .unwrap();
    bytes[125..133].copy_from_slice(&posted.to_le_bytes());
    response["result"]["value"][0]["data"][0] = json!(radar_types::b64::encode(&bytes));
    response
}

fn portfolio_wallet(
    mint: radar_types::Address,
    amount: u64,
    decimals: u8,
    state: u8,
) -> Vec<Value> {
    let mut answers = wallet_with_tokens();
    answers[0]["result"]["value"] = json!(1_000_000_000_u64);
    let parsed = &mut answers[1]["result"]["value"][0]["account"]["data"]["parsed"]["info"];
    parsed["mint"] = json!(mint);
    parsed["state"] = json!(if state == 1 { "initialized" } else { "frozen" });
    parsed["tokenAmount"] = json!({"amount":amount.to_string(),"decimals":decimals});
    let mut mint_bytes = vec![0; 82];
    let wrapped = mint == radar_types::Asset::WRAPPED_SOL_MINT;
    mint_bytes[36..44].copy_from_slice(&(if wrapped { 0_u64 } else { 1_000_000 }).to_le_bytes());
    mint_bytes[44] = decimals;
    mint_bytes[45] = 1;
    let mut token = vec![0; 165];
    token[..32].copy_from_slice(mint.as_bytes());
    token[32..64].copy_from_slice(&[0x55; 32]);
    token[64..72].copy_from_slice(&amount.to_le_bytes());
    token[108] = state;
    if wrapped {
        token[109..113].copy_from_slice(&1_u32.to_le_bytes());
        token[113..121].copy_from_slice(&99_u64.to_le_bytes());
    }
    let raw = |bytes: Vec<u8>| {
        json!({"owner":radar_onchain::rpc::TOKEN_PROGRAM_ID,
        "data":[radar_types::b64::encode(&bytes),"base64"]})
    };
    let values = if mint < radar_types::Address::new([0x44; 32]) {
        vec![raw(mint_bytes), raw(token)]
    } else {
        vec![raw(token), raw(mint_bytes)]
    };
    answers[3]["result"]["value"] = json!(values);
    answers
}

#[test]
fn portfolio_command_marks_whole_curve_holding_and_preserves_unknown_historical_results() {
    let mut answers = portfolio_wallet(radar_types::Address::new([0x22; 32]), 1000, 6, 1);
    answers.push(curve_response());
    answers.push(portfolio_price(778, 777));
    let (endpoint, server) = fixture(answers);
    let output = portfolio_command(&endpoint);
    assert!(output.status.success(), "{:?}", output.stderr);
    let packet: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        packet["observed_holdings_usd_band"],
        json!({"lower_micro_usd":115_375_075,
        "upper_micro_usd":115_407_839,"oldest_slot":50})
    );
    let rows = packet["holdings"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let position = rows
        .iter()
        .find(|row| row["holding"]["role"] == "position")
        .unwrap();
    assert_eq!(position["mark"]["raw_tokens"], "1000");
    assert_eq!(
        position["mark"]["net_lamports_at_observed_state"],
        "29595403"
    );
    assert_eq!(
        position["holding"]["value"]["known"],
        json!({"value":3_316_421,"as_of":53})
    );
    assert_eq!(
        position["upper_value"]["known"],
        json!({"value":3_317_363,"as_of":53})
    );
    assert_eq!(position["holding"]["cost"], json!({"unknown":"no_price"}));
    for field in [
        "wallet_inventory_complete",
        "inventory_atomic",
        "economic_reconciliation_complete",
        "execution_guaranteed",
    ] {
        assert_eq!(packet[field], false);
    }
    for field in [
        "realised_pnl",
        "equity_micro_usd",
        "usable_capital_micro_usd",
    ] {
        assert!(packet[field].is_null());
    }
    assert_eq!(packet["authority"], "read_only");
    assert_eq!(server.join().unwrap().len(), 6);
}

#[test]
fn portfolio_command_keeps_frozen_unpriced_and_zero_holdings_without_assuming_usdc_peg() {
    for kind in 0..5 {
        let mint = if kind == 3 {
            radar_types::Asset::USDC_MINT
        } else if kind == 4 {
            radar_types::Asset::WRAPPED_SOL_MINT
        } else {
            radar_types::Address::new([0x22; 32])
        };
        let amount = if kind == 2 {
            0
        } else if kind == 4 {
            2_000_000_000
        } else {
            1000
        };
        let mut answers = portfolio_wallet(
            mint,
            amount,
            if kind == 4 { 9 } else { 6 },
            if kind == 0 { 2 } else { 1 },
        );
        if kind == 1 {
            answers.push(json!({"result":{"context":{"slot":778},"value":[null,null,null]}}));
        }
        answers.push(portfolio_price(778, 777));
        let (endpoint, server) = fixture(answers);
        let output = portfolio_command(&endpoint);
        assert!(output.status.success(), "{:?}", output.stderr);
        let packet: Value = serde_json::from_slice(&output.stdout).unwrap();
        if kind <= 1 || kind == 3 {
            assert!(packet["observed_holdings_usd_band"].is_null());
            assert!(
                packet["holdings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["holding"]["value"]["unknown"] == "no_price")
            );
        } else {
            let (low, high) = if kind == 4 {
                (336_175_962, 336_271_428)
            } else {
                (112_058_654, 112_090_476)
            };
            assert_eq!(
                packet["observed_holdings_usd_band"],
                json!({"lower_micro_usd":low,"upper_micro_usd":high,"oldest_slot":50})
            );
        }
        assert_eq!(server.join().unwrap().len(), if kind == 1 { 6 } else { 5 });
    }
}

#[test]
fn portfolio_command_refuses_old_inventory_or_oracle_banks_without_partial_output() {
    for fault in 0..6 {
        let mut answers = if fault <= 2 {
            collected_reads()[..3].to_vec()
        } else {
            portfolio_wallet(
                radar_types::Address::new([0x22; 32]),
                1000,
                6,
                if fault == 3 { 2 } else { 1 },
            )
        };
        if fault <= 2 {
            answers[fault]["result"]["context"]["slot"] = json!(49);
        } else {
            if fault >= 4 {
                answers.push(curve_response());
            }
            answers.push(if fault == 5 {
                json!({"error":{"message":"private provider detail"}})
            } else {
                portfolio_price(if fault == 3 { 52 } else { 777 }, 50)
            });
        }
        let (endpoint, server) = fixture(answers);
        let output = portfolio_command(&endpoint);
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private provider detail"));
        assert_eq!(
            server.join().unwrap().len(),
            if fault <= 2 {
                3
            } else if fault == 3 {
                5
            } else {
                6
            }
        );
    }
}

#[test]
fn portfolio_marks_refuse_old_market_context_or_changed_mint_program_and_units() {
    for fault in 0..3 {
        let mut answers = portfolio_wallet(radar_types::Address::new([0x22; 32]), 1000, 6, 1);
        let mut market = curve_response();
        match fault {
            0 => market["result"]["context"]["slot"] = json!(52),
            1 => {
                market["result"]["value"][0]["owner"] =
                    json!(radar_pumpfun::token::TOKEN_2022_PROGRAM)
            }
            _ => {
                let mut mint = radar_types::b64::decode(
                    market["result"]["value"][0]["data"][0].as_str().unwrap(),
                )
                .unwrap();
                mint[44] = 7;
                market["result"]["value"][0]["data"][0] = json!(radar_types::b64::encode(&mint));
            }
        }
        answers.push(market);
        answers.push(portfolio_price(778, 777));
        let (endpoint, server) = fixture(answers);
        let output = portfolio_command(&endpoint);
        assert!(output.status.success(), "{:?}", output.stderr);
        let packet: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(packet["observed_holdings_usd_band"].is_null());
        let refused = packet["holdings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["holding"]["role"] == "position")
            .unwrap();
        assert_eq!(
            refused["mark"]["refusal"],
            "portfolio market context, program or units differ"
        );
        assert_eq!(server.join().unwrap().len(), 6);
    }
}

#[test]
fn native_valued_wallet_preserves_tokens_and_never_claims_equity_or_usable_capital() {
    for (tokens, lamports, balance_slot, lower, upper) in [
        (false, 0_u64, 50, 0, 0),
        (false, 1_000_000_001, 51, 112_058_654, 112_090_477),
        (true, 1_000_000_001, 50, 112_058_654, 112_090_477),
    ] {
        let mut answers = if tokens {
            wallet_with_tokens()
        } else {
            collected_reads()[..3].to_vec()
        };
        answers[0]["result"]["value"] = json!(lamports);
        answers[0]["result"]["context"]["slot"] = json!(balance_slot);
        answers.extend(sol_price_reads());
        let (endpoint, server) = fixture(answers);
        let output = valued_wallet_command(&endpoint);
        assert!(output.status.success(), "{:?}", output.stderr);
        let packet: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(packet["native_sol"]["raw_amount"], lamports.to_string());
        assert_eq!(packet["native_sol"]["slot"], balance_slot.to_string());
        assert_eq!(
            packet["native_sol"]["usd_value"],
            json!({"lower_micro_usd":lower,"upper_micro_usd":upper})
        );
        assert_eq!(packet["sol_price"]["observation"]["context_slot"], 51);
        assert_eq!(packet["sol_price"]["observation"]["posted_slot"], 50);
        assert_eq!(
            packet["sol_price"]["account"],
            radar_onchain::sol_price::ACCOUNT
        );
        assert_eq!(packet["sol_price"]["source"], "pyth_receiver_full");
        assert_eq!(packet["wallet_inventory_complete"], false);
        assert_eq!(packet["execution_guaranteed"], false);
        assert_eq!(packet["authority"], "read_only");
        assert!(packet["usd_value"].is_null());
        assert!(packet["realised_pnl"].is_null());
        assert!(packet["usable_capital_micro_usd"].is_null());
        if tokens {
            assert_eq!(packet["token_program"]["accounts"][0]["raw_amount"], "7");
            assert_eq!(packet["raw_token_verification"]["slot"], "53");
        }
        assert!(packet["common_reported_slot"].is_null());
        let calls = server.join().unwrap();
        assert_eq!(calls.len(), if tokens { 5 } else { 4 });
        assert_eq!(
            calls.last().unwrap()["params"][0],
            json!([radar_onchain::sol_price::ACCOUNT])
        );
    }
}

#[test]
fn native_valuation_refuses_old_bank_contexts_and_failed_prices_without_partial_output() {
    for fault in 0..5 {
        let mut answers = collected_reads()[..3].to_vec();
        if fault == 0 {
            answers[0]["result"]["context"]["slot"] = json!(49);
        } else {
            let mut price = sol_price_reads();
            match fault {
                1 => answers[0]["result"]["context"]["slot"] = json!(52),
                2 => price[0]["result"]["value"][0]["owner"] = json!("foreign"),
                3 => price[0] = json!({"error":{"message":"private provider detail"}}),
                _ => {
                    let mut bytes = radar_types::b64::decode(
                        price[0]["result"]["value"][0]["data"][0].as_str().unwrap(),
                    )
                    .unwrap();
                    bytes[93..101]
                        .copy_from_slice(&i64::try_from(now() - 61).unwrap().to_le_bytes());
                    price[0]["result"]["value"][0]["data"][0] =
                        json!(radar_types::b64::encode(&bytes));
                }
            }
            answers.extend(price);
        }
        let (endpoint, server) = fixture(answers);
        let output = valued_wallet_command(&endpoint);
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private provider detail"));
        assert_eq!(server.join().unwrap().len(), if fault == 0 { 3 } else { 4 });
    }
}

#[test]
fn actual_sol_price_command_reads_receiver_verified_bounds_and_refuses_missing_scope() {
    let (endpoint, server) = fixture(sol_price_reads());
    let args = [
        "sol-price-read",
        "--rpc",
        &endpoint,
        "--min-slot",
        "50",
        "--max-age-secs",
        "60",
        "--max-confidence-bps",
        "100",
    ];
    let output = Command::new(env!("CARGO_BIN_EXE_radar"))
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let evidence: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(evidence["authority"], "read_only");
    assert_eq!(evidence["source"], "pyth_receiver_full");
    assert_eq!(evidence["wallet_inventory_complete"], false);
    assert_eq!(evidence["execution_guaranteed"], false);
    assert_eq!(
        evidence["observation"]["lower_micro_usd_per_sol"],
        112_058_654
    );
    assert_eq!(
        evidence["observation"]["upper_micro_usd_per_sol"],
        112_090_476
    );
    let calls = server.join().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["method"], "getMultipleAccounts");
    assert_eq!(
        calls[0]["params"],
        json!([[radar_onchain::sol_price::ACCOUNT],
        {"encoding":"base64","commitment":"finalized"}])
    );
    for length in [1, 3, 5, 7] {
        let failed = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args(&args[..length])
            .output()
            .unwrap();
        assert!(!failed.status.success());
        assert_eq!(failed.stdout, Vec::<u8>::new());
        assert!(String::from_utf8_lossy(&failed.stderr).contains("sol-price-read needs"));
    }
    for (index, invalid) in [(2, " "), (4, "bad"), (6, "bad"), (8, "10001")] {
        let mut damaged = args;
        damaged[index] = invalid;
        let failed = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args(damaged)
            .output()
            .unwrap();
        assert!(!failed.status.success());
        assert_eq!(failed.stdout, Vec::<u8>::new());
        assert!(String::from_utf8_lossy(&failed.stderr).contains("sol-price-read needs"));
    }
}

#[test]
fn sol_price_command_publishes_private_complete_packets_and_retains_previous_on_rpc_failure() {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = directory.path().join("price.json");
    let (endpoint, server) = fixture(sol_price_reads());
    let command = |endpoint: &str| {
        Command::new(env!("CARGO_BIN_EXE_radar"))
            .args([
                "sol-price-read",
                "--rpc",
                endpoint,
                "--min-slot",
                "50",
                "--max-age-secs",
                "60",
                "--max-confidence-bps",
                "100",
                "--output",
                path.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    let output = command(&endpoint);
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({"price_written":true,"authority":"read_only"})
    );
    assert_eq!(server.join().unwrap().len(), 1);
    let previous = std::fs::read(&path).unwrap();
    let packet: Value = serde_json::from_slice(&previous).unwrap();
    assert_eq!(
        packet["observation"]["upper_micro_usd_per_sol"],
        112_090_476
    );
    let (failed_endpoint, server) =
        fixture(vec![json!({"error":{"message":"private provider detail"}})]);
    let failed = command(&failed_endpoint);
    assert!(!failed.status.success());
    assert_eq!(failed.stdout, Vec::<u8>::new());
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("private provider detail"));
    assert_eq!(std::fs::read(&path).unwrap(), previous);
    assert_eq!(server.join().unwrap().len(), 1);
    let missing = Command::new(env!("CARGO_BIN_EXE_radar"))
        .args([
            "sol-price-read",
            "--rpc",
            "http://127.0.0.1:1",
            "--min-slot",
            "50",
            "--max-age-secs",
            "60",
            "--max-confidence-bps",
            "100",
            "--output",
        ])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(
        String::from_utf8_lossy(&missing.stderr)
            .contains("sol-price-read needs --output <private-file>")
    );
    assert_eq!(std::fs::read(&path).unwrap(), previous);
}

fn settlement_bytes() -> Vec<u8> {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&[0xAB; 64]);
    bytes.extend_from_slice(&[1, 0, 0, 1]);
    bytes.extend_from_slice(&[0x55; 32]);
    bytes.extend_from_slice(&[0xAA; 32]);
    bytes.push(0);
    bytes
}

fn settlement_result() -> Value {
    json!({"slot":50,"version":"legacy","transaction":[radar_types::b64::encode(&settlement_bytes()),"base64"],
        "blockTime":null,"meta":{"err":null,"fee":5000,"preBalances":[10000],"postBalances":[5000],
            "preTokenBalances":[],"postTokenBalances":[]}})
}

#[test]
fn address_activity_command_fetches_finalized_raw_failures_without_claiming_coverage() {
    for failed in [false, true] {
        let mut raw = settlement_result();
        let err = if failed {
            json!({"InstructionError":[0,1]})
        } else {
            Value::Null
        };
        raw["meta"]["err"] = err.clone();
        let signature = radar_types::Signature::new([0xAB; 64]).to_string();
        let (endpoint, server) = fixture(vec![
            json!({"result":[{"signature":signature,"slot":50,
            "err":err,"confirmationStatus":"finalized"}]}),
            json!({"result":raw}),
        ]);
        let before = now();
        let output = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args([
                "wallet-activity-read",
                "--wallet",
                &radar_types::Address::new([0x55; 32]).to_string(),
                "--after-slot",
                "49",
                "--through-slot",
                "50",
                "--rpc",
                &endpoint,
            ])
            .output()
            .expect("activity command");
        assert!(output.status.success(), "{:?}", output.stderr);
        let evidence: Value = serde_json::from_slice(&output.stdout).expect("activity evidence");
        assert_eq!(evidence["authority"], "read_only");
        assert_eq!(evidence["signature_scan_finished"], true);
        assert_eq!(evidence["transaction_fetch_finished"], true);
        assert_eq!(evidence["wallet_coverage_complete"], false);
        assert_eq!(evidence["economic_reconciliation_complete"], false);
        assert_eq!(
            evidence["transactions"][0]["outcome"],
            if failed { "failed" } else { "succeeded" }
        );
        assert_eq!(evidence["transactions"][0]["network_fee_lamports"], "5000");
        assert_eq!(evidence["transactions"][0]["classification"], "unresolved");
        assert!(evidence["read_started_at_unix_secs"].as_u64().unwrap() >= before);
        assert!(evidence["read_completed_at_unix_secs"].as_u64().unwrap() <= now());
        let calls = server.join().expect("activity RPC");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["method"], "getSignaturesForAddress");
        assert_eq!(calls[0]["params"][1]["commitment"], "finalized");
        assert_eq!(calls[0]["params"][1]["minContextSlot"], 50);
        assert_eq!(calls[1]["method"], "getTransaction");
        assert_eq!(calls[1]["params"][0], signature);
        assert_eq!(calls[1]["params"][1]["encoding"], "base64");
    }
}

fn settlement_command(endpoint: &str) -> std::process::Output {
    let mut file = tempfile::NamedTempFile::new().expect("signed input");
    file.write_all(&settlement_bytes()).expect("file");
    Command::new(env!("CARGO_BIN_EXE_radar"))
        .args([
            "settlement-read",
            "--wallet",
            &radar_types::Address::new([0x55; 32]).to_string(),
            "--transaction",
            file.path().to_str().expect("path"),
            "--min-slot",
            "50",
            "--rpc",
            endpoint,
        ])
        .output()
        .expect("command")
}

#[test]
fn settlement_command_preserves_effects_fees_and_read_window_without_reconciling() {
    for err in [Value::Null, json!({"InstructionError":[0,1]})] {
        let mut result = settlement_result();
        result["meta"]["err"] = err.clone();
        let (endpoint, server) = fixture(vec![json!({"result":result})]);
        let before = now();
        let output = settlement_command(&endpoint);
        assert!(output.status.success(), "{:?}", output.stderr);
        let evidence: Value = serde_json::from_slice(&output.stdout).expect("evidence");
        assert_eq!(
            evidence["transaction_base64"],
            radar_types::b64::encode(&settlement_bytes())
        );
        assert_eq!(
            evidence["outcome"],
            if err.is_null() { "succeeded" } else { "failed" }
        );
        assert_eq!(evidence["network_fee_lamports"], "5000");
        assert_eq!(evidence["pre_balances_lamports"], json!(["10000"]));
        assert_eq!(evidence["post_balances_lamports"], json!(["5000"]));
        assert_eq!(evidence["pre_token_balances"], json!([]));
        assert_eq!(evidence["post_token_balances"], json!([]));
        assert_eq!(evidence["operation_reconciled"], false);
        assert_eq!(evidence["authority"], "read_only");
        let start = evidence["read_started_at_unix_secs"]
            .as_u64()
            .expect("start");
        let end = evidence["read_completed_at_unix_secs"]
            .as_u64()
            .expect("end");
        assert!(before <= start && start <= end && end <= now());
        let calls = server.join().expect("server");
        assert_eq!(
            calls,
            vec![json!({"jsonrpc":"2.0","id":1,"method":"getTransaction",
            "params":[radar_types::Signature::new([0xAB;64]).to_string(),
                {"encoding":"base64","commitment":"finalized","maxSupportedTransactionVersion":0}]})]
        );
    }
}

#[test]
fn unknown_mismatched_or_incomplete_settlement_emits_no_partial_packet() {
    let mut mismatch = settlement_result();
    mismatch["transaction"][0] = json!("different bytes");
    let mut incomplete = settlement_result();
    incomplete["meta"]
        .as_object_mut()
        .expect("meta")
        .remove("err");
    for answer in [
        json!({"result":null}),
        json!({"result":mismatch}),
        json!({"result":incomplete}),
        json!({"error":{"message":"private provider detail"}}),
    ] {
        let (endpoint, server) = fixture(vec![answer]);
        let output = settlement_command(&endpoint);
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private provider detail"));
        assert_eq!(server.join().expect("server").len(), 1);
    }
}

fn collected_reads() -> Vec<Value> {
    vec![
        json!({"result":{"context":{"slot":50},"value":u64::MAX}}),
        json!({"result":{"context":{"slot":51},"value":[]}}),
        json!({"result":{"context":{"slot":52},"value":[]}}),
        json!({"result":{"context":{"slot":53},"value":{"err":null}}}),
        json!({"result":{"context":{"slot":54},"value":5000}}),
    ]
}

fn wallet_with_tokens() -> Vec<Value> {
    let wallet = radar_types::Address::new([0x55; 32]);
    let mint = radar_types::Address::new([0x22; 32]);
    let account = radar_types::Address::new([0x44; 32]);
    let program = radar_onchain::rpc::TOKEN_PROGRAM_ID;
    let mut mint_data = vec![0; 82];
    mint_data[36..44].copy_from_slice(&10u64.to_le_bytes());
    mint_data[44] = 6;
    mint_data[45] = 1;
    let mut account_data = vec![0; 165];
    account_data[..32].copy_from_slice(mint.as_bytes());
    account_data[32..64].copy_from_slice(wallet.as_bytes());
    account_data[64..72].copy_from_slice(&7u64.to_le_bytes());
    account_data[108] = 2;
    let mut answers = collected_reads()[..3].to_vec();
    answers[1]["result"]["value"] = json!([{"pubkey":account.to_string(),
        "account":{"owner":program,"data":{"parsed":{"info":{
            "mint":mint.to_string(),"owner":wallet.to_string(),"state":"frozen",
            "tokenAmount":{"amount":"7","decimals":6}}}}}}]);
    answers.push(json!({"result":{"context":{"slot":53},"value":[
        {"owner":program,"data":[radar_types::b64::encode(&mint_data),"base64"]},
        {"owner":program,"data":[radar_types::b64::encode(&account_data),"base64"]}]}}));
    answers
}

#[test]
fn actual_wallet_and_collector_require_raw_tokens_and_emit_no_partial_packet_on_failure() {
    let wallet = radar_types::Address::new([0x55; 32]).to_string();
    let (endpoint, server) = fixture(wallet_with_tokens());
    let output = Command::new(env!("CARGO_BIN_EXE_radar"))
        .args(["wallet-read", "--wallet", &wallet, "--rpc", &endpoint])
        .output()
        .expect("command");
    assert!(output.status.success(), "{:?}", output.stderr);
    let evidence: Value = serde_json::from_slice(&output.stdout).expect("evidence");
    assert_eq!(evidence["raw_token_verification"]["slot"], "53");
    assert_eq!(
        evidence["raw_token_verification"]["accounts"][0]["state"],
        "frozen"
    );
    assert_eq!(
        evidence["raw_token_verification"]["accounts"][0]["raw_amount"],
        "7"
    );
    assert_eq!(
        evidence["raw_token_verification"]["inventory_complete"],
        false
    );
    assert!(evidence["common_reported_slot"].is_null());
    let calls = server.join().expect("server");
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls[3]["params"],
        json!([
        [radar_types::Address::new([0x22;32]).to_string(),radar_types::Address::new([0x44;32]).to_string()],
        {"encoding":"base64","commitment":"finalized"}])
    );
    let mut combined = wallet_with_tokens();
    combined.extend_from_slice(&collected_reads()[3..]);
    let (endpoint, server) = fixture(combined);
    let output = collect(&endpoint);
    assert!(output.status.success(), "{:?}", output.stderr);
    let evidence: Value = serde_json::from_slice(&output.stdout).expect("combined");
    assert_eq!(
        evidence["wallet_evidence"]["raw_token_verification"]["accounts"][0]["raw_amount"],
        "7"
    );
    assert_eq!(server.join().expect("server").len(), 6);
    let mut old = wallet_with_tokens()[3].clone();
    old["result"]["context"]["slot"] = json!(51);
    let mut missing = wallet_with_tokens()[3].clone();
    missing["result"]["value"][0] = Value::Null;
    for bad in [
        old,
        missing,
        json!({"error":{"message":"private provider detail"}}),
    ] {
        let mut answers = wallet_with_tokens();
        answers[3] = bad;
        let (endpoint, server) = fixture(answers);
        let output = collect(&endpoint);
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private provider detail"));
        assert_eq!(server.join().expect("server").len(), 4);
    }
}

fn collect(endpoint: &str) -> std::process::Output {
    let mut file = tempfile::NamedTempFile::new().expect("input");
    let mut bytes = vec![0; 100];
    bytes[0] = 1;
    bytes[65] = 1;
    file.write_all(&bytes).expect("transaction");
    Command::new(env!("CARGO_BIN_EXE_radar"))
        .args([
            "evidence-read",
            "--wallet",
            &radar_types::Address::new([0x55; 32]).to_string(),
            "--transaction",
            file.path().to_str().expect("path"),
            "--min-slot",
            "50",
            "--rpc",
            endpoint,
        ])
        .output()
        .expect("collector")
}

#[test]
fn collector_emits_both_issuer_read_packets_with_exact_amounts_bytes_contexts_and_windows() {
    let (endpoint, server) = fixture(collected_reads());
    let before = now();
    let output = collect(&endpoint);
    assert!(output.status.success(), "{:?}", output.stderr);
    let evidence: Value = serde_json::from_slice(&output.stdout).expect("packet");
    let wallet = &evidence["wallet_evidence"];
    let transaction = &evidence["transaction_evidence"];
    assert_eq!(
        wallet["wallet"],
        radar_types::Address::new([0x55; 32]).to_string()
    );
    let mut expected = vec![0; 100];
    expected[0] = 1;
    expected[65] = 1;
    assert_eq!(
        transaction["transaction_base64"],
        radar_types::b64::encode(&expected)
    );
    assert_eq!(
        transaction["message_base64"],
        radar_types::b64::encode(&expected[65..])
    );
    assert_eq!(wallet["native_sol"]["raw_amount"], u64::MAX.to_string());
    assert_eq!(wallet["native_sol"]["slot"], "50");
    assert_eq!(wallet["token_program"]["slot"], "51");
    assert_eq!(wallet["token_2022"]["slot"], "52");
    assert_eq!(transaction["simulation"]["slot"], "53");
    assert_eq!(transaction["network_fee"]["slot"], "54");
    assert_eq!(transaction["network_fee"]["lamports"], "5000");
    assert!(wallet["usd_value"].is_null());
    assert!(wallet["realised_pnl"].is_null());
    assert!(transaction["rent_and_other_instruction_costs"].is_null());
    for read in [wallet, transaction] {
        assert_eq!(read["authority"], "read_only");
        assert_eq!(read["commitment"], "finalized");
        let start = read["read_started_at_unix_secs"].as_u64().expect("start");
        let end = read["read_completed_at_unix_secs"].as_u64().expect("end");
        assert!(before <= start && start <= end && end <= now());
    }
    assert!(
        wallet["read_completed_at_unix_secs"]
            .as_u64()
            .expect("wallet end")
            <= transaction["read_started_at_unix_secs"]
                .as_u64()
                .expect("transaction start")
    );
    let calls = server.join().expect("server");
    assert_eq!(calls.len(), 5);
    assert_eq!(calls[0]["method"], "getBalance");
    assert_eq!(calls[1]["method"], "getTokenAccountsByOwner");
    assert_eq!(calls[2]["method"], "getTokenAccountsByOwner");
    assert_eq!(calls[3]["method"], "simulateTransaction");
    assert_eq!(calls[4]["method"], "getFeeForMessage");
    assert_eq!(calls[3]["params"][0], transaction["transaction_base64"]);
    assert_eq!(calls[4]["params"][0], transaction["message_base64"]);
    assert_eq!(transaction["minimum_context_slot"], "50");
}

#[test]
fn collector_refuses_every_failed_read_without_emitting_a_partial_issuer_packet() {
    for index in 0..5 {
        let mut answers = collected_reads();
        answers.truncate(index + 1);
        answers[index] = json!({"error":{"message":"private provider detail"}});
        let (endpoint, server) = fixture(answers);
        let output = collect(&endpoint);
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private provider detail"));
        assert_eq!(server.join().expect("server").len(), index + 1);
    }
}

#[test]
fn actual_transaction_read_preserves_bytes_and_reports_separate_fee_and_simulation() {
    let (endpoint, server) = fixture(vec![
        json!({"result":{"context":{"slot":50},"value":{"err":null}}}),
        json!({"result":{"context":{"slot":51},"value":5000}}),
    ]);
    let mut bytes = vec![0; 100];
    bytes[0] = 1;
    bytes[65] = 1;
    bytes[99] = 7;
    let mut file = tempfile::NamedTempFile::new().expect("input");
    file.write_all(&bytes).expect("write");
    let before = now();
    let output = Command::new(env!("CARGO_BIN_EXE_radar"))
        .args([
            "transaction-read",
            "--transaction",
            file.path().to_str().expect("path"),
            "--min-slot",
            "50",
            "--rpc",
            &endpoint,
        ])
        .output()
        .expect("command");
    assert!(output.status.success(), "{:?}", output.stderr);
    let evidence: Value = serde_json::from_slice(&output.stdout).expect("evidence");
    assert_eq!(
        evidence["transaction_base64"],
        radar_types::b64::encode(&bytes)
    );
    assert_eq!(
        evidence["message_base64"],
        radar_types::b64::encode(&bytes[65..])
    );
    assert_eq!(
        evidence["network_fee"],
        json!({"slot":"51","lamports":"5000"})
    );
    assert_eq!(evidence["simulation"]["slot"], "50");
    assert!(evidence["simulation"]["units_consumed"].is_null());
    assert_eq!(evidence["authority"], "read_only");
    assert_eq!(evidence["execution_guaranteed"], false);
    assert!(evidence["rent_and_other_instruction_costs"].is_null());
    let started = evidence["read_started_at_unix_secs"]
        .as_u64()
        .expect("started");
    let completed = evidence["read_completed_at_unix_secs"]
        .as_u64()
        .expect("completed");
    assert!(before <= started && started <= completed && completed <= now());
    let calls = server.join().expect("server");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["method"], "simulateTransaction");
    assert_eq!(
        calls[0]["params"],
        json!([evidence["transaction_base64"],{
        "encoding":"base64","commitment":"finalized","sigVerify":false,
        "replaceRecentBlockhash":false,"minContextSlot":50}])
    );
    assert_eq!(calls[1]["method"], "getFeeForMessage");
    assert_eq!(
        calls[1]["params"],
        json!([evidence["message_base64"],{
        "commitment":"finalized","minContextSlot":50}])
    );
}

#[test]
fn actual_transaction_read_prints_no_partial_evidence_on_unknown_simulation_or_fee() {
    let good = json!({"result":{"context":{"slot":50},"value":{"err":null}}});
    for answers in [
        vec![json!({"result":{"context":{"slot":50},"value":{}}})],
        vec![
            good.clone(),
            json!({"result":{"context":{"slot":51},"value":null}}),
        ],
        vec![good, json!({"error":{"message":"private fee endpoint"}})],
    ] {
        let (endpoint, server) = fixture(answers);
        let mut bytes = vec![0; 100];
        bytes[0] = 1;
        bytes[65] = 1;
        let mut file = tempfile::NamedTempFile::new().expect("input");
        file.write_all(&bytes).expect("write");
        let output = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args([
                "transaction-read",
                "--transaction",
                file.path().to_str().expect("path"),
                "--min-slot",
                "50",
                "--rpc",
                &endpoint,
            ])
            .output()
            .expect("command");
        assert!(!output.status.success());
        assert_eq!(output.stdout, Vec::<u8>::new());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private"));
        server.join().expect("server");
    }
}

#[test]
fn transaction_file_limit_accepts_the_full_packet_and_refuses_one_extra_byte() {
    let mut bytes = vec![0; radar_onchain::preflight::MAX_TRANSACTION_BYTES];
    bytes[0] = 1;
    bytes[65] = 1;
    let last = bytes.len() - 1;
    bytes[last] = 7;
    let mut file = tempfile::NamedTempFile::new().expect("input");
    file.write_all(&bytes).expect("write");
    let (endpoint, server) = fixture(vec![
        json!({"result":{"context":{"slot":0},"value":{"err":null}}}),
        json!({"result":{"context":{"slot":0},"value":0}}),
    ]);
    let output = Command::new(env!("CARGO_BIN_EXE_radar"))
        .args([
            "transaction-read",
            "--transaction",
            file.path().to_str().expect("path"),
            "--min-slot",
            "0",
            "--rpc",
            &endpoint,
        ])
        .output()
        .expect("command");
    assert!(output.status.success(), "{:?}", output.stderr);
    let evidence: Value = serde_json::from_slice(&output.stdout).expect("evidence");
    assert_eq!(
        evidence["transaction_base64"],
        radar_types::b64::encode(&bytes)
    );
    server.join().expect("server");
    file.write_all(&[0]).expect("extra byte");
    let output = Command::new(env!("CARGO_BIN_EXE_radar"))
        .args([
            "transaction-read",
            "--transaction",
            file.path().to_str().expect("path"),
            "--min-slot",
            "0",
            "--rpc",
            "http://127.0.0.1:1",
        ])
        .output()
        .expect("command");
    assert!(!output.status.success());
    assert_eq!(output.stdout, Vec::<u8>::new());
    assert!(String::from_utf8_lossy(&output.stderr).contains("single unsigned legacy transaction"));
}

fn fixture(answers: Vec<Value>) -> (String, std::thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let endpoint = format!("http://{}", listener.local_addr().expect("address"));
    listener.set_nonblocking(true).expect("nonblocking");
    let handle = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut requests = Vec::new();
        for answer in answers {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "command omitted an RPC read");
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            };
            // Windows can inherit the listener's nonblocking mode. The accept
            // loop is bounded separately; request reads use the timeout below.
            stream
                .set_nonblocking(false)
                .expect("blocking request read");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            let mut reader = BufReader::new(stream.try_clone().expect("reader"));
            let mut length = None;
            loop {
                let mut line = String::new();
                assert_ne!(reader.read_line(&mut line).expect("header"), 0);
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = Some(value.trim().parse::<usize>().expect("length"));
                }
            }
            let mut body = vec![0; length.expect("body length")];
            reader.read_exact(&mut body).expect("body");
            requests.push(serde_json::from_slice(&body).expect("request"));
            let body = answer.to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).expect("response");
        }
        requests
    });
    (endpoint, handle)
}

#[test]
fn the_actual_command_reports_zero_only_after_three_reads_and_preserves_read_times() {
    let (endpoint, server) = fixture(vec![
        json!({"result":{"context":{"slot":50},"value":0}}),
        json!({"result":{"context":{"slot":51},"value":[]}}),
        json!({"result":{"context":{"slot":52},"value":[]}}),
    ]);
    let wallet = radar_types::Address::new([0x55; 32]).to_string();
    let before = now();
    let output = Command::new(env!("CARGO_BIN_EXE_radar"))
        .args(["wallet-read", "--wallet", &wallet, "--rpc", &endpoint])
        .output()
        .expect("command");
    let after = now();
    assert!(output.status.success(), "{:?}", output.stderr);
    let evidence: Value = serde_json::from_slice(&output.stdout).expect("JSON evidence");
    assert_eq!(evidence["version"], 1);
    assert_eq!(evidence["wallet"], wallet);
    assert_eq!(evidence["native_sol"]["raw_amount"], "0");
    assert!(evidence["common_reported_slot"].is_null());
    let started = evidence["read_started_at_unix_secs"]
        .as_u64()
        .expect("start");
    let completed = evidence["read_completed_at_unix_secs"]
        .as_u64()
        .expect("completion");
    assert!(before <= started && started <= completed && completed <= after);
    let calls = server.join().expect("RPC fixture");
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0]["method"], "getBalance");
    assert_eq!(calls[1]["method"], "getTokenAccountsByOwner");
    assert_eq!(calls[2]["method"], "getTokenAccountsByOwner");
}

#[test]
fn a_failed_program_read_exits_without_partial_json_or_provider_details() {
    let (endpoint, server) = fixture(vec![
        json!({"result":{"context":{"slot":50},"value":123}}),
        json!({"error":{"message":"provider-private-detail"}}),
    ]);
    let wallet = radar_types::Address::new([0x55; 32]).to_string();
    let output = Command::new(env!("CARGO_BIN_EXE_radar"))
        .args(["wallet-read", "--wallet", &wallet, "--rpc", &endpoint])
        .output()
        .expect("command");
    assert!(!output.status.success());
    assert_eq!(output.stdout.len(), 0);
    let error = String::from_utf8(output.stderr).expect("error");
    assert!(error.contains("SPL token read failed"));
    assert!(!error.contains("provider-private-detail"));
    assert_eq!(server.join().expect("RPC fixture").len(), 2);
}

fn curve_response() -> Value {
    let mut mint = vec![0; 82];
    mint[36..44].copy_from_slice(&1_000_000u64.to_le_bytes());
    mint[44] = 6;
    mint[45] = 1;
    let mut curve = radar_pumpfun::curve::DISCRIMINATOR.to_vec();
    for amount in [
        1_000_000u64,
        30_000_000_000,
        500_000,
        1_000_000_000,
        1_000_000,
    ] {
        curve.extend_from_slice(&amount.to_le_bytes());
    }
    curve.push(0);
    curve.extend_from_slice(&[0x33; 32]);
    let mut fees = radar_pumpfun::fees::FEE_CONFIG_DISCRIMINATOR.to_vec();
    fees.extend_from_slice(&[0; 33]);
    for fee in [0u64, 95, 30] {
        fees.extend_from_slice(&fee.to_le_bytes());
    }
    fees.extend_from_slice(&1u32.to_le_bytes());
    fees.extend_from_slice(&0u128.to_le_bytes());
    for fee in [0u64, 95, 30] {
        fees.extend_from_slice(&fee.to_le_bytes());
    }
    let accounts: Vec<Value> = [
        (mint,radar_pumpfun::token::SPL_TOKEN_PROGRAM),
        (curve,radar_pumpfun::pda::PROGRAM_ID),
        (fees,radar_pumpfun::pda::FEE_PROGRAM),
    ].into_iter().map(|(data,owner)|json!({"data":[radar_types::b64::encode(&data),"base64"],"owner":owner.to_string()})).collect();
    json!({"result":{"context":{"slot":778},"value":accounts}})
}

#[test]
fn the_curve_exit_command_reads_only_the_derived_accounts_at_one_finalized_context() {
    let (endpoint, server) = fixture(vec![curve_response()]);
    let mint = radar_types::Address::new([0x22; 32]);
    let before = now();
    let output = Command::new(env!("CARGO_BIN_EXE_radar"))
        .args([
            "curve-exit",
            "--mint",
            &mint.to_string(),
            "--raw-tokens",
            "1000",
            "--rpc",
            &endpoint,
        ])
        .output()
        .expect("command");
    let after = now();
    assert!(output.status.success(), "{:?}", output.stderr);
    let quote: Value = serde_json::from_slice(&output.stdout).expect("JSON quote");
    assert_eq!(quote["slot"], "778");
    assert_eq!(quote["mint"], mint.to_string());
    assert_eq!(quote["gross_lamports"], "29970030");
    assert_eq!(quote["venue_fee_upper_lamports"], "374627");
    assert_eq!(quote["net_lamports_at_observed_state"], "29595403");
    assert_eq!(quote["raw_tokens"], "1000");
    assert_eq!(quote["authority"], "read_only");
    let started = quote["read_started_at_unix_secs"].as_u64().expect("start");
    let completed = quote["read_completed_at_unix_secs"]
        .as_u64()
        .expect("completion");
    assert!(before <= started && started <= completed && completed <= after);
    let calls = server.join().expect("RPC fixture");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["method"], "getMultipleAccounts");
    assert_eq!(
        calls[0]["params"],
        json!([
            [mint.to_string(),radar_pumpfun::pda::bonding_curve(&mint).expect("curve").to_string(),radar_pumpfun::pda::fee_config().expect("fees").to_string()],
            {"encoding":"base64","commitment":"finalized"}
        ])
    );
}

#[test]
fn the_curve_command_rejects_a_correctly_encoded_fee_account_with_the_wrong_owner() {
    let mut answer = curve_response();
    answer["result"]["value"][2]["owner"] = json!("11111111111111111111111111111111");
    let (endpoint, server) = fixture(vec![answer]);
    let output = Command::new(env!("CARGO_BIN_EXE_radar"))
        .args([
            "curve-exit",
            "--mint",
            &radar_types::Address::new([0x22; 32]).to_string(),
            "--raw-tokens",
            "1000",
            "--rpc",
            &endpoint,
        ])
        .output()
        .expect("command");
    assert!(!output.status.success());
    assert_eq!(output.stdout.len(), 0);
    assert!(
        String::from_utf8(output.stderr)
            .expect("error")
            .contains("market program owner differs")
    );
    assert_eq!(server.join().expect("RPC fixture").len(), 1);
}

#[test]
fn the_curve_command_accounts_for_captured_extensions_and_larger_exotic_fees() {
    let capture: Value = serde_json::from_str(include_str!(
        "../../radar-pumpfun/tests/fixtures/pumpfun_fee_extension.json"
    ))
    .expect("fee capture");
    let hex = capture["data_hex"].as_str().expect("hex");
    let original: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("byte"))
        .collect();
    for (protocol, bound, fee, net) in [
        (95u64, "125", "374627", "29595403"),
        (600u64, "630", "1888113", "28081917"),
    ] {
        let mut bytes = original.clone();
        bytes[161..169].copy_from_slice(&protocol.to_le_bytes());
        let mut answer = curve_response();
        answer["result"]["value"][2]["data"][0] = json!(radar_types::b64::encode(&bytes));
        let (endpoint, server) = fixture(vec![answer]);
        let output = Command::new(env!("CARGO_BIN_EXE_radar"))
            .args([
                "curve-exit",
                "--mint",
                &radar_types::Address::new([0x22; 32]).to_string(),
                "--raw-tokens",
                "1000",
                "--rpc",
                &endpoint,
            ])
            .output()
            .expect("command");
        assert!(output.status.success(), "{:?}", output.stderr);
        let quote: Value = serde_json::from_slice(&output.stdout).expect("quote");
        assert_eq!(quote["venue_fee_upper_bps"], bound);
        assert_eq!(quote["venue_fee_upper_lamports"], fee);
        assert_eq!(quote["net_lamports_at_observed_state"], net);
        assert_eq!(quote["authority"], "read_only");
        assert_eq!(server.join().expect("RPC fixture").len(), 1);
    }
}

#[test]
fn known_account_activity_command_scans_retained_missing_account_and_keeps_incomplete_authority() {
    let wallet = radar_types::Address::new([0x55; 32]);
    let token = radar_types::Address::new([0x56; 32]);
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(),json!({"authority":"protected_operator_inventory_comparison","wallet":wallet,
        "token_account_comparison":{"coverage":"opening_and_current_accounts_only","opening_accounts_retained":true,
            "historical_account_coverage_complete":false,"portfolio_state_updated":false,
            "accounts":[{"address":token,"current_present":false}]}}).to_string()).unwrap();
    let signature = radar_types::Signature::new([0xAB; 64]).to_string();
    let (endpoint, server) = fixture(vec![
        json!({"result":[]}),
        json!({"result":[{"signature":signature,"slot":50,"err":null,"confirmationStatus":"finalized"}]}),
        json!({"result":settlement_result()}),
    ]);
    let output = Command::new(env!("CARGO_BIN_EXE_radar"))
        .args([
            "wallet-activity-read",
            "--wallet",
            &wallet.to_string(),
            "--after-slot",
            "49",
            "--through-slot",
            "50",
            "--rpc",
            &endpoint,
            "--inventory-review",
            file.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["coverage"],
        "provider_reported_known_address_history"
    );
    assert_eq!(report["queried_addresses"], json!([wallet, token]));
    assert_eq!(report["transactions"].as_array().unwrap().len(), 1);
    assert_eq!(report["signature_scan_finished"], true);
    assert_eq!(report["transaction_fetch_finished"], true);
    assert_eq!(report["wallet_coverage_complete"], false);
    assert_eq!(report["targets_owned_by_wallet_verified"], false);
    assert_eq!(
        report["signatures"][0]["reported_for_addresses"],
        json!([token])
    );
    let calls = server.join().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[1]["params"][0], token.to_string());
    assert_eq!(calls[2]["method"], "getTransaction");
}
