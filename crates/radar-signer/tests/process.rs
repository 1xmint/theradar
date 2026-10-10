// SPDX-License-Identifier: Apache-2.0
//! The adversarial test, run against the real process.
//!
//! The unit tests prove the rules. This proves the *binary* — the thing that
//! will actually hold the key — enforces them: same executable, same argument
//! parsing, same environment handling, same stdin loop.
//!
//! The distinction matters because every failure mode this process exists to
//! prevent lives in the wiring. A library that refuses correctly and a binary
//! that never calls it is a system with no signer at all, and the library's own
//! tests are green either way.

use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Child, Command, Stdio};

/// The programs the test signer will accept.
const DEX: [u8; 32] = [0x11; 32];
const MINT: [u8; 32] = [0x22; 32];
const SYSTEM: [u8; 32] = [0u8; 32];

/// The secret half of the test wallet. Deterministic; not a real key.
const SEED: [u8; 32] = [0x5A; 32];

#[test]
fn privy_process_rechecks_venue_trade_direction_after_a_valid_issuer_proof() {
    let scratch = Scratch::new("privy-trade-direction");
    let policy = policy_file(&scratch.0, &open_policy());
    let pump = *radar_decode::pumpfun::PROGRAM_ID.as_bytes();
    let mut command = privy_only_command(&policy);
    command.env(
        "RADAR_SIGNER_PROGRAMS",
        format!("{},{}", b58(&pump), b58(&SYSTEM)),
    );
    let mut signer = Signer::from_command(command);
    for (instruction, _, _) in radar_decode::pumpfun::KNOWN
        .iter()
        .filter(|(ix, _, _)| ix.is_trade())
    {
        let mut data = instruction.discriminator().as_bytes().to_vec();
        data.extend(10_u64.to_le_bytes());
        data.extend(20_u64.to_le_bytes());
        let bytes = transaction(&[wallet(), MINT, pump, SYSTEM], &[(2, vec![0, 1], data)]);
        for (action, buying) in [("buy", true), ("reduce", false), ("exit", false)] {
            let mut input = privy_request(&bytes);
            input["authorization"]["action"] = serde_json::json!(action);
            input["authorization"]["nonce"] =
                serde_json::json!(format!("{}-{action}", instruction.anchor_name()));
            attest(&mut input);
            let answer = signer.ask(&input);
            if instruction.is_buy() == buying {
                assert_eq!(
                    answer["outcome"], "authorised",
                    "{instruction:?}/{action}: {answer}"
                );
                assert_eq!(
                    signer.ask(&input)["outcome"],
                    "refused",
                    "one-time authority"
                );
            } else {
                assert_eq!(
                    answer["outcome"], "refused",
                    "{instruction:?}/{action}: {answer}"
                );
                assert!(
                    answer["reasons"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|reason| reason
                            .as_str()
                            .unwrap_or_default()
                            .contains("contradicts the authorized action")),
                    "{answer}"
                );
                // A proof from a trusted issuer cannot override transaction
                // direction. Even a refused attempt consumes its durable nonce;
                // correcting the action requires fresh issuer authority.
                input["authorization"]["action"] =
                    serde_json::json!(if instruction.is_buy() { "buy" } else { "exit" });
                attest(&mut input);
                let retry = signer.ask(&input);
                assert_eq!(retry["outcome"], "refused", "{retry}");
                assert!(retry["reasons"].to_string().contains("nonce is reused"));
                input["authorization"]["nonce"] =
                    serde_json::json!(format!("{}-{action}-corrected", instruction.anchor_name()));
                attest(&mut input);
                let corrected = signer.ask(&input);
                assert_eq!(corrected["outcome"], "authorised", "{corrected}");
            }
        }
    }
}

/// A signer process with a pipe to it.
struct Signer {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
}

impl Signer {
    /// Starts the binary with a key file and an allowlist, and no customer key.
    fn start(key_file: &std::path::Path, programs: &str) -> Self {
        Self::start_with(key_file, programs, None)
    }

    /// Starts with a policy file the test supplies.
    ///
    /// Separate from [`Self::start_with`] because most tests want a policy wide
    /// enough to be out of the way, and the ones about the policy want to choose
    /// it. A single helper with a permissive default would make it too easy to
    /// write a test that passes because the clamp never engaged.
    fn start_under(
        key_file: &std::path::Path,
        programs: &str,
        policy_file: Option<&std::path::Path>,
    ) -> Self {
        Self::spawn(key_file, programs, None, policy_file)
    }

    /// Starts the binary, optionally with a Privy authorization key.
    ///
    /// Writes a permissive policy beside the key, because ADR 0008 makes a
    /// policy mandatory and these tests are about other things. The clamp has
    /// its own tests, which supply their own.
    fn start_with(key_file: &std::path::Path, programs: &str, privy: Option<&str>) -> Self {
        let permissive = key_file.with_file_name("policy.json");
        std::fs::write(
            &permissive,
            serde_json::to_string(&open_policy()).expect("serialises"),
        )
        .expect("write");
        Self::spawn(key_file, programs, privy, Some(&permissive))
    }

