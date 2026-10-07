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
