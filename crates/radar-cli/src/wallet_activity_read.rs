// SPDX-License-Identifier: Apache-2.0
//! Operator address history collection, without wallet authority.

use std::time::Duration;

use radar_onchain::{Budget, RpcClient};
use radar_types::Address;
use serde_json::json;

pub fn run(args: &[String]) -> Result<(), String> {
    let wallet: Address = crate::flag(args, "--wallet")
        .and_then(|value| value.parse().ok())
        .ok_or("wallet-activity-read needs --wallet <address>")?;
    let bound = |flag, error| {
        crate::flag(args, flag)
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or(error)
    };
    let after = bound(
        "--after-slot",
        "wallet-activity-read needs --after-slot <u64>",
    )?;
    let through = bound(
        "--through-slot",
        "wallet-activity-read needs --through-slot <u64>",
    )?;
    let endpoint = crate::flag(args, "--rpc")
        .filter(|value| !value.trim().is_empty())
        .ok_or("wallet-activity-read needs an explicit --rpc <URL>")?;
    let started = crate::wallet_read::now()?;
    // Three pages and 33 transaction reads fit the call bound. Larger histories
    // explicitly truncate; the budget never grants execution or inference calls.
    let mut budget = Budget::new(36, 3, Duration::from_secs(20));
    let mut evidence = radar_onchain::wallet_activity::read(
        &RpcClient::new(endpoint),
        wallet,
        after,
        through,
        &mut budget,
    )?;
    evidence["read_started_at_unix_secs"] = json!(started);
    evidence["read_completed_at_unix_secs"] = json!(crate::wallet_read::now()?);
    println!("{evidence}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_history_requires_explicit_identity_bounds_and_endpoint() {
        let valid = [
            "--wallet",
            "11111111111111111111111111111111",
            "--after-slot",
            "40",
            "--through-slot",
            "50",
            "--rpc",
            "http://127.0.0.1:1",
        ]
        .map(str::to_owned);
        for (index, value, error) in [
            (
                1,
                "invalid",
                "wallet-activity-read needs --wallet <address>",
            ),
            (3, "bad", "wallet-activity-read needs --after-slot <u64>"),
            (5, "bad", "wallet-activity-read needs --through-slot <u64>"),
            (3, "50", "activity needs after-slot below through-slot"),
            (3, "51", "activity needs after-slot below through-slot"),
            (7, " ", "wallet-activity-read needs an explicit --rpc <URL>"),
        ] {
            let mut args = valid.clone();
            args[index] = value.into();
            assert_eq!(run(&args), Err(error.into()));
        }
        for (size, error) in [
            (0, "wallet-activity-read needs --wallet <address>"),
            (2, "wallet-activity-read needs --after-slot <u64>"),
            (4, "wallet-activity-read needs --through-slot <u64>"),
            (6, "wallet-activity-read needs an explicit --rpc <URL>"),
        ] {
            assert_eq!(run(&valid[..size]), Err(error.into()));
        }
    }
}