    fn spawn(
        key_file: &std::path::Path,
        programs: &str,
        privy: Option<&str>,
        policy_file: Option<&std::path::Path>,
    ) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_radar-signer"));
        command
            .env("RADAR_SIGNER_MODE", "local")
            .env("RADAR_SIGNER_KEY", key_file)
            .env("RADAR_SIGNER_PROGRAMS", programs);
        match policy_file {
            Some(path) => command.env("RADAR_SIGNER_POLICY", path),
            None => command.env_remove("RADAR_SIGNER_POLICY"),
        };
        // Removed rather than left unset, so a variable in the developer's own
        // environment cannot make the no-key test pass.
        match privy {
            Some(material) => command.env("RADAR_PRIVY_AUTHORIZATION_KEY", material),
            None => command.env_remove("RADAR_PRIVY_AUTHORIZATION_KEY"),
        };
        command
            .env("RADAR_SIGNER_PRIVY_APP_ID", "cmthhkznr0a3u0cl86prxlb7x")
            .env("RADAR_SIGNER_PRIVY_WALLET_ID", "abc")
            .env("RADAR_SIGNER_PRIVY_WALLET_ADDRESS", b58(&wallet()));
        command.env(
            "RADAR_SIGNER_NONCE_DIR",
            nonce_directory(key_file.parent().expect("parent")),
        );
        configure_issuer(&mut command);
        Self::from_command(command)
    }

    fn from_command(mut command: Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the signer binary must start");
        let stdout = child.stdout.take().expect("piped");
        Self {
            child,
            reader: BufReader::new(stdout),
        }
    }

    /// Sends one request and reads one answer.
    fn ask(&mut self, request: &serde_json::Value) -> serde_json::Value {
        let stdin = self.child.stdin.as_mut().expect("piped");
        writeln!(stdin, "{request}").expect("write");
        stdin.flush().expect("flush");

        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .expect("the signer must answer");
        serde_json::from_str(&line).expect("the answer must be JSON")
    }
}

impl Drop for Signer {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.wait();
    }
}

/// A policy wide enough not to be what a test is about.
fn open_policy() -> radar_risk::Policy {
    radar_risk::Policy {
        autonomy: radar_risk::Autonomy::Capped,
        max_position: radar_types::MicroUsd(1_000_000_000),
        max_canary: radar_types::MicroUsd(1_000_000_000),
        max_input_staleness: radar_types::SlotDelta(100_000),
        ..radar_risk::Policy::CLOSED
    }
}

/// Writes a policy file and returns its path.
fn policy_file(dir: &std::path::Path, policy: &radar_risk::Policy) -> std::path::PathBuf {
    let path = dir.join("policy.json");
    std::fs::write(&path, serde_json::to_string(policy).expect("serialises")).expect("write");
    path
}

/// Writes a Solana keypair file and returns its path.
fn key_file(dir: &std::path::Path) -> std::path::PathBuf {
    use ed25519_dalek::SigningKey;

    let signing = SigningKey::from_bytes(&SEED);
    let mut bytes = SEED.to_vec();
    bytes.extend_from_slice(&signing.verifying_key().to_bytes());

    let path = dir.join("signer.json");
    std::fs::write(&path, serde_json::to_string(&bytes).expect("serialises")).expect("write");
    path
}

/// The wallet the test signer signs for.
fn wallet() -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(&SEED)
        .verifying_key()
        .to_bytes()
}

fn b58(bytes: &[u8; 32]) -> String {
    radar_types::Address::new(*bytes).to_string()
}

/// Builds a transaction with one signature slot.
///
/// `accounts[0]` is the fee payer. Instructions are `(program_index, indices, data)`.
fn transaction(accounts: &[[u8; 32]], instructions: &[(u8, Vec<u8>, Vec<u8>)]) -> String {
    let mut out = vec![1u8];
    out.extend_from_slice(&[0u8; 64]);
    out.push(1);
    out.push(0);
    out.push(0);
    out.push(u8::try_from(accounts.len()).expect("small"));
    for a in accounts {
        out.extend_from_slice(a);
    }
    out.extend_from_slice(&[0xAA; 32]);
    out.push(u8::try_from(instructions.len()).expect("small"));
    for (program, indices, data) in instructions {
        out.push(*program);
        out.push(u8::try_from(indices.len()).expect("small"));
        out.extend_from_slice(indices);
        out.push(u8::try_from(data.len()).expect("small"));
        out.extend_from_slice(data);
    }
    radar_types::b64::encode(&out)
}

/// The honest transaction: a swap on the allowed DEX, in the authorised mint.
fn honest() -> String {
    transaction(
        &[wallet(), MINT, DEX, SYSTEM],
        &[(2, vec![0, 1], vec![0xAB, 0xCD])],
    )
}

fn request(transaction: &str, mint: &[u8; 32], now_slot: u64) -> serde_json::Value {
    serde_json::json!({
        "sign": "local",
        "authorization": {
            "nonce": "test-nonce",
            "mint": b58(mint),
            "action": "buy",
            "max_notional": 50_000_000u64,
            "expires_after": 1_150u64,
            "needs_operator_signature": false,
        },
        "transaction": transaction,
        "now_slot": now_slot,
        // Unbounded, so a refusal in these tests is about the property under
        // test rather than about the caller's own ceiling. The ceiling has its
        // own tests in `verify`.
        "max_lamports": u64::MAX,
    })
}

