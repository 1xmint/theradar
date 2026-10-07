// SPDX-License-Identifier: Apache-2.0
//! A requested hypothetical exit against one node-reported market context.
//! No wallet ownership, USD price, full execution costs or signing is inferred.

use radar_onchain::curve_market::{self, CurveMarket};
use radar_onchain::{Budget, RpcClient};
use radar_types::Address;
use serde_json::{Value, json};
use std::time::Duration;

fn estimate(market: &CurveMarket, raw_tokens: u64) -> Result<Value, String> {
    if market.mint_authority_active || market.token.freeze_authority.is_some() {
        return Err("mint or freeze authority remains active".into());
    }
    if raw_tokens > market.token.supply {
        return Err("requested tokens exceed mint supply".into());
    }
    let gross = market
        .curve
        .sell(raw_tokens)
        .ok_or("curve cannot price this exit")?;
    if gross.lamports > market.curve.real_sol_reserves {
        return Err("real SOL reserves do not cover the gross exit".into());
    }
    // Require coverage from zero and bound across every observed row, including
    // flat fees. No guessed market-cap tier or remembered constant enters here.
    let upper = market
        .fee_upper_bound()
        .ok_or("fee schedule has no coverage from zero")?;
    if upper.total_bps() >= 10_000 {
        return Err("fee bound consumes the exit".into());
    }
    let fee = upper.charge(gross.lamports);
    let net = gross.lamports - fee;
    if net == 0 {
        return Err("fee rounding consumes the exit".into());
    }
    Ok(json!({
        "version":1, "authority":"read_only", "commitment":"finalized",
        "mint":market.mint.to_string(), "slot":market.slot.get().to_string(),
        "creator":market.curve.creator.to_string(), "decimals":market.token.decimals,
        "raw_tokens":raw_tokens.to_string(), "mint_supply":market.token.supply.to_string(),
        "gross_lamports":gross.lamports.to_string(), "venue_fee_upper_lamports":fee.to_string(),
        "net_lamports_at_observed_state":net.to_string(), "impact_bps":gross.impact_bps.to_string(),
        "venue_fee_upper_bps":upper.total_bps().to_string(),
        "fee_bound_basis":"maximum_total_across_observed_flat_and_tiers",
        "real_sol_reserves":market.curve.real_sol_reserves.to_string(),
        "usd_value":null, "exit_capacity":null, "network_fee":null,
        "transaction_simulation":null, "wallet_ownership":"unverified"
    }))
}

