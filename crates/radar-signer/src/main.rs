// SPDX-License-Identifier: Apache-2.0
//! The signer process.
//!
//! Runs as its own systemd unit under its own user, with the key file readable
//! by nobody else. It has no network, no listener and no RPC: requests arrive on
//! stdin as newline-delimited JSON and answers leave on stdout.
//!
//! Everything that decides anything lives in the library, so this file is short
//! enough to read in full before trusting it. That is the point of it being
//! short.
//!
//! # Configuration
//!
//! - `RADAR_SIGNER_MODE` — `local` (default) or `privy`. Privy-only instances
//!   never load a Solana private key and refuse local signing requests.
//! - `RADAR_SIGNER_KEY` — required Solana keypair JSON path in local mode.
//! - `RADAR_PRIVY_AUTHORIZATION_KEY` — optional in local mode, required in Privy
//!   mode. A configured key also requires `RADAR_SIGNER_PRIVY_APP_ID`,
//!   `RADAR_SIGNER_PRIVY_WALLET_ID` and `RADAR_SIGNER_PRIVY_WALLET_ADDRESS`.
//!   These bind signing requests to trusted startup configuration.
//! - `RADAR_SIGNER_NONCE_DIR` — existing private persistent directory, required
//!   with any Privy key. Each nonce is consumed before the key is used, even if
//!   the attempt then fails. Missing state refuses rather than being recreated.
//! - `RADAR_SIGNER_ISSUER_PUBLIC_KEY` — trusted base58 Ed25519 public key, and
//!   `RADAR_SIGNER_MAX_INTENT_LIFETIME_SECS` — required positive validity cap.
//!   Both are required with a Privy key. No issuer private key is loaded here.
//! - `RADAR_SIGNER_PROGRAMS` — comma-separated base58 program ids that may
//!   appear in a signed transaction. Absent means every request is refused; an
//!   empty allowlist is a signer that will sign anything, and that is the one
//!   configuration mistake with no upper bound on its cost.

use std::io::{BufRead as _, Write as _};
use std::path::PathBuf;

use radar_risk::Policy;
use radar_signer::attestation::Issuer;
use radar_signer::privy::{AuthorizationKey, WalletScope, authorise};
use radar_signer::protocol::{Envelope, PrivyAuthorization, Response, bounds_of, place_signature};
use radar_signer::replay::ReplayStore;
use radar_signer::{Allowlist, Key, check};
use radar_types::{Address, Slot};

fn main() -> std::process::ExitCode {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    // Loaded once, at start. A signer that re-reads its allowlist per request
    // is a signer whose rules can be changed by whoever can write that file
    // while it runs.
    let config = match Config::from_env() {
        Ok(c) => Some(c),
        Err(why) => {
            // Still serve, still refuse. Exiting would make a misconfiguration
            // look like a crash, and the executor would retry it forever.
            eprintln!("radar-signer: refusing everything: {why}");
            None
        }
    };

    if let Some(c) = &config {
        eprintln!(
            "radar-signer: ready as {} with {} allowed programs, policy {:?} up to {} micro-USD",
            c.key
                .as_ref()
                .map_or_else(|| "Privy-only".to_owned(), |key| key.public().to_string()),
            c.allowlist.programs.len(),
            c.policy.autonomy,
            c.policy.max_position.get()
        );
    }

    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }

        let response = config.as_ref().map_or_else(
            || Response::refused("signer is not configured"),
            |c| handle(&line, c),
        );

        // Every decision is logged before it is returned. A signature that
        // reached the chain with no line here would mean this process was not
        // the one that made it.
        eprintln!(
            "radar-signer: {}",
            match &response {
                Response::Signed { signature, .. } => format!("signed {signature}"),
                // The signature itself is a header value for a request that has
                // not been sent yet, and logging it would put a usable
                // authorisation in a file with looser permissions than the key
                // that made it. The nonce is what a later question needs.
                Response::Authorised { .. } => "authorised a Privy request".to_owned(),
                Response::Refused { reasons } => format!("refused: {}", reasons.join("; ")),
            }
        );

        let Ok(json) = serde_json::to_string(&response) else {
            continue;
        };
        if writeln!(stdout, "{json}").is_err() || stdout.flush().is_err() {
            break;
        }
    }
    std::process::ExitCode::SUCCESS
}