/// A scratch directory that cleans itself up.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("radar-signer-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_process_signs_an_honest_transaction() {
    let scratch = Scratch::new("honest");
    let mut signer = Signer::start(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
    );

    let answer = signer.ask(&request(&honest(), &MINT, 1_000));
    assert_eq!(answer["outcome"], "signed", "got {answer}");
    assert_eq!(answer["wallet"], b58(&wallet()));

    // The returned transaction must carry the signature in its first slot and
    // leave the message untouched. A signer that re-serialised the message
    // would be signing bytes it verified and returning bytes it did not.
    let returned =
        radar_types::b64::decode(answer["transaction"].as_str().expect("string")).expect("base64");
    let original = radar_types::b64::decode(&honest()).expect("base64");
    assert_eq!(returned.len(), original.len());
    assert_eq!(
        &returned[65..],
        &original[65..],
        "the message must be unchanged"
    );
    assert_ne!(&returned[1..65], &[0u8; 64], "a signature must be present");
}

#[test]
fn the_process_refuses_a_substituted_mint() {
    // The attack the separate process exists for: a compromised executor holds
    // a valid authorization for one token and builds a transaction for another.
    let scratch = Scratch::new("substituted");
    let mut signer = Signer::start(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
    );

    let other = transaction(
        &[wallet(), [0x99; 32], DEX, SYSTEM],
        &[(2, vec![0, 1], vec![0xAB])],
    );
    let answer = signer.ask(&request(&other, &MINT, 1_000));
    assert_eq!(answer["outcome"], "refused", "got {answer}");
    assert!(
        answer["reasons"]
            .as_array()
            .expect("array")
            .iter()
            .any(|r| r
                .as_str()
                .unwrap_or_default()
                .contains("is not in the transaction")),
        "got {answer}"
    );
}

#[test]
fn the_process_refuses_an_unlisted_program() {
    let scratch = Scratch::new("program");
    let mut signer = Signer::start(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
    );

    let evil = transaction(
        &[wallet(), MINT, [0xEE; 32], SYSTEM],
        &[(2, vec![0, 1], vec![0xAB])],
    );
    assert_eq!(
        signer.ask(&request(&evil, &MINT, 1_000))["outcome"],
        "refused"
    );
}

#[test]
fn the_process_refuses_an_oversized_spend() {
    let scratch = Scratch::new("oversize");
    let mut signer = Signer::start(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
    );

    let mut data = 2u32.to_le_bytes().to_vec();
    data.extend_from_slice(&60_000_000u64.to_le_bytes());
    let big = transaction(
        &[wallet(), MINT, DEX, SYSTEM],
        &[(2, vec![0, 1], vec![0xAB]), (3, vec![0, 1], data)],
    );
    let answer = signer.ask(&request(&big, &MINT, 1_000));
    assert_eq!(answer["outcome"], "refused", "got {answer}");
}

#[test]
fn the_process_refuses_an_expired_authorization() {
    let scratch = Scratch::new("expired");
    let mut signer = Signer::start(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
    );
    assert_eq!(
        signer.ask(&request(&honest(), &MINT, 99_999))["outcome"],
        "refused"
    );
}

#[test]
fn an_unconfigured_signer_refuses_rather_than_dying() {
    // A misconfiguration that looked like a crash would have the executor retry
    // forever. It must answer, and every answer must be a refusal.
    let scratch = Scratch::new("unconfigured");
    let missing = scratch.0.join("no-such-key.json");
    let mut signer = Signer::start(&missing, &b58(&DEX));

    for _ in 0..3 {
        let answer = signer.ask(&request(&honest(), &MINT, 1_000));
        assert_eq!(answer["outcome"], "refused", "got {answer}");
    }
}

#[test]
fn an_empty_allowlist_refuses_everything() {
    // A signer with no allowlist signs anything. Of every misconfiguration
    // available here, that is the one with no upper bound on its cost, so it
    // must not start into a permissive state.
    let scratch = Scratch::new("noallowlist");
    let mut signer = Signer::start(&key_file(&scratch.0), "");
    assert_eq!(
        signer.ask(&request(&honest(), &MINT, 1_000))["outcome"],
        "refused"
    );
}

#[test]
fn garbage_does_not_stop_the_process_serving() {
    // A malformed request must not become a denial of service against the only
    // component that can stop a bad trade.
    let scratch = Scratch::new("garbage");
    let mut signer = Signer::start(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
    );

    for junk in [
        serde_json::json!("not an object"),
        serde_json::json!({"sign": "local", "authorization": 5}),
        serde_json::json!({"sign": "local", "authorization": {}, "transaction": "!!!", "now_slot": 1}),
    ] {
        assert_eq!(signer.ask(&junk)["outcome"], "refused");
    }
    // Still serving, and still correct, afterwards.
    assert_eq!(
        signer.ask(&request(&honest(), &MINT, 1_000))["outcome"],
        "signed"
    );
}

/// A transaction in a token the authorisation does not cover.
fn substituted_mint() -> String {
    transaction(
        &[wallet(), [0x99; 32], DEX, SYSTEM],
        &[(2, vec![0, 1], vec![0xAB, 0xCD])],
    )
}

/// A base64 PKCS#8 P-256 key, as Privy's dashboard would hand one over.
fn privy_key() -> String {
    let pkcs8 = ring::signature::EcdsaKeyPair::generate_pkcs8(
        &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
        &ring::rand::SystemRandom::new(),
    )
    .expect("a key pair");
    radar_types::b64::encode(pkcs8.as_ref())
}

