// SPDX-License-Identifier: Apache-2.0
//! The signer's verification core.
//!
//! Split from the binary on purpose. The rules that decide whether a
//! transaction may be signed are the most security-critical code in Radar, and
//! they are worth being able to test without a key, a socket or a process.
//!
//! The binary is deliberately thin: read a request, call [`verify::check`],
//! sign the verified bytes or refuse. Everything that decides anything is here.
//!
//! # What this defends against
//!
//! The transaction must match the caller-supplied authorization and this
//! signer's policy. The Privy process additionally verifies a configured issuer's
//! signature over the exact intent, checks its own clock and consumes the nonce
//! persistently. The library signing methods alone do not enforce these guards.
//! An isolated issuer that actually runs the kernel against trusted portfolio
//! state is still absent; authenticated provenance does not prove that decision.

pub mod attestation;
pub mod canonical;
pub mod key;
pub mod privy;
pub mod protocol;
pub mod replay;
pub mod turnkey;
pub mod tx;
pub mod verify;

pub use key::{Key, KeyError};
pub use tx::{DecodeError, Instruction, Message, decode};
pub use verify::{Allowlist, Checked, Rejection, check};
