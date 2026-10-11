// SPDX-License-Identifier: Apache-2.0
//! Account-pinned asset observations for the paper equity executor.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const PAPER_API: &str = "https://paper-api.alpaca.markets";

#[derive(Deserialize, Serialize)]
struct Asset {
    id: String,
    symbol: String,
    #[serde(rename = "class")]
    asset_class: String,
    status: Option<String>,
    exchange: Option<String>,
    tradable: Option<bool>,
    fractionable: Option<bool>,
    marginable: Option<bool>,
    shortable: Option<bool>,
    attributes: Option<Vec<String>>,
}

fn collect(
    expected: &str,
    requested: &str,
    mut read: impl FnMut(&str) -> Result<String, String>,
) -> Result<Value, String> {
    if expected.trim().is_empty() {
        return Err("expected paper account identity is required".into());
    }
    let symbols = super::alpaca_market::symbols(requested)?;
    // A ticker is a path segment here. URL normalization must not turn dots
    // into a request for the asset collection or another endpoint.
    if symbols.iter().any(|symbol| symbol == "." || symbol == "..") {
        return Err("asset symbols cannot be URL dot segments".into());
    }
    let started = super::wallet_read::now()?;
    let before = super::alpaca_reconcile::account(&read("/v2/account")?, expected)?;
    let mut assets = Vec::new();
    let mut identities = BTreeSet::new();
    for symbol in &symbols {
        let asset: Asset = serde_json::from_str(&read(&format!("/v2/assets/{symbol}"))?)
            .map_err(|_| "invalid paper asset response")?;
        if asset.symbol != *symbol
            || asset.id.trim().is_empty()
            || !identities.insert(asset.id.clone())
            || asset.asset_class != "us_equity"
        {
            return Err("paper asset identity or class differs from requested equities".into());
        }
        assets.push(asset);
    }
    let after = super::alpaca_reconcile::account(&read("/v2/account")?, expected)?;
    Ok(
        json!({"version":1,"venue":"alpaca","environment":"paper","source":PAPER_API,
        "expected_account_id":expected,"requested_symbols":symbols,"assets":assets,
        "read_started_unix_secs":started,"read_completed_unix_secs":super::wallet_read::now()?,
        "selected_account_fields_unchanged":before==after,"account_before":before,"account_after":after,
        "atomic_snapshot":false,"order_eligibility_verified":false,"execution_authority":false}),
    )
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let values: Vec<&str> = args.iter().map(String::as_str).collect();
    let [
        "alpaca-assets-read",
        "--account-id",
        expected,
        "--symbols",
        symbols,
    ] = values.as_slice()
    else {
        return Err(
            "usage: radar alpaca-assets-read --account-id <paper-id> --symbols SPY,QQQ".into(),
        );
    };
    let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret =
        super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = super::alpaca_read::agent();
    let report = collect(expected, symbols, |path| {
        super::alpaca_read::get(&agent, &format!("{PAPER_API}{path}"), &key, &secret)
    })?;
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> Value {
        if path == "/v2/account" {
            json!({"id":"paper-id","status":"ACTIVE","cash":"50","account_number":"private-number"})
        } else {
            let symbol = path.strip_prefix("/v2/assets/").unwrap();
            json!({"id":format!("asset-{symbol}"),"symbol":symbol,"class":"us_equity",
                "status":"active","exchange":"ARCA","tradable":true,"fractionable":true,
                "marginable":false,"shortable":false,"attributes":["ptp_no_exception"],
                "unknown_secret":"sentinel-private-secret"})
        }
    }

    #[test]
    fn observations_bind_requested_tickers_and_preserve_unknown_or_false_flags() {
        for value in [Value::Null, json!(false), json!(true)] {
            let mut paths = Vec::new();
            let mut accounts = 0;
            let report = collect("paper-id", "SPY,BRK.B", |path| {
                paths.push(path.to_owned());
                let mut body = fixture(path);
                if path == "/v2/account" {
                    accounts += 1;
                    if accounts == 2 {
                        body["cash"] = json!("49");
                    }
                } else {
                    for field in ["tradable", "fractionable", "marginable", "shortable"] {
                        body[field] = value.clone();
                    }
                    body["status"] = json!("inactive");
                }
                Ok(body.to_string())
            })
            .unwrap();
            assert_eq!(
                paths,
                [
                    "/v2/account",
                    "/v2/assets/SPY",
                    "/v2/assets/BRK.B",
                    "/v2/account"
                ]
            );
            assert_eq!(report["requested_symbols"], json!(["SPY", "BRK.B"]));
            assert_eq!(report["assets"].as_array().unwrap().len(), 2);
            for (index, symbol) in ["SPY", "BRK.B"].into_iter().enumerate() {
                assert_eq!(report["assets"][index]["id"], format!("asset-{symbol}"));
                assert_eq!(report["assets"][index]["symbol"], symbol);
                assert_eq!(report["assets"][index]["class"], "us_equity");
                assert_eq!(report["assets"][index]["exchange"], "ARCA");
                assert_eq!(
                    report["assets"][index]["attributes"],
                    json!(["ptp_no_exception"])
                );
                assert_eq!(report["assets"][index]["status"], "inactive");
                for field in ["tradable", "fractionable", "marginable", "shortable"] {
                    assert_eq!(report["assets"][index][field], value);
                }
            }
            assert_eq!(report["selected_account_fields_unchanged"], false);
            assert_eq!(report["account_before"]["cash"], "50");
            assert_eq!(report["account_after"]["cash"], "49");
            for flag in [
                "atomic_snapshot",
                "order_eligibility_verified",
                "execution_authority",
            ] {
                assert_eq!(report[flag], false);
            }
            assert!(!report.to_string().contains("private-number"));
            assert!(!report.to_string().contains("sentinel-private-secret"));
        }
        let stable = collect("paper-id", "SPY", |path| Ok(fixture(path).to_string())).unwrap();
        assert_eq!(stable["selected_account_fields_unchanged"], true);
        let absent = collect("paper-id", "SPY", |path| {
            Ok(if path == "/v2/account" {
                fixture(path)
            } else {
                json!({"id":"asset-SPY","symbol":"SPY","class":"us_equity"})
            }
            .to_string())
        })
        .unwrap();
        for field in [
            "status",
            "exchange",
            "tradable",
            "fractionable",
            "marginable",
            "shortable",
            "attributes",
        ] {
            assert!(absent["assets"][0][field].is_null(), "{field}");
        }
    }

    #[test]
    fn unsafe_queries_and_wrong_or_duplicate_asset_identities_refuse() {
        for symbols in [
            "",
            "spy",
            "SPY,SPY",
            "SPY,",
            ".",
            "..",
            "SPY/QQQ",
            "SPY?x=1",
            "SPY%2fQQQ",
            " SPY",
            "é",
        ] {
            assert!(
                collect("paper-id", symbols, |_| panic!(
                    "invalid symbols must not read"
                ))
                .is_err()
            );
        }
        assert!(collect(" ", "SPY", |_| panic!("missing identity must not read")).is_err());
        for (field, value) in [
            ("symbol", json!("QQQ")),
            ("id", json!("")),
            ("id", json!(" ")),
            ("class", json!("crypto")),
            ("class", Value::Null),
            ("tradable", json!("true")),
        ] {
            assert!(
                collect("paper-id", "SPY", |path| {
                    let mut body = fixture(path);
                    if path != "/v2/account" {
                        body[field] = value.clone();
                    }
                    Ok(body.to_string())
                })
                .is_err(),
                "{field}"
            );
        }
        assert!(
            collect("paper-id", "SPY,QQQ", |path| {
                let mut body = fixture(path);
                if path != "/v2/account" {
                    body["id"] = json!("same-id");
                }
                Ok(body.to_string())
            })
            .is_err()
        );
    }

    #[test]
    fn every_failed_read_and_changed_account_identity_refuses_the_entire_report() {
        for stage in 0..4 {
            for malformed in [false, true] {
                let mut index = 0;
                assert!(
                    collect("paper-id", "SPY,QQQ", |path| {
                        let current = index;
                        index += 1;
                        if current == stage {
                            return if malformed {
                                Ok("null".into())
                            } else {
                                Err("transport failed".into())
                            };
                        }
                        Ok(fixture(path).to_string())
                    })
                    .is_err()
                );
            }
        }
        for stage in [0, 3] {
            let mut index = 0;
            assert!(
                collect("paper-id", "SPY,QQQ", |path| {
                    let mut body = fixture(path);
                    if index == stage {
                        body["id"] = json!("other-account");
                    }
                    index += 1;
                    Ok(body.to_string())
                })
                .is_err()
            );
        }
    }
}
