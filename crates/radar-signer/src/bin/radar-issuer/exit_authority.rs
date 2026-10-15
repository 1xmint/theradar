// SPDX-License-Identifier: Apache-2.0
//! Bound reviewed sales to retained inventory, not an amount supplied on stdin.

use radar_journal::OperationLog;
use radar_risk::Action;
use serde_json::Value;

use super::{Config, Snapshot, inventory, reconciliation};

fn sold(bytes: &[u8]) -> Result<u64, String> {
    use radar_decode::pumpfun::Instruction;
    let message = radar_signer::decode(bytes).map_err(|_| "transaction refused")?;
    let mut total = 0_u64;
    for instruction in message.instructions {
        if instruction.program_id != *radar_decode::pumpfun::PROGRAM_ID.as_bytes() {
            continue;
        }
        let known = radar_decode::decode(radar_decode::Program::PumpFun, &instruction.data)
            .known()
            .copied()
            .and_then(radar_decode::Instruction::pumpfun);
        if !matches!(known, Some(Instruction::Sell | Instruction::SellV2)) {
            continue;
        }
        let raw = known
            .and_then(|known| radar_decode::pumpfun::trade_args(known, &instruction.data))
            .and_then(Result::ok)
            .and_then(|trade| trade.exact.tokens())
            .ok_or("unreadable sale quantity")?;
        total = total.checked_add(raw).ok_or("sale quantity overflow")?;
    }
    if total == 0 {
        return Err("exit requires a readable positive curve sale".into());
    }
    Ok(total)
}

pub(super) fn review(
    bytes: &[u8],
    snapshot: &Snapshot,
    config: &Config,
    history: &Value,
    log: &OperationLog,
    now: u64,
) -> Result<Option<u64>, String> {
    if snapshot.proposal.action == Action::Buy {
        return Ok(None);
    }
    let amount = sold(bytes)?;
    reconciliation::verify_inventory(log, config, snapshot, history, now)?;
    reconciliation::verify_token_anchors(log, config, history)?;
    let accounts =
        inventory::capture_accounts(&snapshot.wallet_evidence, snapshot.state.now.get())?;
    let mut held = 0_u64;
    for account in accounts {
        if account.holding.mint != snapshot.proposal.mint {
            continue;
        }
        // Initialized alone does not prove unrestricted spendability. The
        // existing exact-byte simulation and signer checks still apply; token
        // extensions, ATAs and live ownership remain separate activation work.
        if account.state != "initialized" {
            return Err("sale holding has unsupported account state".into());
        }
        held = held
            .checked_add(account.holding.raw_amount)
            .ok_or("sale holding overflow")?;
    }
    if amount > held || (snapshot.proposal.action == Action::Exit && amount != held) {
        return Err("sale quantity does not fit reviewed holdings and action".into());
    }
    Ok(Some(amount))
}