/// What the process was told at start.
struct Config {
    key: Option<Key>,
    allowlist: Allowlist,
    /// This signer's own policy, and the reason it is here rather than taken
    /// from the caller.
    ///
    /// The Privy process verifies an issuer-signed intent and consumes its nonce
    /// before key use. This authenticates a configured key, not kernel execution
    /// or portfolio state: the offline issuer relies on separately provisioned
    /// evidence and has no live snapshot adapter or reconciliation yet.
    /// Every one of them is clamped against this, unconditionally
    /// ([ADR 0008](https://github.com/hey-vera/radar/blob/main/docs/adr/0008-the-signer-holds-its-own-policy.md)).
    ///
    /// Loaded once, at start, like the allowlist and for the same reason: rules
    /// re-read per request are rules whoever can write that file can change
    /// while the process runs.
    policy: Policy,
    /// The Privy authorization key, when this instance serves customers.
    ///
    /// `None` is a refusal for the customer lane and leaves the local lane
    /// untouched. An instance with no customers needs no customer key, and
    /// refusing to start without one would take down the lane that works.
    privy: Option<(AuthorizationKey, WalletScope, ReplayStore, Issuer)>,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        let mode = std::env::var("RADAR_SIGNER_MODE").unwrap_or_else(|_| "local".to_owned());
        let key = match mode.as_str() {
            "local" => {
                let path = std::env::var("RADAR_SIGNER_KEY")
                    .map_err(|_| "RADAR_SIGNER_KEY is not set".to_owned())?;
                Some(Key::load(&PathBuf::from(path)).map_err(|e| e.to_string())?)
            }
            "privy" => None,
            _ => return Err("RADAR_SIGNER_MODE must be local or privy".to_owned()),
        };

        let listed = std::env::var("RADAR_SIGNER_PROGRAMS")
            .map_err(|_| "RADAR_SIGNER_PROGRAMS is not set".to_owned())?;
        let mut programs = Vec::new();
        for entry in listed.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let address: Address = entry
                .parse()
                .map_err(|_| format!("`{entry}` is not a base58 address"))?;
            programs.push(*address.as_bytes());
        }
        if programs.is_empty() {
            // An empty allowlist would sign anything. Of every misconfiguration
            // available here, it is the one with no upper bound on its cost.
            return Err("RADAR_SIGNER_PROGRAMS is empty".to_owned());
        }

        // No default, and no fallback. Rule 8: a signer with no policy loaded
        // refuses everything rather than accepting whatever bounds the caller
        // asserts, which is the state this ran in until ADR 0008.
        let policy_path = std::env::var("RADAR_SIGNER_POLICY")
            .map_err(|_| "RADAR_SIGNER_POLICY is not set".to_owned())?;
        let policy_text = std::fs::read_to_string(&policy_path)
            .map_err(|e| format!("cannot read {policy_path}: {e}"))?;
        let policy: Policy = serde_json::from_str(&policy_text)
            .map_err(|e| format!("{policy_path} is not a policy: {e}"))?;

        // Optional, and its absence is a refusal rather than a failure to
        // start. An instance with no customers needs no customer key, and
        // refusing to run without one would take the local lane down too.
        let privy = match std::env::var("RADAR_PRIVY_AUTHORIZATION_KEY") {
            Ok(material) if !material.trim().is_empty() => {
                let app = std::env::var("RADAR_SIGNER_PRIVY_APP_ID")
                    .map_err(|_| "RADAR_SIGNER_PRIVY_APP_ID is not set".to_owned())?;
                let wallet_id = std::env::var("RADAR_SIGNER_PRIVY_WALLET_ID")
                    .map_err(|_| "RADAR_SIGNER_PRIVY_WALLET_ID is not set".to_owned())?;
                let wallet = std::env::var("RADAR_SIGNER_PRIVY_WALLET_ADDRESS")
                    .map_err(|_| "RADAR_SIGNER_PRIVY_WALLET_ADDRESS is not set".to_owned())?
                    .parse::<Address>()
                    .map_err(|_| "configured wallet is not a base58 address".to_owned())?;
                let nonce_dir = std::env::var("RADAR_SIGNER_NONCE_DIR")
                    .map_err(|_| "RADAR_SIGNER_NONCE_DIR is not set".to_owned())?;
                let issuer = std::env::var("RADAR_SIGNER_ISSUER_PUBLIC_KEY")
                    .map_err(|_| "RADAR_SIGNER_ISSUER_PUBLIC_KEY is not set".to_owned())?
                    .parse::<Address>()
                    .map_err(|_| "issuer key is not a base58 public key".to_owned())?;
                let lifetime = std::env::var("RADAR_SIGNER_MAX_INTENT_LIFETIME_SECS")
                    .map_err(|_| "RADAR_SIGNER_MAX_INTENT_LIFETIME_SECS is not set".to_owned())?
                    .parse::<u64>()
                    .map_err(|_| "intent lifetime is not an unsigned integer".to_owned())?;
                Some((
                    AuthorizationKey::parse(&material).map_err(|e| e.to_string())?,
                    WalletScope::new(&app, &wallet_id, wallet).map_err(|e| e.to_string())?,
                    ReplayStore::at(&PathBuf::from(nonce_dir)).map_err(|e| e.to_string())?,
                    Issuer::new(&issuer, lifetime).map_err(str::to_owned)?,
                ))
            }
            _ => None,
        };
        if mode == "privy" && privy.is_none() {
            return Err("no Privy authorization key is configured".to_owned());
        }

        Ok(Self {
            key,
            allowlist: Allowlist { programs },
            policy,
            privy,
        })
    }
}

