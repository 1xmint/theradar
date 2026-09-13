// SPDX-License-Identifier: Apache-2.0
//! The connection: subscribe, fold what arrives into the tape, reconnect.
//!
//! # Timestamps
//!
//! A transaction update carries its slot and not its time. Block time arrives
//! in a separate block-meta update for the same slot, in either order. So a
//! transaction waits here until its block's time is known, and is dropped and
//! counted if it has not arrived within [`MAX_WAIT_SLOTS`]. A trade stamped
//! with the moment it was *received* would be off by the feed's own lag, and
//! that lag is the one number a live screen exists to keep honest.
//!
//! # The token
//!
//! [`Config::token`] is a paid credential. It is sent only as the `x-token`
//! header Yellowstone providers read, and [`Config`]'s `Debug` never prints it.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use prost::Message as _;
use radar_types::Signature;
use tonic::codec::CompressionEncoding;
use tonic::transport::{ClientTlsConfig, Endpoint};

use crate::Live;
use crate::decode::{Decoded, decode};
use crate::proto::{
    CommitmentLevel, SubscribeRequest, SubscribeRequestFilterBlocksMeta,
    SubscribeRequestFilterTransactions, SubscribeRequestPing, SubscribeUpdate, UpdateOneof,
};
use crate::tx::Tx;

/// The venues subscribed to when `RADAR_STREAM_PROGRAMS` is unset.
///
/// Every address checked as a live executable program on mainnet on
/// 2026-09-13 with `getMultipleAccounts`. Aggregators are deliberately absent:
/// a Jupiter route calls one of these, so it arrives anyway.
pub const DEFAULT_PROGRAMS: [&str; 12] = [
    "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P", // pump.fun bonding curve
    "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA", // PumpSwap
    "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8", // Raydium AMM v4
    "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C", // Raydium CPMM
    "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK", // Raydium CLMM
    "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj", // Raydium LaunchLab
    "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo", // Meteora DLMM
    "Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB", // Meteora pools
    "cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG", // Meteora DAMM v2
    "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN", // Meteora bonding curve
    "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc", // Orca Whirlpool
    "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG", // Moonshot
];

/// How many slots a transaction waits for its block's time before it is
/// dropped. About a minute of chain; a block-meta update later than that is
/// not coming.
pub const MAX_WAIT_SLOTS: u64 = 150;

/// The largest single update accepted. Yellowstone providers document 64 MB.
const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

/// Where to connect and what to ask for.
#[derive(Clone, PartialEq, Eq)]
pub struct Config {
    /// The provider's gRPC URL, e.g. `https://grpc.solanatracker.io`.
    pub endpoint: String,
    /// The provider's API token. Secret.
    pub token: Option<String>,
    /// Programs whose transactions are streamed.
    pub programs: Vec<String>,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("endpoint", &self.endpoint)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("programs", &self.programs.len())
            .finish()
    }
}

/// The variable naming the provider's URL. Unset means no live feed.
pub const ENDPOINT_VAR: &str = "RADAR_STREAM_ENDPOINT";
/// The variable holding the provider's token.
pub const TOKEN_VAR: &str = "RADAR_STREAM_TOKEN";
/// An optional comma-separated override of [`DEFAULT_PROGRAMS`].
pub const PROGRAMS_VAR: &str = "RADAR_STREAM_PROGRAMS";

impl Config {
    /// Reads the configuration.
    ///
    /// `Ok(None)` when no endpoint is set: the live feed is off, and the
    /// market routes read the store as before.
    ///
    /// # Errors
    ///
    /// An endpoint that is not an `http(s)` URL, or a program that is not an
    /// address. Refused rather than skipped: a typo in a program list would
    /// otherwise silently stream nothing from that venue.
    pub fn from_vars(get: &impl Fn(&str) -> Option<String>) -> Result<Option<Self>, String> {
        let Some(endpoint) = get(ENDPOINT_VAR).map(|e| e.trim().to_owned()) else {
            return Ok(None);
        };
        if endpoint.is_empty() {
            return Ok(None);
        }
        if !(endpoint.starts_with("https://") || endpoint.starts_with("http://")) {
            return Err(format!("{ENDPOINT_VAR} must be an http:// or https:// URL"));
        }
        let token = get(TOKEN_VAR)
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
        let programs: Vec<String> = match get(PROGRAMS_VAR) {
            Some(list) if !list.trim().is_empty() => list
                .split(',')
                .map(|p| p.trim().to_owned())
                .filter(|p| !p.is_empty())
                .collect(),
            _ => DEFAULT_PROGRAMS.iter().map(|p| (*p).to_owned()).collect(),
        };
        if let Some(bad) = programs
            .iter()
            .find(|p| p.parse::<radar_types::Address>().is_err())
        {
            return Err(format!(
                "{PROGRAMS_VAR} contains '{bad}', which is not an address"
            ));
        }
        Ok(Some(Self {
            endpoint,
            token,
            programs,
        }))
    }
}

