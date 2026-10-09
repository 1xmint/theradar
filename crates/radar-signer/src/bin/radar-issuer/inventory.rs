// SPDX-License-Identifier: Apache-2.0
//! Compare protected current observations with retained buys, not a reconciler.

use std::collections::BTreeMap;

use radar_types::Address;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Config, Snapshot, evidence_integer, snapshot_current, wallet_evidence_expiry};

#[derive(Clone, Deserialize, PartialEq, Eq)]
struct Account {
    address: Address,
    mint: Address,
    program: Address,
    decimals: u8,
    state: String,
    raw_amount: String,
}

#[derive(Clone)]
struct Quantity {
    program: Address,
    decimals: u8,
    raw: u64,
}

fn add(
    totals: &mut BTreeMap<Address, Quantity>,
    mint: Address,
    program: Address,
    decimals: u8,
    raw: u64,
) -> Result<(), String> {
    let quantity = totals.entry(mint).or_insert(Quantity {
        program,
        decimals,
        raw: 0,
    });
    if quantity.program != program || quantity.decimals != decimals {
        return Err("inventory mint program or units differ".into());
    }
    quantity.raw = quantity
        .raw
        .checked_add(raw)
        .ok_or("inventory quantity overflow")?;
    Ok(())
}

fn observations(value: &Value, maximum: u64) -> Result<BTreeMap<Address, Quantity>, String> {
    let raw = &value["raw_token_verification"];
    if raw["authority"] != "read_only" || raw["inventory_complete"] != false {
        return Err("raw inventory evidence missing or unsupported".into());
    }
    let array = |value: &Value| {
        value
            .as_array()
            .cloned()
            .ok_or("inventory account array missing")
    };
    let mut listed = BTreeMap::new();
    let mut minimum = evidence_integer(&value["native_sol"], "slot")?;
    for read in ["token_program", "token_2022"] {
        minimum = minimum.max(evidence_integer(&value[read], "slot")?);
        for account in array(&value[read]["accounts"])? {
            let account: Account =
                serde_json::from_value(account).map_err(|_| "invalid listed inventory account")?;
            if !matches!(
                account.state.as_str(),
                "initialized" | "frozen" | "uninitialized"
            ) || listed.insert(account.address, account).is_some()
            {
                return Err("unknown state or duplicate listed inventory account".into());
            }
        }
    }
    let mut verified = BTreeMap::new();
    for account in array(&raw["accounts"])? {
        if account["owner"] != value["wallet"] {
            return Err("raw inventory belongs to another wallet".into());
        }
        let account: Account =
            serde_json::from_value(account).map_err(|_| "invalid raw inventory account")?;
        if verified.insert(account.address, account).is_some() {
            return Err("duplicate raw inventory account".into());
        }
    }
    if listed != verified {
        return Err("raw inventory does not match enumeration".into());
    }
    if listed.is_empty() {
        if !raw["slot"].is_null() {
            return Err("empty enumeration has unexpected raw context".into());
        }
    } else {
        let slot = evidence_integer(raw, "slot")?;
        if slot < minimum || slot > maximum {
            return Err("raw inventory context is outside reviewed bounds".into());
        }
    }
    let mut totals = BTreeMap::new();
    for account in listed.into_values() {
        add(
            &mut totals,
            account.mint,
            account.program,
            account.decimals,
            account
                .raw_amount
                .parse()
                .map_err(|_| "invalid inventory quantity")?,
        )?;
    }
    Ok(totals)
}

fn compare(
    observed: BTreeMap<Address, Quantity>,
    lots: &[Value],
    minimum: u64,
) -> Result<Vec<Value>, String> {
    let mut acquired = BTreeMap::new();
    for lot in lots {
        if evidence_integer(lot, "execution_slot")? > minimum {
            return Err("wallet observation precedes retained acquisition".into());
        }
        let address = |field: &str| {
            serde_json::from_value::<Address>(lot[field].clone())
                .map_err(|_| "invalid acquisition identity")
        };
        let decimals = lot["decimals"]
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .ok_or("invalid acquisition units")?;
        add(
            &mut acquired,
            address("mint")?,
            address("token_program")?,
            decimals,
            evidence_integer(lot, "net_acquired_raw")?,
        )?;
    }
    let mut rows = BTreeMap::new();
    for (mint, quantity) in &acquired {
        rows.insert(*mint, (quantity.clone(), 0));
    }
    for (mint, quantity) in observed {
        let row = rows.entry(mint).or_insert((
            Quantity {
                raw: 0,
                ..quantity.clone()
            },
            0,
        ));
        if row.0.program != quantity.program || row.0.decimals != quantity.decimals {
            return Err("observed inventory differs from acquisition units or program".into());
        }
        row.1 = quantity.raw;
    }
    Ok(rows.into_iter().map(|(mint, (acquired, observed))| json!({
        "mint":mint,"token_program":acquired.program,"decimals":acquired.decimals,
        "retained_acquired_raw":acquired.raw.to_string(),"observed_raw":observed.to_string(),
        "unexplained_excess_raw":observed.saturating_sub(acquired.raw).to_string(),
        "unaccounted_reduction_raw":acquired.raw.saturating_sub(observed).to_string(),
        "quantity_matches":observed == acquired.raw,"spendable":null
    })).collect())
}