/// A request for a Privy authorization signature.
fn privy_request(transaction: &str) -> serde_json::Value {
    let now = unix_now();
    let mut request = serde_json::json!({
        "sign": "privy",
        "authorization": {
            "nonce": "test-nonce",
            "mint": b58(&MINT),
            "action": "buy",
            "max_notional": 50_000_000u64,
            "expires_after": 1_150u64,
            "needs_operator_signature": false,
        },
        "request": {
            "method": "POST",
            "url": "https://api.privy.io/v1/wallets/abc/rpc",
            "body": {
                "method": "signTransaction",
                "params": {"transaction": transaction, "encoding": "base64"},
            },
            "headers": {"privy-app-id": "cmthhkznr0a3u0cl86prxlb7x"},
        },
        "wallet": b58(&wallet()),
        "now_slot": 1_000u64,
        "max_lamports": u64::MAX,
        "proof": {
            "issued_at_unix_secs": now - 1,
            "expires_at_unix_secs": now + 59,
            "signature": "",
        },
    });
    attest(&mut request);
    request
}

// A separate, non-wallet test key. Production must keep the issuer's private
// key outside the model, Serve, executor and signer; only its public key is here.
fn issuer_key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[0x6B; 32])
}

fn configure_issuer(command: &mut Command) {
    command
        .env(
            "RADAR_SIGNER_ISSUER_PUBLIC_KEY",
            b58(&issuer_key().verifying_key().to_bytes()),
        )
        .env("RADAR_SIGNER_MAX_INTENT_LIFETIME_SECS", "60");
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

fn attest(request: &mut serde_json::Value) {
    use ed25519_dalek::Signer as _;
    let intent: radar_signer::protocol::PrivyAuthorization =
        serde_json::from_value(request.clone()).expect("intent");
    let payload = radar_signer::attestation::payload(&intent).expect("payload");
    request["proof"]["signature"] = serde_json::json!(radar_types::b64::encode(
        &issuer_key().sign(payload.as_bytes()).to_bytes()
    ));
}

#[test]
fn a_signer_with_no_policy_refuses_everything() {
    // Rule 8, and ADR 0008's whole premise.
    //
    // Before this, the signer had no opinion about whether Radar was permitted
    // to trade at all: the only ceilings it enforced were the ones written on
    // the authorisation it was handed, by the caller. An operator who never
    // configured a policy got a signer that signed whatever the caller said was
    // approved.
    //
    // Absent must mean refuse, not "accept the caller's judgement".
    let scratch = Scratch::new("no-policy");
    let mut signer = Signer::start_under(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
        None,
    );

    let answer = signer.ask(&request(&honest(), &MINT, 1_000));
    assert_eq!(
        answer["outcome"], "refused",
        "a signer with no policy must refuse: {answer}"
    );
}

#[test]
fn a_closed_policy_refuses_a_transaction_the_caller_says_is_authorised() {
    // `Policy::CLOSED` enforced at the key rather than only at the decision.
    //
    // The request is well formed and entirely within its own stated bounds.
    // What refuses it is the file this process loaded, which the caller does
    // not control -- and that is the difference the ADR is about.
    let scratch = Scratch::new("closed-policy");
    let closed = policy_file(&scratch.0, &radar_risk::Policy::CLOSED);
    let mut signer = Signer::start_under(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
        Some(&closed),
    );

    let answer = signer.ask(&request(&honest(), &MINT, 1_000));
    assert_eq!(answer["outcome"], "refused", "{answer}");

    // And the same signer under an open policy signs it, or the refusal above
    // says nothing about the policy.
    let open = policy_file(&scratch.0, &open_policy());
    let mut permitted = Signer::start_under(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
        Some(&open),
    );
    let allowed = permitted.ask(&request(&honest(), &MINT, 1_000));
    assert_eq!(
        allowed["outcome"], "signed",
        "the same request under an open policy must sign: {allowed}"
    );
}

#[test]
fn an_untagged_request_is_refused_rather_than_assumed_to_be_a_local_one() {
    // The version-skew property, at the process boundary where it would happen.
    //
    // The signer answers two kinds of request now, and the tag has no default
    // on purpose: a deployment that updates the executor and not the signer, or
    // the other way round, must stop signing rather than guess which kind of
    // signature was wanted. Refusing is recoverable; guessing is not.
    let scratch = Scratch::new("untagged");
    let mut signer = Signer::start(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
    );

    let mut untagged = request(&honest(), &MINT, 1_000);
    untagged
        .as_object_mut()
        .expect("an object")
        .remove("sign")
        .expect("the tag was there to remove");

    let answer = signer.ask(&untagged);
    assert_eq!(
        answer["outcome"], "refused",
        "an untagged request must be refused, not assumed: {answer}"
    );
}

#[test]
fn a_privy_request_with_no_authorization_key_configured_is_refused_by_name() {
    // Rule 8, and the wording matters as much as the refusal. An instance with
    // no customer key must say which thing is missing, or an operator goes to
    // Privy's dashboard looking for a fault in their account.
    let scratch = Scratch::new("no-privy-key");
    let mut signer = Signer::start(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
    );

    let answer = signer.ask(&privy_request(&honest()));
    assert_eq!(answer["outcome"], "refused", "{answer}");
    assert!(
        answer["reasons"][0]
            .as_str()
            .unwrap_or_default()
            .contains("no Privy authorization key"),
        "the refusal must name what is missing: {answer}"
    );
}

#[test]
fn a_blank_authorization_key_is_absent_rather_than_a_key() {
    // `RADAR_PRIVY_AUTHORIZATION_KEY=` left in an env file, which is what a
    // half-finished deployment looks like.
    //
    // Treating an empty value as key material means trying to parse it, which
    // fails, which makes `Config::from_env` return an error -- and an error
    // there refuses *everything*, taking the local signing lane down with it.
    // A blank value must mean the same as an absent one.
    let scratch = Scratch::new("blank-privy-key");
    let mut signer = Signer::start_with(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
        Some("   "),
    );

    // The local lane still works, which is the half that would break.
    let local = signer.ask(&request(&honest(), &MINT, 1_000));
    assert_eq!(
        local["outcome"], "signed",
        "a blank customer key must not disable local signing: {local}"
    );

    let customer = signer.ask(&privy_request(&honest()));
    assert!(
        customer["reasons"][0]
            .as_str()
            .unwrap_or_default()
            .contains("no Privy authorization key"),
        "a blank value must read as absent, by name: {customer}"
    );
}

#[test]
fn the_running_process_checks_a_privy_request_before_it_authorises_one() {
    // ADR 0007's claim is that the Privy key lives in *this process* behind
    // *this check*. A library test does not establish that the binary wired the
    // two together, and the wiring is the part that could be missing.
    //
    // Both directions in one test, deliberately: a gate that refuses everything
    // is not a gate, so the honest request must be authorised for the refusal
    // below to mean anything.
    let scratch = Scratch::new("privy-gate");
    let mut signer = Signer::start_with(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
        Some(&privy_key()),
    );

    let honest = signer.ask(&privy_request(&honest()));
    assert_eq!(
        honest["outcome"], "authorised",
        "the honest request must be authorised, or this test proves nothing: {honest}"
    );
    assert!(
        !honest["signature"].as_str().unwrap_or_default().is_empty(),
        "an authorised answer carries a header value: {honest}"
    );

    let mut substituted = privy_request(&substituted_mint());
    substituted["authorization"]["nonce"] = serde_json::json!("substituted-mint");
    attest(&mut substituted);
    let substituted = signer.ask(&substituted);
    assert_eq!(
        substituted["outcome"], "refused",
        "a request for a token the authorisation does not cover must be refused: {substituted}"
    );
}

#[test]
fn a_privy_request_whose_body_carries_no_transaction_is_refused() {
    // The most tempting place in the whole lane to write "nothing to check,
    // carry on". A request whose contents cannot be read is one nothing
    // inspected, and signing it would authorise bytes nobody looked at.
    let scratch = Scratch::new("privy-no-transaction");
    let mut signer = Signer::start_with(
        &key_file(&scratch.0),
        &format!("{},{}", b58(&DEX), b58(&SYSTEM)),
        Some(&privy_key()),
    );

    let mut request = privy_request(&honest());
    request["request"]["body"]["params"] = serde_json::json!({});
    attest(&mut request);

    let answer = signer.ask(&request);
    assert_eq!(answer["outcome"], "refused", "{answer}");
}

fn privy_only_command(policy: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_radar-signer"));
    command
        .env("RADAR_SIGNER_MODE", "privy")
        .env_remove("RADAR_SIGNER_KEY")
        .env(
            "RADAR_SIGNER_PROGRAMS",
            format!("{},{}", b58(&DEX), b58(&SYSTEM)),
        )
        .env("RADAR_SIGNER_POLICY", policy)
        .env("RADAR_PRIVY_AUTHORIZATION_KEY", privy_key())
        .env("RADAR_SIGNER_PRIVY_APP_ID", "cmthhkznr0a3u0cl86prxlb7x")
        .env("RADAR_SIGNER_PRIVY_WALLET_ID", "abc")
        .env("RADAR_SIGNER_PRIVY_WALLET_ADDRESS", b58(&wallet()));
    command.env(
        "RADAR_SIGNER_NONCE_DIR",
        nonce_directory(policy.parent().expect("parent")),
    );
    configure_issuer(&mut command);
    command
}

fn nonce_directory(parent: &std::path::Path) -> std::path::PathBuf {
    let dir = parent.join("nonces");
    std::fs::create_dir_all(&dir).expect("nonce directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
            .expect("permissions");
    }
    dir
}

#[test]
fn privy_only_needs_no_local_key_and_refuses_local_signing() {
    let scratch = Scratch::new("privy-only");
    let policy = policy_file(&scratch.0, &open_policy());
    let mut signer = Signer::from_command(privy_only_command(&policy));
    assert_eq!(
        signer.ask(&privy_request(&honest()))["outcome"],
        "authorised"
    );
    assert_eq!(
        signer.ask(&request(&honest(), &MINT, 1_000))["outcome"],
        "refused"
    );
    let mut wrong = privy_request(&honest());
    wrong["wallet"] = serde_json::json!(b58(&[0x44; 32]));
    wrong["authorization"]["nonce"] = serde_json::json!("wrong-wallet");
    attest(&mut wrong);
    assert_eq!(signer.ask(&wrong)["outcome"], "refused");
    wrong = privy_request(&honest());
    wrong["request"]["url"] = serde_json::json!("https://api.privy.io/v1/wallets/other/rpc");
    wrong["authorization"]["nonce"] = serde_json::json!("wrong-url");
    attest(&mut wrong);
    assert_eq!(signer.ask(&wrong)["outcome"], "refused");
}

#[test]
fn privy_only_remains_closed_under_the_shipped_policy() {
    let scratch = Scratch::new("privy-closed");
    let policy = policy_file(&scratch.0, &radar_risk::Policy::SHIPPED);
    let mut signer = Signer::from_command(privy_only_command(&policy));
    assert_eq!(signer.ask(&privy_request(&honest()))["outcome"], "refused");
}

#[test]
fn missing_or_invalid_privy_configuration_never_falls_back() {
    let scratch = Scratch::new("privy-config");
    let policy = policy_file(&scratch.0, &open_policy());
    for name in [
        "RADAR_PRIVY_AUTHORIZATION_KEY",
        "RADAR_SIGNER_PRIVY_APP_ID",
        "RADAR_SIGNER_PRIVY_WALLET_ID",
        "RADAR_SIGNER_PRIVY_WALLET_ADDRESS",
        "RADAR_SIGNER_POLICY",
        "RADAR_SIGNER_PROGRAMS",
        "RADAR_SIGNER_NONCE_DIR",
        "RADAR_SIGNER_ISSUER_PUBLIC_KEY",
        "RADAR_SIGNER_MAX_INTENT_LIFETIME_SECS",
    ] {
        let mut command = privy_only_command(&policy);
        command.env_remove(name);
        let mut signer = Signer::from_command(command);
        assert_eq!(
            signer.ask(&privy_request(&honest()))["outcome"],
            "refused",
            "{name}"
        );
    }
    for (name, value) in [
        ("RADAR_SIGNER_MODE", "typo"),
        ("RADAR_SIGNER_PRIVY_WALLET_ID", "abc/other"),
        ("RADAR_SIGNER_PRIVY_APP_ID", ""),
        ("RADAR_SIGNER_PRIVY_WALLET_ADDRESS", "invalid"),
        ("RADAR_PRIVY_AUTHORIZATION_KEY", " "),
        ("RADAR_SIGNER_ISSUER_PUBLIC_KEY", "invalid"),
        (
            "RADAR_SIGNER_ISSUER_PUBLIC_KEY",
            "11111111111111111111111111111111",
        ),
        ("RADAR_SIGNER_MAX_INTENT_LIFETIME_SECS", "0"),
        ("RADAR_SIGNER_MAX_INTENT_LIFETIME_SECS", "-1"),
    ] {
        let mut command = privy_only_command(&policy);
        command.env(name, value);
        let mut signer = Signer::from_command(command);
        assert_eq!(
            signer.ask(&privy_request(&honest()))["outcome"],
            "refused",
            "{name}"
        );
    }
}

#[test]
fn a_privy_nonce_cannot_be_reused_after_restart_or_for_changed_bytes() {
    let scratch = Scratch::new("privy-restart");
    let policy = policy_file(&scratch.0, &open_policy());
    let original = privy_request(&honest());
    {
        let mut signer = Signer::from_command(privy_only_command(&policy));
        assert_eq!(signer.ask(&original)["outcome"], "authorised");
    }
    let mut restarted = Signer::from_command(privy_only_command(&policy));
    assert_eq!(restarted.ask(&original)["outcome"], "refused");
    let mut changed = original.clone();
    changed["now_slot"] = serde_json::json!(1_001);
    let mut changed_bytes = radar_types::b64::decode(&honest()).expect("transaction");
    *changed_bytes.last_mut().expect("instruction data") = 0xCE;
    changed["request"]["body"]["params"]["transaction"] =
        serde_json::json!(radar_types::b64::encode(&changed_bytes));
    attest(&mut changed);
    assert_eq!(restarted.ask(&changed)["outcome"], "refused");
    changed["authorization"]["nonce"] = serde_json::json!("fresh-intent");
    attest(&mut changed);
    assert_eq!(restarted.ask(&changed)["outcome"], "authorised");
}

#[test]
fn concurrent_privy_processes_cannot_consume_one_nonce_twice() {
    let scratch = Scratch::new("privy-concurrent");
    let policy = policy_file(&scratch.0, &open_policy());
    let first = Signer::from_command(privy_only_command(&policy));
    let second = Signer::from_command(privy_only_command(&policy));
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let threads: Vec<_> = [first, second]
        .into_iter()
        .map(|mut signer| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                signer.ask(&privy_request(&honest()))
            })
        })
        .collect();
    barrier.wait();
    let answers: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().expect("joined"))
        .collect();
    assert_eq!(
        answers
            .iter()
            .filter(|answer| answer["outcome"] == "authorised")
            .count(),
        1,
        "{answers:?}"
    );
    assert_eq!(
        answers
            .iter()
            .filter(|answer| answer["outcome"] == "refused")
            .count(),
        1,
        "{answers:?}"
    );
}

