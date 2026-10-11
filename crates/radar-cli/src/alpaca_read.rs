// SPDX-License-Identifier: Apache-2.0
//! Paper account observations only. These broker values are not an authorization.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const PAPER_API: &str = "https://paper-api.alpaca.markets";

#[derive(Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct BrokerAccount {
    pub(super) id: String,
    pub(super) status: String,
    pub(super) currency: Option<String>,
    pub(super) cash: Option<String>,
    pub(super) equity: Option<String>,
    pub(super) buying_power: Option<String>,
    pub(super) trading_blocked: Option<bool>,
    pub(super) account_blocked: Option<bool>,
    pub(super) trade_suspended_by_user: Option<bool>,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Position {
    pub(super) asset_id: String,
    pub(super) symbol: String,
    pub(super) asset_class: String,
    pub(super) qty: String,
    pub(super) side: String,
    pub(super) market_value: Option<String>,
    pub(super) cost_basis: String,
    unrealized_pl: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Clock {
    timestamp: String,
    is_open: bool,
    next_open: String,
    next_close: String,
}

pub(super) fn credential(value: Option<String>) -> Result<String, String> {
    value
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "configure both Alpaca paper credentials privately".into())
}

pub(super) fn agent() -> ureq::Agent {
    // Alpaca's custom credential headers must never follow a redirect.
    ureq::Agent::config_builder()
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into()
}

pub(super) fn get(
    agent: &ureq::Agent,
    url: &str,
    key: &str,
    secret: &str,
) -> Result<String, String> {
    let mut response = agent
        .get(url)
        .header("APCA-API-KEY-ID", key)
        .header("APCA-API-SECRET-KEY", secret)
        .call()
        .map_err(|_| "Alpaca paper request failed")?;
    if response.status() != 200 {
        return Err("Alpaca paper request returned an unexpected status".into());
    }
    response
        .body_mut()
        .read_to_string()
        .map_err(|_| "Alpaca paper response could not be read".into())
}

