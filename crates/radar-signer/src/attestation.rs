// SPDX-License-Identifier: Apache-2.0
//! Verification of an issuer's exact Privy intent at the process boundary.
//!
//! The process trusts only a startup public key, never a key in the request.
//! This verifies provenance; it does not prove the issuer ran the risk kernel
//! against trusted portfolio state. That isolated issuer is still required.

use ed25519_dalek::{Signature, VerifyingKey};
use radar_types::Address;

use crate::canonical::canonicalise;
use crate::protocol::PrivyAuthorization;

/// Startup trust anchor and maximum validity interval, with no permissive default.
pub struct Issuer {
    key: VerifyingKey,
    max_lifetime_secs: u64,
}

impl Issuer {
    /// Constructs the trust anchor from an operator-configured public key.
    ///
    /// # Errors
    /// Refuses invalid or weak keys and a zero lifetime.
    pub fn new(public: &Address, max_lifetime_secs: u64) -> Result<Self, &'static str> {
        let key = VerifyingKey::from_bytes(public.as_bytes()).map_err(|_| "invalid issuer key")?;
        if key.is_weak() || max_lifetime_secs == 0 {
            return Err("weak issuer key or zero intent lifetime");
        }
        Ok(Self {
            key,
            max_lifetime_secs,
        })
    }

    /// Verifies the entire intent and its time window before any wallet key use.
    /// `now_unix_secs` must come from the signer's clock, never the wire request.
    ///
    /// # Errors
    /// Refuses invalid signatures, unsupported payload values and invalid time.
    pub fn check(
        &self,
        intent: &PrivyAuthorization,
        now_unix_secs: u64,
    ) -> Result<(), &'static str> {
        let proof = &intent.proof;
        let lifetime = proof
            .expires_at_unix_secs
            .checked_sub(proof.issued_at_unix_secs)
            .ok_or("invalid intent time window")?;
        if lifetime == 0
            || lifetime > self.max_lifetime_secs
            || now_unix_secs < proof.issued_at_unix_secs
            || now_unix_secs >= proof.expires_at_unix_secs
        {
            return Err("intent is not current or exceeds the configured lifetime");
        }
        let bytes = radar_types::b64::decode(&proof.signature).ok_or("invalid issuer signature")?;
        let signature = Signature::from_slice(&bytes).map_err(|_| "invalid issuer signature")?;
        let payload = payload(intent)?;
        self.key
            .verify_strict(payload.as_bytes(), &signature)
            .map_err(|_| "intent does not have the configured issuer's signature")
    }
}

/// The v1 signing transcript. The signature itself is excluded. The domain
/// separates this proof from wallet transactions and other issuer protocols.
/// All integer values are encoded exactly using Radar's canonical JSON subset;
/// clients must not round u64 values through JavaScript's floating point numbers.
///
/// # Errors
/// Refuses payload values the canonicaliser cannot represent exactly.
pub fn payload(intent: &PrivyAuthorization) -> Result<String, &'static str> {
    canonicalise(&serde_json::json!({
        "domain": "radar/privy-intent/v1",
        "authorization": intent.authorization,
        "request": intent.request,
        "wallet": intent.wallet,
        "now_slot": intent.now_slot,
        "max_lamports": intent.max_lamports,
        "issued_at_unix_secs": intent.proof.issued_at_unix_secs,
        "expires_at_unix_secs": intent.proof.expires_at_unix_secs,
    }))
    .map_err(|_| "intent payload cannot be encoded exactly")
}
