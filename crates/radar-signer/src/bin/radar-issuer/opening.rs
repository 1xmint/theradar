// SPDX-License-Identifier: Apache-2.0
//! Genesis quantities before recorded trades; not complete economic coverage.

use std::collections::BTreeSet;

use radar_journal::{OpeningInventoryRecord, OpeningTokenHolding};
use radar_types::Slot;
use serde_json::{Value, json};

use super::{Config, Snapshot, evidence_integer, inventory};

pub(super) fn capture(
    snapshot: &Snapshot,
    config: &Config,
    now: u64,
) -> Result<OpeningInventoryRecord, String> {
    let history = json!({"accounting_checkpoint":"","lots":[]});
    // Reuse exactly the existing protected read checks. A genesis snapshot
    // cannot borrow the checkpoint of previous trades or invent opening lots.
    let reviewed = inventory::review(snapshot, config, &history, now, None)?;
    let holdings = reviewed["tokens_by_mint"]
        .as_array()
        .ok_or("opening holdings missing")?
        .iter()
        .map(|row| {
            Ok(OpeningTokenHolding {
                mint: serde_json::from_value(row["mint"].clone())
                    .map_err(|_| "invalid opening mint")?,
                token_program: serde_json::from_value(row["token_program"].clone())
                    .map_err(|_| "invalid opening program")?,
                decimals: row["decimals"]
                    .as_u64()
                    .and_then(|n| u8::try_from(n).ok())
                    .ok_or("invalid opening units")?,
                raw_amount: evidence_integer(row, "observed_raw")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let value = &snapshot.wallet_evidence;
    Ok(OpeningInventoryRecord {
        wallet: config.wallet,
        native_lamports: snapshot.sol_lamports,
        native_slot: Slot(evidence_integer(&value["native_sol"], "slot")?),
        token_program_slot: Slot(evidence_integer(
            value.get("token_program").ok_or("missing token read")?,
            "slot",
        )?),
        token_2022_slot: Slot(evidence_integer(
            value.get("token_2022").ok_or("missing Token-2022 read")?,
            "slot",
        )?),
        raw_token_slot: if reviewed["raw_token_slot"].is_null() {
            None
        } else {
            Some(Slot(evidence_integer(&reviewed, "raw_token_slot")?))
        },
        read_started_at_unix_secs: reviewed["read_started_at_unix_secs"]
            .as_u64()
            .ok_or("opening start missing")?,
        read_completed_at_unix_secs: reviewed["read_completed_at_unix_secs"]
            .as_u64()
            .ok_or("opening completion missing")?,
        holdings,
    })
}

pub(super) fn lots(
    opening: &OpeningInventoryRecord,
    wallet: radar_types::Address,
    current: &Value,
    acquisitions: &[Value],
) -> Result<Vec<Value>, String> {
    let floor = opening
        .native_slot
        .max(opening.token_program_slot)
        .max(opening.token_2022_slot);
    if opening.wallet != wallet
        || opening.read_completed_at_unix_secs < opening.read_started_at_unix_secs
        || current["read_started_at_unix_secs"]
            .as_u64()
            .ok_or("current start missing")?
            < opening.read_completed_at_unix_secs
        || evidence_integer(&current["native_sol"], "slot")? < opening.native_slot.get()
        || opening.raw_token_slot.is_none() != opening.holdings.is_empty()
        || opening.raw_token_slot.is_some_and(|slot| slot < floor)
    {
        return Err("opening inventory identity or read bounds are inconsistent".into());
    }
    let floor = floor.max(opening.raw_token_slot.unwrap_or(floor));
    for read in ["token_program", "token_2022"] {
        if evidence_integer(&current[read], "slot")? < floor.get() {
            return Err("current token observation precedes opening inventory".into());
        }
    }
    for lot in acquisitions {
        if evidence_integer(lot, "execution_slot")? <= floor.get() {
            return Err("retained acquisition does not follow opening inventory".into());
        }
    }
    let mut seen = BTreeSet::new();
    opening.holdings.iter().map(|holding| {
        if !seen.insert(holding.mint) { return Err("duplicate opening mint".into()); }
        Ok(json!({"mint":holding.mint,"token_program":holding.token_program,"decimals":holding.decimals,
            "net_acquired_raw":holding.raw_amount.to_string(),"execution_slot":"0"}))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_types::Address;

    fn opening() -> OpeningInventoryRecord {
        OpeningInventoryRecord {
            wallet: Address::new([4; 32]),
            native_lamports: 100,
            native_slot: Slot(40),
            token_program_slot: Slot(41),
            token_2022_slot: Slot(42),
            raw_token_slot: Some(Slot(43)),
            read_started_at_unix_secs: 1,
            read_completed_at_unix_secs: 2,
            holdings: vec![OpeningTokenHolding {
                mint: Address::new([2; 32]),
                token_program: Address::new([3; 32]),
                decimals: 6,
                raw_amount: 10,
            }],
        }
    }
    fn current() -> Value {
        json!({"native_sol":{"slot":"44"},"token_program":{"slot":"45"},"token_2022":{"slot":"46"},"read_started_at_unix_secs":2})
    }

    #[test]
    fn opening_lots_preserve_units_and_refuse_older_or_inconsistent_read_bounds() {
        let record = opening();
        let rows = lots(
            &record,
            record.wallet,
            &current(),
            &[json!({"execution_slot":"44"})],
        )
        .expect("after opening");
        assert_eq!(rows[0]["net_acquired_raw"], "10");
        assert_eq!(rows[0]["mint"], json!(record.holdings[0].mint));
        assert_eq!(
            rows[0]["token_program"],
            json!(record.holdings[0].token_program)
        );
        assert_eq!(rows[0]["decimals"], 6);
        for slot in ["42", "43"] {
            assert!(
                lots(
                    &record,
                    record.wallet,
                    &current(),
                    &[json!({"execution_slot":slot})]
                )
                .is_err()
            );
        }
        for case in [
            "wallet",
            "window",
            "raw_absent",
            "raw_old",
            "duplicate",
            "holdings_absent",
        ] {
            let mut bad = record.clone();
            match case {
                "wallet" => bad.wallet = Address::new([9; 32]),
                "window" => bad.read_started_at_unix_secs = 3,
                "raw_absent" => bad.raw_token_slot = None,
                "raw_old" => bad.raw_token_slot = Some(Slot(41)),
                "duplicate" => bad.holdings.push(bad.holdings[0].clone()),
                "holdings_absent" => bad.holdings.clear(),
                _ => unreachable!(),
            }
            assert!(
                lots(&bad, record.wallet, &current(), &[]).is_err(),
                "{case}"
            );
        }
        for (read, slot) in [
            ("native_sol", "39"),
            ("token_program", "42"),
            ("token_2022", "42"),
        ] {
            let mut bad = current();
            bad[read]["slot"] = json!(slot);
            assert!(lots(&record, record.wallet, &bad, &[]).is_err(), "{read}");
        }
        let mut bad = current();
        bad["read_started_at_unix_secs"] = json!(1);
        assert!(lots(&record, record.wallet, &bad, &[]).is_err());
        let mut empty = record.clone();
        empty.holdings.clear();
        empty.raw_token_slot = None;
        empty.native_slot = Slot(43);
        assert_eq!(
            lots(&empty, empty.wallet, &current(), &[]).expect("empty opening quantities"),
            Vec::<Value>::new()
        );
        let mut bad = current();
        bad["token_program"]["slot"] = json!("42");
        assert!(
            lots(&empty, empty.wallet, &bad, &[]).is_err(),
            "native context also bounds empty opening"
        );
    }
}
