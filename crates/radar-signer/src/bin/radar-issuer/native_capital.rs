// SPDX-License-Identifier: Apache-2.0
//! Counted native cash after durable claims and the operating reserve.
//! Reads protected wallet evidence and owned journal claims; never grants authority.

use radar_journal::{OperationLog, OperationState};
use radar_types::{
    Asset, AssetRole, Balance, Decimals, Holding, Portfolio, Slot, TokenQuantity, Unvaluable,
    Valuation,
};
use serde_json::{Value, json};

use super::{Config, Snapshot, evidence_integer, expired, wallet_evidence_expiry};

fn counted(
    snapshot: &Snapshot,
    config: &Config,
    operations: &OperationLog,
) -> Result<Value, String> {
    let native_slot = Slot(evidence_integer(
        &snapshot.wallet_evidence["native_sol"],
        "slot",
    )?);
    // A released claim is already reflected only in balances at/after its
    // retained settlement. Never combine an older balance with a newer release.
    for (id, entry) in operations.entries() {
        if matches!(
            entry.state,
            OperationState::Confirmed(_) | OperationState::Reconciled(_)
        ) {
            let record = operations
                .settlement(id)
                .ok_or("released capital lacks retained settlement")?;
            if evidence_integer(&record.review, "slot")? > native_slot.get() {
                return Err("native capital observation precedes released settlement".into());
            }
        }
    }
    for transfer in operations.native_transfers() {
        if evidence_integer(&transfer.review, "execution_slot")? > native_slot.get() {
            return Err("native capital observation precedes retained transfer".into());
        }
    }
    if let Some(opening) = operations.opening_inventory()
        && (opening.wallet != config.wallet || opening.native_slot > native_slot)
    {
        return Err("native capital observation does not cover opening inventory".into());
    }
    let mut portfolio = Portfolio::at(config.wallet, native_slot);
    let unknown = Valuation::Unknown(Unvaluable::QuoteUnpriced);
    portfolio
        .hold(
            Asset::Sol,
            Holding::new(
                AssetRole::Cash,
                Balance::Counted(TokenQuantity::lamports(snapshot.sol_lamports)),
                unknown,
                unknown,
            ),
        )
        .map_err(|_| "native capital balance unusable")?;
    let mut claims = Vec::new();
    for (id, entry) in operations.outstanding() {
        let amount = entry.reserved.ok_or("outstanding capital claim absent")?;
        if entry.intent.asset != Asset::Sol || entry.intent.at > native_slot {
            return Err("native capital claim asset or observation context unsupported".into());
        }
        portfolio
            .reserve(Asset::Sol, amount, entry.intent.at)
            .map_err(|_| "native capital cannot cover outstanding claims")?;
        claims.push(json!({"operation":id.as_str(),"state":entry.state,"quantity":amount}));
    }
    let reserved = portfolio
        .reserved(Asset::Sol, Decimals::NATIVE_SOL)
        .map_err(|_| "native capital claim total unavailable")?;
    let free = portfolio
        .free(Asset::Sol)
        .map_err(|_| "native capital free quantity unavailable")?;
    // The floor is held in addition to pending operation claims (which already
    // include their own fees). A measured shortfall leaves zero above the floor.
    let after_floor = free.raw().saturating_sub(config.fee_reserve_lamports);
    Ok(json!({"status":"counted","slot":native_slot,
        "observed_lamports":snapshot.sol_lamports.to_string(),
        "outstanding_claim_lamports":reserved.raw().to_string(),
        "free_after_claims_lamports":free.raw().to_string(),
        "operating_reserve_lamports":config.fee_reserve_lamports.to_string(),
        "above_operating_reserve_lamports":after_floor.to_string(),
        "outstanding_claims":claims}))
}

pub(super) fn review(
    snapshot: &Snapshot,
    config: &Config,
    operations: &OperationLog,
    now: u64,
) -> Result<Value, String> {
    if snapshot.wallet != config.wallet || snapshot.accounting_checkpoint != operations.checkpoint()
    {
        return Err("native capital snapshot does not cover configured wallet and journal".into());
    }
    let expires = wallet_evidence_expiry(snapshot, config, now)?;
    if expired(now, expires) {
        return Err("native capital evidence has no remaining validity".into());
    }
    let mut report = match counted(snapshot, config, operations) {
        Ok(report) => report,
        Err(refusal) => json!({"status":"unknown","refusal":refusal,
            "above_operating_reserve_lamports":null}),
    };
    report["accounting_checkpoint"] = json!(operations.checkpoint());
    report["authority"] = json!("read_only");
    report["wallet"] = json!(config.wallet);
    report["read_expires_at_unix_secs"] = json!(expires);
    report["usable_capital_micro_usd"] = Value::Null;
    report["equity_micro_usd"] = Value::Null;
    report["portfolio_state_updated"] = json!(false);
    report["wallet_inventory_complete"] = json!(false);
    report["economic_reconciliation_complete"] = json!(false);
    report["reservation_released"] = json!(false);
    Ok(report)
}