#[test]
fn failed_or_interrupted_privy_attempts_remain_consumed() {
    let scratch = Scratch::new("privy-interrupted");
    let policy = policy_file(&scratch.0, &open_policy());
    let directory = nonce_directory(&scratch.0);
    // A crash after reserving, before signing/output, cannot silently release it.
    radar_signer::replay::ReplayStore::at(&directory)
        .expect("store")
        .claim("test-nonce")
        .expect("reserve");
    let mut signer = Signer::from_command(privy_only_command(&policy));
    assert_eq!(signer.ask(&privy_request(&honest()))["outcome"], "refused");
    let mut invalid = privy_request(&substituted_mint());
    invalid["authorization"]["nonce"] = serde_json::json!("rejected-attempt");
    attest(&mut invalid);
    assert_eq!(signer.ask(&invalid)["outcome"], "refused");
    let mut corrected = privy_request(&honest());
    corrected["authorization"]["nonce"] = serde_json::json!("rejected-attempt");
    attest(&mut corrected);
    assert_eq!(signer.ask(&corrected)["outcome"], "refused");
}

#[test]
fn a_missing_nonce_directory_is_never_recreated_by_the_signer() {
    let scratch = Scratch::new("privy-missing-state");
    let policy = policy_file(&scratch.0, &open_policy());
    let missing = scratch.0.join("missing-state");
    let mut command = privy_only_command(&policy);
    command.env("RADAR_SIGNER_NONCE_DIR", &missing);
    let mut signer = Signer::from_command(command);
    assert_eq!(signer.ask(&privy_request(&honest()))["outcome"], "refused");
    assert!(!missing.exists());
}

