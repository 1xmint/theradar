// SPDX-License-Identifier: Apache-2.0
//! Typed paper broker claims. Persistence orders effects; it never grants authority.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ownership::Owner;
use crate::{Applied, Journal, JournalError, Outcome, Stage, Verified};

/// Direction in the initial long-only equity lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerSide {
    /// Claims cash, never margin.
    Buy,
    /// Claims owned shares; proceeds are not available before reconciliation.
    Sell,
}

/// Immutable quantity-based paper equity limit intent, not a risk authorization.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerIntent {
    /// Public paper account identity, pinned by the owner of this journal.
    pub account_id: String,
    /// Retained identity reused for lookup, never silently regenerated on retry.
    pub client_order_id: String,
    /// US equity ticker; eligibility is verified separately.
    pub symbol: String,
    /// Buy or reduction of owned long shares.
    pub side: BrokerSide,
    /// Shares in billionths, never Solana token units.
    pub quantity_nanoshares: u64,
    /// Limit price in micro-USD per share, not an executable quote.
    pub limit_microusd_per_share: u64,
    /// Explicit cash reserve for fees; adequacy needs separate evidence.
    pub fee_bound_microusd: u64,
}

impl BrokerIntent {
    fn validate(&self, account: &str) -> Result<(), BrokerError> {
        if self.account_id != account
            || self.client_order_id.is_empty()
            || !self
                .client_order_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || self.symbol.is_empty()
            || !self
                .symbol
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
            || self.quantity_nanoshares == 0
            || self.limit_microusd_per_share == 0
        {
            return Err(BrokerError::InvalidIntent);
        }
        self.cash_claim_microusd()?;
        Ok(())
    }

    /// Cash held by this intent; buys ceil the limit notional before adding fees.
    /// Sells claim fees and owned shares, never expected sale proceeds.
    ///
    /// # Errors
    /// Arithmetic outside the supported unsigned cash range.
    pub fn cash_claim_microusd(&self) -> Result<u64, BrokerError> {
        let notional = match self.side {
            BrokerSide::Buy => u64::try_from(
                (u128::from(self.quantity_nanoshares) * u128::from(self.limit_microusd_per_share))
                    .div_ceil(1_000_000_000),
            )
            .map_err(|_| BrokerError::InvalidIntent)?,
            BrokerSide::Sell => 0,
        };
        notional
            .checked_add(self.fee_bound_microusd)
            .ok_or(BrokerError::InvalidIntent)
    }
}

/// Both states retain the entire immutable reservation. No release exists yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerState {
    /// Intent and reservation have reached the journal; no submission released.
    Reserved,
    /// Recorded before releasing the effect; a restart must look up the same ID.
    SubmissionUnknown,
}

/// Typed entry hashed into a broker journal event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerEntry {
    /// Immutable order and claim.
    pub intent: BrokerIntent,
    /// Recorded submission state.
    pub state: BrokerState,
}

/// A refusal to advance broker history or release an effect.
#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    /// Lock acquisition, read or durable write failed.
    #[error("broker journal persistence failed: {0}")]
    Journal(#[from] JournalError),
    /// Account, identifiers, units or arithmetic are unsupported.
    #[error("invalid or mismatched paper broker intent")]
    InvalidIntent,
    /// A retained ID cannot be rebound to another order.
    #[error("client order ID already binds another intent")]
    Conflict,
    /// No such reservation, or submission may already have happened.
    #[error("order is not reserved for a first submission; reconcile before proceeding")]
    NotReserved,
    /// Broken/torn/mixed history or an illegal immutable transition.
    #[error("broker journal history is not valid; reconcile before proceeding")]
    History,
    /// A write may have reached disk despite returning an error.
    #[error("broker journal write outcome is unknown; reopen and verify history")]
    Poisoned,
}

/// Exclusive owner of replayed paper claims. Actual caller: radar alpaca-order-reserve.
/// Protected stable paths/sidecars are required. This does not resist host rollback,
/// establish economic completeness or grant capital permission.
pub struct BrokerLog {
    _owner: Owner,
    journal: Journal,
    account: String,
    entries: BTreeMap<String, BrokerEntry>,
    poisoned: bool,
}

