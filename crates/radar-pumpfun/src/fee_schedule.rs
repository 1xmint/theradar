// SPDX-License-Identifier: Apache-2.0
//! Complete observed fee schedules for conservative, tier-independent bounds.
//! The caller is the one-context `radar-onchain` curve reader. Historical
//! `FeeConfig::parse` callers still read only the standard schedule prefix.

use crate::fees::{FeeConfig, Fees, Tier};

/// Standard schedule plus the extension captured at mainnet slot 454343743.
/// No token classification or current market-cap tier is inferred.
pub struct FeeSchedule {
    /// Historical prefix, including non-pump flat fees and standard tiers.
    pub standard: FeeConfig,
    /// Stable tiers; empty for the historical, zero-padded layout.
    pub stable: Vec<Tier>,
    /// Exotic flat fees, absent when the extension was not observed.
    pub exotic: Option<Fees>,
}

impl FeeSchedule {
    /// Parses the historical prefix and a complete extension when present.
    /// All-zero reserved bytes retain the historical interpretation.
    ///
    /// # Errors
    /// Refuses truncated fields/vectors, unknown nonzero suffixes and invalid
    /// standard layouts. Never allocates from an unchecked vector count.
    pub fn parse(data: &[u8]) -> Result<Self, &'static str> {
        let standard = FeeConfig::parse(data).map_err(|_| "fee prefix refused")?;
        // Successful prefix parsing has already proved these bytes exist.
        let at = 69 + standard.tiers.len() * 40;
        let mut tail = &data[at..];
        if tail.iter().all(|byte| *byte == 0) {
            return Ok(Self {
                standard,
                stable: Vec::new(),
                exotic: None,
            });
        }
        let count = u32::from_le_bytes(take::<4>(&mut tail)?) as usize;
        // Never reserve memory from the claimed count. A row is pushed only
        // after all its bytes were read; a forged count stops at the first
        // incomplete row. Allocation is bounded by successfully read bytes.
        let mut stable = Vec::new();
        for _ in 0..count {
            stable.push(Tier {
                threshold_lamports: u128::from_le_bytes(take::<16>(&mut tail)?),
                fees: fees(&mut tail)?,
            });
        }
        let exotic = Some(fees(&mut tail)?);
        if tail.iter().any(|byte| *byte != 0) {
            return Err("unknown fee trailing data");
        }
        Ok(Self {
            standard,
            stable,
            exotic,
        })
    }

    /// Highest observed total fee, including both tier schedules and flat fees.
    /// Requires coverage from zero for standard and observed stable schedules.
    /// This is a conservative bound, not a prediction of venue tier selection.
    #[must_use]
    pub fn upper_bound(&self) -> Option<Fees> {
        if !self
            .standard
            .tiers
            .iter()
            .any(|tier| tier.threshold_lamports == 0)
            || (self.exotic.is_some()
                && !self.stable.iter().any(|tier| tier.threshold_lamports == 0))
        {
            return None;
        }
        self.standard
            .tiers
            .iter()
            .chain(&self.stable)
            .map(|tier| tier.fees)
            .chain(std::iter::once(self.standard.flat))
            .chain(self.exotic)
            .max_by_key(Fees::total_bps)
    }
}

fn take<const N: usize>(bytes: &mut &[u8]) -> Result<[u8; N], &'static str> {
    let (head, tail) = bytes.split_at_checked(N).ok_or("fee extension truncated")?;
    *bytes = tail;
    head.try_into().map_err(|_| "fee field size differs")
}

fn fees(bytes: &mut &[u8]) -> Result<Fees, &'static str> {
    Ok(Fees {
        lp_bps: u64::from_le_bytes(take::<8>(bytes)?),
        protocol_bps: u64::from_le_bytes(take::<8>(bytes)?),
        creator_bps: u64::from_le_bytes(take::<8>(bytes)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn captured() -> Vec<u8> {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/pumpfun_fee_extension.json"))
                .expect("capture JSON");
        let hex = value["data_hex"].as_str().expect("captured bytes");
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex"))
            .collect()
    }

    #[test]
    fn captured_extension_preserves_each_fee_component_and_legacy_padding() {
        let bytes = captured();
        assert_eq!(bytes.len(), 4097);
        let schedule = FeeSchedule::parse(&bytes).expect("complete capture");
        assert_eq!(schedule.standard.tiers.len(), 1);
        assert_eq!(schedule.stable.len(), 1);
        assert_eq!(schedule.stable[0].threshold_lamports, 0);
        let expected = Fees {
            lp_bps: 0,
            protocol_bps: 95,
            creator_bps: 30,
        };
        assert_eq!(schedule.stable[0].fees, expected);
        assert_eq!(schedule.exotic, Some(expected));
        assert_eq!(schedule.upper_bound(), Some(expected));
        let mut legacy = bytes[..109].to_vec();
        legacy.resize(4073, 0);
        let parsed = FeeSchedule::parse(&legacy).expect("historical padding");
        assert_eq!(parsed.stable, []);
        assert_eq!(parsed.exotic, None);
        assert_eq!(parsed.upper_bound(), Some(expected));
    }

    #[test]
    fn every_partial_extension_and_unknown_suffix_refuses() {
        let bytes = captured();
        for end in 110..177 {
            assert!(FeeSchedule::parse(&bytes[..end]).is_err(), "end {end}");
        }
        assert!(FeeSchedule::parse(&bytes[..177]).is_ok());
        let mut bytes = bytes;
        bytes[177] = 1;
        assert!(FeeSchedule::parse(&bytes).is_err());
        bytes[109..113].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(FeeSchedule::parse(&bytes).is_err());
        bytes[0] = 0;
        assert!(FeeSchedule::parse(&bytes).is_err());
    }

    #[test]
    fn bound_includes_all_stable_rows_and_exotic_flat_components() {
        let mut bytes = captured()[..177].to_vec();
        // Second stable tier above any practical market cap; still in the bound.
        bytes[109..113].copy_from_slice(&2u32.to_le_bytes());
        let mut row = u128::MAX.to_le_bytes().to_vec();
        for component in [20u64, 400, 30] {
            row.extend_from_slice(&component.to_le_bytes());
        }
        bytes.splice(153..153, row);
        let mut schedule = FeeSchedule::parse(&bytes).expect("two rows");
        assert_eq!(schedule.stable[1].threshold_lamports, u128::MAX);
        assert_eq!(
            schedule.stable[1].fees,
            Fees {
                lp_bps: 20,
                protocol_bps: 400,
                creator_bps: 30
            }
        );
        assert_eq!(schedule.upper_bound().expect("stable max").total_bps(), 450);
        schedule.exotic = Some(Fees {
            lp_bps: 100,
            protocol_bps: 200,
            creator_bps: 300,
        });
        assert_eq!(schedule.upper_bound().expect("exotic max").total_bps(), 600);
        schedule.standard.flat.protocol_bps = 700;
        assert_eq!(schedule.upper_bound().expect("flat max").total_bps(), 730);
    }

    #[test]
    fn missing_coverage_never_becomes_a_zero_fee() {
        let mut schedule = FeeSchedule::parse(&captured()).expect("capture");
        schedule.standard.tiers[0].threshold_lamports = 1;
        assert!(schedule.upper_bound().is_none());
        schedule.standard.tiers[0].threshold_lamports = 0;
        schedule.stable[0].threshold_lamports = 1;
        assert!(schedule.upper_bound().is_none());
        schedule.stable.clear();
        assert!(schedule.upper_bound().is_none());
    }
}
