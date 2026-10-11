// SPDX-License-Identifier: Apache-2.0
//! Hyperliquid public observations for the paper-first research lane.
//! No exchange endpoint, account credential, signing or order submission.

use serde_json::{Value, json};

fn endpoint(network: &str) -> Result<&'static str, String> {
    match network {
        "testnet" => Ok("https://api.hyperliquid-testnet.xyz/info"),
        "mainnet" => Ok("https://api.hyperliquid.xyz/info"),
        _ => Err("Hyperliquid network must be testnet or mainnet".into()),
    }
}

fn collect(mut read: impl FnMut(Value) -> Result<Value, String>) -> Result<Value, String> {
    let metadata = read(json!({"type":"metaAndAssetCtxs"}))?;
    let pair = metadata
        .as_array()
        .filter(|pair| pair.len() == 2)
        .ok_or("Hyperliquid metadata/context pair missing")?;
    let universe = pair[0]["universe"]
        .as_array()
        .ok_or("Hyperliquid universe missing")?;
    let contexts = pair[1]
        .as_array()
        .filter(|contexts| contexts.len() == universe.len())
        .ok_or("Hyperliquid contexts do not cover universe")?;
    let mut markets = Vec::new();
    for coin in ["BTC", "ETH", "SOL"] {
        let matches: Vec<_> = universe
            .iter()
            .enumerate()
            .filter(|(_, asset)| asset["name"] == coin)
            .collect();
        if matches.len() != 1 {
            return Err(format!(
                "Hyperliquid universe must identify {coin} uniquely"
            ));
        }
        let (index, asset) = matches[0];
        let book = read(json!({"type":"l2Book","coin":coin}))?;
        if book["coin"] != coin
            || !book["time"].is_u64()
            || book["levels"].as_array().is_none_or(|levels| {
                levels.len() != 2 || levels.iter().any(|side| !side.is_array())
            })
        {
            return Err("Hyperliquid book identity, time or sides missing".into());
        }
        markets.push(json!({"coin":coin,"asset_index":index,
            "metadata":asset,"context":contexts[index],"book":book}));
    }
    Ok(
        json!({"version":1,"venue":"hyperliquid","authority":"read_only",
        "mode":"paper_research","markets":markets,
        "atomic_snapshot":false,"freshness_verified":false,
        "tradable_account_verified":false,"execution_enabled":false}),
    )
}

fn network(args: &[String], environment: Option<String>) -> Result<String, String> {
    if args.iter().any(|arg| arg == "--network") && crate::flag(args, "--network").is_none() {
        return Err("perps-read needs --network <testnet|mainnet>".into());
    }
    Ok(crate::flag(args, "--network")
        .or(environment)
        .unwrap_or_else(|| "testnet".into()))
}

pub fn run(args: &[String]) -> Result<(), String> {
    let network = network(args, std::env::var("RADAR_HYPERLIQUID_NETWORK").ok())?;
    let endpoint = endpoint(&network)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .into();
    let started = crate::wallet_read::now()?;
    let mut report = collect(|request| {
        let body = agent
            .post(endpoint)
            .send_json(&request)
            .map_err(|error| format!("Hyperliquid public read failed: {error}"))?
            .body_mut()
            .read_to_string()
            .map_err(|error| error.to_string())?;
        serde_json::from_str(&body).map_err(|_| "invalid Hyperliquid public response".into())
    })?;
    report["network"] = json!(network);
    report["read_started_at_unix_secs"] = json!(started);
    report["read_completed_at_unix_secs"] = json!(crate::wallet_read::now()?);
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> Value {
        json!([{"universe":[{"name":"SOL"},{"name":"BTC"},{"name":"ETH"}]},
            [{"funding":"-0.00001"},{"funding":"0.00002"},{"funding":"0"}]])
    }

    fn book(coin: &Value) -> Value {
        json!({"coin":coin,"time":1_700_000_000_000_u64,"levels":[[],[]]})
    }

    #[test]
    fn public_reads_bind_contexts_by_universe_index_and_never_grant_authority() {
        let mut requests = Vec::new();
        let report = collect(|request| {
            requests.push(request.clone());
            Ok(if request["type"] == "metaAndAssetCtxs" {
                metadata()
            } else {
                book(&request["coin"])
            })
        })
        .unwrap();
        assert_eq!(
            requests,
            vec![
                json!({"type":"metaAndAssetCtxs"}),
                json!({"type":"l2Book","coin":"BTC"}),
                json!({"type":"l2Book","coin":"ETH"}),
                json!({"type":"l2Book","coin":"SOL"})
            ]
        );
        assert_eq!(report["markets"][0]["asset_index"], 1);
        assert_eq!(report["markets"][0]["context"]["funding"], "0.00002");
        assert_eq!(report["markets"][2]["context"]["funding"], "-0.00001");
        assert_eq!(report["execution_enabled"], false);
        assert_eq!(report["freshness_verified"], false);
        assert_eq!(
            endpoint("testnet").unwrap(),
            "https://api.hyperliquid-testnet.xyz/info"
        );
        assert_eq!(
            endpoint("mainnet").unwrap(),
            "https://api.hyperliquid.xyz/info"
        );
        assert!(endpoint("unknown").is_err());
        assert_eq!(network(&[], None).unwrap(), "testnet");
        assert_eq!(network(&[], Some("mainnet".into())).unwrap(), "mainnet");
        assert_eq!(
            network(
                &["--network".into(), "testnet".into()],
                Some("mainnet".into())
            )
            .unwrap(),
            "testnet"
        );
        assert!(network(&["--network".into()], Some("mainnet".into())).is_err());
        assert!(run(&["--network".into(), "unknown".into()]).is_err());
    }

    #[test]
    fn missing_ambiguous_or_mismatched_public_observations_refuse() {
        for bad in [
            json!(null),
            json!([{}, [], []]),
            json!([{}, []]),
            json!([{"universe":[]},{}]),
            json!([{"universe":[{"name":"BTC"}]},[]]),
            json!([{"universe":[{"name":"BTC"},{"name":"BTC"}]},[{},{}]]),
            json!([{"universe":[{"name":"BTC"}]},[{}]]),
        ] {
            assert!(
                collect(|request| Ok(if request["type"] == "metaAndAssetCtxs" {
                    bad.clone()
                } else {
                    book(&request["coin"])
                }))
                .is_err()
            );
        }
        for bad in [
            json!({"coin":"other","time":1,"levels":[[],[]]}),
            json!({"coin":"BTC","levels":[[],[]]}),
            json!({"coin":"BTC","time":1}),
            json!({"coin":"BTC","time":1,"levels":[[]]}),
            json!({"coin":"BTC","time":1,"levels":[[],{}]}),
        ] {
            assert!(
                collect(|request| Ok(if request["type"] == "metaAndAssetCtxs" {
                    metadata()
                } else {
                    bad.clone()
                }))
                .is_err()
            );
        }
        assert!(collect(|_| Err("transport unavailable".into())).is_err());
    }
}
