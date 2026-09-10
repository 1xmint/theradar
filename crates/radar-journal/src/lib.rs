// SPDX-License-Identifier: Apache-2.0
//! What the autonomous surface decided, written before it acted.
//!
//! # Why this exists rather than another log
//!
//! [`radar_analyst::log`] already records what was asked, what was measured and
//! what was said, and its reason is exactly right: the log is what turns a
//! public mistake into a **correction** rather than an argument. This does not
//! replace it and does not change it.
//!
//! It records the reply. It does not record the winner selection, the scoring
//! mode, the payout signature and its validity bounds, or the **ordering** of an
//! external effect against the durable record of intending it. Those are the
//! places where an unattended process loses money or publishes twice, and Radar
//! is about to run a money-bearing loop with no human in it.
//!
//! The spend reservation *is* recorded now, by [`OperationLog`]: a claim on
//! capital survives a restart, a change recorded twice is applied once, and the
//! effect runs only after the intent reached disk. That is also where
//! [`Outcome::Uncertain`] finally has a state machine behind it —
//! [`OperationState::SubmissionUnknown`] cannot be written off as a failure,
//! which is how a reservation frees itself while the transaction is still
//! landing.
//!
//! [ADR 0017](https://github.com/hey-vera/radar/blob/main/docs/adr/0017-the-journal-records-intent-before-effect-and-replay-proves-only-the-decision.md).
//!
//! # The one rule
//!
//! **The durable intent exists before the effect.** [`Journal::record`] returns
//! an error rather than a written event when the write fails, and a caller that
//! publishes or pays anyway has broken the only guarantee here. The type helps:
//! [`Recorded`] is the receipt, it cannot be constructed outside this crate, and
//! the effectful call sites take one.
//!
//! # What a chain proves, and what it does not
//!
//! Each event carries the hash of the one before it, so an alteration in the
//! middle is visible from a later event. That detects tampering **relative to a
//! trusted checkpoint**. It does not resist a host attacker who rewrites the
//! whole chain — they can recompute every link. Off-host checkpointing is what
//! answers that, and it is an operator configuration rather than a property of
//! this file. Saying so here rather than letting "hash-chained" be read as more
//! than it is.
//!
//! # What is never written
//!
//! Credentials, signing keys, authorization headers, credential-bearing URLs,
//! and model chain-of-thought. [`Event::redacted`] is the only door for
//! caller-supplied detail and it is bounded; untrusted content stays data and is
//! escaped by `serde_json` on the way out.

#![forbid(unsafe_code)]

mod event;
mod file;
mod operation;

pub use event::{Correlation, Event, MAX_REDACTED, Outcome, Recorded, SCHEMA_VERSION, Stage};
pub use file::{Journal, JournalError, Verified};
pub use operation::{
    Applied, Intent, OperationEntry, OperationError, OperationId, OperationLog, OperationState,
    Released,
};
