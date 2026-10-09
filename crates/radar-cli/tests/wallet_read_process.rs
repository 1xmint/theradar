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