impl BrokerLog {
    /// Takes cooperative ownership and refuses damaged or mixed-domain history.
    ///
    /// # Errors
    /// Missing identity, lock/read failures, or invalid history/intent binding.
    pub fn open(path: impl Into<PathBuf>, account: &str) -> Result<Self, BrokerError> {
        if account.trim().is_empty() {
            return Err(BrokerError::InvalidIntent);
        }
        let (owner, path) = Owner::acquire(path.into())?;
        let journal = Journal::open(path)?;
        if !matches!(journal.verify()?, Verified::Intact { .. }) {
            return Err(BrokerError::History);
        }
        let mut entries: BTreeMap<String, BrokerEntry> = BTreeMap::new();
        for event in journal.events()? {
            let entry = event.broker.ok_or(BrokerError::History)?;
            entry.intent.validate(account)?;
            let outcome = match entry.state {
                BrokerState::Reserved => Outcome::Ok,
                BrokerState::SubmissionUnknown => Outcome::Uncertain,
            };
            if event.schema != crate::SCHEMA_VERSION
                || event.stage != Stage::BrokerOrder
                || event.outcome != outcome
                || event.operation.is_some()
                || event.correlation.receipt.as_deref()
                    != Some(entry.intent.client_order_id.as_str())
            {
                return Err(BrokerError::History);
            }
            match entries.get(&entry.intent.client_order_id) {
                None if entry.state == BrokerState::Reserved => {}
                Some(prior)
                    if prior.intent == entry.intent
                        && prior.state == BrokerState::Reserved
                        && entry.state == BrokerState::SubmissionUnknown => {}
                _ => return Err(BrokerError::History),
            }
            entries.insert(entry.intent.client_order_id.clone(), entry);
        }
        Ok(Self {
            _owner: owner,
            journal,
            account: account.to_owned(),
            entries,
            poisoned: false,
        })
    }

    fn healthy(&self) -> Result<(), BrokerError> {
        if self.poisoned {
            Err(BrokerError::Poisoned)
        } else {
            Ok(())
        }
    }

    fn append(&mut self, entry: BrokerEntry, at: u64) -> Result<(), BrokerError> {
        self.healthy()?;
        // Any failed append might have committed. Do not continue from stale
        // sequence/memory; only a fresh integrity-checked replay can resolve it.
        self.poisoned = true;
        self.journal.record_broker(entry.clone(), at)?;
        self.entries
            .insert(entry.intent.client_order_id.clone(), entry);
        self.poisoned = false;
        Ok(())
    }

    /// Persists an immutable claim; exact repeats do not append or free it.
    ///
    /// # Errors
    /// Invalid/conflicting input or uncertain persistence; no submission occurs.
    pub fn reserve(&mut self, intent: BrokerIntent, at: u64) -> Result<Applied, BrokerError> {
        self.healthy()?;
        intent.validate(&self.account)?;
        if let Some(prior) = self.entries.get(&intent.client_order_id) {
            return if prior.intent == intent {
                Ok(Applied::AlreadySeen)
            } else {
                Err(BrokerError::Conflict)
            };
        }
        self.append(
            BrokerEntry {
                intent,
                state: BrokerState::Reserved,
            },
            at,
        )?;
        Ok(Applied::Advanced)
    }

    /// Records uncertainty before invoking an effect with the stored immutable intent.
    /// Never retries, even if the effect returns an error or the process restarts.
    /// The caller must separately obtain deterministic capital authorization;
    /// this ordering API is not an execution permission. No production submit caller yet.
    ///
    /// # Errors
    /// Not a first reserved submission, poison, or failure to persist before the effect.
    pub fn submit<T>(
        &mut self,
        client_id: &str,
        at: u64,
        effect: impl FnOnce(&BrokerIntent) -> T,
    ) -> Result<T, BrokerError> {
        self.healthy()?;
        let mut entry = self
            .entries
            .get(client_id)
            .filter(|e| e.state == BrokerState::Reserved)
            .cloned()
            .ok_or(BrokerError::NotReserved)?;
        entry.state = BrokerState::SubmissionUnknown;
        self.append(entry.clone(), at)?;
        Ok(effect(&entry.intent))
    }

    /// Every retained claim in deterministic client-ID order, including uncertain submissions.
    ///
    /// # Errors
    /// Uncertain write outcome; reopen before claiming a complete local view.
    pub fn entries(&self) -> Result<impl Iterator<Item = &BrokerEntry>, BrokerError> {
        self.healthy()?;
        Ok(self.entries.values())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Correlation, Event, OperationLog};

    fn intent() -> BrokerIntent {
        BrokerIntent {
            account_id: "paper-id".into(),
            client_order_id: "radar_1".into(),
            symbol: "SPY".into(),
            side: BrokerSide::Buy,
            quantity_nanoshares: 10_000_001,
            limit_microusd_per_share: 100_000_001,
            fee_bound_microusd: 7,
        }
    }

