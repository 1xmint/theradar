// SPDX-License-Identifier: Apache-2.0
//! Live observed spot holdings. Marks do not establish equity, basis or authority.

use std::{collections::BTreeMap, time::Duration};

use radar_onchain::rpc::{TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID};
use radar_onchain::{Budget, RpcClient, curve_market, sol_price};
use radar_types::{
    Address, Asset, AssetRole, Balance, Decimals, Holding, MicroUsd, Portfolio, Slot,
    TokenQuantity, Unvaluable, Valuation,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct RawHolding {
    mint: Address,
    program: Address,
    decimals: u8,
    raw_amount: String,
    state: String,
    mint_authority_active: bool,
    freeze_authority: Option<Address>,
    delegate: Option<Value>,
}

struct Group {
    raw: u64,
    decimals: Decimals,
    program: Address,
    eligible: bool,
}

fn integer(value: &Value) -> Result<u64, String> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse().ok())
        .ok_or_else(|| "portfolio observation integer missing".into())
}

fn groups(evidence: &Value) -> Result<BTreeMap<Asset, Group>, String> {
    let rows = evidence["raw_token_verification"]["accounts"]
        .as_array()
        .ok_or("verified portfolio accounts missing")?;
    let mut groups = BTreeMap::<Asset, Group>::new();
    for row in rows {
        let row: RawHolding = serde_json::from_value(row.clone())
            .map_err(|_| "verified portfolio account malformed")?;
        let program = row.program.to_string();
        let asset = if program == TOKEN_PROGRAM_ID {
            Asset::spl(row.mint)
        } else if program == TOKEN_2022_PROGRAM_ID {
            Asset::token_2022(row.mint)
        } else {
            return Err("portfolio token program unsupported".into());
        };
        let decimals =
            Decimals::from_mint_account(row.decimals).ok_or("portfolio decimals unsupported")?;
        let group = groups.entry(asset).or_insert(Group {
            raw: 0,
            decimals,
            program: row.program,
            eligible: true,
        });
        if group.decimals != decimals || group.program != row.program {
            return Err("portfolio holding units differ".into());
        }
        group.raw = group
            .raw
            .checked_add(
                row.raw_amount
                    .parse::<u64>()
                    .map_err(|_| "portfolio holding quantity invalid")?,
            )
            .ok_or("portfolio holding quantity overflow")?;
        group.eligible &= row.state == "initialized"
            && !row.mint_authority_active
            && row.freeze_authority.is_none()
            && row.delegate.is_none();
    }
    Ok(groups)
}

struct Mark {
    lamports: u64,
    slot: Slot,
    evidence: Value,
}

fn curve_mark(
    rpc: &RpcClient,
    budget: &mut Budget,
    asset: Asset,
    group: &Group,
    minimum: Slot,
) -> Result<Mark, String> {
    let mint = asset.mint().ok_or("portfolio mint absent")?;
    let market = curve_market::read(rpc, budget, mint)?;
    if market.slot < minimum
        || market.token.decimals != group.decimals.get()
        || market.token.program.id() != group.program
    {
        return Err("portfolio market context, program or units differ".into());
    }
    let evidence = crate::curve_exit::estimate(&market, group.raw)?;
    Ok(Mark {
        lamports: integer(&evidence["net_lamports_at_observed_state"])?,
        slot: market.slot,
        evidence,
    })
}

fn known(value: u64, slot: Slot) -> Valuation {
    Valuation::Known {
        value: MicroUsd(value),
        as_of: slot,
    }
}

fn band(
    lamports: u64,
    slot: Slot,
    price: &sol_price::Observation,
) -> Result<(Valuation, Valuation), String> {
    let (low, high) = crate::wallet_read::native_band(
        lamports,
        price.lower_micro_usd_per_sol,
        price.upper_micro_usd_per_sol,
    )?;
    // The price was posted before its bank read. Never redate it to that bank.
    let oldest = slot.min(price.posted_slot);
    Ok((known(low, oldest), known(high, oldest)))
}

type Marks = BTreeMap<Asset, (Valuation, Value)>;

fn total(portfolio: &Portfolio, marks: &Marks) -> Result<Option<Value>, String> {
    let mut lower = 0_u64;
    let mut upper = 0_u64;
    let mut oldest = Slot(u64::MAX);
    let mut complete = true;
    for (asset, holding) in portfolio.holdings() {
        let (high, _) = marks
            .get(&asset)
            .ok_or("portfolio upper value unavailable")?;
        match (holding.value(), *high) {
            (
                Valuation::Known { value: low, as_of },
                Valuation::Known {
                    value: high,
                    as_of: high_slot,
                },
            ) => {
                lower = lower
                    .checked_add(low.get())
                    .ok_or("portfolio lower total overflow")?;
                upper = upper
                    .checked_add(high.get())
                    .ok_or("portfolio upper total overflow")?;
                oldest = oldest.min(as_of).min(high_slot);
            }
            _ => complete = false,
        }
    }
    Ok(complete
        .then(|| json!({"lower_micro_usd":lower,"upper_micro_usd":upper,"oldest_slot":oldest})))
}

