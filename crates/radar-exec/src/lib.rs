// SPDX-License-Identifier: Apache-2.0
//! Execution: route, gate, sign, submit.
//!
//! The last stage, and the one holding the least authority. By the time control
//! reaches here the kernel has already decided the trade is permitted and bounded
//! it; this crate's remaining jobs are to build a transaction that fits inside
//! those bounds and to check the trade still pays for itself after costs.
//!
//! **There is no reconcile stage.** This line named one until 2026-09-07 and
//! there has never been a module behind it: `submit` sends and reports what the
//! node said, and nothing here goes back afterwards to find out what actually
//! happened on chain. Said plainly because a named stage reads as a built one,
//! and the gap between them is where a fill nobody checked would live.
//!
//! **The routing stage prices; it does not produce a transaction.** As of
//! 2026-09-09 [`route`] calls Jupiter's Router (`api.jup.ag/swap/v2/build`),
//! which returns raw instructions and address lookup tables rather than the
//! signer-readable legacy transaction the deprecated `lite-api` endpoint gave.
//! So [`pipeline::Routing`] is implemented by [`Router`] as an explicit refusal
//! that says why. Said here for the same reason the missing reconcile stage is:
//! a named stage reads as a working one.
//!
//! It cannot sign. The key is in another process, reached over a pipe, and that
//! process re-decodes whatever this one built. So a compromised executor can
//! waste fees and produce refusals — it cannot move funds outside an
//! authorization the kernel issued.
//!
//! ```text
//!   Authorization ──▶ route ──▶ economics gate ──▶ signer ──▶ submit ──▶ status
//!        (kernel)      (here)       (here)        (separate)   (here)
//! ```
//!
//! The economics gate sits *after* routing because it needs the route's measured
//! impact, and *before* signing because a trade that does not pay for itself
//! should never reach the process that holds the key.

pub mod customer_signing;
pub mod economics;
pub mod pipeline;
pub mod route;
pub mod signer_client;
pub mod submit;

pub use economics::{Costs, Economics, FailureRisk};
pub use pipeline::{Attempt, Outcome, execute};
pub use route::{Credentials, Quote, QuoteRequest, Route, RouteError, Router};
pub use signer_client::StreamSigner;
pub use submit::{Finality, SubmitError, Submitter};
