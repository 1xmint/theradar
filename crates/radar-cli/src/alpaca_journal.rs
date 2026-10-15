// SPDX-License-Identifier: Apache-2.0
//! Actual operator callers for durable paper claims. No network or order submission.

use std::io::Read as _;
use std::path::Path;

use radar_journal::{Applied, BrokerIntent, BrokerLog, BrokerSide};
use serde_json::{Value, json};

fn intent(path: &str) -> Result<BrokerIntent, String> {
    // Resource bound on an operator intent, not a market or capital limit.
    const MAX_BYTES: u64 = 65_536;
    let file = std::fs::File::open(path).map_err(|_| "paper intent file unavailable")?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "paper intent read failed")?;
    if bytes.len() > usize::try_from(MAX_BYTES).map_err(|_| "paper intent size overflow")? {
        return Err("paper intent exceeds 64 KiB".into());
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| "invalid typed paper intent; do not include credentials".into())
}

pub(super) fn report(log: &BrokerLog, account: &str, changed: bool) -> Result<Value, String> {
    let entries = log.entries().map_err(|e| e.to_string())?;
    let residual = log.claims().map_err(|e| e.to_string())?;
    let mut claims = Vec::new();
    for entry in entries {
        let remaining = residual
            .get(&entry.intent.client_order_id)
            .ok_or("missing retained paper claim")?;
        let cash = entry
            .intent
            .cash_claim_microusd()
            .map_err(|e| e.to_string())?;
        claims.push(json!({"intent":entry.intent,"state":entry.state,
        "cancel_unknown_order_id":entry.cancel_unknown_order_id,
        "cash_claim_microusd":cash.to_string(),
        "observed_remaining_cash_claim_microusd":remaining.cash_microusd.to_string(),
        "observed_remaining_share_claim_nanoshares":remaining.shares_nanoshares.to_string(),
        "retained_execution_progress":remaining.progress,
        "share_claim_nanoshares":match entry.intent.side {
            BrokerSide::Buy => "0".to_owned(),
            BrokerSide::Sell => entry.intent.quantity_nanoshares.to_string(),
        }}));
    }
    Ok(
        json!({"version":1,"venue":"alpaca","environment":"paper","expected_account_id":account,
        "new_reservation_written":changed,"claims":claims,"broker_reconciled":false,
        "retry_authorized":false,"claim_release_authorized":false,"execution_authority":false}),
    )
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = match values.as_slice() {
        ["alpaca-order-reserve", "--journal", path, "--account-id", account, "--intent", input] => {
            let intent = intent(input)?;
            let mut log = BrokerLog::open(*path, account).map_err(|e| e.to_string())?;
            let applied = log.reserve(intent, super::wallet_read::now()?).map_err(|e| e.to_string())?;
            report(&log, account, applied == Applied::Advanced)?
        },
        ["alpaca-order-journal", "--journal", path, "--account-id", account] => {
            // A missing log is not proof of no claims. Only reserve may initialize it.
            if !Path::new(path).is_file() { return Err("paper order journal is missing".into()); }
            let log = BrokerLog::open(*path, account).map_err(|e| e.to_string())?;
            report(&log, account, false)?
        },
        _ => return Err("usage: radar alpaca-order-reserve --journal <private-file> --account-id <paper-id> --intent <typed-intent>; radar alpaca-order-journal --journal <private-file> --account-id <paper-id>".into()),
    };
    println!("{output}");
    Ok(())
}
