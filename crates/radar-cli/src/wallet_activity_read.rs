// SPDX-License-Identifier: Apache-2.0
//! Operator address history collection, without wallet authority.

use serde_json::Value;
use std::io::Read as _;
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
    // Known-account mode shares one budget across targets; larger histories
    // explicitly truncate. These budgets grant no execution or inference calls.
    let rpc = RpcClient::new(endpoint);
    let mut evidence = if args.iter().any(|arg| arg == "--inventory-review") {
        let path = crate::flag(args, "--inventory-review")
            .ok_or("activity needs --inventory-review <path>")?;
        let addresses = targets(&path, wallet)?;
        let mut budget = Budget::new(108, 16, Duration::from_secs(20));
        radar_onchain::wallet_activity::read_known_accounts(
            &rpc,
            wallet,
            &addresses,
            after,
            through,
            &mut budget,
        )?
    } else {
        let mut budget = Budget::new(36, 3, Duration::from_secs(20));
        radar_onchain::wallet_activity::read(&rpc, wallet, after, through, &mut budget)?
    };
    evidence["read_started_at_unix_secs"] = json!(started);
    evidence["read_completed_at_unix_secs"] = json!(crate::wallet_read::now()?);
    println!("{evidence}");
    Ok(())
}

fn targets(path: &str, wallet: Address) -> Result<Vec<Address>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "activity inventory review unavailable")?
        .take(1_048_577)
        .read_to_end(&mut bytes)
        .map_err(|_| "activity inventory review unavailable")?;
    if bytes.len() > 1_048_576 {
        return Err("activity inventory review exceeds size limit".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "invalid activity inventory review")?;
    let comparison = &value["token_account_comparison"];
    if value["authority"] != "protected_operator_inventory_comparison"
        || value["wallet"] != wallet.to_string()
        || comparison["coverage"] != "opening_and_current_accounts_only"
        || !comparison["opening_accounts_retained"].is_boolean()
        || comparison["historical_account_coverage_complete"] != false
        || comparison["portfolio_state_updated"] != false
    {
        return Err("activity inventory review identity or coverage is unsupported".into());
    }
    let rows = comparison["accounts"]
        .as_array()
        .filter(|rows| rows.len() < 16)
        .ok_or("activity needs at most fifteen known token accounts")?;
    let mut addresses = std::collections::BTreeSet::new();
    for row in rows {
        let address: Address = serde_json::from_value(row["address"].clone())
            .map_err(|_| "invalid activity account address")?;
        if address == wallet || !addresses.insert(address) {
            return Err("activity account targets duplicate wallet or account".into());
        }
    }
    Ok(addresses.into_iter().collect())
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

    fn review_file() -> (tempfile::NamedTempFile, Value, Address) {
        let wallet = Address::new([0x55; 32]);
        let report = json!({"authority":"protected_operator_inventory_comparison","wallet":wallet,
            "token_account_comparison":{"coverage":"opening_and_current_accounts_only","opening_accounts_retained":false,
            "historical_account_coverage_complete":false,"portfolio_state_updated":false,
            "accounts":[{"address":Address::new([2;32]),"current_present":true},{"address":Address::new([1;32]),"current_present":false}]}});
        (tempfile::NamedTempFile::new().unwrap(), report, wallet)
    }

    #[test]
    fn known_targets_preserve_missing_current_accounts_and_reject_identity_or_coverage_claims() {
        let (file, report, wallet) = review_file();
        let path = file.path().to_str().unwrap();
        std::fs::write(path, report.to_string()).unwrap();
        assert_eq!(
            targets(path, wallet).unwrap(),
            vec![Address::new([1; 32]), Address::new([2; 32])]
        );
        for (pointer, value) in [
            ("/authority", json!("untrusted")),
            ("/wallet", json!(Address::new([3; 32]))),
            ("/token_account_comparison/coverage", json!("complete")),
            (
                "/token_account_comparison/opening_accounts_retained",
                Value::Null,
            ),
            (
                "/token_account_comparison/historical_account_coverage_complete",
                json!(true),
            ),
            (
                "/token_account_comparison/portfolio_state_updated",
                json!(true),
            ),
            ("/token_account_comparison/accounts/0/address", json!("bad")),
            ("/token_account_comparison/accounts", Value::Null),
        ] {
            let mut bad = report.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            std::fs::write(path, bad.to_string()).unwrap();
            assert!(targets(path, wallet).is_err(), "{pointer}");
        }
        for address in [wallet, Address::new([1; 32])] {
            let mut bad = report.clone();
            bad["token_account_comparison"]["accounts"][0]["address"] = json!(address);
            std::fs::write(path, bad.to_string()).unwrap();
            assert!(targets(path, wallet).is_err());
        }
    }

    #[test]
    fn known_target_file_size_count_and_malformed_input_are_bounded() {
        let (file, mut report, wallet) = review_file();
        let path = file.path().to_str().unwrap();
        let accounts: Vec<_> = (1..17)
            .map(|seed| json!({"address":Address::new([seed;32])}))
            .collect();
        for count in [0, 15, 16] {
            report["token_account_comparison"]["accounts"] = json!(accounts[..count]);
            std::fs::write(path, report.to_string()).unwrap();
            assert_eq!(targets(path, wallet).is_ok(), count < 16);
        }
        report["token_account_comparison"]["accounts"] = json!([]);
        let mut exact = report.to_string().into_bytes();
        exact.resize(1_048_576, b' ');
        std::fs::write(path, exact).unwrap();
        assert!(targets(path, wallet).is_ok());
        std::fs::write(path, vec![b' '; 1_048_577]).unwrap();
        assert_eq!(
            targets(path, wallet).unwrap_err(),
            "activity inventory review exceeds size limit"
        );
        std::fs::write(path, b"{}").unwrap();
        assert!(targets(path, wallet).is_err());
        std::fs::write(path, b"broken").unwrap();
        assert!(targets(path, wallet).is_err());
        assert!(targets("nonexistent-account-review-file", wallet).is_err());
    }
}
