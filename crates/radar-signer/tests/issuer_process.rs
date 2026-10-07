// SPDX-License-Identifier: Apache-2.0
//! Exercises the offline issuer's actual process, not a caller-built proof.

use serde_json::{Value, json};
use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Child, Command, Stdio};

const SEED: [u8; 32] = [0x6B; 32];

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

fn address(seed: u8) -> String {
    radar_types::Address::new([seed; 32]).to_string()
}

fn write(path: &std::path::Path, value: &Value) {
    std::fs::write(path, value.to_string()).expect("fixture write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("private fixture");
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    config: Value,
    snapshot: Value,
    candidate: Value,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let wallet = address(0x55);
        let mint = address(0x22);
        let mut transaction = vec![1];
        transaction.extend_from_slice(&[0; 64]);
        transaction.extend_from_slice(&[1, 0, 0, 3]);
        transaction.extend_from_slice(&[0x55; 32]);
        transaction.extend_from_slice(&[0x22; 32]);
        transaction.extend_from_slice(&[0x11; 32]);
        transaction.extend_from_slice(&[0xAA; 32]);
        transaction.extend_from_slice(&[1, 2, 2, 0, 1, 2, 0xAB, 0xCD]);
        let proposal = json!({
            "mint":mint, "market":radar_types::Market::PUMP_FUN_BONDING_CURVE,
            "quote":radar_types::Asset::Sol, "creator":address(0x33), "action":"buy",
            "notional":50_000_000, "estimated_round_trip_cost":100_000,
            "oldest_input_slot":999, "simulated_exit_capacity":100_000_000
        });
        let candidate =
            json!({"proposal":proposal, "transaction":radar_types::b64::encode(&transaction)});
        let snapshot = json!({
            "wallet":wallet, "observed_at_unix_secs":unix_now(), "sol_lamports":300_000_000,
            "sol_upper_micro_usd":200_000_000, "fee_upper_lamports":5000, "state":radar_risk::PortfolioState::flat(radar_types::Slot(1000)),
            "proposal":candidate["proposal"], "transaction":candidate["transaction"]
        });
        let config = json!({
            "active":true, "wallet":wallet, "app_id":"test-app", "wallet_id":"test-wallet",
            "policy":radar_risk::Policy {
                autonomy:radar_risk::Autonomy::Capped,
                max_position:radar_types::MicroUsd(50_000_000),
                max_deployed:radar_types::MicroUsd(100_000_000),
                max_per_creator:radar_types::MicroUsd(100_000_000),
                max_daily_loss:radar_types::MicroUsd(10_000_000),
                max_round_trip_cost_bps:100, max_canary:radar_types::MicroUsd::ZERO,
                max_input_staleness:radar_types::SlotDelta(10), max_consecutive_failures:2
            },
            "valid_until_unix_secs":unix_now()+600, "snapshot_path":dir.path().join("snapshot.json"),
            "history_path":dir.path().join("operations.jsonl"), "key_path":dir.path().join("issuer.json"),
            "max_snapshot_age_secs":120, "intent_lifetime_secs":60, "fee_reserve_lamports":5000,
            "programs":[address(0x11)]
        });
        let fixture = Self {
            dir,
            config,
            snapshot,
            candidate,
        };
        fixture.save();
        write(&fixture.dir.path().join("issuer.json"), &json!(SEED));
        std::fs::write(fixture.dir.path().join("operations.jsonl"), b"")
            .expect("provision history");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(
                fixture.dir.path().join("operations.jsonl"),
                std::fs::Permissions::from_mode(0o600),
            )
            .expect("private history");
        }
        fixture
    }
    fn save(&self) {
        write(&self.dir.path().join("config.json"), &self.config);
        write(&self.dir.path().join("snapshot.json"), &self.snapshot);
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_radar-issuer"));
        command.env("RADAR_ISSUER_CONFIG", self.dir.path().join("config.json"));
        command
    }
    fn start(&self) -> Process {
        let mut child = self
            .command()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("issuer starts");
        let reader = BufReader::new(child.stdout.take().expect("stdout"));
        let mut process = Process { child, reader };
        let mut ready = String::new();
        process.reader.read_line(&mut ready).expect("ready");
        assert_eq!(
            serde_json::from_str::<Value>(&ready).expect("ready JSON")["outcome"],
            "ready"
        );
        process
    }
}