fn hold(
    portfolio: &mut Portfolio,
    marks: &mut Marks,
    asset: Asset,
    role: AssetRole,
    quantity: TokenQuantity,
    values: (Valuation, Valuation),
    mark: Value,
) -> Result<(), String> {
    let holding = Holding::new(
        role,
        Balance::Counted(quantity),
        values.0,
        Valuation::Unknown(Unvaluable::NoPrice),
    );
    portfolio
        .hold(asset, holding)
        .map_err(|_| "portfolio attribution unavailable")?;
    marks.insert(asset, (values.1, mark));
    Ok(())
}

type Band = (Valuation, Valuation);

fn mark_values(
    asset: Asset,
    group: &Group,
    token_slot: Slot,
    price: &sol_price::Observation,
    mark: Option<Result<Mark, String>>,
) -> Result<(Band, Value), String> {
    let unknown = (
        Valuation::Unknown(Unvaluable::NoPrice),
        Valuation::Unknown(Unvaluable::NoPrice),
    );
    if group.raw == 0 {
        Ok((
            (known(0, token_slot), known(0, token_slot)),
            json!({"source":"measured_zero_quantity"}),
        ))
    } else if !group.eligible {
        Ok((
            unknown,
            json!({"refusal":"account_state_authority_or_delegate_not_admitted"}),
        ))
    } else if asset == Asset::WrappedSol {
        Ok((
            band(group.raw, token_slot, price)?,
            json!({"source":"verified_classic_wrapped_sol"}),
        ))
    } else {
        match mark {
            Some(Ok(mark)) => Ok((
                band(mark.lamports, token_slot.min(mark.slot), price)?,
                mark.evidence,
            )),
            refused => Ok((
                unknown,
                json!({"refusal":refused.and_then(Result::err)
                .unwrap_or_else(|| "no independent USD mark for this asset".into())}),
            )),
        }
    }
}

fn assemble(
    wallet: Address,
    native: radar_onchain::rpc::Balance,
    groups: BTreeMap<Asset, Group>,
    mut curves: BTreeMap<Asset, Result<Mark, String>>,
    token_slot: Slot,
    price: &sol_price::Observation,
) -> Result<(Portfolio, Marks), String> {
    let mut portfolio = Portfolio::at(wallet, price.context_slot);
    let mut marks = Marks::new();
    hold(
        &mut portfolio,
        &mut marks,
        Asset::Sol,
        AssetRole::Cash,
        TokenQuantity::lamports(native.lamports),
        band(native.lamports, native.slot, price)?,
        json!({"source":"native_balance_and_sol_oracle"}),
    )?;
    for (asset, group) in groups {
        let (values, mark) = mark_values(asset, &group, token_slot, price, curves.remove(&asset))?;
        let role = if matches!(asset, Asset::WrappedSol | Asset::Usdc) {
            AssetRole::Cash
        } else {
            AssetRole::Position
        };
        hold(
            &mut portfolio,
            &mut marks,
            asset,
            role,
            TokenQuantity::new(group.raw, group.decimals),
            values,
            mark,
        )?;
    }
    Ok((portfolio, marks))
}