pub fn run(args: &[String]) -> Result<(), String> {
    let mint: Address = crate::flag(args, "--mint")
        .ok_or("curve-exit needs --mint <address>")?
        .parse()
        .map_err(|_| "curve-exit needs a valid mint")?;
    let raw_tokens: u64 = crate::flag(args, "--raw-tokens")
        .ok_or("curve-exit needs --raw-tokens <N>")?
        .parse()
        .map_err(|_| "curve-exit needs an integer token quantity")?;
    if raw_tokens == 0 {
        return Err("curve-exit needs a positive token quantity".into());
    }
    let endpoint = crate::flag(args, "--rpc")
        .filter(|value| !value.trim().is_empty())
        .ok_or("curve-exit needs an explicit --rpc <URL>")?;
    let started = crate::wallet_read::now()?;
    let market = curve_market::read(
        &RpcClient::new(endpoint),
        &mut Budget::new(1, 0, Duration::from_secs(20)),
        mint,
    )?;
    let mut result = estimate(&market, raw_tokens)?;
    result["read_started_at_unix_secs"] = json!(started);
    result["read_completed_at_unix_secs"] = json!(crate::wallet_read::now()?);
    println!("{result}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_pumpfun::fees::Tier;
    use radar_pumpfun::token::{MintAccount, TokenProgram};
    use radar_pumpfun::{BondingCurve, FeeConfig, Fees};
    use radar_types::Slot;

    fn market() -> CurveMarket {
        let fees = Fees {
            lp_bps: 0,
            protocol_bps: 95,
            creator_bps: 30,
        };
        CurveMarket {
            mint: Address::new([0x22; 32]),
            slot: Slot(777),
            mint_authority_active: false,
            token: MintAccount {
                program: TokenProgram::Spl,
                decimals: 6,
                supply: 1_000_000,
                freeze_authority: None,
                initialized: true,
            },
            curve: BondingCurve {
                virtual_token_reserves: 1_000_000,
                virtual_sol_reserves: 30_000_000_000,
                real_token_reserves: 500_000,
                real_sol_reserves: 1_000_000_000,
                token_total_supply: 1_000_000,
                complete: false,
                creator: Address::new([0x33; 32]),
            },
            fees: radar_pumpfun::fee_schedule::FeeSchedule {
                standard: FeeConfig {
                    flat: fees,
                    tiers: vec![Tier {
                        threshold_lamports: 0,
                        fees,
                    }],
                },
                stable: Vec::new(),
                exotic: None,
            },
        }
    }

    #[test]
    fn an_exit_uses_the_largest_observed_fee_and_exact_integer_proceeds() {
        let mut market = market();
        market.fees.standard.tiers.push(Tier {
            threshold_lamports: u128::MAX,
            fees: Fees {
                lp_bps: 0,
                protocol_bps: 250,
                creator_bps: 0,
            },
        });
        let gross = market.curve.sell(1000).expect("gross").lamports;
        let output = estimate(&market, 1000).expect("quote");
        assert_eq!(output["venue_fee_upper_bps"], "250");
        let fee = u64::try_from((u128::from(gross) * 250).div_ceil(10_000)).expect("fee");
        assert_eq!(output["venue_fee_upper_lamports"], fee.to_string());
        assert_eq!(output["gross_lamports"], gross.to_string());
        assert_eq!(
            output["net_lamports_at_observed_state"],
            (gross - fee).to_string()
        );
        assert_eq!(output["mint"], market.mint.to_string());
        assert_eq!(output["slot"], "777");
        assert_eq!(output["decimals"], 6);
        assert_eq!(output["creator"], market.curve.creator.to_string());
        assert_eq!(output["authority"], "read_only");
        assert_eq!(output["wallet_ownership"], "unverified");
        for field in [
            "usd_value",
            "exit_capacity",
            "network_fee",
            "transaction_simulation",
        ] {
            assert!(output[field].is_null());
        }
        market.fees.standard.flat.protocol_bps = 500;
        assert_eq!(
            estimate(&market, 1000).expect("flat ceiling")["venue_fee_upper_bps"],
            "530"
        );
    }

    #[test]
    fn active_authorities_missing_fee_coverage_and_unfillable_sizes_refuse() {
        let mut value = market();
        value.mint_authority_active = true;
        assert!(estimate(&value, 1000).is_err());
        value = market();
        value.token.freeze_authority = Some(Address::new([1; 32]));
        assert!(estimate(&value, 1000).is_err());
        value = market();
        value.curve.complete = true;
        assert!(estimate(&value, 1000).is_err());
        value = market();
        value.fees.standard.tiers.clear();
        assert!(estimate(&value, 1000).is_err());
        value = market();
        value.fees.standard.tiers[0].threshold_lamports = 1;
        assert!(estimate(&value, 1000).is_err());
        value = market();
        value.fees.standard.flat.protocol_bps = 10_000;
        assert!(estimate(&value, 1000).is_err());
        value = market();
        value.fees.standard.flat.protocol_bps = 9970;
        assert!(estimate(&value, 1000).is_err());
        value.fees.standard.flat.protocol_bps = 9969;
        assert!(estimate(&value, 1000).is_ok());
        value = market();
        value.curve.virtual_token_reserves = 1_000_000;
        value.curve.virtual_sol_reserves = 1;
        assert!(estimate(&value, 1).is_err());
        value = market();
        assert!(estimate(&value, 0).is_err());
        assert!(estimate(&value, 1_000_001).is_err());
        value.curve.real_sol_reserves = u64::MAX;
        assert!(estimate(&value, value.token.supply).is_ok());
    }

    #[test]
    fn real_reserves_cover_gross_not_only_fee_adjusted_proceeds() {
        let mut market = market();
        let gross = market.curve.sell(1000).expect("gross").lamports;
        market.curve.real_sol_reserves = gross - 1;
        assert!(estimate(&market, 1000).is_err());
        market.curve.real_sol_reserves = gross;
        assert!(estimate(&market, 1000).is_ok());
    }

    #[test]
    fn bad_arguments_refuse_before_any_rpc() {
        let mint = Address::new([0x22; 32]).to_string();
        for (args, message) in [
            (vec![], "curve-exit needs --mint <address>"),
            (vec!["--mint", "invalid"], "curve-exit needs a valid mint"),
            (vec!["--mint", &mint], "curve-exit needs --raw-tokens <N>"),
            (
                vec!["--mint", &mint, "--raw-tokens", "1.5"],
                "curve-exit needs an integer token quantity",
            ),
            (
                vec!["--mint", &mint, "--raw-tokens", "0"],
                "curve-exit needs a positive token quantity",
            ),
            (
                vec!["--mint", &mint, "--raw-tokens", "1"],
                "curve-exit needs an explicit --rpc <URL>",
            ),
            (
                vec!["--mint", &mint, "--raw-tokens", "1", "--rpc", " "],
                "curve-exit needs an explicit --rpc <URL>",
            ),
        ] {
            assert_eq!(
                run(&args
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>()),
                Err(message.into())
            );
        }
    }
}
