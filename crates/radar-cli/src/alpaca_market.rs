// SPDX-License-Identifier: Apache-2.0
//! Exact IEX snapshot captures with a host-availability replay boundary.
//! Exchange timestamps and prices remain untrusted observations, not executable quotes.

use std::collections::BTreeSet;
use std::io::{Read as _, Write as _};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DATA_API: &str = "https://data.alpaca.markets/v2/stocks/snapshots";
const MAX_CAPTURE_BYTES: u64 = 1_048_576;

#[derive(Deserialize, Serialize)]
enum Format {
    #[serde(rename = "alpaca-iex-snapshot-v1")]
    AlpacaIexSnapshotV1,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Capture {
    format: Format,
    read_started_unix_ms: u64,
    available_at_unix_ms: u64,
    symbols: Vec<String>,
    // Preserve the original JSON numeric spelling; never round through f64
    // and then present the result as the broker's exact price.
    raw_response: String,
}

fn symbols(text: &str) -> Result<Vec<String>, String> {
    let mut seen = BTreeSet::new();
    let mut symbols = Vec::new();
    for symbol in text.split(',') {
        if symbol.is_empty()
            || !symbol
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
            || !seen.insert(symbol)
        {
            return Err("symbols must be unique uppercase tickers separated by commas".into());
        }
        symbols.push(symbol.to_owned());
    }
    Ok(symbols)
}

fn now_ms() -> Result<u64, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "host clock before epoch")?;
    u64::try_from(elapsed.as_millis()).map_err(|_| "host timestamp overflow".into())
}

fn collect(
    symbols: Vec<String>,
    read: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Capture, String> {
    collect_at(symbols, read, now_ms)
}

fn collect_at(
    symbols: Vec<String>,
    read: impl FnOnce(&str) -> Result<String, String>,
    mut clock: impl FnMut() -> Result<u64, String>,
) -> Result<Capture, String> {
    let started = clock()?;
    let raw_response = read(&format!(
        "{DATA_API}?symbols={}&feed=iex",
        symbols.join(",")
    ))?;
    // Ceil availability to the next millisecond. Flooring a completion time
    // would admit data that arrived later within the decision's millisecond.
    let completed = clock()?.checked_add(1).ok_or("host timestamp overflow")?;
    let capture = Capture {
        format: Format::AlpacaIexSnapshotV1,
        read_started_unix_ms: started,
        available_at_unix_ms: completed,
        symbols,
        raw_response,
    };
    view(&capture, completed)?;
    Ok(capture)
}

fn view(capture: &Capture, as_of: u64) -> Result<Value, String> {
    if capture.read_started_unix_ms >= capture.available_at_unix_ms {
        return Err("market capture clock moved backwards".into());
    }
    if capture.available_at_unix_ms > as_of {
        return Err("market capture was unavailable at the replay watermark".into());
    }
    let requested = symbols(&capture.symbols.join(","))?;
    let response: Value =
        serde_json::from_str(&capture.raw_response).map_err(|_| "invalid Alpaca snapshot JSON")?;
    let response = response
        .as_object()
        .ok_or("Alpaca snapshot response must be an object")?;
    let mut snapshots = serde_json::Map::new();
    for symbol in requested {
        let snapshot = response.get(&symbol).cloned().unwrap_or(Value::Null);
        if !snapshot.is_null() && !snapshot.is_object() {
            return Err("Alpaca symbol snapshot must be an object or null".into());
        }
        snapshots.insert(symbol, snapshot);
    }
    Ok(json!({"venue":"alpaca", "source":DATA_API, "feed":"iex",
        "consolidated_market":false, "read_started_unix_ms":capture.read_started_unix_ms,
        "available_at_unix_ms":capture.available_at_unix_ms, "as_of_unix_ms":as_of,
        "snapshots":snapshots, "exchange_times_validated":false, "execution_authority":false}))
}

fn record(path: &Path, capture: &Capture) -> Result<(), String> {
    let bytes = serde_json::to_vec(capture).map_err(|_| "market capture encoding failed")?;
    if u64::try_from(bytes.len()).map_err(|_| "market capture size overflow")? > MAX_CAPTURE_BYTES {
        return Err("market capture exceeds one MiB".into());
    }
    // Linking publishes a complete synced inode and refuses to overwrite an
    // existing historical capture. The final path is absent until publication.
    let mut pending = path.as_os_str().to_os_string();
    pending.push(format!(".{}.pending", std::process::id()));
    let pending = std::path::PathBuf::from(pending);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(&pending)
        .map_err(|_| "market pending file unavailable")?;
    let result = (|| {
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| "market capture write failed")?;
        drop(file);
        std::fs::hard_link(&pending, path)
            .map_err(|_| "market capture publication refused; use a new filename")
    })();
    let _ = std::fs::remove_file(&pending);
    result.map_err(str::to_owned)
}