fn read(rpc: &RpcClient, wallet: Address, bounds: &sol_price::Bounds) -> Result<Value, String> {
    let started = crate::wallet_read::now()?;
    // The raw inventory reader caps distinct addresses at 100; each asset has
    // at most one curve attempt. Calls share a single deadline and allowance.
    let mut budget = Budget::new(105, 0, Duration::from_secs(20));
    let (mut evidence, native) = crate::wallet_read::inventory(rpc, wallet, &mut budget)?;
    evidence["read_started_at_unix_secs"] = json!(started);
    evidence["read_completed_at_unix_secs"] = json!(crate::wallet_read::now()?);
    let mut latest = native.slot;
    for section in ["native_sol", "token_program", "token_2022"] {
        let slot = Slot(integer(&evidence[section]["slot"])?);
        if slot < bounds.minimum_slot {
            return Err("portfolio inventory precedes context floor".into());
        }
        latest = latest.max(slot);
    }
    if !evidence["raw_token_verification"]["slot"].is_null() {
        latest = latest.max(Slot(integer(&evidence["raw_token_verification"]["slot"])?));
    }
    let token_slot = latest;
    let groups = groups(&evidence)?;
    let mut marks = BTreeMap::new();
    for (&asset, group) in &groups {
        if group.raw > 0 && group.eligible && !matches!(asset, Asset::WrappedSol | Asset::Usdc) {
            let mark = curve_mark(rpc, &mut budget, asset, group, token_slot);
            if let Ok(mark) = &mark {
                latest = latest.max(mark.slot);
            }
            marks.insert(asset, mark);
        }
    }
    let price = sol_price::read(rpc, &mut budget, bounds)?;
    if price.context_slot < latest {
        return Err("portfolio oracle bank precedes holdings or marks".into());
    }
    let (portfolio, marks) = assemble(wallet, native, groups, marks, token_slot, &price)?;
    let observed_total = total(&portfolio, &marks)?;
    let rows = portfolio
        .holdings()
        .map(|(asset, holding)| {
            let (high, mark) = marks
                .get(&asset)
                .ok_or("portfolio upper value unavailable")?;
            Ok(json!({"asset":asset,"holding":holding,"upper_value":high,"mark":mark}))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let completed = crate::wallet_read::now()?;
    crate::wallet_read::check_window(started, completed, bounds.maximum_age_secs)?;
    Ok(
        json!({"version":1,"authority":"read_only","wallet":wallet,"commitment":"finalized",
        "read_started_at_unix_secs":started,"read_completed_at_unix_secs":completed,
        "wallet_evidence":evidence,"sol_price":{"account":sol_price::ACCOUNT,"source":"pyth_receiver_full","observation":price},
        "holdings":rows,"observed_holdings_usd_band":observed_total,
        "valuation_basis":"oracle_confidence_and_hypothetical_exit_before_network_costs",
        "inventory_atomic":false,"wallet_inventory_complete":false,"economic_reconciliation_complete":false,
        "equity_micro_usd":null,"usable_capital_micro_usd":null,"realised_pnl":null,"execution_guaranteed":false}),
    )
}

pub fn run(args: &[String]) -> Result<(), String> {
    let wallet = crate::flag(args, "--wallet")
        .and_then(|v| v.parse().ok())
        .ok_or("portfolio-read needs --wallet <address>")?;
    let endpoint = crate::flag(args, "--rpc")
        .filter(|v| !v.trim().is_empty())
        .ok_or("portfolio-read needs an explicit --rpc <URL>")?;
    let bounds = sol_price::Bounds {
        minimum_slot: Slot(
            crate::flag(args, "--min-slot")
                .and_then(|v| v.parse().ok())
                .ok_or("portfolio-read needs --min-slot <u64>")?,
        ),
        maximum_age_secs: crate::flag(args, "--max-age-secs")
            .and_then(|v| v.parse().ok())
            .ok_or("portfolio-read needs --max-age-secs <u64>")?,
        maximum_confidence_bps: crate::flag(args, "--max-confidence-bps")
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| *v <= 10_000)
            .ok_or("portfolio-read needs --max-confidence-bps <0..10000>")?,
    };
    println!("{}", read(&RpcClient::new(endpoint), wallet, &bounds)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_pumpfun::token::{SPL_TOKEN_PROGRAM, TOKEN_2022_PROGRAM};

    fn price() -> sol_price::Observation {
        sol_price::Observation {
            context_slot: Slot(100),
            posted_slot: Slot(80),
            published_at_unix_secs: 1,
            lower_micro_usd_per_sol: 2_000_000,
            upper_micro_usd_per_sol: 3_000_000,
        }
    }

    fn raw() -> Value {
        json!({"mint":Address::new([9;32]),"program":SPL_TOKEN_PROGRAM,"decimals":6,
            "raw_amount":"7","state":"initialized","mint_authority_active":false,
            "freeze_authority":null,"delegate":null})
    }

    fn inventory(rows: &[Value]) -> Value {
        json!({"raw_token_verification":{"accounts":rows}})
    }

    #[test]
    fn grouping_keeps_programs_and_units_distinct_and_refuses_quantity_overflow() {
        let original = raw();
        let mut extended = original.clone();
        extended["program"] = json!(TOKEN_2022_PROGRAM);
        let grouped = groups(&inventory(&[original.clone(), original.clone(), extended])).unwrap();
        let classic = Asset::spl(Address::new([9; 32]));
        assert_eq!(grouped[&classic].raw, 14);
        assert_eq!(grouped[&classic].decimals.get(), 6);
        assert!(grouped[&classic].eligible);
        assert_eq!(grouped[&Asset::token_2022(Address::new([9; 32]))].raw, 7);
        for (field, value) in [
            ("program", json!(Address::new([8; 32]))),
            ("decimals", json!(7)),
            ("decimals", json!(255)),
            ("raw_amount", json!("bad")),
            ("raw_amount", json!(u64::MAX.to_string())),
        ] {
            let mut bad = original.clone();
            bad[field] = value;
            assert!(groups(&inventory(&[original.clone(), bad])).is_err());
        }
        for (field, value) in [
            ("state", json!("frozen")),
            ("state", json!("uninitialized")),
            ("mint_authority_active", json!(true)),
            ("freeze_authority", json!(Address::new([8; 32]))),
            ("delegate", json!({"raw_allowance":"1"})),
        ] {
            let mut bad = original.clone();
            bad[field] = value;
            assert!(!groups(&inventory(&[bad, original.clone()])).unwrap()[&classic].eligible);
        }
    }

    #[test]
    fn marks_preserve_posting_dates_wrapped_units_and_unknown_stablecoin_prices() {
        assert_eq!(
            band(1_000_000_000, Slot(90), &price()).unwrap(),
            (known(2_000_000, Slot(80)), known(3_000_000, Slot(80)))
        );
        assert_eq!(
            band(1, Slot(70), &price()).unwrap(),
            (known(0, Slot(70)), known(1, Slot(70)))
        );
        let mut group = Group {
            raw: 1_000_000_000,
            decimals: Decimals::NATIVE_SOL,
            program: SPL_TOKEN_PROGRAM,
            eligible: true,
        };
        let (wrapped, _) =
            mark_values(Asset::WrappedSol, &group, Slot(90), &price(), None).unwrap();
        assert_eq!(wrapped.0, known(2_000_000, Slot(80)));
        assert!(
            mark_values(Asset::Usdc, &group, Slot(90), &price(), None)
                .unwrap()
                .0
                .0
                .micro_usd()
                .is_none()
        );
        group.eligible = false;
        assert!(
            mark_values(Asset::WrappedSol, &group, Slot(90), &price(), None)
                .unwrap()
                .0
                .0
                .micro_usd()
                .is_none()
        );
        group.raw = 0;
        assert_eq!(
            mark_values(Asset::Usdc, &group, Slot(90), &price(), None)
                .unwrap()
                .0,
            (known(0, Slot(90)), known(0, Slot(90)))
        );
    }

    #[test]
    fn totals_read_the_portfolio_and_unknowns_or_overflow_never_become_partial_sums() {
        let wallet = Address::new([7; 32]);
        let mut portfolio = Portfolio::at(wallet, Slot(100));
        let mut marks = Marks::new();
        hold(
            &mut portfolio,
            &mut marks,
            Asset::Sol,
            AssetRole::Cash,
            TokenQuantity::lamports(1),
            (known(2, Slot(90)), known(3, Slot(80))),
            Value::Null,
        )
        .unwrap();
        hold(
            &mut portfolio,
            &mut marks,
            Asset::WrappedSol,
            AssetRole::Cash,
            TokenQuantity::lamports(1),
            (known(5, Slot(70)), known(7, Slot(75))),
            Value::Null,
        )
        .unwrap();
        assert_eq!(
            total(&portfolio, &marks).unwrap(),
            Some(json!({"lower_micro_usd":7,"upper_micro_usd":10,"oldest_slot":70}))
        );
        for values in [
            (Valuation::Unknown(Unvaluable::NoPrice), known(0, Slot(80))),
            (known(0, Slot(80)), Valuation::Unknown(Unvaluable::NoPrice)),
        ] {
            hold(
                &mut portfolio,
                &mut marks,
                Asset::WrappedSol,
                AssetRole::Cash,
                TokenQuantity::lamports(1),
                values,
                Value::Null,
            )
            .unwrap();
            assert_eq!(total(&portfolio, &marks).unwrap(), None);
        }
        for (low, high) in [(u64::MAX, u64::MAX), (0, u64::MAX)] {
            hold(
                &mut portfolio,
                &mut marks,
                Asset::WrappedSol,
                AssetRole::Cash,
                TokenQuantity::lamports(1),
                (known(low, Slot(80)), known(high, Slot(80))),
                Value::Null,
            )
            .unwrap();
            assert!(total(&portfolio, &marks).is_err());
        }
    }

    #[test]
    fn portfolio_scope_refuses_before_any_network_read() {
        let wallet = Address::new([7; 32]).to_string();
        let args = [
            "--wallet",
            &wallet,
            "--rpc",
            "http://127.0.0.1:1",
            "--min-slot",
            "50",
            "--max-age-secs",
            "60",
            "--max-confidence-bps",
            "100",
        ];
        for length in [0, 2, 4, 6, 8] {
            let args: Vec<_> = args[..length].iter().map(|v| (*v).to_owned()).collect();
            assert!(run(&args).unwrap_err().contains("portfolio-read needs"));
        }
        for (index, bad) in [
            (1, "invalid"),
            (3, " "),
            (5, "bad"),
            (7, "bad"),
            (9, "10001"),
        ] {
            let mut damaged = args;
            damaged[index] = bad;
            let args: Vec<_> = damaged.iter().map(|v| (*v).to_owned()).collect();
            assert!(run(&args).unwrap_err().contains("portfolio-read needs"));
        }
    }
}
