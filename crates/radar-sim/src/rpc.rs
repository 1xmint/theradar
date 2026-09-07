// SPDX-License-Identifier: Apache-2.0
//! Fetching a mint account.
//!
//! Deliberately not routed through the x402 lane. A paid call there settles
//! on-chain before responding — hundreds of milliseconds — and exit analysis
//! sits between a decision and a submission. `radar-provider` keeps the two
//! lanes apart by type for the same reason.

use std::time::Duration;

use radar_types::Address;
use serde::Deserialize;

use crate::mint::{MintError, MintStructure};

/// A public Solana RPC endpoint. Overridable for a paid one.
pub const DEFAULT_RPC: &str = "https://api.mainnet-beta.solana.com";

/// Why a mint account could not be fetched.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// The endpoint could not be reached or refused.
    #[error("rpc: {0}")]
    Transport(String),
    /// The node answered with an error.
    #[error("rpc error: {0}")]
    Node(String),
    /// No account exists at that address.
    ///
    /// For a mint this is decisive: an address with no account is not a token,
    /// and nothing downstream should treat it as one with unknown properties.
    #[error("no account at {0}")]
    NoAccount(String),
    /// The account exists but does not parse as a mint.
    #[error("not a mint: {0}")]
    NotAMint(#[from] MintError),
    /// The response did not have the shape expected.
    #[error("unreadable rpc response: {0}")]
    Malformed(String),
}

#[derive(Deserialize)]
struct Envelope {
    result: Option<ResultValue>,
    error: Option<NodeError>,
}

#[derive(Deserialize)]
struct NodeError {
    message: String,
}

#[derive(Deserialize)]
struct ResultValue {
    value: Option<AccountValue>,
}

#[derive(Deserialize)]
struct AccountValue {
    data: Vec<String>,
    owner: String,
}

/// Fetches mint accounts.
pub struct RpcClient {
    endpoint: String,
    agent: ureq::Agent,
}

impl Default for RpcClient {
    fn default() -> Self {
        Self::new(DEFAULT_RPC)
    }
}

impl RpcClient {
    /// A client against the given endpoint.
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
            .build();
        Self {
            endpoint: endpoint.into(),
            agent: config.into(),
        }
    }

    /// Reads a mint account and parses it.
    ///
    /// # Errors
    ///
    /// Returns [`FetchError`] if the account cannot be fetched, does not exist,
    /// or does not parse as a mint.
    pub fn mint_structure(&self, mint: &Address) -> Result<MintStructure, FetchError> {
        let (raw, owner) = self.data_at(mint)?;
        Ok(MintStructure::parse(&raw, &owner)?)
    }

    /// Reads any account's data and its owning program.
    ///
    /// Split out of [`Self::mint_structure`], which was the only reader here and
    /// parsed as it fetched. The bonding curve and the fee config are two more
    /// accounts this crate now needs, and three copies of one JSON-RPC envelope
    /// is three places for a node's error shape to be handled differently.
    ///
    /// **Named for what it returns, not for what it reads.** `data_at(address)`
    /// says the thing: the bytes at that address, and the program that owns
    /// them. `account` is ambiguous between the account and its contents, and
    /// this returns the contents.
    ///
    /// It is also the name that does not trip a scanner. CodeQL's
    /// cleartext-logging rule classifies data by the *name* it flows from, and
    /// treats `account` as identifying -- so both `account` and `data_at`
    /// had it flag five `println!`s in `radar exit` that print a mint's
    /// decimals and its revoked authorities, which are public on-chain facts a
    /// CLI exists to show. Recorded so the next person renaming this knows why
    /// it is not called what they expect.
    ///
    /// # Errors
    ///
    /// [`FetchError`] if the account cannot be fetched, does not exist, or its
    /// data is not readable base64.
    pub fn data_at(&self, address: &Address) -> Result<(Vec<u8>, String), FetchError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [address.to_string(), { "encoding": "base64" }],
        });

        let mut response = self
            .agent
            .post(&self.endpoint)
            .content_type("application/json")
            .send(body.to_string())
            .map_err(|e| FetchError::Transport(e.to_string()))?;

        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| FetchError::Transport(e.to_string()))?;

        let envelope: Envelope =
            serde_json::from_str(&text).map_err(|e| FetchError::Malformed(e.to_string()))?;

        if let Some(err) = envelope.error {
            return Err(FetchError::Node(err.message));
        }

        let account = envelope
            .result
            .and_then(|r| r.value)
            .ok_or_else(|| FetchError::NoAccount(address.to_string()))?;

        let encoded = account
            .data
            .first()
            .ok_or_else(|| FetchError::Malformed("account data was empty".to_owned()))?;
        let raw = decode_base64(encoded)
            .ok_or_else(|| FetchError::Malformed("account data was not base64".to_owned()))?;

        Ok((raw, account.owner))
    }

    /// A curve-backed quoter for `mint`, priced off the venue itself.
    ///
    /// # Why this exists beside a working aggregator
    ///
    /// `JupiterQuoter` asks the aggregator eight times per candidate to build a
    /// quote ladder. The hourly `radar consider` cron runs forty of them, which
    /// is about 320 calls an hour against a lane that is shut by
    /// `Policy::CLOSED` and cannot trade whatever the answer is.
    ///
    /// This is two account reads instead — the curve, and the fee schedule the
    /// curve's tier is read from — and the arithmetic afterwards is
    /// [`Depth`](crate::curve::Depth)'s, which is pure. Research 0022 asks for
    /// exactly this by name: price the exit off the curve rather than off a
    /// quote ladder.
    ///
    /// # Errors
    ///
    /// [`FetchError`] when either account cannot be read or parsed, including
    /// the case that matters most: a mint with no bonding-curve account is not a
    /// pump.fun token, and saying so is better than pricing it off something
    /// else.
    pub fn depth(&self, mint: &Address) -> Result<crate::curve::Depth, FetchError> {
        let curve_address = radar_pumpfun::pda::bonding_curve(mint)
            .ok_or_else(|| FetchError::Malformed("no bonding-curve PDA".to_owned()))?;
        let (raw, _) = self.data_at(&curve_address)?;
        let curve = radar_pumpfun::curve::BondingCurve::parse(&raw)
            .map_err(|e| FetchError::Malformed(format!("{e:?}")))?;

        let fee_address = radar_pumpfun::pda::fee_config()
            .ok_or_else(|| FetchError::Malformed("no fee-config PDA".to_owned()))?;
        let (fee_raw, _) = self.data_at(&fee_address)?;
        let config = radar_pumpfun::FeeConfig::parse(&fee_raw)
            .map_err(|e| FetchError::Malformed(format!("{e:?}")))?;
        // 0023: the fee is a **schedule**, and which tier a curve pays depends on
        // the curve. Substituting a remembered 125 bps would be asserting a
        // number the chain was not asked for.
        // Looked up by virtual SOL reserves, which is the question
        // `radar-pumpfun`'s own mainnet test asks the schedule and the same one
        // `radar-onchain::dossier` asks it.
        let fees = config
            .fees_at_market_cap(u128::from(curve.virtual_sol_reserves))
            .ok_or_else(|| FetchError::Malformed("no fee tier for this curve".to_owned()))?;

        Ok(crate::curve::Depth::new(*mint, curve, fees))
    }
}

