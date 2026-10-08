// SPDX-License-Identifier: Apache-2.0
//! Offline operator-provisioned kernel issuer. It holds no wallet or Privy key.
//! Configured private files are the trust boundary; stdin carries no authority.
//! This is not yet a live snapshot adapter or a settlement reconciler.

use std::io::{BufRead as _, Read as _, Write as _};
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signer as _, SigningKey};
use radar_journal::{Correlation, Intent, OperationLog};
use radar_risk::{Action, Policy, PortfolioState, Proposal, Verdict};
use radar_signer::protocol::{IntentProof, PrivyAuthorization};
use radar_types::{
    Address, Asset, AssetRole, Balance, Decimals, Holding, Portfolio, TokenQuantity, Valuation,
};
use serde::Deserialize;

#[derive(Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Config {
    active: bool,
    wallet: Address,
    app_id: String,
    wallet_id: String,
    policy: Policy,
    valid_until_unix_secs: u64,
    snapshot_path: PathBuf,
    history_path: PathBuf,
    key_path: PathBuf,
    max_snapshot_age_secs: u64,
    intent_lifetime_secs: u64,
    fee_reserve_lamports: u64,
    programs: Vec<Address>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    wallet: Address,
    observed_at_unix_secs: u64,
    sol_lamports: u64,
    // A conservative upper price bounds how many lamports fit in a USD ceiling.
    sol_upper_micro_usd: u64,
    fee_upper_lamports: u64,
    state: PortfolioState,
    // Until the independent live adapter exists, an operator provisions the
    // measured proposal and exact reviewed bytes together. Stdin cannot invent
    // exit capacity, creator identity, costs or a different transaction.
    proposal: Proposal,
    transaction: String,
    // The operator copies transaction-read output into the private snapshot.
    // Stdin cannot supply or replace this read evidence.
    transaction_evidence: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    proposal: Proposal,
    transaction: String,
}

struct Issuer {
    config_path: PathBuf,
    config: Config,
    key: SigningKey,
    operations: OperationLog,
}

fn private_read(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|_| "configured file unreadable")?;
    let metadata = file
        .metadata()
        .map_err(|_| "configured metadata unreadable")?;
    if !metadata.is_file() {
        return Err("configured path is not a regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("configured file must be private".into());
        }
    }
    let mut bytes = Vec::new();
    file.take(1_048_577)
        .read_to_end(&mut bytes)
        .map_err(|_| "configured file unreadable")?;
    if bytes.len() > 1_048_576 {
        return Err("configured file too large".into());
    }
    Ok(bytes)
}

fn unix_now() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|_| "host clock unavailable".into())
}

fn expired(now: u64, deadline: u64) -> bool {
    now >= deadline
}

fn snapshot_current(now: u64, observed: u64, max_age: u64) -> bool {
    now.checked_sub(observed).is_some_and(|age| age <= max_age)
}

fn evidence_integer(value: &serde_json::Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_str()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| "invalid transaction evidence integer".into())
}

fn transaction_evidence_expiry(
    snapshot: &Snapshot,
    config: &Config,
    checked: &radar_signer::verify::Checked,
    now: u64,
) -> Result<u64, String> {
    let value = &snapshot.transaction_evidence;
    if value["version"] != 1
        || value["authority"] != "read_only"
        || value["commitment"] != "finalized"
        || value["transaction_base64"] != radar_types::b64::encode(checked.bytes())
        || value["message_base64"] != radar_types::b64::encode(checked.signable())
        || value["execution_guaranteed"] != false
        || value["simulation"].get("err") != Some(&serde_json::Value::Null)
        || value["simulation"]["signature_verified"] != false
        || value["simulation"]["blockhash_replaced"] != false
    {
        return Err("transaction evidence does not bind successful exact-byte simulation".into());
    }
    let minimum = evidence_integer(value, "minimum_context_slot")?;
    let simulation = evidence_integer(&value["simulation"], "slot")?;
    let fee = evidence_integer(&value["network_fee"], "slot")?;
    let lamports = evidence_integer(&value["network_fee"], "lamports")?;
    if minimum < snapshot.proposal.oldest_input_slot.get()
        || simulation < minimum
        || fee < minimum
        || simulation > snapshot.state.now.get()
        || fee > snapshot.state.now.get()
        || lamports > snapshot.fee_upper_lamports
    {
        return Err("transaction evidence slots or network fee exceed reviewed bounds".into());
    }
    let started = value["read_started_at_unix_secs"]
        .as_u64()
        .ok_or("missing evidence start time")?;
    let completed = value["read_completed_at_unix_secs"]
        .as_u64()
        .ok_or("missing evidence completion time")?;
    if completed < started
        || completed > now
        || !snapshot_current(now, started, config.max_snapshot_age_secs)
    {
        return Err("transaction evidence read window is not current".into());
    }
    started
        .checked_add(config.max_snapshot_age_secs)
        .ok_or_else(|| "evidence expiry overflow".into())
}

