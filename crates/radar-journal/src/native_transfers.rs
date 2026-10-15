// SPDX-License-Identifier: Apache-2.0
//! Immutable external transfer storage. Economic verification belongs to the caller.

use super::{Applied, Live, OperationError, OperationLog};
use crate::{Correlation, Event, NativeTransferRecord, Outcome, Stage};
use std::collections::BTreeMap;

fn identity(record: &NativeTransferRecord) -> Result<String, OperationError> {
    let bytes = radar_types::b64::decode(&record.signed_transaction)
        .ok_or(OperationError::NativeTransfer)?;
    if bytes.first() != Some(&1)
        || bytes.len() < 65
        || bytes.len() > 1232
        || radar_types::b64::encode(&bytes) != record.signed_transaction
        || record.evidence["transaction_base64"] != record.signed_transaction
    {
        return Err(OperationError::NativeTransfer);
    }
    let signature =
        radar_types::Signature::new(bytes[1..65].try_into().expect("checked extent")).to_string();
    if record.review["signature"] != signature {
        return Err(OperationError::NativeTransfer);
    }
    Ok(signature)
}

fn check_operation(
    record: &NativeTransferRecord,
    operations: &BTreeMap<super::OperationId, Live>,
) -> Result<(), OperationError> {
    if operations.values().any(|live| {
        live.execution
            .as_ref()
            .and_then(|binding| binding.signed_transaction.as_ref())
            == Some(&record.signed_transaction)
    }) {
        return Err(OperationError::NativeTransfer);
    }
    Ok(())
}

impl OperationLog {
    /// Retained external native transfers, once each in deterministic signature order.
    pub fn native_transfers(&self) -> impl Iterator<Item = &NativeTransferRecord> {
        self.native_transfers.values()
    }

    /// Persist caller-verified evidence before updating memory. Identical repeats
    /// are no-ops. This grants no authority and changes no portfolio or claim.
    ///
    /// # Errors
    /// Invalid identity, conflicting evidence, operation collision or write failure.
    pub fn record_native_transfer(
        &mut self,
        record: NativeTransferRecord,
        at: u64,
    ) -> Result<Applied, OperationError> {
        let key = identity(&record)?;
        check_operation(&record, &self.operations)?;
        if let Some(prior) = self.native_transfers.get(&key) {
            return if prior == &record {
                Ok(Applied::AlreadySeen)
            } else {
                Err(OperationError::NativeTransfer)
            };
        }
        self.journal.record(
            Stage::NativeTransfer,
            Outcome::Ok,
            at,
            Correlation {
                native_transfer: Some(record.clone()),
                ..Correlation::default()
            },
            self.build.clone(),
            vec![],
            None,
            None,
        )?;
        self.native_transfers.insert(key, record);
        Ok(Applied::Advanced)
    }
}

pub(super) fn replay(
    events: &[Event],
    operations: &BTreeMap<super::OperationId, Live>,
) -> Result<BTreeMap<String, NativeTransferRecord>, OperationError> {
    let mut records = BTreeMap::new();
    for event in events {
        if event.stage != Stage::NativeTransfer && event.correlation.native_transfer.is_none() {
            continue;
        }
        let record = event
            .correlation
            .native_transfer
            .as_ref()
            .ok_or(OperationError::NativeTransfer)?;
        if event.stage != Stage::NativeTransfer
            || event.outcome != Outcome::Ok
            || event.operation.is_some()
            || event.correlation
                != (Correlation {
                    native_transfer: Some(record.clone()),
                    ..Correlation::default()
                })
        {
            return Err(OperationError::NativeTransfer);
        }
        let key = identity(record)?;
        check_operation(record, operations)?;
        if records.get(&key).is_some_and(|prior| prior != record) {
            return Err(OperationError::NativeTransfer);
        }
        records.insert(key, record.clone());
    }
    Ok(records)
}