pub(super) fn review(
    snapshot: &Snapshot,
    config: &Config,
    history: &Value,
    now: u64,
    opening: Option<&radar_journal::OpeningInventoryRecord>,
) -> Result<Value, String> {
    if snapshot.wallet != config.wallet
        || snapshot.accounting_checkpoint != history["accounting_checkpoint"]
        || !snapshot_current(
            now,
            snapshot.observed_at_unix_secs,
            config.max_snapshot_age_secs,
        )
    {
        return Err("inventory snapshot does not cover configured wallet and journal".into());
    }
    wallet_evidence_expiry(snapshot, config, now)?;
    let value = &snapshot.wallet_evidence;
    let minimum = ["token_program", "token_2022"]
        .into_iter()
        .map(|read| evidence_integer(&value[read], "slot"))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or("inventory contexts missing")?;
    let observed = observations(value, snapshot.state.now.get())?;
    let mut lots = history["lots"]
        .as_array()
        .ok_or("acquisition lots missing")?
        .clone();
    if let Some(opening) = opening {
        lots.extend(super::opening::lots(opening, config.wallet, value, &lots)?);
    }
    let mut rows = compare(observed, &lots, minimum)?;
    for row in &mut rows {
        let baseline = opening
            .and_then(|opening| {
                opening
                    .holdings
                    .iter()
                    .find(|h| row["mint"] == h.mint.to_string())
            })
            .map_or(0, |holding| holding.raw_amount);
        let expected = evidence_integer(row, "retained_acquired_raw")?;
        row["expected_raw"] = json!(expected.to_string());
        row["opening_raw"] = if opening.is_some() {
            json!(baseline.to_string())
        } else {
            Value::Null
        };
        row["retained_acquired_raw"] = json!((expected - baseline).to_string());
    }
    Ok(
        json!({"version":1,"authority":"protected_operator_inventory_comparison",
        "wallet":config.wallet,"accounting_checkpoint":history["accounting_checkpoint"],
        "read_started_at_unix_secs":value["read_started_at_unix_secs"],
        "read_completed_at_unix_secs":value["read_completed_at_unix_secs"],
        "native_sol":{"slot":value["native_sol"]["slot"],
            "raw_amount":value["native_sol"]["raw_amount"],"decimals":9},
        "token_program_slot":value["token_program"]["slot"],
        "token_2022_slot":value["token_2022"]["slot"],
        "raw_token_slot":value["raw_token_verification"]["slot"],
        "acquisition_history":history,"tokens_by_mint":rows,
        "opening_inventory":opening,"opening_cost_basis_micro_usd":null,"wallet_inventory_complete":false,
        "current_exposure_micro_usd":null,"realised_loss_today_micro_usd":null,
        "portfolio_state_updated":false,"economic_reconciliation_complete":false,
        "reservation_released":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(seed: u8, amount: u64) -> Value {
        json!({"address":Address::new([seed;32]),"mint":Address::new([2;32]),
            "program":Address::new([3;32]),"decimals":6,"state":"frozen",
            "raw_amount":amount.to_string(),"owner":Address::new([4;32])})
    }
    fn packet() -> Value {
        let accounts = json!([account(5, 7), account(6, 3)]);
        json!({"wallet":Address::new([4;32]),"native_sol":{"slot":"40"},
            "token_program":{"slot":"41","accounts":accounts},
            "token_2022":{"slot":"42","accounts":[]},
            "raw_token_verification":{"slot":"43","accounts":accounts,
                "authority":"read_only","inventory_complete":false}})
    }
    fn lot(raw: u64) -> Value {
        json!({"mint":Address::new([2;32]),"token_program":Address::new([3;32]),
            "decimals":6,"execution_slot":"41","net_acquired_raw":raw.to_string()})
    }

    #[test]
    fn quantity_comparison_keeps_excess_reductions_and_zero_separate() {
        for acquired in [0, 7, 10, 12] {
            let observed = observations(&packet(), 43).expect("frozen quantities");
            let rows = compare(observed, &[lot(acquired)], 41).expect("comparison");
            assert_eq!(rows[0]["observed_raw"], "10");
            assert_eq!(rows[0]["retained_acquired_raw"], acquired.to_string());
            assert_eq!(
                rows[0]["unexplained_excess_raw"],
                10u64.saturating_sub(acquired).to_string()
            );
            assert_eq!(
                rows[0]["unaccounted_reduction_raw"],
                acquired.saturating_sub(10).to_string()
            );
            assert_eq!(rows[0]["quantity_matches"], acquired == 10);
            assert!(rows[0]["spendable"].is_null());
        }
        let rows = compare(BTreeMap::new(), &[lot(12)], 41).expect("missing holding");
        assert_eq!(rows[0]["observed_raw"], "0");
        assert_eq!(rows[0]["unaccounted_reduction_raw"], "12");
        let rows = compare(observations(&packet(), 43).expect("observed"), &[], 41)
            .expect("unexplained holding");
        assert_eq!(rows[0]["retained_acquired_raw"], "0");
        assert_eq!(rows[0]["unexplained_excess_raw"], "10");
        let mut absent_mint = lot(12);
        absent_mint["mint"] = json!(Address::new([8; 32]));
        let rows = compare(
            observations(&packet(), 43).expect("observed"),
            &[absent_mint],
            41,
        )
        .expect("different mint union");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["mint"], json!(Address::new([2; 32])));
        assert_eq!(rows[0]["unexplained_excess_raw"], "10");
        assert_eq!(rows[1]["mint"], json!(Address::new([8; 32])));
        assert_eq!(rows[1]["unaccounted_reduction_raw"], "12");
    }

    #[test]
    fn inventory_arrays_identity_units_state_and_raw_context_must_agree() {
        for field in [
            "address",
            "mint",
            "program",
            "decimals",
            "state",
            "raw_amount",
            "owner",
        ] {
            let mut bad = packet();
            bad["raw_token_verification"]["accounts"][0][field] = match field {
                "decimals" => json!(9),
                "state" => json!("initialized"),
                "raw_amount" => json!("8"),
                _ => json!(Address::new([9; 32])),
            };
            assert!(observations(&bad, 43).is_err(), "{field}");
        }
        for read in ["token_program", "token_2022", "raw_token_verification"] {
            let mut bad = packet();
            bad[read]["accounts"] = Value::Null;
            assert!(observations(&bad, 43).is_err(), "array {read}");
        }
        for slot in [Value::Null, json!("41"), json!("44")] {
            let mut bad = packet();
            bad["raw_token_verification"]["slot"] = slot;
            assert!(observations(&bad, 43).is_err());
        }
        for field in ["authority", "inventory_complete"] {
            let mut bad = packet();
            bad["raw_token_verification"][field] = Value::Null;
            assert!(observations(&bad, 43).is_err());
        }
        let mut empty = packet();
        for read in ["token_program", "token_2022", "raw_token_verification"] {
            empty[read]["accounts"] = json!([]);
        }
        assert!(observations(&empty, 43).is_err());
        empty["raw_token_verification"]["slot"] = Value::Null;
        assert!(
            observations(&empty, 43)
                .expect("empty observation, no completeness")
                .is_empty()
        );
    }

    #[test]
    fn duplicate_accounts_and_changed_same_mint_metadata_or_overflow_refuse() {
        let mut duplicate = packet();
        duplicate["token_2022"]["accounts"] = json!([account(5, 7)]);
        assert!(observations(&duplicate, 43).is_err());
        let mut duplicate = packet();
        duplicate["raw_token_verification"]["accounts"] =
            json!([account(5, 7), account(6, 3), account(5, 7)]);
        assert!(observations(&duplicate, 43).is_err());
        for field in ["program", "decimals", "raw_amount", "state"] {
            let mut bad = packet();
            let value = match field {
                "program" => json!(Address::new([9; 32])),
                "decimals" => json!(9),
                "state" => json!("unknown"),
                _ => json!(u64::MAX.to_string()),
            };
            for read in ["token_program", "raw_token_verification"] {
                bad[read]["accounts"][0][field] = value.clone();
            }
            assert!(observations(&bad, 43).is_err(), "{field}");
        }
        for field in [
            "token_program",
            "decimals",
            "execution_slot",
            "net_acquired_raw",
        ] {
            let mut second = lot(1);
            second[field] = match field {
                "token_program" => json!(Address::new([9; 32])),
                "decimals" => json!(9),
                "execution_slot" => json!("42"),
                _ => json!(u64::MAX.to_string()),
            };
            assert!(
                compare(BTreeMap::new(), &[lot(1), second], 41).is_err(),
                "{field}"
            );
        }
        for field in ["token_program", "decimals"] {
            let mut changed = lot(10);
            changed[field] = if field == "decimals" {
                json!(9)
            } else {
                json!(Address::new([9; 32]))
            };
            assert!(
                compare(
                    observations(&packet(), 43).expect("observed"),
                    &[changed],
                    41
                )
                .is_err()
            );
        }
        let rows = compare(
            observations(&packet(), 43).expect("observed"),
            &[lot(4), lot(6)],
            41,
        )
        .expect("two lots");
        assert_eq!(rows[0]["retained_acquired_raw"], "10");
    }
}
