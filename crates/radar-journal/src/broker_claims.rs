// SPDX-License-Identifier: Apache-2.0
//! Residual claims derived from protected caller-verified cash/order observations.

use crate::{BrokerCash, BrokerEntry, BrokerError, BrokerSide, BrokerState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Selected execution progress verified by the private broker caller.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerProgress {
    /// Immutable broker identity bound to the retained client ID.
    pub broker_order_id: String,
    /// Cumulative quantity covered by matched execution intervals.
    pub filled_nanoshares: u64,
    /// Only filled/canceled/expired/rejected; pending cancel/day-done stay open.
    pub terminal: bool,
}

/// Liability still held against the current observed cash/shares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrokerClaim {
    /// Unfilled buy limit bound plus the entire fee reserve; sells hold fees.
    pub cash_microusd: u64,
    /// Unfilled sell shares; buys never claim owned shares.
    pub shares_nanoshares: u64,
    /// Absent means no economic progress has been established; hold everything.
    pub progress: Option<BrokerProgress>,
}

pub(crate) fn validate(
    cash: &BrokerCash,
    prior: Option<&BrokerCash>,
    entries: &BTreeMap<String, BrokerEntry>,
) -> Result<(), BrokerError> {
    let previous = prior.and_then(|p| p.orders.as_ref());
    let Some(orders) = &cash.orders else {
        return if previous.is_some() {
            Err(BrokerError::History)
        } else {
            Ok(())
        };
    };
    if orders.len() != entries.len() {
        return Err(BrokerError::History);
    }
    let mut identities = BTreeSet::new();
    for (id, entry) in entries {
        let order = orders.get(id).ok_or(BrokerError::History)?;
        if entry.state != BrokerState::SubmissionUnknown
            || order.broker_order_id.trim().is_empty()
            || !identities.insert(&order.broker_order_id)
            || order.filled_nanoshares > entry.intent.quantity_nanoshares
        {
            return Err(BrokerError::History);
        }
        if let Some(last) = previous.and_then(|p| p.get(id))
            && (order.broker_order_id != last.broker_order_id
                || order.filled_nanoshares < last.filled_nanoshares
                || (last.terminal && order != last))
        {
            return Err(BrokerError::History);
        }
    }
    Ok(())
}