#[test]
fn loss_of_nonce_state_while_running_refuses_fresh_intents() {
    let scratch = Scratch::new("privy-lost-state");
    let policy = policy_file(&scratch.0, &open_policy());
    let mut signer = Signer::from_command(privy_only_command(&policy));
    assert_eq!(
        signer.ask(&privy_request(&honest()))["outcome"],
        "authorised"
    );
    std::fs::rename(scratch.0.join("nonces"), scratch.0.join("retained-nonces"))
        .expect("retain state");
    let mut fresh = privy_request(&honest());
    fresh["authorization"]["nonce"] = serde_json::json!("fresh-after-loss");
    attest(&mut fresh);
    assert_eq!(signer.ask(&fresh)["outcome"], "refused");
    assert!(!scratch.0.join("nonces").exists());
}

#[test]
fn blank_nonces_refuse_and_nonce_text_never_becomes_a_path() {
    let scratch = Scratch::new("privy-nonce-text");
    let policy = policy_file(&scratch.0, &open_policy());
    let mut signer = Signer::from_command(privy_only_command(&policy));
    for nonce in ["", " \t\n "] {
        let mut request = privy_request(&honest());
        request["authorization"]["nonce"] = serde_json::json!(nonce);
        attest(&mut request);
        assert_eq!(signer.ask(&request)["outcome"], "refused");
    }
    assert_eq!(
        std::fs::read_dir(scratch.0.join("nonces"))
            .expect("read")
            .count(),
        0
    );
    let mut escaped = privy_request(&honest());
    escaped["authorization"]["nonce"] = serde_json::json!("../outside");
    attest(&mut escaped);
    assert_eq!(signer.ask(&escaped)["outcome"], "authorised");
    assert!(!scratch.0.join("outside").exists());
    assert_eq!(signer.ask(&escaped)["outcome"], "refused");
}

