// SPDX-License-Identifier: Apache-2.0
//! Mint, bonding curve and fee schedule from one node-reported context.
//! An actual caller is `radar curve-exit`; this reader never signs or values USD.

use crate::{Budget, MultiAccountRead, OwnedAccount, RpcClient};
use radar_pumpfun::fee_schedule::FeeSchedule;
use radar_pumpfun::token::MintAccount;
use radar_pumpfun::{BondingCurve, Fees, pda};
use radar_types::{Address, Slot};

/// Parsed mint, curve and fees attributed to one RPC response context.
/// Provider truth and network identity remain deployment trust assumptions.
pub struct CurveMarket {
    /// The requested mint, not metadata supplied by a token creator.
    pub mint: Address,
    /// The shared node-reported context slot.
    pub slot: Slot,
    /// Reserves and creator from the correctly owned derived curve account.
    pub curve: BondingCurve,
    /// Observed schedule, without an assumed market-cap tier.
    pub fees: FeeSchedule,
    /// Parsed token program, decimals, supply and freeze authority.
    pub token: MintAccount,
    /// Current COption tag; this read does not establish a historical latch.
    pub mint_authority_active: bool,
}

impl CurveMarket {
    /// Maximum total fee among the observed rows, including flat fees.
    /// Returns no bound when the tier schedule does not cover from zero.
    /// This is not a tier prediction or a guarantee against later updates.
    #[must_use]
    pub fn fee_upper_bound(&self) -> Option<Fees> {
        self.fees.upper_bound()
    }
}

fn addresses(mint: Address) -> Result<[Address; 3], String> {
    Ok([
        mint,
        pda::bonding_curve(&mint).ok_or("curve address unavailable")?,
        pda::fee_config().ok_or("fee address unavailable")?,
    ])
}

fn owner(account: &OwnedAccount) -> Result<Address, String> {
    account
        .owner
        .as_ref()
        .ok_or("account owner missing")?
        .parse()
        .map_err(|_| "account owner invalid".into())
}

fn at_one_slot(mint: Address, together: &MultiAccountRead) -> Result<CurveMarket, String> {
    let slot = together.slot.ok_or("market context slot missing")?;
    if together.accounts.len() != 3 {
        return Err("market account count differs".into());
    }
    let accounts: Vec<&OwnedAccount> = together
        .accounts
        .iter()
        .map(|value| value.as_ref().ok_or("market account missing"))
        .collect::<Result<_, _>>()?;
    let mint_owner = owner(accounts[0])?;
    if owner(accounts[1])? != pda::PROGRAM_ID || owner(accounts[2])? != pda::FEE_PROGRAM {
        return Err("market program owner differs".into());
    }
    let token = MintAccount::parse(&accounts[0].data, &mint_owner)
        .map_err(|_| "mint layout or extension refused")?;
    // The existing parser records initialization as a boolean. At this boundary
    // only the canonical initialized byte is accepted; 2 is not initialized.
    if accounts[0].data[45] != 1 {
        return Err("mint is not canonically initialized".into());
    }
    let curve = BondingCurve::parse(&accounts[1].data).map_err(|_| "curve layout refused")?;
    let fees = FeeSchedule::parse(&accounts[2].data).map_err(str::to_owned)?;
    // Holders may burn tokens after the curve records its original supply.
    // A lower current supply is not evidence that the accounts were mismatched.
    if token.supply == 0 || token.supply > curve.token_total_supply {
        return Err("mint supply is zero or exceeds the curve total".into());
    }
    Ok(CurveMarket {
        mint,
        slot,
        curve,
        fees,
        token,
        mint_authority_active: accounts[0].data[..4] != [0; 4],
    })
}