/// Decodes standard base64.
///
/// Hand-rolled rather than pulling a crate: this is the only base64 in the
/// workspace outside test fixtures, and a dependency here would end up in the
/// tree of anything that links exit analysis.
#[must_use]
pub fn decode_base64(s: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;

    for byte in s.bytes() {
        if byte == b'=' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        let value = ALPHABET.iter().position(|c| *c == byte)?;
        buffer = (buffer << 6) | u32::try_from(value).ok()?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buffer >> bits) & 0xFF).unwrap_or(0));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes_the_shapes_solana_returns() {
        assert_eq!(decode_base64("").as_deref(), Some(&[][..]));
        assert_eq!(decode_base64("QQ==").as_deref(), Some(&b"A"[..]));
        assert_eq!(decode_base64("QUI=").as_deref(), Some(&b"AB"[..]));
        assert_eq!(decode_base64("QUJD").as_deref(), Some(&b"ABC"[..]));
        assert_eq!(decode_base64("QUJDRA==").as_deref(), Some(&b"ABCD"[..]));
    }

    #[test]
    fn base64_round_trips_a_full_byte_range() {
        // The mint parser reads fixed offsets out of this, so a decoder that is
        // subtly wrong would shift every field rather than fail visibly.
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let original: Vec<u8> = (0u8..=255).collect();

        let mut encoded = String::new();
        for chunk in original.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= chunk.len() {
                    encoded.push(ALPHABET[((n >> (18 - i * 6)) & 0x3F) as usize] as char);
                } else {
                    encoded.push('=');
                }
            }
        }
        assert_eq!(decode_base64(&encoded).as_deref(), Some(&original[..]));
    }

    #[test]
    fn base64_refuses_characters_outside_the_alphabet() {
        // Returning a truncated buffer would hand the mint parser bytes that
        // happen to parse into a different token's properties.
        assert_eq!(decode_base64("QU!D"), None);
        assert_eq!(decode_base64("****"), None);
    }

    #[test]
    fn whitespace_inside_the_payload_is_ignored() {
        assert_eq!(decode_base64("QU\nJD").as_deref(), Some(&b"ABC"[..]));
    }
}