#[test]
fn the_process_refuses_a_forged_or_modified_issuer_intent() {
    let scratch = Scratch::new("issuer-tampering");
    let policy = policy_file(&scratch.0, &open_policy());
    let mut signer = Signer::from_command(privy_only_command(&policy));
    let original = privy_request(&honest());
    // Change the signed values themselves: a fresh clock read can coincide
    // with the original expiry when this loop crosses a second boundary.
    let issued = original["proof"]["issued_at_unix_secs"]
        .as_u64()
        .expect("issued");
    let expires = original["proof"]["expires_at_unix_secs"]
        .as_u64()
        .expect("expires");
    for (path, value) in [
        ("/authorization/nonce", serde_json::json!("forged-fresh")),
        ("/authorization/mint", serde_json::json!(b58(&[0x99; 32]))),
        ("/authorization/action", serde_json::json!("sell")),
        ("/authorization/max_notional", serde_json::json!(1)),
        ("/authorization/expires_after", serde_json::json!(1_151)),
        (
            "/authorization/needs_operator_signature",
            serde_json::json!(true),
        ),
        ("/wallet", serde_json::json!(b58(&[0x44; 32]))),
        ("/now_slot", serde_json::json!(1_001)),
        ("/max_lamports", serde_json::json!(1)),
        ("/request/method", serde_json::json!("GET")),
        (
            "/request/url",
            serde_json::json!("https://api.privy.io/v1/wallets/other/rpc"),
        ),
        ("/request/headers/privy-app-id", serde_json::json!("other")),
        (
            "/request/body/params/transaction",
            serde_json::json!("different"),
        ),
        ("/proof/issued_at_unix_secs", serde_json::json!(issued + 1)),
        (
            "/proof/expires_at_unix_secs",
            serde_json::json!(expires - 1),
        ),
        ("/proof/signature", serde_json::json!("AAAA")),
    ] {
        let mut forged = original.clone();
        *forged.pointer_mut(path).expect("field") = value;
        let answer = signer.ask(&forged);
        assert_eq!(answer["outcome"], "refused", "{path}: {answer}");
    }
    // Invalid proofs must not consume an authentic intent's nonce.
    assert_eq!(signer.ask(&original)["outcome"], "authorised");
}

#[test]
fn only_the_configured_issuer_and_protocol_domain_are_accepted() {
    use ed25519_dalek::Signer as _;
    let scratch = Scratch::new("issuer-key-domain");
    let policy = policy_file(&scratch.0, &open_policy());
    let mut signer = Signer::from_command(privy_only_command(&policy));
    let original = privy_request(&honest());
    let intent = serde_json::from_value(original.clone()).expect("intent");
    let payload = radar_signer::attestation::payload(&intent).expect("payload");
    let other_key = ed25519_dalek::SigningKey::from_bytes(&[0x7C; 32]);
    for signature in [
        other_key.sign(payload.as_bytes()),
        issuer_key().sign(
            payload
                .replace("radar/privy-intent/v1", "other/protocol/v1")
                .as_bytes(),
        ),
    ] {
        let mut forged = original.clone();
        forged["proof"]["signature"] =
            serde_json::json!(radar_types::b64::encode(&signature.to_bytes()));
        let answer = signer.ask(&forged);
        assert_eq!(answer["outcome"], "refused", "{answer}");
    }
    assert_eq!(signer.ask(&original)["outcome"], "authorised");
}

