// SPDX-License-Identifier: Apache-2.0
//! Completion from already reverified retained valuation, never a new authority.

use radar_types::{Settlement, TokenQuantity};
use serde_json::Value;

use super::evidence_integer as integer;

pub(super) fn reviewed(value: &Value) -> Result<Settlement, String> {
    let sale = &value["sale_proceeds"];
    if !sale.is_object() {
        return Ok(Settlement::Completed(TokenQuantity::lamports(integer(
            value,
            "wallet_net_debit_lamports",
        )?)));
    }
    let spent = integer(sale, "network_fee_lamports")?
        .checked_add(integer(sale, "tip_lamports")?)
        .and_then(|cost| cost.checked_add(integer(sale, "rent_paid_lamports").ok()?))
        .ok_or("sale outgoing cash exceeds native range")?;
    let received = integer(sale, "gross_proceeds_lamports")?
        .checked_add(integer(sale, "rent_refund_lamports")?)
        .ok_or("sale incoming cash exceeds native range")?;
    Ok(Settlement::CompletedCashFlow {
        spent: TokenQuantity::lamports(spent),
        received: TokenQuantity::lamports(received),
    })
}

pub(super) fn projected(completion: Settlement, pre: u64) -> Option<u64> {
    match completion {
        Settlement::Completed(spent) => pre.checked_sub(spent.raw()),
        Settlement::CompletedCashFlow { spent, received } => {
            pre.checked_sub(spent.raw())?.checked_add(received.raw())
        }
        _ => None,
    }
}