    fn rewrite(path: &std::path::Path, mut change: impl FnMut(&mut Event)) {
        let mut previous = String::new();
        let lines: Vec<_> = Journal::open(path)
            .unwrap()
            .events()
            .unwrap()
            .into_iter()
            .map(|mut event| {
                change(&mut event);
                event.previous = previous.clone();
                event.id = event.digest();
                previous = event.id.clone();
                serde_json::to_string(&event).unwrap()
            })
            .collect();
        std::fs::write(path, format!("{}\n", lines.join("\n"))).unwrap();
        assert!(matches!(
            Journal::open(path).unwrap().verify().unwrap(),
            Verified::Intact { .. }
        ));
    }

    #[test]
    fn intent_claim_units_ceil_buy_cost_and_keep_sell_fees_without_proceeds() {
        assert_eq!(intent().cash_claim_microusd().unwrap(), 1_000_008);
        let mut sell = intent();
        sell.side = BrokerSide::Sell;
        assert_eq!(sell.cash_claim_microusd().unwrap(), 7);
        let mut tiny = intent();
        tiny.quantity_nanoshares = 1;
        tiny.limit_microusd_per_share = 1;
        assert_eq!(tiny.cash_claim_microusd().unwrap(), 8);
        tiny.quantity_nanoshares = 1_000_000_000;
        assert_eq!(tiny.cash_claim_microusd().unwrap(), 8);
        tiny.quantity_nanoshares = u64::MAX;
        tiny.limit_microusd_per_share = u64::MAX;
        assert!(tiny.cash_claim_microusd().is_err());
        tiny.quantity_nanoshares = 1_000_000_000;
        tiny.fee_bound_microusd = 1;
        assert!(tiny.cash_claim_microusd().is_err());
    }

