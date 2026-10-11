// SPDX-License-Identifier: Apache-2.0
//! Receiver-verified SOL/USD observations for `radar sol-price-read`.
//! RPC/network identity remains a deployment trust assumption. Confidence
//! bounds describe this oracle observation, not a guaranteed execution price.

use crate::{Budget, MultiAccountRead, RpcClient};
use radar_types::{Address, Slot};
use serde::Serialize;

/// Sponsored shard-zero SOL/USD account, independently captured on mainnet.
pub const ACCOUNT: &str = "7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE";
const OWNER: &str = "rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ";
const DISCRIMINATOR: [u8; 8] = [34, 241, 35, 99, 157, 126, 244, 205];
const FEED: [u8; 32] = [
    0xef, 0x0d, 0x8b, 0x6f, 0xda, 0x2c, 0xeb, 0xa4, 0x1d, 0xa1, 0x5d, 0x40, 0x95, 0xd1, 0xda, 0x39,
    0x2a, 0x0d, 0x2f, 0x8e, 0xd0, 0xc6, 0xc7, 0xbc, 0x0f, 0x4c, 0xfa, 0xc8, 0xc2, 0x80, 0xb5, 0x6d,
];

/// Explicit freshness and confidence requirements; there is no permissive default.
pub struct Bounds {
    /// Oldest acceptable bank context and oracle posting slot.
    pub minimum_slot: Slot,
    /// Maximum age of the oracle publication in host-clock seconds.
    pub maximum_age_secs: u64,
    /// Maximum confidence/price ratio in basis points.
    pub maximum_confidence_bps: u32,
}

/// Integer USD band for one SOL, rounded outward to micro-USD.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Observation {
    /// Bank context carrying the receiver account.
    pub context_slot: Slot,
    /// Receiver's recorded posting slot, independently checked against context.
    pub posted_slot: Slot,
    /// Oracle publication time, independently checked against the host clock.
    pub published_at_unix_secs: u64,
    /// Lower oracle confidence bound, rounded down.
    pub lower_micro_usd_per_sol: u64,
    /// Upper oracle confidence bound, rounded up.
    pub upper_micro_usd_per_sol: u64,
}

fn scaled(value: u128, exponent: i32, upper: bool) -> Result<u64, String> {
    let shift = exponent.checked_add(6).ok_or("oracle exponent overflow")?;
    let power = 10_u128
        .checked_pow(shift.unsigned_abs())
        .ok_or("oracle scale exceeds integer range")?;
    let amount = if shift >= 0 {
        value.checked_mul(power).ok_or("oracle price overflow")?
    } else if upper {
        value.div_ceil(power)
    } else {
        value / power
    };
    u64::try_from(amount).map_err(|_| "oracle price exceeds dollar range".into())
}

fn decode(read: &MultiAccountRead, bounds: &Bounds, now: u64) -> Result<Observation, String> {
    let slot = read.slot.ok_or("oracle context missing")?;
    let [Some(account)] = read.accounts.as_slice() else {
        return Err("oracle account missing or count differs".into());
    };
    let data = &account.data;
    if account.owner.as_deref() != Some(OWNER)
        || data.len() != 134
        || data[..8] != DISCRIMINATOR
        || data[40] != 1
        || data[41..73] != FEED
    {
        return Err("oracle owner, layout, full verification or feed identity differs".into());
    }
    // PriceUpdateV2 uses Borsh little-endian; Full occupies one byte. The
    // allocated account is 134 bytes because Partial occupies two bytes.
    let signed = i64::from_le_bytes(data[73..81].try_into().expect("checked layout"));
    let confidence = u64::from_le_bytes(data[81..89].try_into().expect("checked layout"));
    let exponent = i32::from_le_bytes(data[89..93].try_into().expect("checked layout"));
    let published = i64::from_le_bytes(data[93..101].try_into().expect("checked layout"));
    let posted = u64::from_le_bytes(data[125..133].try_into().expect("checked layout"));
    let price = u64::try_from(signed).map_err(|_| "oracle price is not positive")?;
    let published = u64::try_from(published).map_err(|_| "oracle publication time invalid")?;
    if price == 0
        || confidence >= price
        || bounds.maximum_confidence_bps > 10_000
        || u128::from(confidence) * 10_000
            > u128::from(price) * u128::from(bounds.maximum_confidence_bps)
    {
        return Err("oracle confidence or price refused".into());
    }
    if slot < bounds.minimum_slot
        || posted < bounds.minimum_slot.get()
        || posted > slot.get()
        || now
            .checked_sub(published)
            .is_none_or(|age| age > bounds.maximum_age_secs)
    {
        return Err("oracle context or publication is not current".into());
    }
    let lower = scaled(u128::from(price - confidence), exponent, false)?;
    let upper = scaled(u128::from(price) + u128::from(confidence), exponent, true)?;
    if lower == 0 {
        return Err("oracle lower bound is below dollar precision".into());
    }
    Ok(Observation {
        context_slot: slot,
        posted_slot: Slot(posted),
        published_at_unix_secs: published,
        lower_micro_usd_per_sol: lower,
        upper_micro_usd_per_sol: upper,
    })
}