impl Issuer {
    fn load(path: &Path) -> Result<Self, String> {
        let config: Config = serde_json::from_slice(&private_read(path)?)
            .map_err(|_| "invalid issuer configuration")?;
        if !config.active
            || config.policy.is_closed()
            || config.max_snapshot_age_secs == 0
            || config.intent_lifetime_secs == 0
            || config.fee_reserve_lamports == 0
            || config.programs.is_empty()
        {
            return Err("issuer requires an active bounded configuration".into());
        }
        radar_signer::privy::WalletScope::new(&config.app_id, &config.wallet_id, config.wallet)
            .map_err(|_| "invalid wallet scope")?;
        // Never create missing capital history as an empty account.
        private_read(&config.history_path)?;
        let operations =
            OperationLog::open(&config.history_path).map_err(|_| "history unavailable")?;
        let seed: [u8; 32] = serde_json::from_slice(&private_read(&config.key_path)?)
            .map_err(|_| "invalid issuer key")?;
        Ok(Self {
            config_path: path.to_owned(),
            config,
            key: SigningKey::from_bytes(&seed),
            operations,
        })
    }

    fn prepare(
        &self,
        candidate: &Candidate,
        snapshot: &Snapshot,
        now: u64,
    ) -> Result<(PrivyAuthorization, String, u64), String> {
        let mut authorization =
            match radar_risk::evaluate(&candidate.proposal, &snapshot.state, &self.config.policy) {
                // Startup permits only self-authorising policies; the kernel
                // cannot produce an operator-required decision under them.
                Verdict::Authorised(value) => *value,
                Verdict::Refused { .. } => return Err("risk kernel refused".into()),
            };
        // Narrow the kernel window to the signer's policy window.
        authorization.expires_after = radar_types::Slot(
            authorization.expires_after.get().min(
                snapshot
                    .state
                    .now
                    .get()
                    .checked_add(self.config.policy.max_input_staleness.get())
                    .ok_or("slot expiry overflow")?,
            ),
        );
        let max_lamports = u64::try_from(
            u128::from(authorization.max_notional.get()) * 1_000_000_000
                / u128::from(snapshot.sol_upper_micro_usd),
        )
        .map_err(|_| "lamport ceiling overflow")?;
        if max_lamports == 0 {
            return Err("lamport ceiling is zero".into());
        }
        let reserved = max_lamports
            .checked_add(self.config.fee_reserve_lamports)
            .ok_or("fee reservation overflow")?;
        let bytes = radar_types::b64::decode(&candidate.transaction)
            .ok_or("invalid transaction encoding")?;
        let allowlist = radar_signer::Allowlist {
            programs: self.config.programs.iter().map(|a| *a.as_bytes()).collect(),
        };
        let checked = radar_signer::check(
            &authorization,
            &bytes,
            &self.config.wallet,
            &allowlist,
            &self.config.policy,
            radar_signer::verify::CallerBounds {
                now: snapshot.state.now,
                max_lamports,
            },
        )
        .map_err(|_| "transaction refused")?;
        let evidence_expiry = transaction_evidence_expiry(snapshot, &self.config, &checked, now)?;
        let expires = now
            .checked_add(self.config.intent_lifetime_secs)
            .ok_or("intent expiry overflow")?
            .min(self.config.valid_until_unix_secs)
            .min(evidence_expiry)
            .min(
                snapshot
                    .observed_at_unix_secs
                    .checked_add(self.config.max_snapshot_age_secs)
                    .ok_or("snapshot expiry overflow")?,
            );
        if expired(now, expires) {
            return Err("snapshot has no remaining validity".into());
        }
        let intent = PrivyAuthorization {
            authorization,
            request: radar_signer::privy::PrivyRequest {
                method: "POST".into(),
                url: format!(
                    "https://api.privy.io/v1/wallets/{}/rpc",
                    self.config.wallet_id
                ),
                headers: [("privy-app-id".into(), serde_json::json!(self.config.app_id))]
                    .into_iter()
                    .collect(),
                body: serde_json::json!({"method":"signTransaction", "params":{"encoding":"base64", "transaction":candidate.transaction}}),
            },
            wallet: self.config.wallet.to_string(),
            now_slot: snapshot.state.now.get(),
            max_lamports,
            proof: IntentProof {
                issued_at_unix_secs: now,
                expires_at_unix_secs: expires,
                signature: String::new(),
            },
        };
        // Encode before reserving: an encoding refusal never holds capital.
        let payload = radar_signer::attestation::payload(&intent).map_err(str::to_owned)?;
        Ok((intent, payload, reserved))
    }

