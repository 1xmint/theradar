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

#[path = "radar-issuer/settlement.rs"]
mod settlement;

#[path = "radar-issuer/sale_proceeds.rs"]
mod sale_proceeds;
#[path = "radar-issuer/valuation.rs"]
mod valuation;

#[path = "radar-issuer/basis.rs"]
mod basis;

#[path = "radar-issuer/acquisitions.rs"]
mod acquisitions;

#[path = "radar-issuer/native_transfers.rs"]
mod native_transfers;

#[path = "radar-issuer/cash.rs"]
mod cash;

#[path = "radar-issuer/inventory.rs"]
mod inventory;

#[path = "radar-issuer/opening.rs"]
mod opening;

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
    // Operator's coverage assertion, not proof of economic correctness.
    // Empty only for a journal with no events. Never accepted from stdin.
    accounting_checkpoint: String,
    // Until the independent live adapter exists, an operator provisions the
    // measured proposal and exact reviewed bytes together. Stdin cannot invent
    // exit capacity, creator identity, costs or a different transaction.
    proposal: Proposal,
    transaction: String,
    // The operator copies transaction-read output into the private snapshot.
    // Stdin cannot supply or replace this read evidence.
    transaction_evidence: serde_json::Value,
    // Bind the reviewed balance to protected wallet-read evidence.
    wallet_evidence: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    proposal: Proposal,
    transaction: String,
}

impl Snapshot {
    fn execution_binding(
        &self,
        transaction: &str,
    ) -> Result<radar_journal::ExecutionBinding, String> {
        Ok(radar_journal::ExecutionBinding {
            wallet: self.wallet,
            transaction: transaction.to_owned(),
            signed_transaction: None,
            reviewed_proposal: Some(
                serde_json::to_value(&self.proposal)
                    .map_err(|_| "reviewed proposal could not be normalized")?,
            ),
        })
    }
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
        .ok_or_else(|| "invalid read evidence integer".into())
}

fn wallet_evidence_expiry(snapshot: &Snapshot, config: &Config, now: u64) -> Result<u64, String> {
    let value = &snapshot.wallet_evidence;
    if value["version"] != 1
        || value["authority"] != "read_only"
        || value["commitment"] != "finalized"
        || value["wallet"] != config.wallet.to_string()
        || value["native_sol"]["decimals"] != 9
        || evidence_integer(&value["native_sol"], "raw_amount")? != snapshot.sol_lamports
        || !value["token_program"]["accounts"].is_array()
        || !value["token_2022"]["accounts"].is_array()
    {
        return Err("wallet evidence does not bind the reviewed native balance".into());
    }
    // Preserve separate bank contexts; matching slot numbers are not proof of
    // an atomic read. Token quantities are not dollar exposure or realised loss.
    for read in ["native_sol", "token_program", "token_2022"] {
        let slot = evidence_integer(&value[read], "slot")?;
        if slot < snapshot.proposal.oldest_input_slot.get() || slot > snapshot.state.now.get() {
            return Err("wallet evidence slot is outside reviewed bounds".into());
        }
    }
    read_evidence_expiry(value, config, now)
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
    read_evidence_expiry(value, config, now)
}

fn read_evidence_expiry(
    value: &serde_json::Value,
    config: &Config,
    now: u64,
) -> Result<u64, String> {
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
        return Err("evidence read window is not current".into());
    }
    started
        .checked_add(config.max_snapshot_age_secs)
        .ok_or_else(|| "evidence expiry overflow".into())
}

fn verified_signed(
    binding: &radar_journal::ExecutionBinding,
    wallet: Address,
    signed: &[u8],
) -> Result<String, String> {
    if binding.wallet != wallet {
        return Err("operation wallet does not match configured wallet".into());
    }
    let unsigned =
        radar_types::b64::decode(&binding.transaction).ok_or("invalid recorded transaction")?;
    if signed.len() > 1232
        || signed.len() < 134
        || signed[0] != 1
        || signed[65] != 1
        || unsigned.len() != signed.len()
        || unsigned[0] != 1
        || unsigned[65..] != signed[65..]
    {
        return Err("signed transaction does not bind exact authorized bytes".into());
    }
    let message = radar_signer::tx::decode(signed).map_err(|_| "invalid signed transaction")?;
    if message.fee_payer() != Some(*binding.wallet.as_bytes()) {
        return Err("signed transaction has unsupported wallet/signers".into());
    }
    let signature: [u8; 64] = signed[1..65]
        .try_into()
        .map_err(|_| "invalid signature extent")?;
    ed25519_dalek::VerifyingKey::from_bytes(binding.wallet.as_bytes())
        .map_err(|_| "invalid wallet public key")?
        .verify_strict(
            &signed[65..],
            &ed25519_dalek::Signature::from_bytes(&signature),
        )
        .map_err(|_| "wallet signature verification failed")?;
    Ok(radar_types::b64::encode(signed))
}