pub(crate) fn claim(
    entry: &BrokerEntry,
    progress: Option<&BrokerProgress>,
) -> Result<BrokerClaim, BrokerError> {
    let remaining = match progress {
        Some(progress) if progress.terminal => 0,
        Some(progress) => entry
            .intent
            .quantity_nanoshares
            .checked_sub(progress.filled_nanoshares)
            .ok_or(BrokerError::History)?,
        None => entry.intent.quantity_nanoshares,
    };
    let mut residual = entry.intent.clone();
    residual.quantity_nanoshares = remaining;
    Ok(BrokerClaim {
        cash_microusd: residual.cash_claim_microusd()?,
        shares_nanoshares: match entry.intent.side {
            BrokerSide::Buy => 0,
            BrokerSide::Sell => remaining,
        },
        progress: progress.cloned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BrokerIntent, BrokerLog, Journal};

    fn entry(side: BrokerSide) -> BrokerEntry {
        BrokerEntry {
            intent: BrokerIntent {
                account_id: "paper-id".into(),
                client_order_id: format!("order_{side:?}"),
                symbol: "SPY".into(),
                side,
                quantity_nanoshares: 1_000_000_001,
                limit_microusd_per_share: 1_000_001,
                fee_bound_microusd: 7,
            },
            state: BrokerState::SubmissionUnknown,
        }
    }

    fn progress(filled: u64, terminal: bool) -> BrokerProgress {
        BrokerProgress {
            broker_order_id: "broker-1".into(),
            filled_nanoshares: filled,
            terminal,
        }
    }

    fn cash(orders: Option<BTreeMap<String, BrokerProgress>>) -> BrokerCash {
        BrokerCash {
            account_id: "paper-id".into(),
            started: 2,
            completed: 3,
            observation: serde_json::json!({"selected":"caller-verified"}),
            orders,
        }
    }

    #[test]
    fn residual_claims_ceil_unfilled_buy_cost_keep_sell_shares_and_never_release_fees() {
        for side in [BrokerSide::Buy, BrokerSide::Sell] {
            let entry = entry(side);
            let original = claim(&entry, None).unwrap();
            assert_eq!(original.progress, None);
            assert_eq!(
                original.cash_microusd,
                if side == BrokerSide::Buy {
                    1_000_009
                } else {
                    7
                }
            );
            assert_eq!(
                original.shares_nanoshares,
                if side == BrokerSide::Sell {
                    1_000_000_001
                } else {
                    0
                }
            );
            for (filled, terminal, cash, shares) in [
                (0, false, 1_000_009, 1_000_000_001),
                (500_000_000, false, 500_008, 500_000_001),
                (1_000_000_001, false, 7, 0),
                (0, true, 7, 0),
                (500_000_000, true, 7, 0),
            ] {
                let progress = progress(filled, terminal);
                let residual = claim(&entry, Some(&progress)).unwrap();
                assert_eq!(
                    residual.cash_microusd,
                    if side == BrokerSide::Buy { cash } else { 7 }
                );
                assert_eq!(
                    residual.shares_nanoshares,
                    if side == BrokerSide::Sell { shares } else { 0 }
                );
                assert_eq!(residual.progress, Some(progress));
            }
            assert!(claim(&entry, Some(&progress(1_000_000_002, false))).is_err());
        }
    }

    #[test]
    fn progress_requires_complete_bound_submissions_monotonic_fills_and_immutable_terminal_state() {
        let entry = entry(BrokerSide::Buy);
        let id = entry.intent.client_order_id.clone();
        let entries = BTreeMap::from([(id.clone(), entry.clone())]);
        let orders = BTreeMap::from([(id.clone(), progress(500_000_000, false))]);
        let prior = cash(Some(orders.clone()));
        assert!(validate(&cash(None), None, &entries).is_ok());
        assert!(validate(&cash(None), Some(&cash(None)), &entries).is_ok());
        assert!(validate(&cash(None), Some(&prior), &entries).is_err());
        assert!(validate(&prior, Some(&prior), &entries).is_ok());
        assert!(validate(&cash(Some(BTreeMap::new())), None, &BTreeMap::new()).is_ok());
        assert!(validate(&cash(Some(BTreeMap::new())), None, &entries).is_err());
        let mut extra = orders.clone();
        extra.insert("unretained".into(), progress(0, false));
        assert!(validate(&cash(Some(extra)), None, &entries).is_err());
        assert!(
            validate(
                &cash(Some(BTreeMap::from([("wrong".into(), progress(0, false))]))),
                None,
                &entries
            )
            .is_err()
        );
        let mut reserved = entries.clone();
        reserved.get_mut(&id).unwrap().state = BrokerState::Reserved;
        assert!(validate(&prior, None, &reserved).is_err());
        for (binding, filled, terminal) in [
            ("", 500_000_000, false),
            (" ", 500_000_000, false),
            ("other", 500_000_000, false),
            ("broker-1", 499_999_999, false),
            ("broker-1", 1_000_000_002, false),
        ] {
            let mut changed = orders.clone();
            changed.insert(
                id.clone(),
                BrokerProgress {
                    broker_order_id: binding.into(),
                    filled_nanoshares: filled,
                    terminal,
                },
            );
            assert!(validate(&cash(Some(changed)), Some(&prior), &entries).is_err());
        }
        let terminal = cash(Some(BTreeMap::from([(
            id.clone(),
            progress(500_000_000, true),
        )])));
        assert!(validate(&terminal, Some(&prior), &entries).is_ok());
        assert!(validate(&terminal, Some(&terminal), &entries).is_ok());
        for changed in [progress(500_000_000, false), progress(500_000_001, true)] {
            assert!(
                validate(
                    &cash(Some(BTreeMap::from([(id.clone(), changed)]))),
                    Some(&terminal),
                    &entries
                )
                .is_err()
            );
        }
        let mut two = entries;
        let sell = self::entry(BrokerSide::Sell);
        two.insert(sell.intent.client_order_id.clone(), sell.clone());
        let duplicate = cash(Some(BTreeMap::from([
            (id, progress(0, false)),
            (sell.intent.client_order_id, progress(0, false)),
        ])));
        assert!(validate(&duplicate, None, &two).is_err());
    }

    #[test]
    fn economic_claims_replay_atomically_with_cash_and_refuse_invalid_updates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("economic.jsonl");
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        log.observe_cash(cash(Some(BTreeMap::new())), 3).unwrap();
        let entry = entry(BrokerSide::Buy);
        let id = entry.intent.client_order_id.clone();
        log.reserve(entry.intent.clone(), 3).unwrap();
        assert_eq!(log.claims().unwrap()[&id].cash_microusd, 1_000_009);
        log.submit(&id, 3, |_| ()).unwrap();
        let partial = BrokerCash {
            started: 3,
            completed: 4,
            ..cash(Some(BTreeMap::from([(
                id.clone(),
                progress(500_000_000, false),
            )])))
        };
        log.observe_cash(partial.clone(), 4).unwrap();
        assert_eq!(log.claims().unwrap()[&id].cash_microusd, 500_008);
        let bytes = std::fs::read(&path).unwrap();
        let mut invalid = partial.clone();
        invalid.started = 4;
        invalid.completed = 5;
        invalid
            .orders
            .as_mut()
            .unwrap()
            .get_mut(&id)
            .unwrap()
            .filled_nanoshares = 0;
        assert!(log.observe_cash(invalid.clone(), 5).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        drop(log);
        let mut log = BrokerLog::open(&path, "paper-id").unwrap();
        assert_eq!(log.cash().unwrap(), Some(&partial));
        assert_eq!(log.claims().unwrap()[&id].cash_microusd, 500_008);
        assert!(log.submit(&id, 4, |_| panic!("no retry")).is_err());
        drop(log);
        // Even a validly hash-chained illegal transition must refuse on replay.
        Journal::open(&path)
            .unwrap()
            .record_broker_cash(invalid, 5)
            .unwrap();
        assert!(BrokerLog::open(&path, "paper-id").is_err());
    }
}