    fn issue(&mut self, candidate: &Candidate) -> Result<PrivyAuthorization, String> {
        let now = unix_now()?;
        let current: Config = serde_json::from_slice(&private_read(&self.config_path)?)
            .map_err(|_| "invalid current mandate")?;
        if current != self.config {
            return Err("mandate changed; restart issuer against current configuration".into());
        }
        if expired(now, self.config.valid_until_unix_secs) {
            return Err("mandate expired".into());
        }
        // Without settlement reconciliation, no second decision may assume
        // the first one's exposure, loss or unknown submission vanished.
        if self.operations.outstanding().next().is_some() {
            return Err("outstanding operation requires reconciliation".into());
        }
        let snapshot: Snapshot = serde_json::from_slice(&private_read(&self.config.snapshot_path)?)
            .map_err(|_| "invalid trusted snapshot")?;
        if snapshot.wallet != self.config.wallet
            || snapshot.sol_upper_micro_usd == 0
            || !snapshot_current(
                now,
                snapshot.observed_at_unix_secs,
                self.config.max_snapshot_age_secs,
            )
        {
            return Err("snapshot is not current for the configured wallet".into());
        }
        if candidate.proposal != snapshot.proposal || candidate.transaction != snapshot.transaction
        {
            return Err("candidate does not match independently provisioned evidence".into());
        }
        if snapshot.fee_upper_lamports == 0
            || snapshot.fee_upper_lamports > self.config.fee_reserve_lamports
        {
            return Err("reviewed transaction fee is not covered by the reservation".into());
        }
        if candidate.proposal.action != Action::Buy
            || candidate.proposal.quote != Asset::Sol
            || candidate.proposal.notional.get() == 0
            || candidate.proposal.oldest_input_slot > snapshot.state.now
        {
            return Err("issuer currently accepts only current native-SOL buys".into());
        }
        let (mut intent, payload, reserved) = self.prepare(candidate, &snapshot, now)?;
        let mut portfolio = Portfolio::at(self.config.wallet, snapshot.state.now);
        portfolio
            .hold(
                Asset::Sol,
                Holding::new(
                    AssetRole::Cash,
                    Balance::Counted(TokenQuantity::new(
                        snapshot.sol_lamports,
                        Decimals::NATIVE_SOL,
                    )),
                    Valuation::Unknown(radar_types::Unvaluable::QuoteUnpriced),
                    Valuation::Unknown(radar_types::Unvaluable::QuoteUnpriced),
                ),
            )
            .map_err(|_| "wallet balance unusable")?;
        let id = self
            .operations
            .propose(
                Intent {
                    asset: Asset::Sol,
                    amount: TokenQuantity::new(reserved, Decimals::NATIVE_SOL),
                    at: snapshot.state.now,
                },
                now,
                Correlation {
                    mint: Some(intent.authorization.mint.to_string()),
                    receipt: Some(intent.authorization.nonce.clone()),
                    ..Correlation::default()
                },
            )
            .map_err(|_| "proposal could not be persisted")?;
        self.operations
            .reserve(&id, &mut portfolio, now)
            .map_err(|_| "capital reservation refused")?;
        // Once the proof leaves this process it may cause an effect. Persist
        // unknown before signing/output, and never clear it on a pipe error.
        let key = &self.key;
        self.operations
            .submit(&id, now, |_| {
                intent.proof.signature =
                    radar_types::b64::encode(&key.sign(payload.as_bytes()).to_bytes());
                Ok::<(), ()>(())
            })
            .map_err(|_| "issuance could not be persisted")?
            .map_err(|()| "issuance failed")?;
        if expired(unix_now()?, intent.proof.expires_at_unix_secs) {
            return Err("intent expired during persistence".into());
        }
        Ok(intent)
    }
}

fn run() -> Result<(), String> {
    let path = std::env::var_os("RADAR_ISSUER_CONFIG").ok_or("issuer configuration missing")?;
    let mut issuer = Issuer::load(Path::new(&path))?;
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut output = std::io::stdout().lock();
    writeln!(
        output,
        "{}",
        serde_json::json!({"outcome":"ready", "issuer_public_key":
        Address::new(issuer.key.verifying_key().to_bytes()).to_string()})
    )
    .and_then(|()| output.flush())
    .map_err(|_| "output failed")?;
    loop {
        let mut raw = Vec::new();
        let count = input
            .by_ref()
            .take(65_537)
            .read_until(b'\n', &mut raw)
            .map_err(|_| "input failed")?;
        if count == 0 {
            return Ok(());
        }
        if count > 65_536 || raw.last() != Some(&b'\n') {
            return Err("candidate line too large or incomplete".into());
        }
        let answer = serde_json::from_slice::<Candidate>(&raw)
            .map_err(|_| "invalid candidate".to_owned())
            .and_then(|candidate| issuer.issue(&candidate));
        let value = match answer {
            Ok(intent) => serde_json::json!({"outcome":"issued", "intent":intent}),
            Err(reason) => serde_json::json!({"outcome":"refused", "reason":reason}),
        };
        writeln!(output, "{value}")
            .and_then(|()| output.flush())
            .map_err(|_| "output failed")?;
    }
}

fn main() {
    if let Err(reason) = run() {
        eprintln!("radar-issuer refused: {reason}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn time_boundaries_are_exclusive_and_future_reads_are_not_fresh() {
        assert!(!super::expired(99, 100));
        assert!(super::expired(100, 100));
        assert!(super::expired(101, 100));
        assert!(super::snapshot_current(120, 100, 20));
        assert!(super::snapshot_current(100, 100, 20));
        assert!(!super::snapshot_current(121, 100, 20));
        assert!(!super::snapshot_current(99, 100, 20));
    }
}