#[test]
fn legacy_or_incomplete_privy_proofs_fail_closed() {
    let scratch = Scratch::new("issuer-missing-proof");
    let policy = policy_file(&scratch.0, &open_policy());
    let mut signer = Signer::from_command(privy_only_command(&policy));
    for name in ["issued_at_unix_secs", "expires_at_unix_secs", "signature"] {
        let mut request = privy_request(&honest());
        request["proof"]
            .as_object_mut()
            .expect("proof")
            .remove(name);
        assert_eq!(signer.ask(&request)["outcome"], "refused", "{name}");
    }
    let mut legacy = privy_request(&honest());
    legacy.as_object_mut().expect("request").remove("proof");
    assert_eq!(signer.ask(&legacy)["outcome"], "refused");
    assert_eq!(
        signer.ask(&privy_request(&honest()))["outcome"],
        "authorised"
    );
}

#[test]
fn expiry_uses_the_signer_clock_and_its_configured_lifetime() {
    let scratch = Scratch::new("issuer-expiry");
    let policy = policy_file(&scratch.0, &open_policy());
    let mut signer = Signer::from_command(privy_only_command(&policy));
    let now = unix_now();
    for (issued, expires) in [
        (now - 60, now - 1),    // expired, even with a caller slot in the past
        (now + 100, now + 160), // future issue time
        (now - 1, now + 60),    // 61 seconds exceeds the operator's 60-second test cap
        (now, now),
        (now, now - 1),
        (0, u64::MAX),
    ] {
        let mut request = privy_request(&honest());
        request["now_slot"] = serde_json::json!(0);
        request["proof"]["issued_at_unix_secs"] = serde_json::json!(issued);
        request["proof"]["expires_at_unix_secs"] = serde_json::json!(expires);
        attest(&mut request);
        let answer = signer.ask(&request);
        assert_eq!(answer["outcome"], "refused", "{issued}/{expires}: {answer}");
    }
    assert_eq!(
        signer.ask(&privy_request(&honest()))["outcome"],
        "authorised"
    );
}

#[test]
fn issuer_time_boundaries_are_exact_and_unsigned_values_never_wrap() {
    let public = radar_types::Address::new(issuer_key().verifying_key().to_bytes());
    let issuer = radar_signer::attestation::Issuer::new(&public, 60).expect("issuer");
    let mut request = privy_request(&honest());
    request["proof"]["issued_at_unix_secs"] = serde_json::json!(100);
    request["proof"]["expires_at_unix_secs"] = serde_json::json!(160);
    attest(&mut request);
    let intent = serde_json::from_value(request.clone()).expect("intent");
    assert!(issuer.check(&intent, 99).is_err());
    assert!(issuer.check(&intent, 100).is_ok());
    assert!(issuer.check(&intent, 159).is_ok());
    assert!(issuer.check(&intent, 160).is_err());
    assert!(issuer.check(&intent, u64::MAX).is_err());
    request["proof"]["issued_at_unix_secs"] = serde_json::json!(u64::MAX - 60);
    request["proof"]["expires_at_unix_secs"] = serde_json::json!(u64::MAX);
    attest(&mut request);
    let intent = serde_json::from_value(request).expect("intent");
    assert!(issuer.check(&intent, u64::MAX - 60).is_ok());
    assert!(issuer.check(&intent, u64::MAX - 1).is_ok());
    assert!(issuer.check(&intent, u64::MAX).is_err());
}

#[test]
fn invalid_issuer_curve_points_refuse_at_startup() {
    let invalid = radar_types::Address::new([0x02; 32]);
    assert!(ed25519_dalek::VerifyingKey::from_bytes(invalid.as_bytes()).is_err());
    assert!(radar_signer::attestation::Issuer::new(&invalid, 60).is_err());
    let scratch = Scratch::new("issuer-invalid-point");
    let policy = policy_file(&scratch.0, &open_policy());
    let mut command = privy_only_command(&policy);
    command.env("RADAR_SIGNER_ISSUER_PUBLIC_KEY", invalid.to_string());
    let mut signer = Signer::from_command(command);
    assert_eq!(signer.ask(&privy_request(&honest()))["outcome"], "refused");
}

#[test]
fn a_non_exact_issuer_payload_is_refused_without_consuming_the_nonce() {
    let scratch = Scratch::new("issuer-payload-float");
    let policy = policy_file(&scratch.0, &open_policy());
    let mut signer = Signer::from_command(privy_only_command(&policy));
    let mut request = privy_request(&honest());
    request["request"]["body"]["extra"] = serde_json::json!(1.25);
    assert_eq!(signer.ask(&request)["outcome"], "refused");
    assert_eq!(
        signer.ask(&privy_request(&honest()))["outcome"],
        "authorised"
    );
}

#[cfg(unix)]
#[test]
fn nonce_state_with_shared_permissions_is_refused() {
    use std::os::unix::fs::PermissionsExt as _;
    let scratch = Scratch::new("privy-permissions");
    let policy = policy_file(&scratch.0, &open_policy());
    let command = privy_only_command(&policy);
    std::fs::set_permissions(
        scratch.0.join("nonces"),
        std::fs::Permissions::from_mode(0o750),
    )
    .expect("permissions");
    let mut signer = Signer::from_command(command);
    assert_eq!(signer.ask(&privy_request(&honest()))["outcome"], "refused");
}