/// Handles a request for a Privy authorization signature.
///
/// The wallet is bound to startup configuration, so `config.key` is not
/// involved at all. What is involved is the same `verify::check` -- through
/// `privy::authorise`, which reads the transaction out of the request body
/// rather than being handed one.
fn handle_privy(privy: &PrivyAuthorization, config: &Config) -> Response {
    let Some((key, scope, replay, issuer)) = config.privy.as_ref() else {
        // Rule 8. No key means no customer signing, not an unsigned request --
        // an unsigned request would simply be rejected by Privy, but reporting
        // it as anything other than "not configured" would send an operator to
        // the wrong place.
        return Response::refused("no Privy authorization key is configured");
    };
    let Ok(wallet) = privy.wallet.parse::<Address>() else {
        return Response::refused("the wallet is not a base58 address");
    };
    if &wallet != scope.wallet() {
        return Response::refused("the wallet does not match the configured Privy wallet");
    }
    if let Err(why) = check_issuer(privy, issuer) {
        return Response::refused(why);
    }
    // Reserve before using the key. Even a rejected or interrupted attempt is
    // consumed: retry requires a newly issued authorization, not erasing state.
    if replay.claim(&privy.authorization.nonce).is_err() {
        return Response::refused("authorization nonce is reused, empty or cannot be persisted");
    }
    // Persistence may block. Recheck the clock after it, immediately before
    // checked key use; an expired attempt remains consumed.
    if let Err(why) = check_issuer(privy, issuer) {
        return Response::refused(why);
    }

    match authorise(
        key,
        &privy.request,
        &privy.authorization,
        scope,
        &config.allowlist,
        &config.policy,
        radar_signer::verify::CallerBounds {
            now: Slot(privy.now_slot),
            max_lamports: privy.max_lamports,
        },
    ) {
        Ok(signature) => Response::Authorised { signature },
        Err(why) => Response::refused(why.to_string()),
    }
}

fn check_issuer(intent: &PrivyAuthorization, issuer: &Issuer) -> Result<(), &'static str> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "signer clock is unavailable")?;
    issuer.check(intent, now.as_secs())
}

/// Handles one request.
fn handle(line: &str, config: &Config) -> Response {
    // Unparseable is refused, and that includes an untagged request from an
    // older caller. A deployment that updates one side and not the other stops
    // signing rather than guessing which kind of signature was wanted.
    let Ok(envelope) = serde_json::from_str::<Envelope>(line) else {
        return Response::refused("unreadable request");
    };
    let request = match envelope {
        Envelope::Local(request) => request,
        Envelope::Privy(privy) => return handle_privy(&privy, config),
    };
    let Some(key) = config.key.as_ref() else {
        return Response::refused("local Solana signing is disabled in Privy-only mode");
    };
    let Some(bytes) = radar_types::b64::decode(&request.transaction) else {
        return Response::refused("transaction is not base64");
    };

    let checked = match check(
        &request.authorization,
        &bytes,
        &key.public(),
        &config.allowlist,
        &config.policy,
        bounds_of(&request),
    ) {
        Ok(c) => c,
        Err(rejections) => {
            return Response::Refused {
                reasons: rejections.iter().map(ToString::to_string).collect(),
            };
        }
    };

    let signature = key.sign(&checked);
    let Some(signed) = place_signature(
        checked.bytes(),
        checked.message().message_offset,
        0,
        signature.as_bytes(),
    ) else {
        // The transaction verified but has no room for a signature. That is the
        // executor's bug, and signing something else to work around it is not
        // this process's decision to make.
        return Response::refused("no signature slot in the transaction");
    };

    Response::Signed {
        signature: signature.to_string(),
        wallet: key.public().to_string(),
        transaction: radar_types::b64::encode(&signed),
    }
}