struct Process {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
}
impl Process {
    fn ask(&mut self, value: &Value) -> Value {
        let stdin = self.child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{value}").expect("request");
        stdin.flush().expect("flush");
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("answer");
        serde_json::from_str(&line).expect("JSON answer")
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.wait();
    }
}

#[test]
fn issuance_runs_the_kernel_and_reserves_before_a_verifiable_proof_leaves() {
    let fixture = Fixture::new();
    let mut issuer = fixture.start();
    let answer = issuer.ask(&fixture.candidate);
    assert_eq!(answer["outcome"], "issued", "{answer}");
    let intent: radar_signer::protocol::PrivyAuthorization =
        serde_json::from_value(answer["intent"].clone()).expect("intent");
    let proposal = serde_json::from_value(fixture.candidate["proposal"].clone()).expect("proposal");
    let state = serde_json::from_value(fixture.snapshot["state"].clone()).expect("state");
    let policy = serde_json::from_value(fixture.config["policy"].clone()).expect("policy");
    let mut expected = radar_risk::evaluate(&proposal, &state, &policy)
        .authorisation()
        .expect("kernel decision")
        .clone();
    expected.expires_after = radar_types::Slot(1010);
    assert_eq!(intent.authorization, expected);
    assert_eq!(intent.max_lamports, 250_000_000);
    assert_eq!(intent.now_slot, 1000);
    assert_eq!(intent.wallet, address(0x55));
    assert_eq!(
        intent.request.transaction(),
        fixture.candidate["transaction"].as_str()
    );
    let public = radar_types::Address::new(
        ed25519_dalek::SigningKey::from_bytes(&SEED)
            .verifying_key()
            .to_bytes(),
    );
    radar_signer::attestation::Issuer::new(&public, 60)
        .expect("trust")
        .check(&intent, unix_now())
        .expect("proof");
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
    drop(issuer);
    let operations = radar_journal::OperationLog::open(fixture.dir.path().join("operations.jsonl"))
        .expect("history");
    let entries: Vec<_> = operations.outstanding().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].1.state,
        radar_journal::OperationState::SubmissionUnknown
    );
    assert_eq!(entries[0].1.intent.amount.raw(), 250_005_000);
    drop(operations);
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
}

#[test]
fn stdin_cannot_choose_authority_or_invent_the_market_evidence() {
    let fixture = Fixture::new();
    let mut issuer = fixture.start();
    for field in [
        "authorization",
        "proof",
        "state",
        "max_lamports",
        "wallet",
        "policy",
    ] {
        let mut candidate = fixture.candidate.clone();
        candidate[field] = json!("caller chooses");
        assert_eq!(
            issuer.ask(&candidate)["reason"],
            "invalid candidate",
            "{field}"
        );
    }
    for (pointer, value) in [
        ("/proposal/creator", json!(address(0x44))),
        ("/proposal/estimated_round_trip_cost", json!(0)),
        ("/proposal/simulated_exit_capacity", json!(999_000_000)),
        ("/transaction", json!("different bytes")),
    ] {
        let mut candidate = fixture.candidate.clone();
        *candidate.pointer_mut(pointer).expect("field") = value;
        assert_eq!(
            issuer.ask(&candidate)["reason"],
            "candidate does not match independently provisioned evidence",
            "{pointer}"
        );
    }
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
}

#[test]
fn unreadable_stale_future_wrong_wallet_and_unpriced_snapshots_refuse() {
    let mut fixture = Fixture::new();
    let original = fixture.snapshot.clone();
    let mut issuer = fixture.start();
    for (field, value) in [
        ("wallet", json!(address(0x66))),
        ("sol_upper_micro_usd", json!(0)),
        ("observed_at_unix_secs", json!(unix_now() - 121)),
        ("observed_at_unix_secs", json!(unix_now() + 600)),
    ] {
        fixture.snapshot = original.clone();
        fixture.snapshot[field] = value;
        fixture.save();
        assert_eq!(
            issuer.ask(&fixture.candidate)["reason"],
            "snapshot is not current for the configured wallet",
            "{field}"
        );
    }
    std::fs::write(fixture.dir.path().join("snapshot.json"), b"invalid").expect("damage");
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "invalid trusted snapshot"
    );
    fixture.snapshot = original;
    fixture.save();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
}

