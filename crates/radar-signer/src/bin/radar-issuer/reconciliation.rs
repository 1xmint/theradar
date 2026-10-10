// SPDX-License-Identifier: Apache-2.0
//! Durable completion of supported operations; retained economic facts replay after restart.

use radar_journal::{Applied, OperationLog, OperationState};
use radar_types::{Asset, AssetRole, Balance, Holding, Portfolio, TokenQuantity, Valuation};
use serde_json::{Value, json};

use super::{Config, Snapshot, acquisitions, evidence_integer as integer, inventory};

fn verify_token_anchors(
    log: &OperationLog,
    config: &Config,
    history: &Value,
) -> Result<(), String> {
    let sales = history["sales"]
        .as_array()
        .ok_or("retained sales missing")?;
    let mut lots = history["lots"]
        .as_array()
        .ok_or("retained lots missing")?
        .iter()
        .map(|lot| (lot, false))
        .chain(sales.iter().map(|sale| (sale, true)))
        .map(|(row, sale)| Ok((integer(row, "execution_slot")?, row, sale)))
        .collect::<Result<Vec<_>, String>>()?;
    lots.sort_by_key(|(slot, _, _)| *slot);
    let mut quantities = std::collections::BTreeMap::<radar_types::Address, u64>::new();
    for (_, lot, sale) in lots {
        let units = if sale { &lot["proceeds"] } else { lot };
        let mint = serde_json::from_value(units["mint"].clone()).map_err(|_| "lot mint missing")?;
        let id = log
            .entries()
            .find(|(id, _)| lot["operation"] == id.as_str())
            .map(|(id, _)| id)
            .ok_or("lot operation missing")?;
        let facts = &log
            .valuation(id)
            .ok_or("lot valuation missing")?
            .settlement
            .review;
        let amount = |field: &str| -> Result<u64, String> {
            facts[field]
                .as_array()
                .ok_or("token anchors missing")?
                .iter()
                .filter(|row| {
                    row["owner"] == config.wallet.to_string() && row["mint"] == units["mint"]
                })
                .try_fold(0_u64, |sum, row| {
                    sum.checked_add(integer(row, "raw_amount")?)
                        .ok_or_else(|| "token anchor quantity overflow".into())
                })
        };
        let pre = quantities.entry(mint).or_default();
        let post = if sale {
            pre.checked_sub(integer(units, "net_disposed_raw")?)
        } else {
            pre.checked_add(integer(units, "net_acquired_raw")?)
        }
        .ok_or("token history exceeds retained quantities")?;
        // Valuation already verifies the exact token delta. Bind its starting
        // point to retained inventory; the verified delta then anchors the end.
        // Rechecking the end here would duplicate that independent guarantee.
        if amount("pre_token_balances")? != *pre {
            return Err("token transaction anchors do not match retained quantities".into());
        }
        *pre = post;
    }
    Ok(())
}

pub(super) fn apply(
    log: &mut OperationLog,
    config: &Config,
    snapshot: &Snapshot,
    operation: &str,
    now: u64,
) -> Result<Value, String> {
    let (id, entry) = log
        .entries()
        .find(|(id, _)| id.as_str() == operation)
        .map(|(id, entry)| (id.clone(), *entry))
        .ok_or("unknown operation")?;
    // Reverify the exact signed binding, normalized valuation, duplicate identity,
    // cost basis and fee facts before touching any claim, including on retry.
    let history = acquisitions::review(log, config)?;
    let value = &log
        .valuation(&id)
        .ok_or("operation has no retained valuation")?
        .review;
    let completion = super::cash_completion::reviewed(value)?;
    let repeated = entry.state == OperationState::Reconciled(completion);
    if !repeated {
        if entry.state != OperationState::SubmissionUnknown || log.outstanding().count() != 1 {
            return Err("reconciliation requires one unknown submitted operation".into());
        }
        verify_inventory(log, config, snapshot, &history, now)?;
        verify_token_anchors(log, config, &history)?;
        let flow = history["recorded_native_cash_flows"]
            .as_array()
            .ok_or("native cash flows missing")?
            .iter()
            .find(|row| row["operation"] == operation)
            .ok_or("operation cash flow missing")?;
        let pre = integer(flow, "pre_lamports")?;
        let post = integer(flow, "post_lamports")?;
        // Rehold against the verified pre-execution balance. Reholding against
        // today's already-debited cash would subtract the same trade twice.
        let mut portfolio = Portfolio::at(config.wallet, entry.intent.at);
        portfolio
            .hold(
                Asset::Sol,
                Holding::new(
                    AssetRole::Cash,
                    Balance::Counted(TokenQuantity::lamports(pre)),
                    Valuation::Unknown(radar_types::Unvaluable::QuoteUnpriced),
                    Valuation::Unknown(radar_types::Unvaluable::QuoteUnpriced),
                ),
            )
            .map_err(|_| "pre-execution cash unusable")?;
        if super::cash_completion::projected(completion, pre) != Some(post) {
            return Err("operation debit does not match verified cash effect".into());
        }
        log.rehold(&mut portfolio)
            .map_err(|_| "operation reservation cannot be reheld")?;
        // OperationLog validates on a copy and persists completion before it
        // changes the portfolio or releases the claim. Retained valuation/basis
        // remain in the journal, and acquisitions::review replays terminal buys.
        if log
            .reconcile(&id, completion, &mut portfolio, now)
            .map_err(|_| "operation completion could not be persisted")?
            != Applied::Advanced
        {
            return Err("operation completion did not advance".into());
        }
    }
    Ok(
        json!({"operation":operation,"outcome":if repeated {"already_reconciled"} else {"reconciled"},
        "accounting_checkpoint":log.checkpoint(),"operation_reconciled":true,
        "reservation_released":true,"recorded_economic_effects_retained":true,
        "wallet_inventory_complete":false,"live_snapshot_adapter":false}),
    )
}

fn verify_inventory(
    log: &OperationLog,
    config: &Config,
    snapshot: &Snapshot,
    history: &Value,
    now: u64,
) -> Result<(), String> {
    let opening = log
        .opening_inventory()
        .ok_or("reconciliation requires opening inventory")?;
    if opening
        .holdings
        .iter()
        .any(|holding| holding.raw_amount != 0)
    {
        return Err("opening inventory cost basis unknown".into());
    }
    let retained = log.native_transfers().cloned().collect::<Vec<_>>();
    let compared = inventory::review(snapshot, config, history, now, Some(opening), &retained)?;
    if compared["recorded_native_cash_comparison"]["balance_matches"] != true
        || compared["recorded_native_cash_comparison"]["transaction_anchors_match"] != true
        || !compared["tokens_by_mint"]
            .as_array()
            .ok_or("token comparison missing")?
            .iter()
            .all(|row| row["quantity_matches"] == true)
    {
        return Err("wallet balances do not match retained economic effects".into());
    }
    if !compared["reviewed_external_native_transfers"]
        .as_array()
        .ok_or("native transfer comparison missing")?
        .iter()
        .all(|row| retained.iter().any(|record| record.review == *row))
    {
        return Err("external cash effects must be retained before completion".into());
    }
    Ok(())
}
