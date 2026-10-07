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
//! signer's policy. Issuer authenticity is not verified: a replaced executor
//! can forge an authorization within those bounds. The Privy process also
//! consumes each nonce persistently before using its key; this library alone
//! does not enforce single use. Independent portfolio accounting and issuance
//! authentication remain prerequisites for live autonomy.

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
