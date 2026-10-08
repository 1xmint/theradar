// SPDX-License-Identifier: Apache-2.0
//! Operator collection of the two read packets consumed by the offline issuer.
//! This does not fill unknown valuation, risk state or costs in an issuer snapshot.

use std::time::Duration;

use radar_onchain::{Budget, RpcClient};
use radar_types::Address;
use serde_json::json;

pub fn run(args: &[String]) -> Result<(), String> {
    let wallet: Address = crate::flag(args, "--wallet")
        .and_then(|value| value.parse().ok())
        .ok_or("evidence-read needs --wallet <address>")?;
    let path = crate::flag(args, "--transaction")
        .ok_or("evidence-read needs --transaction <binary-file>")?;
    let minimum = crate::flag(args, "--min-slot")
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or("evidence-read needs --min-slot <u64>")?;
    let endpoint = crate::flag(args, "--rpc")
        .filter(|value| !value.trim().is_empty())
        .ok_or("evidence-read needs an explicit --rpc <URL>")?;
    let bytes = crate::transaction_read::bytes(&path)?;
    let rpc = RpcClient::new(endpoint);
    let mut budget = Budget::new(5, 0, Duration::from_secs(20));
    let started = crate::wallet_read::now()?;
    let mut wallet_evidence = crate::wallet_read::read(&rpc, wallet, &mut budget)?;
    wallet_evidence["read_started_at_unix_secs"] = json!(started);
    wallet_evidence["read_completed_at_unix_secs"] = json!(crate::wallet_read::now()?);
    let transaction_started = crate::wallet_read::now()?;
    let mut transaction_evidence = rpc.transaction_preflight(&bytes, minimum, &mut budget)?;
    transaction_evidence["read_started_at_unix_secs"] = json!(transaction_started);
    transaction_evidence["read_completed_at_unix_secs"] = json!(crate::wallet_read::now()?);
    // A later failure must not leave a partial packet looking ready for issuance.
    println!(
        "{}",
        json!({"wallet_evidence":wallet_evidence,"transaction_evidence":transaction_evidence})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_invalid_collection_scope_refuses_before_reading_files_or_network() {
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
        for (index, value, expected) in [
            (1, "invalid", "evidence-read needs --wallet <address>"),
            (5, "bad", "evidence-read needs --min-slot <u64>"),
            (7, " ", "evidence-read needs an explicit --rpc <URL>"),
        ] {
            let mut args = valid.clone();
            args[index] = value.into();
            assert_eq!(run(&args), Err(expected.into()));
        }
        for (length, expected) in [
            (0, "evidence-read needs --wallet <address>"),
            (2, "evidence-read needs --transaction <binary-file>"),
            (4, "evidence-read needs --min-slot <u64>"),
            (6, "evidence-read needs an explicit --rpc <URL>"),
            (8, "transaction file unavailable"),
        ] {
            assert_eq!(run(&valid[..length]), Err(expected.into()));
        }
    }
}
