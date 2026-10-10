// SPDX-License-Identifier: Apache-2.0
//! Operator finalized transaction evidence; cannot release an issuer claim.

use std::time::Duration;

use radar_onchain::{Budget, RpcClient};
use radar_types::Address;
use serde_json::json;

pub fn run(args: &[String]) -> Result<(), String> {
    let wallet: Address = crate::flag(args, "--wallet")
        .and_then(|value| value.parse().ok())
        .ok_or("settlement-read needs --wallet <address>")?;
    let path = crate::flag(args, "--transaction")
        .ok_or("settlement-read needs --transaction <signed-binary-file>")?;
    let minimum = crate::flag(args, "--min-slot")
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or("settlement-read needs --min-slot <u64>")?;
    let endpoint = crate::flag(args, "--rpc")
        .filter(|value| !value.trim().is_empty())
        .ok_or("settlement-read needs an explicit --rpc <URL>")?;
    let bytes = crate::transaction_read::bytes(&path)?;
    let started = crate::wallet_read::now()?;
    let mut budget = Budget::new(1, 0, Duration::from_secs(20));
    let mut evidence =
        RpcClient::new(endpoint).settlement_evidence(&bytes, wallet, minimum, &mut budget)?;
    evidence["read_started_at_unix_secs"] = json!(started);
    evidence["read_completed_at_unix_secs"] = json!(crate::wallet_read::now()?);
    println!("{evidence}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_needs_an_explicit_wallet_file_slot_and_endpoint() {
        let valid = [
            "--wallet",
            "11111111111111111111111111111111",
            "--transaction",
            "absent",
            "--min-slot",
            "0",
            "--rpc",
            "http://127.0.0.1:1",
        ]
        .map(str::to_owned);
        for (index, value, error) in [
            (1, "invalid", "settlement-read needs --wallet <address>"),
            (5, "bad", "settlement-read needs --min-slot <u64>"),
            (7, " ", "settlement-read needs an explicit --rpc <URL>"),
        ] {
            let mut args = valid.clone();
            args[index] = value.into();
            assert_eq!(run(&args), Err(error.into()));
        }
        for (size, error) in [
            (0, "settlement-read needs --wallet <address>"),
            (
                2,
                "settlement-read needs --transaction <signed-binary-file>",
            ),
            (4, "settlement-read needs --min-slot <u64>"),
            (6, "settlement-read needs an explicit --rpc <URL>"),
            (8, "transaction file unavailable"),
        ] {
            assert_eq!(run(&valid[..size]), Err(error.into()));
        }
    }
}