#[test]
fn kernel_refusals_and_insufficient_cash_never_produce_a_proof() {
    let mut fixture = Fixture::new();
    let original = fixture.snapshot.clone();
    let mut issuer = fixture.start();
    for (pointer, value) in [
        ("/state/halted", json!(true)),
        ("/state/realised_loss_today", json!(10_000_000)),
        ("/state/deployed", json!(50_000_001)),
        ("/state/consecutive_failures", json!(2)),
        ("/proposal/oldest_input_slot", json!(989)),
        ("/proposal/simulated_exit_capacity", Value::Null),
        ("/proposal/estimated_round_trip_cost", json!(500_001)),
        ("/proposal/notional", json!(50_000_001)),
    ] {
        fixture.snapshot = original.clone();
        *fixture.snapshot.pointer_mut(pointer).expect("field") = value;
        fixture.save();
        let mut candidate = fixture.candidate.clone();
        candidate["proposal"] = fixture.snapshot["proposal"].clone();
        assert_eq!(
            issuer.ask(&candidate)["reason"],
            "risk kernel refused",
            "{pointer}"
        );
    }
    for (field, value) in [
        ("action", json!("exit")),
        ("quote", json!("wrapped_sol")),
        ("notional", json!(0)),
        ("oldest_input_slot", json!(1001)),
    ] {
        fixture.snapshot = original.clone();
        fixture.snapshot["proposal"][field] = value;
        fixture.save();
        let mut candidate = fixture.candidate.clone();
        candidate["proposal"] = fixture.snapshot["proposal"].clone();
        assert_eq!(
            issuer.ask(&candidate)["reason"],
            "issuer currently accepts only current native-SOL buys",
            "{field}"
        );
    }
    fixture.snapshot = original;
    fixture.snapshot["sol_lamports"] = json!(250_004_999);
    fixture.save();
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "capital reservation refused"
    );
    fixture.snapshot["sol_lamports"] = json!(250_005_000);
    fixture.save();
    assert_eq!(issuer.ask(&fixture.candidate)["outcome"], "issued");
}

#[test]
fn startup_refuses_missing_history_and_unbounded_config_and_runtime_revocation() {
    for (pointer, value) in [
        ("/active", json!(false)),
        ("/intent_lifetime_secs", json!(0)),
        ("/max_snapshot_age_secs", json!(0)),
        ("/fee_reserve_lamports", json!(0)),
        ("/programs", json!([])),
        ("/policy/autonomy", json!("observe")),
        ("/history_path", json!("missing-history.jsonl")),
        ("/key_path", json!("missing-issuer-key.json")),
    ] {
        let mut fixture = Fixture::new();
        *fixture.config.pointer_mut(pointer).expect("field") = value;
        fixture.save();
        let output = fixture
            .command()
            .stdin(Stdio::null())
            .output()
            .expect("startup");
        assert!(!output.status.success(), "{pointer}");
        assert!(output.stdout.is_empty());
    }
    let mut fixture = Fixture::new();
    let mut issuer = fixture.start();
    fixture.config["active"] = json!(false);
    fixture.save();
    assert_eq!(
        issuer.ask(&fixture.candidate)["reason"],
        "mandate changed; restart issuer against current configuration"
    );
}