/// Read the fixed receiver-owned SOL/USD account through finalized direct RPC.
///
/// # Errors
/// Refuses absent, stale, future, partial, misidentified or unrepresentable
/// observations. Error text contains no endpoint or provider response.
pub fn read(
    client: &RpcClient,
    budget: &mut Budget,
    bounds: &Bounds,
) -> Result<Observation, String> {
    let account: Address = ACCOUNT
        .parse()
        .map_err(|_| "fixed oracle account invalid")?;
    let result = client
        .accounts(budget, &[account])
        .map_err(|_| "oracle RPC read failed")?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "host clock unavailable")?
        .as_secs();
    decode(&result, bounds, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured via finalized getAccountInfo on 2026-10-11 at slot 455726015.
    // SDK: pyth-network/pyth-crosschain, price_update.rs PriceUpdateV2 and
    // pythnet_sdk/messages.rs PriceFeedMessage. This is captured data, not a
    // freshly serialized mock of the decoder's own assumptions.
    const CAPTURE: &str = "IvEjY51+9M1gMUcENA3t3zcf1CRyFI8kjp0abRpesqw6zYt/1dayQwHvDYtv2izrpB2hXUCV0do5Kg0vjtDGx7wPTPrIwoC1bf82BJwCAAAALUcYAAAAAAD4////qsDLagAAAACpwMtqAAAAAFi/XZcCAAAAbTwuAAAAAACD0ykbAAAAAAA=";
    const PUBLISHED: u64 = 1_791_738_026;

    fn fixture() -> (MultiAccountRead, Bounds) {
        (
            MultiAccountRead {
                slot: Some(Slot(455_726_015)),
                accounts: vec![Some(crate::OwnedAccount {
                    data: radar_types::b64::decode(CAPTURE).unwrap(),
                    owner: Some(OWNER.into()),
                })],
            },
            Bounds {
                minimum_slot: Slot(455_725_955),
                maximum_age_secs: 60,
                maximum_confidence_bps: 100,
            },
        )
    }

    #[test]
    fn captured_receiver_price_is_scaled_outward_without_floats() {
        let (packet, bounds) = fixture();
        assert_eq!(
            decode(&packet, &bounds, PUBLISHED).unwrap(),
            Observation {
                context_slot: Slot(455_726_015),
                posted_slot: Slot(455_725_955),
                published_at_unix_secs: PUBLISHED,
                lower_micro_usd_per_sol: 112_058_654,
                upper_micro_usd_per_sol: 112_090_476,
            }
        );
        assert_eq!(scaled(101, -8, false).unwrap(), 1);
        assert_eq!(scaled(101, -8, true).unwrap(), 2);
        assert_eq!(scaled(100, -8, true).unwrap(), 1);
        assert_eq!(scaled(3, -6, true).unwrap(), 3);
        assert_eq!(scaled(3, -5, false).unwrap(), 30);
        for exponent in [i32::MIN, i32::MAX, 40, -50] {
            assert!(scaled(3, exponent, false).is_err());
        }
        assert!(scaled(u128::MAX, -5, true).is_err());
        assert!(scaled(u128::from(u64::MAX) + 1, -6, false).is_err());
    }

    #[test]
    fn receiver_identity_and_complete_layout_are_required() {
        let (packet, bounds) = fixture();
        for offset in [0, 40, 41] {
            let mut bad = packet.clone();
            bad.accounts[0].as_mut().unwrap().data[offset] ^= 1;
            assert!(decode(&bad, &bounds, PUBLISHED).is_err());
        }
        for owner in [None, Some(ACCOUNT.into())] {
            let mut bad = packet.clone();
            bad.accounts[0].as_mut().unwrap().owner = owner;
            assert!(decode(&bad, &bounds, PUBLISHED).is_err());
        }
        for length in [0, 8, 40, 73, 133, 135] {
            let mut bad = packet.clone();
            bad.accounts[0].as_mut().unwrap().data.resize(length, 0);
            assert!(decode(&bad, &bounds, PUBLISHED).is_err());
        }
        for accounts in [vec![], vec![None], vec![packet.accounts[0].clone(); 2]] {
            let mut bad = packet.clone();
            bad.accounts = accounts;
            assert!(decode(&bad, &bounds, PUBLISHED).is_err());
        }
    }

    #[test]
    fn publication_and_bank_context_boundaries_refuse_future_or_stale_prices() {
        let (packet, mut bounds) = fixture();
        assert!(decode(&packet, &bounds, PUBLISHED + 60).is_ok());
        assert!(decode(&packet, &bounds, PUBLISHED + 61).is_err());
        assert!(decode(&packet, &bounds, PUBLISHED - 1).is_err());
        let mut bad = packet.clone();
        bad.slot = None;
        assert!(decode(&bad, &bounds, PUBLISHED).is_err());
        bad.slot = Some(Slot(455_725_954));
        assert!(decode(&bad, &bounds, PUBLISHED).is_err());
        bounds.minimum_slot = Slot(455_725_956);
        assert!(decode(&packet, &bounds, PUBLISHED).is_err());
        bounds.minimum_slot = Slot(455_725_954);
        bad.slot = Some(Slot(455_725_954));
        assert!(decode(&bad, &bounds, PUBLISHED).is_err());
        bad.slot = Some(Slot(455_725_955));
        assert!(decode(&bad, &bounds, PUBLISHED).is_ok());
        bounds.minimum_slot = Slot(455_725_955);
        assert!(decode(&bad, &bounds, PUBLISHED).is_ok());
    }

    #[test]
    fn confidence_positive_price_and_integer_ranges_are_enforced() {
        let (packet, mut bounds) = fixture();
        for (price, confidence, expected) in [
            (100_i64, 1_u64, true),
            (100, 2, false),
            (100, 100, false),
            (0, 0, false),
            (-1, 0, false),
        ] {
            let mut bad = packet.clone();
            let data = &mut bad.accounts[0].as_mut().unwrap().data;
            data[73..81].copy_from_slice(&price.to_le_bytes());
            data[81..89].copy_from_slice(&confidence.to_le_bytes());
            data[89..93].copy_from_slice(&(-6_i32).to_le_bytes());
            assert_eq!(decode(&bad, &bounds, PUBLISHED).is_ok(), expected);
        }
        bounds.maximum_confidence_bps = 10_001;
        assert!(decode(&packet, &bounds, PUBLISHED).is_err());
        bounds.maximum_confidence_bps = 10_000;
        assert!(decode(&packet, &bounds, PUBLISHED).is_ok());
        let mut equal = packet.clone();
        let data = &mut equal.accounts[0].as_mut().unwrap().data;
        data[73..81].copy_from_slice(&100_i64.to_le_bytes());
        data[81..89].copy_from_slice(&100_u64.to_le_bytes());
        assert_eq!(
            decode(&equal, &bounds, PUBLISHED).unwrap_err(),
            "oracle confidence or price refused"
        );
        bounds.maximum_confidence_bps = 0;
        assert!(decode(&packet, &bounds, PUBLISHED).is_err());
        equal.accounts[0].as_mut().unwrap().data[81..89].copy_from_slice(&0_u64.to_le_bytes());
        assert!(decode(&equal, &bounds, PUBLISHED).is_ok());
        for (offset, bytes) in [
            (93, (-1_i64).to_le_bytes().to_vec()),
            (89, (-38_i32).to_le_bytes().to_vec()),
        ] {
            let mut bad = packet.clone();
            bad.accounts[0].as_mut().unwrap().data[offset..offset + bytes.len()]
                .copy_from_slice(&bytes);
            assert!(decode(&bad, &fixture().1, PUBLISHED).is_err());
        }
    }
}