/// The subscription: confirmed, successful, non-vote transactions touching
/// any of the programs, plus every block's time.
#[must_use]
pub fn subscribe_request(config: &Config) -> SubscribeRequest {
    SubscribeRequest {
        transactions: [(
            "radar".to_owned(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                account_include: config.programs.clone(),
            },
        )]
        .into(),
        blocks_meta: [("radar".to_owned(), SubscribeRequestFilterBlocksMeta {})].into(),
        commitment: Some(CommitmentLevel::Confirmed as i32),
        ping: None,
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Transactions waiting for their block's time.
#[derive(Default)]
struct Pending {
    waiting: BTreeMap<u64, Vec<(Signature, Decoded)>>,
    times: BTreeMap<u64, i64>,
    newest_slot: u64,
}

impl Pending {
    /// Takes an update and returns what is ready to apply, stamped.
    fn transaction(
        &mut self,
        slot: u64,
        signature: Signature,
        decoded: Decoded,
    ) -> Option<(u64, Signature, i64, Decoded)> {
        self.newest_slot = self.newest_slot.max(slot);
        if let Some(&time) = self.times.get(&slot) {
            return Some((slot, signature, time, decoded));
        }
        self.waiting
            .entry(slot)
            .or_default()
            .push((signature, decoded));
        None
    }

    /// Records a block's time and releases what was waiting on it.
    fn block_time(&mut self, slot: u64, time: i64) -> Vec<(u64, Signature, i64, Decoded)> {
        self.newest_slot = self.newest_slot.max(slot);
        self.times.insert(slot, time);
        self.waiting
            .remove(&slot)
            .unwrap_or_default()
            .into_iter()
            .map(|(signature, decoded)| (slot, signature, time, decoded))
            .collect()
    }

    /// Forgets old block times and drops transactions that waited too long.
    /// Returns how many were dropped.
    fn prune(&mut self) -> u64 {
        let floor = self.newest_slot.saturating_sub(MAX_WAIT_SLOTS);
        self.times = self.times.split_off(&floor);
        let kept = self.waiting.split_off(&floor);
        let dropped = std::mem::replace(&mut self.waiting, kept);
        dropped.values().map(|v| v.len() as u64).sum()
    }
}

/// How long a refused credential waits before trying again.
///
/// Five minutes, not thirty seconds. A wrong or expired token does not fix
/// itself, and retrying it every half-minute gets the address rate-limited: on
/// 2026-09-13 PublicNode answered a tokenless probe with `PermissionDenied`
/// five times and then with HTTP 429.
pub const REFUSED_DELAY: Duration = Duration::from_secs(5 * 60);

/// Why a session ended.
#[derive(Debug, PartialEq, Eq)]
struct Ended {
    reason: String,
    /// The provider refused the credential, rather than the connection failing.
    refused: bool,
}

impl Ended {
    fn retry(reason: String) -> Self {
        Self {
            reason,
            refused: false,
        }
    }
}

/// How long to wait before reconnecting.
///
/// Doubles from one second to thirty, and drops back to one second after a
/// session that stayed up a minute, so a provider restart costs a second. A
/// refused credential waits [`REFUSED_DELAY`].
fn next_delay(previous: Duration, lasted: Duration, refused: bool) -> Duration {
    if refused {
        return REFUSED_DELAY;
    }
    if lasted > Duration::from_secs(60) {
        return Duration::from_secs(1);
    }
    (previous * 2).clamp(Duration::from_secs(1), Duration::from_secs(30))
}

/// Runs the feed forever, reconnecting with [`next_delay`].
pub async fn run(config: Config, live: Arc<Live>) {
    let mut delay = Duration::ZERO;
    loop {
        let started = Instant::now();
        let outcome = session(&config, &live).await;
        live.status.connected.store(false, Ordering::Relaxed);
        let ended = outcome
            .err()
            .unwrap_or_else(|| Ended::retry("the provider ended the stream".to_owned()));
        delay = next_delay(delay, started.elapsed(), ended.refused);
        // One line per session end, for journald. Never the token: the
        // reasons come from tonic's status and transport errors, which do not
        // carry request metadata.
        eprintln!(
            "radar-stream: session ended: {}; reconnecting in {}s",
            ended.reason,
            delay.as_secs()
        );
        live.status.set_error(ended.reason);
        live.status.reconnects.fetch_add(1, Ordering::Relaxed);
        tokio::time::sleep(delay).await;
    }
}

/// The open subscription: updates in, requests (pongs) out.
type Subscription = (
    tonic::Streaming<SubscribeUpdate>,
    tokio::sync::mpsc::Sender<SubscribeRequest>,
);

/// Connects and subscribes.
async fn subscribe(config: &Config) -> Result<Subscription, Ended> {
    let mut endpoint = Endpoint::from_shared(config.endpoint.clone())
        .map_err(|e| Ended::retry(format!("bad endpoint: {e}")))?
        .connect_timeout(Duration::from_secs(10))
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .http2_keep_alive_interval(Duration::from_secs(15))
        .keep_alive_while_idle(true);
    if config.endpoint.starts_with("https://") {
        endpoint = endpoint
            .tls_config(ClientTlsConfig::new().with_webpki_roots())
            .map_err(|e| Ended::retry(format!("tls: {e}")))?;
    }
    let channel = endpoint
        .connect()
        .await
        .map_err(|e| Ended::retry(format!("connect: {e}")))?;

    let mut grpc = tonic::client::Grpc::new(channel)
        .accept_compressed(CompressionEncoding::Zstd)
        .max_decoding_message_size(MAX_MESSAGE_BYTES);
    grpc.ready()
        .await
        .map_err(|e| Ended::retry(format!("not ready: {e}")))?;

    let (outbound, requests) = tokio::sync::mpsc::channel::<SubscribeRequest>(8);
    outbound
        .send(subscribe_request(config))
        .await
        .map_err(|_| Ended::retry("request channel closed".to_owned()))?;
    let requests = futures_util::stream::unfold(requests, |mut rx| async move {
        rx.recv().await.map(|m| (m, rx))
    });
    let mut request = tonic::Request::new(requests);
    if let Some(token) = &config.token {
        let value = token.parse().map_err(|_| Ended {
            reason: format!("{TOKEN_VAR} is not a valid header value"),
            refused: true,
        })?;
        request.metadata_mut().insert("x-token", value);
    }

    let codec = tonic_prost::ProstCodec::<SubscribeRequest, SubscribeUpdate>::default();
    let path = http::uri::PathAndQuery::from_static("/geyser.Geyser/Subscribe");
    let updates = grpc
        .streaming(request, path, codec)
        .await
        .map_err(|e| Ended {
            reason: format!("subscribe refused: {} {}", e.code(), e.message()),
            refused: matches!(
                e.code(),
                tonic::Code::PermissionDenied | tonic::Code::Unauthenticated
            ),
        })?
        .into_inner();
    Ok((updates, outbound))
}

/// What one update asks of the session.
#[derive(Debug, PartialEq)]
enum Step {
    /// Transactions now stamped and ready for the tape. Possibly none.
    Apply(Vec<(u64, Signature, i64, Decoded)>),
    /// The provider pinged; answer it.
    Pong,
}

/// Folds one update into the waiting set. Synchronous and network-free, so
/// the order-independence of transactions and block times is testable.
fn step(update: SubscribeUpdate, pending: &mut Pending, live: &Live) -> Step {
    match update.update_oneof {
        Some(UpdateOneof::Transaction(tx_update)) => {
            let Ok(tx) = Tx::try_from(&tx_update) else {
                live.tape().note_unreadable();
                return Step::Apply(Vec::new());
            };
            let decoded = decode(&tx);
            Step::Apply(
                pending
                    .transaction(tx.slot, tx.signature, decoded)
                    .into_iter()
                    .collect(),
            )
        }
        Some(UpdateOneof::BlockMeta(meta)) => Step::Apply(
            meta.block_time
                .map(|time| pending.block_time(meta.slot, time.timestamp))
                .unwrap_or_default(),
        ),
        Some(UpdateOneof::Ping(_)) => Step::Pong,
        Some(UpdateOneof::Pong(_)) | None => Step::Apply(Vec::new()),
    }
}

async fn session(config: &Config, live: &Live) -> Result<(), Ended> {
    let (mut updates, outbound) = subscribe(config).await?;
    live.status.connected.store(true, Ordering::Relaxed);
    live.status.connected_since.store(now(), Ordering::Relaxed);

    let mut pending = Pending::default();
    let mut last_prune = Instant::now();
    while let Some(update) = updates
        .message()
        .await
        .map_err(|e| Ended::retry(format!("stream: {} {}", e.code(), e.message())))?
    {
        live.status.last_message.store(now(), Ordering::Relaxed);
        live.status
            .bytes
            .fetch_add(update.encoded_len() as u64, Ordering::Relaxed);

        let ready = match step(update, &mut pending, live) {
            Step::Apply(ready) => ready,
            Step::Pong => {
                // Providers close a connection that stops answering pings. A
                // ping-only request leaves the filters as they are.
                let pong = SubscribeRequest {
                    ping: Some(SubscribeRequestPing { id: 1 }),
                    ..SubscribeRequest::default()
                };
                outbound
                    .send(pong)
                    .await
                    .map_err(|_| Ended::retry("request channel closed".to_owned()))?;
                Vec::new()
            }
        };

        let prune = last_prune.elapsed() > Duration::from_secs(5);
        if !ready.is_empty() || prune {
            let mut tape = live.tape();
            for (slot, signature, time, decoded) in ready {
                tape.apply(slot, signature, time, decoded);
            }
            if prune {
                tape.note_untimed(pending.prune());
                last_prune = Instant::now();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |k| {
            owned
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
        }
    }

    #[test]
    fn no_endpoint_means_no_feed() {
        assert_eq!(Config::from_vars(&vars(&[])), Ok(None));
        assert_eq!(Config::from_vars(&vars(&[(ENDPOINT_VAR, " ")])), Ok(None));
    }

    #[test]
    fn an_endpoint_without_a_scheme_is_refused() {
        assert!(Config::from_vars(&vars(&[(ENDPOINT_VAR, "grpc.example.com:443")])).is_err());
    }

    #[test]
    fn the_default_programs_are_used_unless_overridden() {
        let config = Config::from_vars(&vars(&[(ENDPOINT_VAR, "https://grpc.example.com")]))
            .unwrap()
            .unwrap();
        assert_eq!(config.programs.len(), DEFAULT_PROGRAMS.len());
        assert_eq!(config.token, None);

        let config = Config::from_vars(&vars(&[
            (ENDPOINT_VAR, "https://grpc.example.com"),
            (PROGRAMS_VAR, "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"),
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(
            config.programs,
            vec!["6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"]
        );
    }

    #[test]
    fn a_program_that_is_not_an_address_is_refused() {
        let err = Config::from_vars(&vars(&[
            (ENDPOINT_VAR, "https://grpc.example.com"),
            (PROGRAMS_VAR, "pumpfun"),
        ]))
        .unwrap_err();
        assert!(err.contains("pumpfun"), "{err}");
    }

    #[test]
    fn every_default_program_is_an_address() {
        for p in DEFAULT_PROGRAMS {
            assert!(p.parse::<radar_types::Address>().is_ok(), "{p}");
        }
    }

    #[test]
    fn the_token_never_appears_in_debug_output() {
        let config = Config {
            endpoint: "https://grpc.example.com".into(),
            token: Some("very-secret-token".into()),
            programs: vec![],
        };
        let shown = format!("{config:?}");
        assert!(!shown.contains("very-secret-token"), "{shown}");
    }

    #[test]
    fn the_subscription_asks_for_confirmed_successful_non_vote_transactions_and_block_times() {
        let config = Config {
            endpoint: "https://grpc.example.com".into(),
            token: None,
            programs: vec!["6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".into()],
        };
        let request = subscribe_request(&config);
        let filter = &request.transactions["radar"];
        assert_eq!((filter.vote, filter.failed), (Some(false), Some(false)));
        assert_eq!(filter.account_include, config.programs);
        assert!(request.blocks_meta.contains_key("radar"));
        assert_eq!(request.commitment, Some(CommitmentLevel::Confirmed as i32));
    }

    #[test]
    fn reconnecting_doubles_from_one_second_to_thirty() {
        let lasted = Duration::from_secs(2);
        let mut delay = Duration::ZERO;
        let mut seen = Vec::new();
        for _ in 0..7 {
            delay = next_delay(delay, lasted, false);
            seen.push(delay.as_secs());
        }
        assert_eq!(seen, vec![1, 2, 4, 8, 16, 30, 30]);
    }

    #[test]
    fn a_session_that_stayed_up_a_minute_reconnects_after_a_second() {
        let long = Duration::from_secs(61);
        assert_eq!(
            next_delay(Duration::from_secs(30), long, false),
            Duration::from_secs(1)
        );
        assert_eq!(
            next_delay(Duration::from_secs(30), Duration::from_secs(60), false),
            Duration::from_secs(30),
            "exactly a minute is not long enough to reset"
        );
    }

    #[test]
    fn a_refused_credential_waits_five_minutes_however_long_the_session_lasted() {
        assert_eq!(
            next_delay(Duration::ZERO, Duration::ZERO, true),
            REFUSED_DELAY
        );
        assert_eq!(
            next_delay(Duration::ZERO, Duration::from_secs(600), true),
            REFUSED_DELAY
        );
    }

    fn block_meta(slot: u64, time: i64) -> SubscribeUpdate {
        SubscribeUpdate {
            filters: vec![],
            update_oneof: Some(UpdateOneof::BlockMeta(
                crate::proto::SubscribeUpdateBlockMeta {
                    slot,
                    block_time: Some(crate::proto::UnixTimestamp { timestamp: time }),
                },
            )),
        }
    }

    #[test]
    fn a_ping_asks_for_a_pong_and_a_block_time_releases_nothing_on_its_own() {
        let live = Live::new(usize::MAX);
        let mut pending = Pending::default();
        let ping = SubscribeUpdate {
            filters: vec![],
            update_oneof: Some(UpdateOneof::Ping(crate::proto::SubscribeUpdatePing {})),
        };
        assert_eq!(step(ping, &mut pending, &live), Step::Pong);
        assert_eq!(
            step(block_meta(5, 1_700_000_000), &mut pending, &live),
            Step::Apply(vec![])
        );
        assert_eq!(pending.times.get(&5), Some(&1_700_000_000));
    }

    #[test]
    fn an_unreadable_transaction_is_counted_not_applied() {
        let live = Live::new(usize::MAX);
        let mut pending = Pending::default();
        let broken = SubscribeUpdate {
            filters: vec![],
            update_oneof: Some(UpdateOneof::Transaction(
                crate::proto::SubscribeUpdateTransaction {
                    transaction: None,
                    slot: 9,
                },
            )),
        };
        assert_eq!(step(broken, &mut pending, &live), Step::Apply(vec![]));
        assert_eq!(live.tape().counts().unreadable, 1);
        assert!(pending.waiting.is_empty());
    }

    #[test]
    fn a_transaction_waits_for_its_block_time_in_either_order() {
        let mut pending = Pending::default();
        let sig = Signature::new([1; 64]);
        assert!(pending.transaction(10, sig, Decoded::default()).is_none());
        let released = pending.block_time(10, 1_700_000_000);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].2, 1_700_000_000);

        let ready = pending.transaction(10, sig, Decoded::default());
        assert_eq!(
            ready.map(|r| r.2),
            Some(1_700_000_000),
            "time already known"
        );
    }

    #[test]
    fn a_transaction_whose_block_time_never_comes_is_dropped_and_counted() {
        let mut pending = Pending::default();
        let sig = Signature::new([1; 64]);
        assert!(pending.transaction(10, sig, Decoded::default()).is_none());
        assert!(pending.transaction(11, sig, Decoded::default()).is_none());
        pending.block_time(10 + MAX_WAIT_SLOTS + 1, 1_700_000_000);
        assert_eq!(
            pending.prune(),
            1,
            "slot 10 waited too long; slot 11 is at the edge"
        );
        assert_eq!(pending.prune(), 0, "dropped once, not twice");
    }
}