impl Issuer {
    fn inventory_mode(&mut self, args: &[String]) -> Result<bool, String> {
        if args == ["--record-opening-inventory"] {
            self.record_opening_inventory()?;
            println!(
                "{}",
                serde_json::json!({"opening_inventory_recorded":true,"portfolio_state_updated":false})
            );
        } else if args == ["--record-native-transfers"] {
            let count = self.record_native_transfers()?;
            println!(
                "{}",
                serde_json::json!({"native_transfers_advanced":count,"accounting_checkpoint":self.operations.checkpoint(),
                "portfolio_state_updated":false,"reservation_released":false})
            );
        } else if args == ["--review-inventory"] {
            println!("{}", self.review_inventory()?);
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    fn review_inventory(&self) -> Result<serde_json::Value, String> {
        let snapshot: Snapshot = serde_json::from_slice(&private_read(&self.config.snapshot_path)?)
            .map_err(|_| "invalid trusted snapshot")?;
        let history = acquisitions::review(&self.operations, &self.config)?;
        inventory::review(
            &snapshot,
            &self.config,
            &history,
            unix_now()?,
            self.operations.opening_inventory(),
            &self
                .operations
                .native_transfers()
                .cloned()
                .collect::<Vec<_>>(),
        )
    }

    fn record_native_transfers(&mut self) -> Result<usize, String> {
        let snapshot: Snapshot = serde_json::from_slice(&private_read(&self.config.snapshot_path)?)
            .map_err(|_| "invalid trusted snapshot")?;
        let now = unix_now()?;
        let history = acquisitions::review(&self.operations, &self.config)?;
        if snapshot.wallet != self.config.wallet
            || snapshot.accounting_checkpoint != self.operations.checkpoint()
            || !snapshot_current(
                now,
                snapshot.observed_at_unix_secs,
                self.config.max_snapshot_age_secs,
            )
        {
            return Err("transfer snapshot does not cover wallet, time and current journal".into());
        }
        let records =
            native_transfers::capture(&snapshot.wallet_evidence, &history, &self.config, now)?;
        let mut advanced = 0;
        for record in records {
            if self
                .operations
                .record_native_transfer(record, now)
                .map_err(|_| "native transfer could not be retained")?
                == radar_journal::Applied::Advanced
            {
                advanced += 1;
            }
        }
        Ok(advanced)
    }

    fn record_opening_inventory(&mut self) -> Result<(), String> {
        let snapshot: Snapshot = serde_json::from_slice(&private_read(&self.config.snapshot_path)?)
            .map_err(|_| "invalid trusted snapshot")?;
        let record = opening::capture(&snapshot, &self.config, unix_now()?)?;
        self.operations
            .record_opening_inventory(record, unix_now()?)
            .map_err(|_| "opening inventory could not be persisted")?;
        Ok(())
    }

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
        let wallet_expiry = wallet_evidence_expiry(snapshot, &self.config, now)?;
        let expires = now
            .checked_add(self.config.intent_lifetime_secs)
            .ok_or("intent expiry overflow")?
            .min(self.config.valid_until_unix_secs)
            .min(evidence_expiry)
            .min(wallet_expiry)
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
                body: serde_json::json!({"method":"signTransaction", "params":{"encoding":"base64", "transaction":radar_types::b64::encode(checked.bytes())}}),
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
        if snapshot.accounting_checkpoint != self.operations.checkpoint() {
            return Err("snapshot accounting does not cover current journal history".into());
        }
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
                    execution: Some(
                        snapshot.execution_binding(
                            intent
                                .request
                                .transaction()
                                .ok_or("request transaction missing")?,
                        )?,
                    ),
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

    fn bind_signed(&mut self, operation: &str, path: &Path) -> Result<(), String> {
        let id = self
            .operations
            .outstanding()
            .find(|(id, _)| id.as_str() == operation)
            .map(|(id, _)| id.clone())
            .ok_or("unknown outstanding operation")?;
        let binding = self
            .operations
            .execution(&id)
            .ok_or("operation has no transaction binding")?;
        let signed = verified_signed(binding, self.config.wallet, &private_read(path)?)?;
        self.operations
            .record_signed(&id, signed, unix_now()?)
            .map_err(|_| "signed transaction could not be persisted")?;
        Ok(())
    }

    fn review_settlement(&self, operation: &str, path: &Path) -> Result<serde_json::Value, String> {
        let (id, entry) = self
            .operations
            .outstanding()
            .find(|(id, _)| id.as_str() == operation)
            .ok_or("unknown outstanding operation")?;
        let binding = self
            .operations
            .execution(id)
            .ok_or("operation has no transaction binding")?;
        let signed = radar_types::b64::decode(
            binding
                .signed_transaction
                .as_deref()
                .ok_or("operation has no signed transaction")?,
        )
        .ok_or("invalid signed binding")?;
        verified_signed(binding, self.config.wallet, &signed)?;
        let evidence =
            serde_json::from_slice(&private_read(path)?).map_err(|_| "invalid settlement JSON")?;
        let mut review = settlement::review(
            binding,
            entry,
            &evidence,
            unix_now()?,
            self.config.max_snapshot_age_secs,
        )?;
        review["operation"] = serde_json::json!(operation);
        Ok(review)
    }

    fn record_settlement(&mut self, operation: &str, path: &Path) -> Result<(), String> {
        // Review exactly one read of the protected input before retaining only
        // its normalized facts. No reread can substitute an unchecked packet.
        let review = self.review_settlement(operation, path)?;
        let id = self
            .operations
            .outstanding()
            .find(|(id, _)| id.as_str() == operation)
            .map(|(id, _)| id.clone())
            .ok_or("unknown outstanding operation")?;
        let signed_transaction = self
            .operations
            .execution(&id)
            .and_then(|binding| binding.signed_transaction.clone())
            .ok_or("operation has no signed transaction")?;
        self.operations
            .record_settlement(
                &id,
                radar_journal::SettlementRecord {
                    signed_transaction,
                    review,
                },
                unix_now()?,
            )
            .map_err(|_| "settlement evidence could not be persisted")?;
        Ok(())
    }

    fn review_valuation(&self, operation: &str, path: &Path) -> Result<serde_json::Value, String> {
        let (id, entry) = self
            .operations
            .outstanding()
            .find(|(id, _)| id.as_str() == operation)
            .ok_or("unknown outstanding operation")?;
        let binding = self
            .operations
            .execution(id)
            .ok_or("operation has no transaction binding")?;
        let record = self
            .operations
            .settlement(id)
            .ok_or("operation has no retained settlement facts")?;
        let signed =
            radar_types::b64::decode(&record.signed_transaction).ok_or("invalid signed binding")?;
        verified_signed(binding, self.config.wallet, &signed)?;
        if record.review["operation"] != operation {
            return Err("retained settlement operation does not match".into());
        }
        let price = serde_json::from_slice(&private_read(path)?)
            .map_err(|_| "invalid protected price JSON")?;
        let mut report = valuation::review(
            binding,
            entry,
            record,
            price,
            &self.config.policy,
            self.config.max_snapshot_age_secs,
        )?;
        report["operation"] = serde_json::json!(operation);
        Ok(report)
    }

    fn record_valuation(&mut self, operation: &str, path: &Path) -> Result<(), String> {
        // Review one protected input read before retaining normalized costs.
        let review = self.review_valuation(operation, path)?;
        if !review["acquisition_costs"].is_object()
            && !review["failed_execution_costs"].is_object()
            && !review["sale_proceeds"].is_object()
        {
            return Err("recording valuation requires classified complete costs".into());
        }
        let id = self
            .operations
            .outstanding()
            .find(|(id, _)| id.as_str() == operation)
            .map(|(id, _)| id.clone())
            .ok_or("unknown outstanding operation")?;
        let settlement = self
            .operations
            .settlement(&id)
            .cloned()
            .ok_or("operation has no retained settlement facts")?;
        self.operations
            .record_valuation(
                &id,
                radar_journal::ValuationRecord { settlement, review },
                unix_now()?,
            )
            .map_err(|_| "valuation could not be persisted")?;
        Ok(())
    }
}

fn run() -> Result<(), String> {
    let path = std::env::var_os("RADAR_ISSUER_CONFIG").ok_or("issuer configuration missing")?;
    let mut issuer = Issuer::load(Path::new(&path))?;
    let args: Vec<_> = std::env::args().skip(1).collect();
    if issuer.inventory_mode(&args)? {
        return Ok(());
    }
    if args == ["--review-acquisitions"] {
        println!(
            "{}",
            acquisitions::review(&issuer.operations, &issuer.config)?
        );
        return Ok(());
    }
    if !args.is_empty() {
        if args.len() != 3 {
            return Err(
                "usage: radar-issuer --bind-signed <operation-id> <private-signed-binary-file>"
                    .into(),
            );
        }
        if args[0] == "--review-settlement" {
            println!(
                "{}",
                issuer.review_settlement(&args[1], Path::new(&args[2]))?
            );
            return Ok(());
        }
        if args[0] == "--review-valuation" {
            println!(
                "{}",
                issuer.review_valuation(&args[1], Path::new(&args[2]))?
            );
            return Ok(());
        }
        if args[0] == "--record-valuation" {
            issuer.record_valuation(&args[1], Path::new(&args[2]))?;
            println!(
                "{}",
                serde_json::json!({"outcome":"recorded","operation":args[1],
                "valuation_recorded":true,"portfolio_state_updated":false,
                "reconciled":false,"reservation_released":false})
            );
            return Ok(());
        }
        if args[0] == "--record-settlement" {
            issuer.record_settlement(&args[1], Path::new(&args[2]))?;
            println!(
                "{}",
                serde_json::json!({"outcome":"recorded","operation":args[1],
                "settlement_evidence_recorded":true,"reconciled":false,"reservation_released":false})
            );
            return Ok(());
        }
        if args[0] != "--bind-signed" {
            return Err("usage: radar-issuer --bind-signed, --review-settlement, --record-settlement, --review-valuation or --record-valuation <operation-id> <private-file>".into());
        }
        issuer.bind_signed(&args[1], Path::new(&args[2]))?;
        println!(
            "{}",
            serde_json::json!({"outcome":"recorded", "operation":args[1], "broadcast":false, "reconciled":false})
        );
        return Ok(());
    }
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
    fn exact_message_and_wallet_signature_are_required_at_packet_boundaries() {
        use ed25519_dalek::Signer as _;
        let key = ed25519_dalek::SigningKey::from_bytes(&[0x42; 32]);
        let wallet = radar_types::Address::new(key.verifying_key().to_bytes());
        let mut minimum = vec![1];
        minimum.extend_from_slice(&[0; 64]);
        minimum.extend_from_slice(&[1, 0, 0, 1]);
        minimum.extend_from_slice(wallet.as_bytes());
        minimum.extend_from_slice(&[0xAA; 32]);
        minimum.push(0);
        let mut maximum = minimum.clone();
        maximum[133] = 1;
        maximum.extend_from_slice(&[0, 0, 0xC6, 0x08]);
        maximum.resize(1232, 0);
        for unsigned in [minimum, maximum] {
            let binding = radar_journal::ExecutionBinding {
                wallet,
                transaction: radar_types::b64::encode(&unsigned),
                signed_transaction: None,
                reviewed_proposal: None,
            };
            let mut signed = unsigned.clone();
            let signature = key.sign(&signed[65..]).to_bytes();
            signed[1..65].copy_from_slice(&signature);
            assert_eq!(
                super::verified_signed(&binding, wallet, &signed).expect("valid boundary"),
                radar_types::b64::encode(&signed)
            );
            for end in 0..signed.len() {
                assert!(super::verified_signed(&binding, wallet, &signed[..end]).is_err());
            }
            let mut oversized = signed.clone();
            oversized.resize(1233, 0);
            assert!(super::verified_signed(&binding, wallet, &oversized).is_err());
            let mut wrong = signed.clone();
            wrong[0] = 2;
            assert!(super::verified_signed(&binding, wallet, &wrong).is_err());
            assert!(
                super::verified_signed(&binding, radar_types::Address::SYSTEM_PROGRAM, &signed)
                    .is_err()
            );
            let mut wrong = binding.clone();
            wrong.transaction = "invalid".into();
            assert!(super::verified_signed(&wrong, wallet, &signed).is_err());
            for at in [65, 69] {
                let mut foreign = unsigned.clone();
                foreign[at] ^= 1;
                let recorded = radar_journal::ExecutionBinding {
                    transaction: radar_types::b64::encode(&foreign),
                    ..binding.clone()
                };
                let signature = key.sign(&foreign[65..]).to_bytes();
                foreign[1..65].copy_from_slice(&signature);
                assert!(super::verified_signed(&recorded, wallet, &foreign).is_err());
            }
            let mut wrong = unsigned;
            wrong[0] = 2;
            assert!(
                super::verified_signed(
                    &radar_journal::ExecutionBinding {
                        transaction: radar_types::b64::encode(&wrong),
                        ..binding
                    },
                    wallet,
                    &signed
                )
                .is_err()
            );
        }
    }
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