fn load(path: &Path) -> Result<Capture, String> {
    let file = std::fs::File::open(path).map_err(|_| "market capture unavailable")?;
    let mut bytes = Vec::new();
    file.take(MAX_CAPTURE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "market capture read failed")?;
    if u64::try_from(bytes.len()).map_err(|_| "market capture size overflow")? > MAX_CAPTURE_BYTES {
        return Err("market capture exceeds one MiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid market capture".into())
}

pub(super) fn quote_input(path: &Path, symbol: &str, as_of: u64) -> Result<(String, u64), String> {
    let capture = load(path)?;
    view(&capture, as_of)?;
    if !capture.symbols.iter().any(|requested| requested == symbol) {
        return Err("quote symbol was not requested in this capture".into());
    }
    Ok((capture.raw_response, capture.available_at_unix_ms))
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let report = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["alpaca-market-read", "--symbols", tickers, "--output", path] => {
            let tickers = symbols(tickers)?;
            let key = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_KEY_ID").ok())?;
            let secret = super::alpaca_read::credential(std::env::var("RADAR_ALPACA_PAPER_SECRET_KEY").ok())?;
            let agent = super::alpaca_read::agent();
            let capture = collect(tickers, |url| super::alpaca_read::get(&agent, url, &key, &secret)
                .map_err(|_| "Alpaca IEX snapshot request failed".into()))?;
            record(Path::new(path), &capture)?;
            json!({"capture_written":true,"feed":"iex","available_at_unix_ms":capture.available_at_unix_ms,"execution_authority":false})
        }
        ["alpaca-market-replay", "--input", path, "--as-of-unix-ms", watermark] => {
            let watermark = watermark.parse().map_err(|_| "replay watermark must be u64 milliseconds")?;
            view(&load(Path::new(path))?, watermark)?
        }
        _ => return Err("usage: radar alpaca-market-read --symbols SPY,QQQ --output <new-file>; radar alpaca-market-replay --input <capture> --as-of-unix-ms <N>".into()),
    };
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Capture {
        Capture { format: Format::AlpacaIexSnapshotV1, read_started_unix_ms: 99, available_at_unix_ms: 100,
            symbols: vec!["SPY".into(),"QQQ".into()], raw_response: r#"{"SPY":{"latestQuote":{"t":"2026-10-09T20:00:00.123456789Z","bp":600.000001,"ap":600.02}},"QQQ":null}"#.into() }
    }

    #[test]
    fn captures_exact_source_body_with_explicit_iex_and_host_availability() {
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let capture = collect(vec!["SPY".into(), "QQQ".into()], |url| {
            assert_eq!(
                url,
                "https://data.alpaca.markets/v2/stocks/snapshots?symbols=SPY,QQQ&feed=iex"
            );
            Ok(fixture().raw_response)
        })
        .unwrap();
        assert!(u128::from(capture.read_started_unix_ms) >= before);
        assert!(capture.available_at_unix_ms > capture.read_started_unix_ms);
        assert!(capture.available_at_unix_ms <= now_ms().unwrap() + 1);
        assert_eq!(capture.raw_response, fixture().raw_response);
        let ceiled = collect_at(vec!["SPY".into()], |_| Ok("{}".into()), || Ok(100)).unwrap();
        assert_eq!(ceiled.read_started_unix_ms, 100);
        assert_eq!(ceiled.available_at_unix_ms, 101);
        let report = view(&fixture(), 101).unwrap();
        assert_eq!(
            report,
            json!({"venue":"alpaca", "source":DATA_API,"feed":"iex",
            "consolidated_market":false,"read_started_unix_ms":99,"available_at_unix_ms":100,
            "as_of_unix_ms":101,"snapshots":serde_json::from_str::<Value>(&fixture().raw_response).unwrap(),
            "exchange_times_validated":false,"execution_authority":false})
        );
    }

    #[test]
    fn replay_refuses_future_data_and_backwards_capture_clocks() {
        assert_eq!(
            view(&fixture(), 99).unwrap_err(),
            "market capture was unavailable at the replay watermark"
        );
        assert_eq!(view(&fixture(), 100).unwrap()["available_at_unix_ms"], 100);
        let mut capture = fixture();
        capture.read_started_unix_ms = 100;
        assert_eq!(
            view(&capture, 100).unwrap_err(),
            "market capture clock moved backwards"
        );
        capture.read_started_unix_ms = 101;
        assert_eq!(
            view(&capture, 100).unwrap_err(),
            "market capture clock moved backwards"
        );
    }

    #[test]
    fn symbols_cannot_inject_queries_and_absence_is_not_a_zero_quote() {
        assert_eq!(
            symbols("BRK.B,SPY,QQQ,ABC-1").unwrap(),
            ["BRK.B", "SPY", "QQQ", "ABC-1"]
        );
        for bad in [
            "",
            "SPY,",
            "SPY,SPY",
            "spy",
            "SPY&feed=sip",
            "SPY/QQQ",
            "SPY QQQ",
        ] {
            assert!(symbols(bad).is_err(), "{bad}");
        }
        let mut capture = fixture();
        capture.raw_response = "{}".into();
        assert_eq!(
            view(&capture, 100).unwrap()["snapshots"],
            json!({"SPY":null,"QQQ":null})
        );
        for bad in ["null", "[]", "invalid", r#"{"SPY":false}"#, r#"{"SPY":42}"#] {
            capture.raw_response = bad.into();
            assert!(view(&capture, 100).is_err(), "{bad}");
        }
        assert!(collect(vec!["SPY".into()], |_| Err("transport refused".into())).is_err());
    }

    #[test]
    fn recording_never_overwrites_history_and_replay_preserves_raw_prices() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.json");
        record(&path, &fixture()).unwrap();
        let original = std::fs::read(&path).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.raw_response, fixture().raw_response);
        assert_eq!(view(&loaded, 100).unwrap(), view(&fixture(), 100).unwrap());
        let mut replacement = fixture();
        replacement.raw_response = "{}".into();
        assert!(record(&path, &replacement).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        std::fs::write(&path, b"{\"format\":").unwrap();
        assert!(load(&path).is_err());
        assert!(load(&directory.path().join("missing")).is_err());
    }

    #[test]
    fn oversized_and_wrong_format_captures_are_refused() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.json");
        let mut capture = fixture();
        capture.raw_response = " ".repeat(usize::try_from(MAX_CAPTURE_BYTES).unwrap());
        assert!(record(&path, &capture).is_err());
        assert!(!path.exists());
        // A valid packet exactly at the size boundary must still load. A
        // syntactically invalid oversized fixture would mask a removed guard.
        capture.raw_response = "{}".into();
        let overhead = serde_json::to_vec(&capture).unwrap().len();
        capture
            .raw_response
            .push_str(&" ".repeat(usize::try_from(MAX_CAPTURE_BYTES).unwrap() - overhead));
        assert_eq!(
            serde_json::to_vec(&capture).unwrap().len(),
            usize::try_from(MAX_CAPTURE_BYTES).unwrap()
        );
        record(&path, &capture).unwrap();
        assert!(load(&path).is_ok());
        capture.raw_response.push(' ');
        let second = directory.path().join("second.json");
        assert_eq!(
            record(&second, &capture).unwrap_err(),
            "market capture exceeds one MiB"
        );
        std::fs::write(&second, serde_json::to_vec(&capture).unwrap()).unwrap();
        assert_eq!(
            load(&second).err().unwrap(),
            "market capture exceeds one MiB"
        );
        let mut wrong = serde_json::to_value(fixture()).unwrap();
        wrong["format"] = json!("alpaca-sip-snapshot-v1");
        std::fs::write(&path, wrong.to_string()).unwrap();
        assert!(load(&path).is_err());
    }
}
