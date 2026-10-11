// SPDX-License-Identifier: Apache-2.0
//! Pure canonical Solana program-derived addresses, without I/O or signing.
//!
//! The existing builder algorithm shared by actual builder and signer callers.
//! Callers supply protocol-defined seeds; this does not validate arbitrary seed
//! lengths or prove an account's contents, owner or spendability.

use radar_types::Address;
use sha2::{Digest, Sha256};

const MARKER: &[u8] = b"ProgramDerivedAddress";

/// Whether a 32-byte value is a point on the ed25519 curve.
///
/// A PDA is precisely an address that is **not**, because that is what
/// guarantees no private key exists for it.
fn on_curve(bytes: &[u8; 32]) -> bool {
    ed25519_dalek::VerifyingKey::from_bytes(bytes).is_ok()
}

/// Derives a program address from seeds, with its bump.
///
/// `None` when every bump from 255 to 0 lands on the curve, which is
/// astronomically unlikely and is returned rather than panicked so that a caller
/// building a transaction gets a refusal instead of a crash.
#[must_use]
pub fn find(seeds: &[&[u8]], program: &Address) -> Option<(Address, u8)> {
    // Counting **down** from 255. Solana's canonical address is the first found
    // in this direction; counting up yields a different address that is equally
    // off-curve and that the program will not accept.
    for bump in (0..=u8::MAX).rev() {
        let mut hasher = Sha256::new();
        for seed in seeds {
            hasher.update(seed);
        }
        hasher.update([bump]);
        hasher.update(program.as_bytes());
        hasher.update(MARKER);
        let digest: [u8; 32] = hasher.finalize().into();
        if !on_curve(&digest) {
            return Some((Address::new(digest), bump));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    const MINT: &str = "6T1BNshzGAKAHvJ3NZ5n62X2eg5rqqsMipUMZJvLpump";
    fn addr(value: &str) -> Address {
        value.parse().unwrap()
    }
    fn program() -> Address {
        addr("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")
    }
    fn bonding_curve(mint: &Address) -> Option<Address> {
        find(&[b"bonding-curve", mint.as_bytes()], &program()).map(|(address, _)| address)
    }
    #[test]
    fn a_derived_address_is_never_on_the_curve() {
        // The property that makes a PDA unsignable. If a derivation returned an
        // on-curve address, someone could hold its key.
        for mint in [MINT, "So11111111111111111111111111111111111111112"] {
            let derived = bonding_curve(&addr(mint)).expect("a derivation");
            assert!(
                !on_curve(derived.as_bytes()),
                "{mint} derived an on-curve address"
            );
        }
    }

    #[test]
    fn the_bump_counts_down_from_255() {
        // Solana's canonical address is the first found counting down. Counting
        // up would yield a different address that is equally off-curve and that
        // the program would reject -- a bug with no symptom until a transaction
        // fails on chain.
        let (global, bump) = find(&[b"global"], &program()).expect("a derivation");
        assert_eq!(bump, 255, "global's canonical bump is the first tried");
        assert_eq!(global, addr("4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf"));

        // And one that is not 255, so the test above is not passing because the
        // search stops immediately every time.
        let (curve, mint_bump) =
            find(&[b"bonding-curve", addr(MINT).as_bytes()], &program()).expect("a derivation");
        assert!(mint_bump < 255, "got {mint_bump}");
        assert_eq!(curve, addr("BfVg4yLn5WcffzTyXvWgqaupqmAeX4en2Pawq32TdZpb"));
    }
}
