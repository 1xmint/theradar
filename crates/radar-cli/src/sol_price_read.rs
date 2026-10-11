// SPDX-License-Identifier: Apache-2.0
//! Independent SOL/USD oracle evidence. No portfolio or signing authority.

use radar_onchain::{Budget, RpcClient, sol_price};
use radar_types::Slot;
use serde_json::json;
use std::time::Duration;

pub fn run(args: &[String]) -> Result<(), String> {
    let endpoint = crate::flag(args, "--rpc")
        .filter(|v| !v.trim().is_empty())
        .ok_or("sol-price-read needs an explicit --rpc <URL>")?;
    let minimum_slot = crate::flag(args, "--min-slot")
        .and_then(|v| v.parse().ok())
        .ok_or("sol-price-read needs --min-slot <u64>")?;
    let maximum_age_secs = crate::flag(args, "--max-age-secs")
        .and_then(|v| v.parse().ok())
        .ok_or("sol-price-read needs --max-age-secs <u64>")?;
    let maximum_confidence_bps = crate::flag(args, "--max-confidence-bps")
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v <= 10_000)
        .ok_or("sol-price-read needs --max-confidence-bps <0..10000>")?;
    let output = crate::flag(args, "--output");
    if args.iter().any(|arg| arg == "--output") && output.is_none() {
        return Err("sol-price-read needs --output <private-file>".into());
    }
    let started = crate::wallet_read::now()?;
    let observation = sol_price::read(
        &RpcClient::new(endpoint),
        &mut Budget::new(1, 0, Duration::from_secs(20)),
        &sol_price::Bounds {
            minimum_slot: Slot(minimum_slot),
            maximum_age_secs,
            maximum_confidence_bps,
        },
    )?;
    let packet = json!({"version":1,"authority":"read_only","commitment":"finalized",
        "asset":"SOL","quote":"USD","source":"pyth_receiver_full","account":sol_price::ACCOUNT,
        "read_started_at_unix_secs":started,"read_completed_at_unix_secs":crate::wallet_read::now()?,
        "observation":observation,"wallet_inventory_complete":false,"execution_guaranteed":false});
    if let Some(path) = output {
        crate::evidence_read::publish(std::path::Path::new(&path), &packet)?;
        println!("{}", json!({"price_written":true,"authority":"read_only"}));
    } else {
        println!("{packet}");
    }
    Ok(())
}
