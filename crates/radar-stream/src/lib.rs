// SPDX-License-Identifier: Apache-2.0
//! The live market feed.
//!
//! A Yellowstone gRPC subscription to every successful transaction touching a
//! Solana trading venue, folded as it arrives into a bounded in-memory [`Tape`]
//! the market routes read. Off unless `RADAR_STREAM_ENDPOINT` is set; see
//! [`feed::Config::from_vars`].
//!
//! - [`proto`]: the wire messages, hand-written and checked against the
//!   vendored `.proto` files.
//! - [`tx`]: a transaction reduced to balances and instructions.
//! - [`decode`]: balances to trades, holders and launches, venue-agnostic.
//! - [`tape`]: the bounded store of all of that.
//! - [`feed`]: the connection.
//!
//! Nothing here holds a key or moves money; it reads public chain data.

pub mod decode;
pub mod feed;
pub mod proto;
pub mod tape;
pub mod tx;

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64};
use std::sync::{Mutex, MutexGuard, PoisonError};

pub use tape::Tape;

/// The default memory budget for the tape: 512 MiB.
///
/// The production box has 3.8 GB with about 2.9 GB free before the feed runs.
/// Override with `RADAR_STREAM_MEMORY_MB`.
pub const DEFAULT_BUDGET_BYTES: usize = 512 * 1024 * 1024;

/// The variable overriding [`DEFAULT_BUDGET_BYTES`], in mebibytes.
pub const MEMORY_VAR: &str = "RADAR_STREAM_MEMORY_MB";

/// The tape and the connection's state, shared between the feed and readers.
#[derive(Debug)]
pub struct Live {
    tape: Mutex<Tape>,
    /// What the connection is doing.
    pub status: Status,
}

impl Live {
    /// An empty live feed with this memory budget.
    #[must_use]
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            tape: Mutex::new(Tape::new(budget_bytes)),
            status: Status::default(),
        }
    }

    /// The tape, locked.
    ///
    /// A poisoned lock is recovered rather than propagated: the only writer is
    /// the feed, and a panic there leaves at worst one transaction half-applied
    /// to a display cache. Refusing every market request afterwards would turn
    /// that into an outage of the whole screen.
    pub fn tape(&self) -> MutexGuard<'_, Tape> {
        self.tape.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The connection's state. Atomics, so reading it never waits on the feed.
#[derive(Debug, Default)]
pub struct Status {
    /// Whether a subscription is open right now.
    pub connected: AtomicBool,
    /// When the current subscription opened, unix seconds.
    pub connected_since: AtomicI64,
    /// When the last update arrived, unix seconds.
    pub last_message: AtomicI64,
    /// Sessions that ended, for any reason.
    pub reconnects: AtomicU64,
    /// Encoded size of every update received, bytes.
    pub bytes: AtomicU64,
    last_error: Mutex<Option<String>>,
}

impl Status {
    /// Why the last session ended.
    #[must_use]
    pub fn last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set_error(&self, reason: String) {
        *self
            .last_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(reason);
    }
}

/// Reads the memory budget.
///
/// # Errors
///
/// A value that is not a whole number of mebibytes, or is zero.
pub fn budget_from_vars(get: &impl Fn(&str) -> Option<String>) -> Result<usize, String> {
    match get(MEMORY_VAR).map(|v| v.trim().to_owned()) {
        None => Ok(DEFAULT_BUDGET_BYTES),
        Some(v) if v.is_empty() => Ok(DEFAULT_BUDGET_BYTES),
        Some(v) => match v.parse::<usize>() {
            Ok(mb) if mb > 0 => Ok(mb.saturating_mul(1024 * 1024)),
            _ => Err(format!(
                "{MEMORY_VAR} must be a positive whole number of MiB, got '{v}'"
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        let owned = pairs.to_vec();
        move |k| {
            owned
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn the_budget_defaults_to_512_mib() {
        assert_eq!(DEFAULT_BUDGET_BYTES, 536_870_912);
        assert_eq!(budget_from_vars(&vars(&[])), Ok(536_870_912));
        assert_eq!(
            budget_from_vars(&vars(&[(MEMORY_VAR, " ")])),
            Ok(536_870_912)
        );
    }

    #[test]
    fn the_budget_is_read_in_mebibytes_and_zero_is_refused() {
        assert_eq!(
            budget_from_vars(&vars(&[(MEMORY_VAR, "300")])),
            Ok(314_572_800)
        );
        assert_eq!(budget_from_vars(&vars(&[(MEMORY_VAR, "1")])), Ok(1_048_576));
        assert!(budget_from_vars(&vars(&[(MEMORY_VAR, "0")])).is_err());
        assert!(budget_from_vars(&vars(&[(MEMORY_VAR, "lots")])).is_err());
    }

    #[test]
    fn the_last_error_is_whatever_was_set_last() {
        let status = Status::default();
        assert_eq!(status.last_error(), None);
        status.set_error("first".into());
        status.set_error("second".into());
        assert_eq!(status.last_error().as_deref(), Some("second"));
    }
}