    #[test]
    fn reservation_and_unknown_submission_survive_restart_without_resubmitting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("paper.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        assert_eq!(log.reserve(intent(), 1).unwrap(), Applied::Advanced);
        let before = std::fs::read(&path).unwrap();
        assert_eq!(log.reserve(intent(), 2).unwrap(), Applied::AlreadySeen);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        drop(log);
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        assert_eq!(
            log.entries().unwrap().next().unwrap().state,
            BrokerState::Reserved
        );
        let result: Result<(), &str> = log
            .submit("radar_1", 3, |stored| {
                assert_eq!(stored, &intent());
                let events = Journal::open(&path).unwrap().events().unwrap();
                assert_eq!(events.len(), 2);
                assert_eq!(
                    events[1].broker.as_ref().unwrap().state,
                    BrokerState::SubmissionUnknown
                );
                Err("response lost")
            })
            .unwrap();
        assert_eq!(result, Err("response lost"));
        assert!(matches!(
            log.submit("radar_1", 4, |_| panic!("must not retry")),
            Err(BrokerError::NotReserved)
        ));
        drop(log);
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        let entries: Vec<_> = log.entries().unwrap().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].intent, intent());
        assert_eq!(entries[0].state, BrokerState::SubmissionUnknown);
        assert_eq!(log.reserve(intent(), 5).unwrap(), Applied::AlreadySeen);
        assert!(matches!(
            log.submit("radar_1", 6, |_| panic!("restart must not retry")),
            Err(BrokerError::NotReserved)
        ));
        assert!(matches!(
            log.submit("missing", 6, |_| panic!("no claim")),
            Err(BrokerError::NotReserved)
        ));
    }

    #[test]
    fn invalid_and_conflicting_intents_cannot_rebind_reserved_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("paper.jsonl");
        assert!(BrokerLog::open(&path, " ").is_err());
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        for (field, value) in [
            ("account_id", serde_json::json!("other")),
            ("client_order_id", serde_json::json!("")),
            ("client_order_id", serde_json::json!("a&x=1")),
            ("symbol", serde_json::json!("")),
            ("symbol", serde_json::json!("spy")),
            ("quantity_nanoshares", serde_json::json!(0)),
            ("limit_microusd_per_share", serde_json::json!(0)),
        ] {
            let mut value_json = serde_json::to_value(intent()).unwrap();
            value_json[field] = value;
            assert!(matches!(
                log.reserve(serde_json::from_value(value_json).unwrap(), 1),
                Err(BrokerError::InvalidIntent)
            ));
        }
        log.reserve(intent(), 1).unwrap();
        for field in [
            "quantity_nanoshares",
            "limit_microusd_per_share",
            "fee_bound_microusd",
        ] {
            let mut value = serde_json::to_value(intent()).unwrap();
            value[field] = serde_json::json!(123);
            assert!(matches!(
                log.reserve(serde_json::from_value(value).unwrap(), 2),
                Err(BrokerError::Conflict)
            ));
        }
        let mut second = intent();
        second.client_order_id = "R-0".into();
        second.symbol = "BRK.B-1".into();
        second.side = BrokerSide::Sell;
        log.reserve(second, 2).unwrap();
        assert_eq!(
            log.entries()
                .unwrap()
                .map(|e| e.intent.client_order_id.as_str())
                .collect::<Vec<_>>(),
            ["R-0", "radar_1"]
        );
    }

    #[test]
    fn ownership_is_exclusive_and_account_or_domain_mismatch_never_looks_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("paper.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        assert!(BrokerLog::open(&path, "paper-id").is_err());
        assert!(OperationLog::open(&path).is_err());
        log.reserve(intent(), 1).unwrap();
        drop(log);
        assert!(matches!(
            OperationLog::open(&path),
            Err(crate::OperationError::BrokerHistory)
        ));
        assert!(BrokerLog::open(&path, "other").is_err());
        assert!(BrokerLog::open(&path, "paper-id").is_ok());
        let other = dir.path().join("other.jsonl");
        Journal::open(&other)
            .unwrap()
            .record(
                Stage::Received,
                Outcome::Ok,
                1,
                Correlation {
                    receipt: Some("unrelated".into()),
                    ..Correlation::default()
                },
                None,
                vec![],
                None,
                None,
            )
            .unwrap();
        assert!(matches!(
            BrokerLog::open(&other, "paper-id"),
            Err(BrokerError::History)
        ));
        assert!(matches!(
            BrokerLog::open(&other, "paper-id"),
            Err(BrokerError::History)
        ));
    }

    #[test]
    fn intact_hashes_do_not_excuse_illegal_transitions_or_changed_payloads() {
        for case in 0..7 {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("paper.jsonl");
            let mut log = BrokerLog::open(&path, "paper-id").unwrap();
            log.reserve(intent(), 1).unwrap();
            log.submit("radar_1", 2, |_| ()).unwrap();
            drop(log);
            rewrite(&path, |event| {
                if event.sequence == 2 {
                    match case {
                        0 => event.broker.as_mut().unwrap().intent.fee_bound_microusd += 1,
                        1 => event.broker.as_mut().unwrap().state = BrokerState::Reserved,
                        2 => event.stage = Stage::Received,
                        3 => event.outcome = Outcome::Ok,
                        4 => event.correlation.receipt = None,
                        5 => event.broker = None,
                        6 => event.schema = 0,
                        _ => unreachable!(),
                    }
                }
            });
            assert!(BrokerLog::open(&path, "paper-id").is_err(), "case {case}");
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("orphan.jsonl");
        Journal::open(&path)
            .unwrap()
            .record_broker(
                BrokerEntry {
                    intent: intent(),
                    state: BrokerState::SubmissionUnknown,
                },
                1,
            )
            .unwrap();
        assert!(BrokerLog::open(&path, "paper-id").is_err());
    }

    #[test]
    fn broken_or_torn_history_refuses_and_failed_append_poison_prevents_effect() {
        for tail in ["{", "broken\n"] {
            use std::io::Write as _;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("paper.jsonl");
            let mut log = BrokerLog::open(&path, "paper-id").unwrap();
            log.reserve(intent(), 1).unwrap();
            drop(log);
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(tail.as_bytes())
                .unwrap();
            assert!(BrokerLog::open(&path, "paper-id").is_err());
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("paper.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        log.reserve(intent(), 1).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            log.submit("radar_1", 2, |_| panic!("persistence failed"))
                .is_err()
        );
        assert!(matches!(log.entries(), Err(BrokerError::Poisoned)));
        assert!(matches!(
            log.reserve(intent(), 3),
            Err(BrokerError::Poisoned)
        ));
        assert!(matches!(
            log.submit("radar_1", 3, |_| panic!("poisoned")),
            Err(BrokerError::Poisoned)
        ));
    }

    #[test]
    fn broker_payload_is_bound_into_the_existing_event_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("paper.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        log.reserve(intent(), 1).unwrap();
        drop(log);
        let mut event: Event =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(event.id, event.digest());
        event.broker.as_mut().unwrap().intent.fee_bound_microusd += 1;
        assert_ne!(event.id, event.digest());
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&event).unwrap()),
        )
        .unwrap();
        assert!(matches!(
            Journal::open(&path).unwrap().verify().unwrap(),
            Verified::Broken { .. }
        ));
        assert!(BrokerLog::open(&path, "paper-id").is_err());
    }
}
