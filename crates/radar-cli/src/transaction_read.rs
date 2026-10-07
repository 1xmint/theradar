// SPDX-License-Identifier: Apache-2.0
//! Operator-only unsigned transaction evidence, never submission or authority.

use std::io::Read as _;
use std::time::Duration;

use radar_onchain::preflight::MAX_TRANSACTION_BYTES;
use radar_onchain::{Budget, RpcClient};

pub fn run(args: &[String]) -> Result<(), String> {
    let path = crate::flag(args, "--transaction")
        .ok_or("transaction-read needs --transaction <binary-file>")?;
    let endpoint = crate::flag(args, "--rpc")
        .filter(|value| !value.trim().is_empty())
        .ok_or("transaction-read needs an explicit --rpc <URL>")?;
    let minimum = crate::flag(args, "--min-slot")
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or("transaction-read needs --min-slot <u64>")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "transaction file unavailable")?
        .take((MAX_TRANSACTION_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "transaction file read failed")?;
    let started = crate::wallet_read::now()?;
    let mut budget = Budget::new(2, 0, Duration::from_secs(20));
    let mut evidence =
        RpcClient::new(endpoint).transaction_preflight(&bytes, minimum, &mut budget)?;
    evidence["read_started_at_unix_secs"] = serde_json::json!(started);
    evidence["read_completed_at_unix_secs"] = serde_json::json!(crate::wallet_read::now()?);
    println!("{evidence}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_and_unavailable_files_refuse_without_network() {
        for (args, error) in [
            (vec![], "transaction-read needs --transaction <binary-file>"),
            (
                vec!["--transaction", "absent"],
                "transaction-read needs an explicit --rpc <URL>",
            ),
            (
                vec!["--transaction", "absent", "--rpc", " "],
                "transaction-read needs an explicit --rpc <URL>",
            ),
            (
                vec!["--transaction", "absent", "--rpc", "http://127.0.0.1:1"],
                "transaction-read needs --min-slot <u64>",
            ),
            (
                vec![
                    "--transaction",
                    "absent",
                    "--rpc",
                    "http://127.0.0.1:1",
                    "--min-slot",
                    "bad",
                ],
                "transaction-read needs --min-slot <u64>",
            ),
            (
                vec![
                    "--transaction",
                    "absent",
                    "--rpc",
                    "http://127.0.0.1:1",
                    "--min-slot",
                    "0",
                ],
                "transaction file unavailable",
            ),
        ] {
            assert_eq!(
                run(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()),
                Err(error.into())
            );
        }
    }
}
