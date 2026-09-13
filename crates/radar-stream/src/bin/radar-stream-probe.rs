// SPDX-License-Identifier: Apache-2.0
//! Purchase-day check: connect to the provider, fold for a while, report.
//!
//! ```text
//! RADAR_STREAM_ENDPOINT=https://grpc.solanatracker.io \
//! RADAR_STREAM_TOKEN=... radar-stream-probe 60
//! ```
//!
//! Prints a line every five seconds and a summary at the end, and exits
//! non-zero if not one trade arrived: that is a wrong endpoint, a refused
//! token, or a filter that matches nothing, and each needs fixing before the
//! site is pointed at the feed. The token is never printed.

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use radar_stream::Live;
use radar_stream::feed::{self, Config};

#[tokio::main]
async fn main() -> ExitCode {
    let get = |k: &str| std::env::var(k).ok();
    let config = match Config::from_vars(&get) {
        Ok(Some(config)) => config,
        Ok(None) => {
            eprintln!("set {} (and {}) first", feed::ENDPOINT_VAR, feed::TOKEN_VAR);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let budget = match radar_stream::budget_from_vars(&get) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    println!(
        "connecting to {} for {seconds}s, {} programs, token {}",
        config.endpoint,
        config.programs.len(),
        if config.token.is_some() { "set" } else { "not set" }
    );
    let live = Arc::new(Live::new(budget));
    tokio::spawn(feed::run(config, Arc::clone(&live)));

    let started = Instant::now();
    let mut ticker = tokio::time::interval(Duration::from_secs(5));
    ticker.tick().await;
    let mut last = (0u64, 0u64, 0u64);
    while started.elapsed() < Duration::from_secs(seconds) {
        ticker.tick().await;
        let (counts, coins, used, newest) = {
            let tape = live.tape();
            (tape.counts(), tape.coins(), tape.used_bytes(), tape.newest())
        };
        let bytes = live.status.bytes.load(Ordering::Relaxed);
        let lag = newest.map(|n| now() - n);
        println!(
            "{:>4}s connected={} tx/s={:>6.1} trades/s={:>6.1} MB/s={:>5.2} coins={coins} tape={:.0}MB behind={} unpriced={} untimed={} unreadable={} launches={} evicted={} reconnects={}{}",
            started.elapsed().as_secs(),
            live.status.connected.load(Ordering::Relaxed),
            per_second(counts.transactions - last.0),
            per_second(counts.fills - last.1),
            per_second(bytes - last.2) / 1_048_576.0,
            mib(used),
            lag.map_or_else(|| "-".to_owned(), |l| format!("{l}s")),
            counts.unpriced,
            counts.untimed,
            counts.unreadable,
            counts.launches,
            counts.evicted,
            live.status.reconnects.load(Ordering::Relaxed),
            live.status
                .last_error()
                .map(|e| format!(" last_error={e}"))
                .unwrap_or_default(),
        );
        last = (counts.transactions, counts.fills, bytes);
    }

    let tape = live.tape();
    let counts = tape.counts();
    let newest = tape.newest().unwrap_or(0);
    let mut active = tape.active(newest - 60, newest + 1);
    active.sort_by_key(|a| std::cmp::Reverse(a.trades));
    println!("\nbusiest coins in the last minute of chain:");
    for a in active.iter().take(10) {
        let name = a
            .launch
            .as_ref()
            .map_or_else(String::new, |l| format!(" {} ({})", l.symbol, l.name));
        println!(
            "  {} trades={:>4} close={:?} quote={}{name}",
            a.mint,
            a.trades,
            a.close,
            a.quote.map_or_else(|| "-".to_owned(), |q| q.to_string()),
        );
    }
    println!(
        "\n{} transactions, {} trades, {} launches, {} coins, {:.0} MB tape",
        counts.transactions,
        counts.fills,
        counts.launches,
        tape.coins(),
        mib(tape.used_bytes())
    );
    if counts.fills == 0 {
        eprintln!("no trades arrived: check the endpoint, the token, and the program list");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

#[expect(clippy::cast_precision_loss, reason = "a rate printed to one decimal")]
fn per_second(n: u64) -> f64 {
    n as f64 / 5.0
}

#[expect(clippy::cast_precision_loss, reason = "megabytes printed whole")]
fn mib(bytes: usize) -> f64 {
    bytes as f64 / 1_048_576.0
}