fn collect(mut read: impl FnMut(&str) -> Result<String, String>) -> Result<Value, String> {
    let started = crate::wallet_read::now()?;
    // Decode only the fields this report needs. Do not echo an entire account
    // response (account number and future sensitive fields included).
    let account: BrokerAccount = serde_json::from_str(&read("/v2/account")?)
        .map_err(|_| "invalid Alpaca paper account response")?;
    let positions: Vec<Position> = serde_json::from_str(&read("/v2/positions")?)
        .map_err(|_| "invalid Alpaca paper positions response")?;
    let clock: Clock = serde_json::from_str(&read("/v2/clock")?)
        .map_err(|_| "invalid Alpaca paper clock response")?;
    Ok(json!({
        "version":1,
        "venue":"alpaca", "environment":"paper", "source":PAPER_API,
        "read_started_unix_secs":started,
        "read_finished_unix_secs":crate::wallet_read::now()?,
        "account":account, "positions":positions, "market_clock":clock,
        "atomic_snapshot":false, "execution_authority":false
    }))
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    if args.len() != 1 {
        return Err("usage: radar alpaca-read (paper credentials from environment)".into());
    }
    let key = credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
    let secret = credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
    let agent = agent();
    let report = collect(|path| get(&agent, &format!("{PAPER_API}{path}"), &key, &secret))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        match path {
            "/v2/account" => json!({"id":"paper-id", "account_number":"private-number",
                "status":"ACTIVE", "currency":"USD", "cash":"50.01", "equity":"52.34",
                "buying_power":"104.68", "trading_blocked":false, "account_blocked":true,
                "trade_suspended_by_user":false}),
            "/v2/positions" => json!([{"asset_id":"asset-id", "symbol":"SPY",
                "asset_class":"us_equity", "qty":"0.004", "side":"long",
                "market_value":"2.33", "cost_basis":"2.50", "unrealized_pl":"-0.17"}]),
            "/v2/clock" => json!({"timestamp":"2026-10-09T16:01:00-04:00", "is_open":false,
                "next_open":"2026-10-12T09:30:00-04:00", "next_close":"2026-10-12T16:00:00-04:00"}),
            _ => panic!("unexpected read {path}"),
        }
        .to_string()
    }

    #[test]
    fn reports_exact_broker_strings_and_flags_without_account_number() {
        let mut paths = Vec::new();
        let report = collect(|path| {
            paths.push(path.to_owned());
            Ok(fixture(path))
        })
        .unwrap();
        assert_eq!(paths, ["/v2/account", "/v2/positions", "/v2/clock"]);
        let mut expected = json!({"version":1, "venue":"alpaca", "environment":"paper",
            "source":PAPER_API, "account":serde_json::from_str::<Value>(&fixture("/v2/account")).unwrap(),
            "positions":serde_json::from_str::<Value>(&fixture("/v2/positions")).unwrap(),
            "market_clock":serde_json::from_str::<Value>(&fixture("/v2/clock")).unwrap(),
            "atomic_snapshot":false,"execution_authority":false});
        expected["account"]
            .as_object_mut()
            .unwrap()
            .remove("account_number");
        expected["read_started_unix_secs"] = report["read_started_unix_secs"].clone();
        expected["read_finished_unix_secs"] = report["read_finished_unix_secs"].clone();
        assert_eq!(report, expected);
    }

    #[test]
    fn empty_positions_and_unknown_account_values_remain_distinct() {
        let report = collect(|path| {
            Ok(match path {
                "/v2/account" => json!({"id":"paper-id","status":"ACCOUNT_UPDATED"}).to_string(),
                "/v2/positions" => "[]".into(),
                _ => fixture(path),
            })
        })
        .unwrap();
        assert_eq!(report["positions"], json!([]));
        assert!(report["account"]["cash"].is_null());
        assert!(report["account"]["trading_blocked"].is_null());
        assert_eq!(report["account"]["status"], "ACCOUNT_UPDATED");
    }

    #[test]
    fn incomplete_or_failed_reads_never_produce_a_report() {
        for path in ["/v2/account", "/v2/positions", "/v2/clock"] {
            for bad in ["null", "{}", "not-json"] {
                let result = collect(|p| Ok(if p == path { bad.into() } else { fixture(p) }));
                assert!(result.is_err(), "{path}: {bad}");
            }
            assert_eq!(
                collect(|p| if p == path {
                    Err("transport refused".into())
                } else {
                    Ok(fixture(p))
                }),
                Err("transport refused".into())
            );
        }
        for missing in [None, Some(String::new()), Some(" \t".into())] {
            assert_eq!(
                credential(missing),
                Err("configure both Alpaca paper credentials privately".into())
            );
        }
        assert_eq!(
            credential(Some("sentinel-key".into())).unwrap(),
            "sentinel-key"
        );
    }

    #[test]
    fn transport_sends_credentials_but_refuses_redirects_and_error_bodies() {
        use std::io::{BufRead as _, BufReader, Write as _};
        use std::net::TcpListener;
        use std::time::Instant;
        for status in ["200 OK", "302 Found", "401 Unauthorized"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}/v2/account", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let started = Instant::now();
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(started.elapsed() < Duration::from_secs(5), "no request");
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("accept failed: {error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    assert!(
                        reader.read_line(&mut line).unwrap() > 0,
                        "incomplete headers"
                    );
                    if line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                assert!(request.starts_with("GET /v2/account HTTP/1.1\r\n"));
                let headers = request.to_ascii_lowercase();
                assert!(headers.contains("apca-api-key-id: sentinel-key\r\n"));
                assert!(headers.contains("apca-api-secret-key: sentinel-secret\r\n"));
                // Redirect target is deliberately unreachable. A followed redirect
                // fails with a different error than the expected status refusal.
                write!(stream, "HTTP/1.1 {status}\r\nLocation: http://127.0.0.1:1/leak\r\nContent-Length: 15\r\nConnection: close\r\n\r\nsentinel-secret").unwrap();
            });
            let result = get(&agent(), &url, "sentinel-key", "sentinel-secret");
            server.join().unwrap();
            match status {
                "200 OK" => assert_eq!(result.unwrap(), "sentinel-secret"),
                "302 Found" => assert_eq!(
                    result.unwrap_err(),
                    "Alpaca paper request returned an unexpected status"
                ),
                _ => assert_eq!(result.unwrap_err(), "Alpaca paper request failed"),
            }
        }
    }
}