/// Reads the mint and its derived curve/fee accounts in one finalized RPC call.
///
/// # Errors
/// Static refusal on missing context/accounts/owners, unsupported layouts or
/// inconsistent supply. Never includes a provider response or credential URL.
pub fn read(client: &RpcClient, budget: &mut Budget, mint: Address) -> Result<CurveMarket, String> {
    let together = client
        .accounts(budget, &addresses(mint)?)
        .map_err(|_| "market RPC read failed")?;
    at_one_slot(mint, &together)
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_pumpfun::token::SPL_TOKEN_PROGRAM;

    fn account(data: Vec<u8>, owner: Address) -> OwnedAccount {
        OwnedAccount {
            data,
            owner: Some(owner.to_string()),
        }
    }
    fn fixture() -> MultiAccountRead {
        let mut mint = vec![0; 82];
        mint[36..44].copy_from_slice(&1_000_000u64.to_le_bytes());
        mint[44] = 6;
        mint[45] = 1;
        let mut curve = radar_pumpfun::curve::DISCRIMINATOR.to_vec();
        for amount in [
            1_000_000u64,
            30_000_000_000,
            500_000,
            1_000_000_000,
            1_000_000,
        ] {
            curve.extend_from_slice(&amount.to_le_bytes());
        }
        curve.push(0);
        curve.extend_from_slice(&[0x33; 32]);
        let mut fees = radar_pumpfun::fees::FEE_CONFIG_DISCRIMINATOR.to_vec();
        fees.extend_from_slice(&[0; 33]);
        for fee in [0u64, 95, 30] {
            fees.extend_from_slice(&fee.to_le_bytes());
        }
        fees.extend_from_slice(&1u32.to_le_bytes());
        fees.extend_from_slice(&0u128.to_le_bytes());
        for fee in [0u64, 95, 30] {
            fees.extend_from_slice(&fee.to_le_bytes());
        }
        MultiAccountRead {
            slot: Some(Slot(777)),
            accounts: vec![
                account(mint, SPL_TOKEN_PROGRAM),
                account(curve, pda::PROGRAM_ID),
                account(fees, pda::FEE_PROGRAM),
            ]
            .into_iter()
            .map(Some)
            .collect(),
        }
    }

    #[test]
    fn one_context_keeps_owned_curve_mint_fees_and_authority_facts() {
        let mut together = fixture();
        let mint = Address::new([0x22; 32]);
        let value = at_one_slot(mint, &together).expect("one read");
        assert_eq!(value.mint, mint);
        assert_eq!(value.slot, Slot(777));
        assert_eq!(value.token.decimals, 6);
        assert_eq!(value.token.supply, 1_000_000);
        assert_eq!(value.curve.virtual_sol_reserves, 30_000_000_000);
        assert_eq!(value.curve.creator, Address::new([0x33; 32]));
        assert_eq!(value.fees.standard.tiers[0].fees.total_bps(), 125);
        assert!(!value.mint_authority_active);
        together.accounts[0].as_mut().expect("mint").data[0] = 1;
        assert!(
            at_one_slot(mint, &together)
                .expect("active authority recorded")
                .mint_authority_active
        );
    }

    #[test]
    fn missing_context_accounts_and_wrong_owners_refuse() {
        let mint = Address::new([0x22; 32]);
        let mut together = fixture();
        together.slot = None;
        assert!(at_one_slot(mint, &together).is_err());
        for index in 0..3 {
            let mut together = fixture();
            together.accounts[index] = None;
            assert!(at_one_slot(mint, &together).is_err());
            let mut together = fixture();
            together.accounts[index].as_mut().expect("account").owner = None;
            assert!(at_one_slot(mint, &together).is_err());
            let mut together = fixture();
            together.accounts[index].as_mut().expect("account").owner =
                Some(Address::new([0x44; 32]).to_string());
            assert!(at_one_slot(mint, &together).is_err());
        }
        let mut together = fixture();
        together.accounts.pop();
        assert!(at_one_slot(mint, &together).is_err());
        together = fixture();
        together.accounts.push(together.accounts[0].clone());
        assert!(at_one_slot(mint, &together).is_err());
        together = fixture();
        together.accounts[0].as_mut().expect("mint").owner = Some("invalid".into());
        assert!(at_one_slot(mint, &together).is_err());
    }

    #[test]
    fn mismatched_supply_bad_initialization_and_unknown_layout_refuse() {
        let mint = Address::new([0x22; 32]);
        for (account, at, value) in [(0, 36, 255), (0, 45, 0), (0, 45, 2), (1, 0, 0), (2, 0, 0)] {
            let mut together = fixture();
            together.accounts[account].as_mut().expect("account").data[at] = value;
            assert!(at_one_slot(mint, &together).is_err());
        }
        let mut together = fixture();
        together.accounts[0].as_mut().expect("mint").data[36..44].fill(0);
        together.accounts[1].as_mut().expect("curve").data[40..48].fill(0);
        assert!(at_one_slot(mint, &together).is_err());
        let mut together = fixture();
        together.accounts[2].as_mut().expect("fees").data.push(1);
        assert!(at_one_slot(mint, &together).is_err());
        together.accounts[2].as_mut().expect("fees").data.pop();
        together.accounts[2].as_mut().expect("fees").data.push(0);
        assert!(at_one_slot(mint, &together).is_ok());
        together.accounts[0].as_mut().expect("mint").data[36..44]
            .copy_from_slice(&500_000u64.to_le_bytes());
        assert_eq!(
            at_one_slot(mint, &together)
                .expect("burned supply")
                .token
                .supply,
            500_000
        );
    }

    #[test]
    fn fee_upper_bound_covers_all_observed_rows_without_guessing_a_tier() {
        let mut market = at_one_slot(Address::new([0x22; 32]), &fixture()).expect("market");
        assert_eq!(market.fee_upper_bound().expect("coverage").total_bps(), 125);
        market.fees.standard.tiers.push(radar_pumpfun::fees::Tier {
            threshold_lamports: u128::MAX,
            fees: Fees {
                lp_bps: 0,
                protocol_bps: 500,
                creator_bps: 0,
            },
        });
        assert_eq!(
            market.fee_upper_bound().expect("all tiers").total_bps(),
            500
        );
        market.fees.standard.flat.creator_bps = 1000;
        assert_eq!(
            market.fee_upper_bound().expect("flat too").total_bps(),
            1095
        );
        market.fees.standard.tiers[0].threshold_lamports = 1;
        assert_eq!(market.fee_upper_bound(), None);
        market.fees.standard.tiers.clear();
        assert_eq!(market.fee_upper_bound(), None);
    }
}