#[test]
fn conversion_rounds_down_and_fee_and_time_evidence_bound_the_proof() {
    let mut fixture = Fixture::new();
    fixture.snapshot["sol_upper_micro_usd"] = json!(200_000_003);
    fixture.snapshot["observed_at_unix_secs"] = json!(unix_now() - 60);
    fixture.config["intent_lifetime_secs"] = json!(300);
    fixture.save();
    let mut issuer = fixture.start();
    for value in [0, 5001] {
        fixture.snapshot["fee_upper_lamports"] = json!(value);
        fixture.save();
        assert_eq!(
            issuer.ask(&fixture.candidate)["reason"],
            "reviewed transaction fee is not covered by the reservation"
        );
    }
    fixture.snapshot["fee_upper_lamports"] = json!(5000);
    fixture.save();
    let answer = issuer.ask(&fixture.candidate);
    assert_eq!(answer["outcome"], "issued", "{answer}");
    assert_eq!(answer["intent"]["max_lamports"], 249_999_996);
    assert_eq!(
        answer["intent"]["proof"]["expires_at_unix_secs"],
        fixture.snapshot["observed_at_unix_secs"]
            .as_u64()
            .expect("observed")
            + 120
    );
    drop(issuer);
    let operations = radar_journal::OperationLog::open(fixture.dir.path().join("operations.jsonl"))
        .expect("history");
    assert_eq!(
        operations
            .outstanding()
            .next()
            .expect("claim")
            .1
            .intent
            .amount
            .raw(),
        250_004_996
    );
    drop(operations);

    for (price, fee, reason) in [
        (u64::MAX, 5000, "lamport ceiling is zero"),
        (1, 5000, "capital reservation refused"),
        (200_000_000, u64::MAX, "fee reservation overflow"),
    ] {
        let mut fixture = Fixture::new();
        fixture.snapshot["sol_upper_micro_usd"] = json!(price);
        fixture.config["fee_reserve_lamports"] = json!(fee);
        fixture.save();
        assert_eq!(fixture.start().ask(&fixture.candidate)["reason"], reason);
    }
    let mut fixture = Fixture::new();
    fixture.config["valid_until_unix_secs"] = json!(unix_now() - 1);
    fixture.save();
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "mandate expired"
    );
}

#[test]
fn unusable_private_files_and_overlong_input_stop_without_issuing() {
    let fixture = Fixture::new();
    let config_path = fixture.dir.path().join("config.json");
    let mut large = fixture.config.to_string();
    large.extend(std::iter::repeat_n(' ', 1_048_577));
    std::fs::write(&config_path, large).expect("oversize valid JSON");
    let output = fixture
        .command()
        .stdin(Stdio::null())
        .output()
        .expect("startup");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    fixture.save();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o644))
            .expect("insecure config");
        let output = fixture
            .command()
            .stdin(Stdio::null())
            .output()
            .expect("startup");
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        fixture.save();
    }
    let mut issuer = fixture.start();
    let mut large = fixture.candidate.to_string();
    large.extend(std::iter::repeat_n(' ', 70_000));
    large.push('\n');
    let mut stdin = issuer.child.stdin.take().expect("stdin");
    let _ = stdin.write_all(large.as_bytes());
    drop(stdin);
    let mut answer = String::new();
    issuer.reader.read_line(&mut answer).expect("EOF");
    assert!(
        answer.is_empty(),
        "no proof from a truncated input: {answer}"
    );
    assert!(!issuer.child.wait().expect("exit").success());
}

#[test]
fn a_broken_output_pipe_keeps_the_claim_unknown_after_restart() {
    let fixture = Fixture::new();
    let mut child = fixture
        .command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("issuer");
    let mut reader = BufReader::new(child.stdout.take().expect("stdout"));
    let mut ready = String::new();
    reader.read_line(&mut ready).expect("ready");
    assert_eq!(
        serde_json::from_str::<Value>(&ready).expect("ready JSON")["outcome"],
        "ready"
    );
    drop(reader);
    let mut stdin = child.stdin.take().expect("stdin");
    writeln!(stdin, "{}", fixture.candidate).expect("candidate");
    stdin.flush().expect("flush");
    drop(stdin);
    assert!(!child.wait().expect("exit").success());
    let operations = radar_journal::OperationLog::open(fixture.dir.path().join("operations.jsonl"))
        .expect("history");
    let entries: Vec<_> = operations.outstanding().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].1.state,
        radar_journal::OperationState::SubmissionUnknown
    );
    drop(operations);
    assert_eq!(
        fixture.start().ask(&fixture.candidate)["reason"],
        "outstanding operation requires reconciliation"
    );
}
