// SPDX-License-Identifier: Apache-2.0
//! The Radar server.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use radar_instruments::{CreatorHistory, CreatorTrackRecord, Registry, SimulateExit};
use radar_serve::chat::Chat;
use radar_serve::{AppState, access, app, chat, customer, market, x402};
use radar_store::Reader;

/// Every instrument Radar exposes. The CLI builds the same list.
fn registry() -> Registry {
    let mut r = Registry::new();
    r.register(CreatorHistory);
    r.register(CreatorTrackRecord);
    r.register(SimulateExit::default());
    r
}

/// Builds the agent from the environment, and says what happened either way.
///
/// Rule 8, and the reporting half is the point. An unconfigured agent and a
/// *misconfigured* one both end up as `None`, and they need different responses
/// from whoever is reading the startup log: one is the shipped state, the other
/// is somebody who set four variables out of five and will otherwise conclude
/// the feature does not work.
fn configure_agent() -> (Option<Chat>, String) {
    let Some(budget) = radar_model::budget_from_vars(&|k| std::env::var(k).ok()) else {
        return (
            None,
            "off (no RADAR_MODEL_DAILY_USD; a model with no budget spends without a ceiling)"
                .to_owned(),
        );
    };

    // Rule 8, and this one is not decoration. A meter that cannot record what it
    // spent cannot enforce a ceiling across a restart, so an agent with no
    // durable ledger is an unmetered spender wearing a meter's clothes -- which
    // is the state this ran in until the ledger was wired, because
    // `Agent::restore` had one caller and it was a unit test.
    let ledger = match radar_serve::ledger::Store::open(&|k| std::env::var(k).ok()) {
        Ok(ledger) => ledger,
        Err(why) => return (None, format!("off ({why})")),
    };

    // Built separately from the boxed provider so the route knows whether there
    // is a credential to link *before* somebody presses the button, rather than
    // discovering it from a failure.
    let linkable = radar_model::codex_from_vars(&|k| std::env::var(k).ok());

    match radar_model::from_vars(&|k| std::env::var(k).ok()) {
        Ok(provider) => {
            let name = provider.name();
            let mut allowlist = radar_agent::Allowlist::new();
            // The read-only instrument registry, and nothing else. Every one of
            // these receives a `&Reader` and structurally cannot write.
            for instrument in registry().iter() {
                allowlist.allow(instrument.spec().name);
            }
            let tools = allowlist.len();
            let today = chat::today_utc();
            let config = radar_agent::Config { budget, allowlist };
            // Restored rather than reset. A ledger from an earlier day is not
            // carried forward by `Meter::restore` -- the budget is daily -- so
            // this is safe to do unconditionally and does the right thing on the
            // first start of a new day.
            let agent = ledger
                .read::<radar_agent::Ledger>(chat::LEDGER_RECORD)
                .map_or_else(
                    || radar_agent::Agent::new(config.clone(), today),
                    |saved| radar_agent::Agent::restore(config.clone(), &saved, today),
                );
            (
                Some(Chat {
                    agent: std::sync::Mutex::new(agent),
                    ledger,
                    provider,
                    linkable,
                    last: std::sync::Mutex::new(radar_serve::chat::LastCall::Never),
                }),
                // Integer arithmetic, because a startup line reporting the
                // ceiling as `$2.00` when it is `$2.004` is a line an operator
                // would reasonably quote back later.
                format!(
                    "on via {name}, {tools} read-only tool(s), ${}.{:06}/day",
                    budget.daily_max.get() / 1_000_000,
                    budget.daily_max.get() % 1_000_000
                ),
            )
        }
        // Printed rather than swallowed. A misconfiguration that produces
        // silence is one an operator debugs by reading source.
        Err(why) => (None, format!("off — {why}")),
    }
}

/// The three settings that govern the customer lane.
///
/// Extracted from `main` because it grew past what one function should hold, and
/// because these three belong together: they are the whole of what decides
/// whether a stranger can reach this instance and what they can spend on it.
///
/// Every one of them fails closed and none of them defaults quietly. A malformed
/// value stops the server rather than resolving to "closed", because the two are
/// indistinguishable once collapsed and an operator would spend the outage
/// looking at the vendor.
///
/// # Errors
///
/// Returns the message to print when any of them cannot be read.
fn customer_lane() -> Result<
    (
        radar_serve::admission::Admission,
        radar_serve::share::Shares,
        Vec<u8>,
        radar_serve::tenant::Customers,
    ),
    String,
> {
    let env = |k: &str| std::env::var(k).ok();
    let admission = radar_serve::admission::Admission::from_vars(&env)?;
    let allowance = radar_serve::share::Allowance::from_vars(&env)?;
    // The same state directory the model ledger uses, and mandatory for the same
    // reason: a meter that cannot record what it spent cannot enforce a ceiling
    // across a restart, and deploys are routine.
    let shares_store = radar_serve::ledger::Store::open(&env)
        .map_err(|why| format!("the chat share meter needs a state directory: {why}"))?;
    // The salt customer identifiers are hashed with before anything long lived
    // records them. Empty when unset, which `Subject::derive` refuses -- so an
    // unsalted instance cannot meter a customer and therefore will not spend on
    // one. Rule 8, and it is why this is not fatal: the operator surface does
    // not need it.
    let salt = env("RADAR_CUSTOMER_SALT")
        .map(String::into_bytes)
        .unwrap_or_default();
    // Each signed-in wallet's own folder, under the same state directory. Not
    // optional either: a watchlist kept nowhere would accept a coin and lose it
    // at the next deploy.
    let customers = radar_serve::tenant::Customers::open(&env)
        .map_err(|why| format!("wallet watchlists need a state directory: {why}"))?;
    Ok((
        admission,
        radar_serve::share::Shares::restored(
            allowance,
            shares_store,
            radar_serve::chat::today_utc(),
        ),
        salt,
        customers,
    ))
}

/// The state behind `/v1/customer/positions`, and the host it reads from for
/// the startup log.
///
/// `RpcClient::from_vars` never fails to construct: `RADAR_RPC` unset falls
/// back to the public node, which is a slower default rather than an unsafe
/// one. So this instance always has positions configured -- `None` on
/// [`AppState`] is reachable only in a test fixture that chooses not to wire
/// it.
fn positions_lane() -> (radar_serve::positions::Positions, String) {
    let positions =
        radar_serve::positions::Positions::new(radar_onchain::rpc::RpcClient::from_vars(&|k| {
            std::env::var(k).ok()
        }));
    let note = positions.host();
    (positions, note)
}

/// Whether this instance builds or prices swaps at all.
///
/// Unlike [`positions_lane`], `None` here *is* the shipped state: `RADAR_TRADE`
/// defaults off (plan 0013 Phase D, ADR 0024, AGENTS rule 8), and pricing a
/// swap needs a Jupiter credential that has no safe fallback the way a public
/// RPC node is a safe fallback for reading balances. `RADAR_TRADE=on` with no
/// credential refuses to **start**, rather than mounting both routes to answer
/// `trading_off` for a reason that is actually a misconfiguration.
///
/// # Errors
///
/// The message to print and refuse startup with.
fn trading_lane() -> Result<(Option<radar_serve::trade::Trading>, String), String> {
    let rpc = radar_onchain::rpc::RpcClient::from_vars(&|k| std::env::var(k).ok());
    let get = |k: &str| std::env::var(k).ok();
    trading_note(radar_serve::trade::from_vars(&get, rpc))
}

/// Turns [`radar_serve::trade::from_vars`]'s decision into the startup status
/// line, kept separate from [`trading_lane`] so this mapping is testable
/// without touching the environment: setting an environment variable from a
/// test is process-global and this workspace runs tests in parallel threads,
/// and `std::env::set_var` is `unsafe` in edition 2024 while this workspace
/// forbids `unsafe_code` -- the same reasoning `Router::from_env`'s own
/// exclusion in `.cargo/mutants.toml` documents for the same shape of
/// problem. Generic over `T` so a test can call this with a plain value
/// instead of a real `Trading`, which needs a live [`radar_exec::route::Router`]
/// to construct.
fn trading_note<T>(result: Result<Option<T>, String>) -> Result<(Option<T>, String), String> {
    match result {
        Ok(Some(trading)) => Ok((Some(trading), "on".to_owned())),
        Ok(None) => Ok((
            None,
            "off — set RADAR_TRADE=on to build and price swaps".to_owned(),
        )),
        Err(why) => Err(why),
    }
}

/// The startup line for the access mode, said plainly every start: an
/// instance serving operational detail to anyone who can reach it should say
/// so in its own logs.
fn access_note(mode: &access::Mode) -> String {
    match mode {
        access::Mode::Enforce(config) => format!("verifying {} tokens", config.team_domain),
        access::Mode::Off => "OFF — anyone who can reach this can read it".to_owned(),
    }
}

/// The startup line for the customer lane. Not a warning: no customer lane
/// means customer routes require operator identity, which is stricter than
/// they will be -- but it is said every start so nobody has to guess which
/// state this is.
fn customer_note(mode: &customer::Mode) -> String {
    match mode {
        customer::Mode::Enforce(config) => {
            format!("verifying Privy tokens for app {}", config.app_id)
        }
        customer::Mode::Off => "off — customer routes require operator identity".to_owned(),
    }
}

/// The market routes' source: the live feed when one is configured, else the store.
///
/// Off unless `RADAR_STREAM_ENDPOINT` is set, and a malformed setting refuses to
/// start rather than quietly serving the store: an operator who configured a
/// paid feed and got the five-minute collector instead would see a working
/// screen and never learn why it lags. Spawns the feed onto the running
/// runtime, so it must be called from inside `main`.
///
/// # Errors
///
/// The message to print when the feed's configuration cannot be read.
fn market_feed(
    env: &impl Fn(&str) -> Option<String>,
) -> Result<(radar_serve::market::Market, String), String> {
    let Some(config) = radar_stream::feed::Config::from_vars(env)? else {
        return Ok((
            radar_serve::market::Market::new(),
            "off (set RADAR_STREAM_ENDPOINT to stream; market routes read the store)".to_owned(),
        ));
    };
    let budget = radar_stream::budget_from_vars(env)?;
    let live = Arc::new(radar_stream::Live::new(budget));
    let note = format!(
        "streaming from {} ({} programs, token {}, {} MiB budget)",
        config.endpoint,
        config.programs.len(),
        if config.token.is_some() {
            "set"
        } else {
            "not set"
        },
        budget / (1024 * 1024),
    );
    tokio::spawn(radar_stream::feed::run(config, Arc::clone(&live)));
    Ok((radar_serve::market::Market::with_live(live), note))
}

/// The address to listen on, or why the configured one cannot be used.
///
/// # A typo does not become a different address
///
/// This used to fall back to `127.0.0.1:8080` on anything it could not parse, so
/// `RADAR_BIND=127.0.0.1;8402` started a server on a port nothing was pointed
/// at. The process is up, the unit is green, `systemctl status` says running --
/// and Caddy answers 502 for a reason nothing on the box reports. An operator
/// debugging that reads the unit, the env file and the logs, every one of which
/// names the right port.
///
/// Rule 8's shape: a configuration value that cannot be read is a refusal to
/// start, not a guess. The default when the variable is **absent** stays, since
/// absence is a state with an obvious right answer and a typo is not.
///
/// # Errors
///
/// A message naming the value, for the operator who has to find the typo.
fn bind_address(configured: Option<&str>) -> Result<SocketAddr, String> {
    let Some(value) = configured else {
        return Ok(SocketAddr::from(([127, 0, 0, 1], 8080)));
    };
    value.parse().map_err(|e| {
        format!(
            "RADAR_BIND={value:?} is not an address ({e}); refusing to start on a port              nobody asked for"
        )
    })
}

/// Prints why the server will not start, and the status that says so.
///
/// Every refusal here reads the same way on purpose: one line on stderr, named
/// process first, and a non-zero exit so systemd records a failure rather than a
/// clean stop. Written once because there are several of them and a refusal that
/// looked different from its neighbours would read as a different kind of event.
fn refused(why: &str) -> ExitCode {
    eprintln!("radar-serve: {why}");
    ExitCode::FAILURE
}

/// How often the market snapshot's trade half is checked for staleness.
///
/// Chosen to bound how far a request's answer can lag the tape without
/// costing much: a tick that finds nothing changed is one directory listing
/// (`Reader::files`), not a rebuild.
const SNAPSHOT_REFRESH_INTERVAL: Duration = Duration::from_secs(20);

/// How often the launch index -- the expensive half, a 7-day scan of
/// `Launches` -- is rebuilt, independent of the trade half's own cadence.
const LAUNCH_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

/// Starts the task that keeps `state.market_snapshot` current, off the
/// request path.
///
/// See [`market::Snapshot`]'s own doc comment for why this exists. Runs for
/// the life of the process; there is nothing to await here because nothing
/// in `main` depends on the first snapshot existing before it starts serving
/// -- a request that arrives first sees [`market::SnapshotCache::peek`]
/// return `None` and answers the honest "not built yet" response, exactly as
/// the packet requires.
fn market_snapshot_refresher(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut refresher = market::Refresher::new();
        // An interval's first tick is immediate: a restart builds its first
        // snapshot at once rather than answering "not built yet" for 20s.
        let mut tick = tokio::time::interval(SNAPSHOT_REFRESH_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let task_state = Arc::clone(&state);
            // The store read and the fold both belong off the async runtime --
            // exactly the reason this task exists rather than a per-request read.
            let done = tokio::task::spawn_blocking(move || {
                refresher.tick(
                    &task_state.store,
                    &task_state.market_snapshot,
                    LAUNCH_REFRESH_INTERVAL,
                );
                refresher
            })
            .await;
            // A panicked tick took its memory with it; start over rather than
            // stop refreshing for the life of the process.
            refresher = done.unwrap_or_default();
        }
    });
}

/// Starts the task that keeps `state.ticker` current, off the request path.
///
/// See [`radar_serve::ticker`]'s own doc comment for why this exists: one read
/// on `radar_serve::POLL`'s interval, published once, so every open
/// `/v1/events`, `/v1/customer/events` and `/v1/market/events` connection
/// reads the channel instead of the store. Mirrors
/// [`market_snapshot_refresher`]'s shape -- an immediate first tick, the read
/// off the async runtime via `spawn_blocking`, and a panicked tick simply
/// tried again next interval rather than stopping the task.
fn ticker_refresher(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(radar_serve::POLL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let task_state = Arc::clone(&state);
            let read = tokio::task::spawn_blocking(move || radar_serve::Tick::read(&task_state))
                .await
                .unwrap_or(None);
            if let Some(value) = read {
                state.ticker.publish(value);
            }
        }
    });
}

/// Builds the shared [`AppState`], out of `main`'s own body so its startup
/// sequence -- validate, then construct, then spawn the background readers --
/// reads as a sequence of steps rather than one function long enough to hide
/// among them. Every argument here is something `main` already produced by
/// the time it calls this; nothing here reaches back into the environment
/// itself.
#[allow(clippy::too_many_arguments)]
fn build_state(
    store_dir: &str,
    access: access::Mode,
    customer: customer::Mode,
    privy: Option<radar_serve::privy::Client>,
    x402: Option<x402::Config>,
    agent: Option<Chat>,
    admission: radar_serve::admission::Admission,
    shares: radar_serve::share::Shares,
    customer_salt: Vec<u8>,
    market: radar_serve::market::Market,
    customers: radar_serve::tenant::Customers,
    positions: radar_serve::positions::Positions,
    trading: Option<radar_serve::trade::Trading>,
) -> Arc<AppState> {
    Arc::new(AppState {
        admission,
        shares,
        customer_salt,
        registry: registry(),
        store: Reader::open(store_dir),
        x402,
        chat: agent,
        access,
        keys: access::KeyCache::new(),
        customer,
        customer_keys: customer::KeyCache::new(),
        privy,
        linker: radar_serve::link::Linker::new(),
        scoreboard: radar_serve::cache::Cache::new(),
        token: radar_serve::cache::Cache::new(),
        // The domain a sign-in is bound to. Unset means no customer sign-in,
        // rather than a guess: a wrong domain here would have wallets sign a
        // message naming a site this is not, and the signature would then be
        // valid somewhere Radar does not control.
        challenges: radar_serve::siws::domain_from(std::env::var("RADAR_CUSTOMER_DOMAIN").ok())
            .map(radar_serve::challenges::Challenges::new),
        market,
        market_snapshot: radar_serve::market::SnapshotCache::with_background_refresh(),
        customers: Some(customers),
        positions: Some(positions),
        trading,
        ticker: radar_serve::ticker::Ticker::new(),
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let store_dir = std::env::var("RADAR_STORE").unwrap_or_else(|_| "./data/store".to_owned());
    let bind = match bind_address(std::env::var("RADAR_BIND").ok().as_deref()) {
        Ok(address) => address,
        Err(why) => return refused(&why),
    };

    // Before anything binds a socket. A server that starts and then discovers
    // it does not know who may look has already answered a request by then.
    let access = match access::Mode::from_vars(&|k| std::env::var(k).ok()) {
        Ok(mode) => mode,
        Err(why) => return refused(&why),
    };

    // Same reasoning, one step weaker. An absent Privy app id is not a
    // contradiction the way an absent Access configuration is -- it means there
    // is no customer lane, and customer routes then require operator identity.
    // A *malformed* one still stops the server, because the failure it produces
    // is indistinguishable from a vendor outage.
    let customer = match customer::Mode::from_vars(&|k| std::env::var(k).ok()) {
        Ok(mode) => mode,
        Err(why) => return refused(&why),
    };

    // Optional, and its absence is reported rather than fatal. An instance
    // without a Privy credential cannot look wallets up, which is not the same
    // as its customers having no wallets -- and it must not stop the operator
    // surface, which is what this process is mostly for today.
    let privy = radar_serve::privy::Credentials::from_vars(&|k| std::env::var(k).ok()).ok();
    let privy_note = privy.as_ref().map_or_else(
        || "off (no RADAR_PRIVY_APP_SECRET; wallets cannot be read)".to_owned(),
        |c| format!("on for application {}", c.app_id()),
    );
    let privy = privy.map(radar_serve::privy::Client::new);

    let x402 = x402::Config::from_env();
    let (agent, agent_note) = configure_agent();

    let (admission, shares, customer_salt, customers) = match customer_lane() {
        Ok(lane) => lane,
        Err(why) => return refused(&why),
    };
    let admission_note = admission.describe();
    let share_note = shares.describe();

    let (market, feed_note) = match market_feed(&|k| std::env::var(k).ok()) {
        Ok(feed) => feed,
        Err(why) => return refused(&why),
    };
    let (positions, positions_note) = positions_lane();
    let (trading, trading_note) = match trading_lane() {
        Ok(lane) => lane,
        Err(why) => return refused(&why),
    };
    let state = build_state(
        &store_dir,
        access.clone(),
        customer.clone(),
        privy,
        x402,
        agent,
        admission,
        shares,
        customer_salt,
        market,
        customers,
        positions,
        trading,
    );

    // Off the request path, per the market module's own doc comment: every
    // route under `/v1/market/` used to read the store directly, up to twice
    // per request, and two such requests in flight pinned a 2-core box.
    // `state.market.live()` (the streamed feed) is untouched -- it never read
    // the store this way and this task never overwrites it.
    market_snapshot_refresher(Arc::clone(&state));

    // The one background reader behind every SSE stream; see its own doc comment.
    ticker_refresher(Arc::clone(&state));

    println!("radar-serve v{}", env!("CARGO_PKG_VERSION"));
    println!("  store      : {store_dir}");
    println!("  instruments: {}", state.registry.len());
    println!(
        "  paid surface: {}",
        if state.x402.is_some() {
            "on"
        } else {
            "off (set RADAR_X402_PAY_TO and RADAR_X402_FACILITATOR to enable)"
        }
    );
    println!("  access     : {}", access_note(&access));
    println!("  admission  : {admission_note}");
    println!("  chat share : {share_note}");
    println!("  customers  : {}", customer_note(&customer));
    // Separate from the line above, because the two can disagree and the
    // disagreement is the interesting state: an instance that verifies customer
    // tokens but cannot read their wallets will sign people in and then fail
    // every wallet lookup, and an operator should see that at start rather than
    // from a support message.
    println!("  wallets    : {privy_note}");
    println!("  positions  : reading balances from {positions_note}");
    println!("  trading    : {trading_note}");
    println!("  agent      : {agent_note}");
    println!("  market feed: {feed_note}");
    println!("  listening  : http://{bind}");

    let listener = match tokio::net::TcpListener::bind(bind).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot bind {bind}: {e}");
            return ExitCode::FAILURE;
        }
    };

    // `into_make_service_with_connect_info` rather than the plain service: the
    // per-visitor rate limit on `/v1/market/quote` falls back to the
    // connection's own peer address when `CF-Connecting-IP` is absent (see
    // `trade`'s module doc comment), and that fallback only exists as
    // `ConnectInfo<SocketAddr>` when the server is told to record it.
    if let Err(e) = axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    {
        eprintln!("server stopped: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::{bind_address, market_feed, trading_note};

    fn vars(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        let owned = pairs.to_vec();
        move |k| {
            owned
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[tokio::test]
    async fn no_stream_endpoint_reads_the_store_and_says_so() {
        let (market, note) = market_feed(&vars(&[])).expect("no feed is a valid state");
        assert!(market.live().is_none());
        assert!(note.starts_with("off"), "{note}");
    }

    #[tokio::test]
    async fn a_stream_endpoint_reads_the_feed_and_never_prints_the_token() {
        let (market, note) = market_feed(&vars(&[
            ("RADAR_STREAM_ENDPOINT", "https://127.0.0.1:9"),
            ("RADAR_STREAM_TOKEN", "very-secret-token"),
            ("RADAR_STREAM_MEMORY_MB", "64"),
        ]))
        .expect("a valid feed configuration");
        assert!(market.live().is_some());
        assert!(note.contains("https://127.0.0.1:9"), "{note}");
        assert!(note.contains("64 MiB"), "{note}");
        assert!(note.contains("token set"), "{note}");
        assert!(!note.contains("very-secret-token"), "{note}");
    }

    #[tokio::test]
    async fn a_malformed_stream_setting_refuses_to_start() {
        assert!(market_feed(&vars(&[("RADAR_STREAM_ENDPOINT", "grpc.example.com")])).is_err());
        assert!(
            market_feed(&vars(&[
                ("RADAR_STREAM_ENDPOINT", "https://grpc.example.com"),
                ("RADAR_STREAM_MEMORY_MB", "0"),
            ]))
            .is_err()
        );
    }

    #[test]
    fn an_absent_variable_uses_the_documented_default() {
        // Absence is a state with an obvious right answer, and it is the state
        // a workstation runs in.
        let bound = bind_address(None).expect("a default");
        assert_eq!(bound.to_string(), "127.0.0.1:8080");
    }

    #[test]
    fn a_configured_address_is_used_as_written() {
        assert_eq!(
            bind_address(Some("127.0.0.1:8402"))
                .expect("an address")
                .to_string(),
            "127.0.0.1:8402"
        );
    }

    #[test]
    fn a_typo_refuses_to_start_rather_than_binding_somewhere_else() {
        // The failure this replaces: a semicolon for a colon started a healthy
        // server on port 8080, which nothing was pointed at. `systemctl status`
        // said running, the unit and the env file both named 8402, and Caddy
        // answered 502 for a reason nothing on the box reported.
        //
        // Re-apply by restoring `.unwrap_or_else(|_| SocketAddr::from(...))`:
        // every one of these becomes the default and this test fails four times.
        for wrong in ["127.0.0.1;8402", "8402", "localhost:8402", ""] {
            let why = bind_address(Some(wrong)).expect_err("not an address");
            assert!(
                why.contains(wrong),
                "the message must name the value: {why}"
            );
        }
    }

    #[test]
    fn trading_note_on_reports_on_and_keeps_the_value() {
        let (value, note) = trading_note(Ok(Some(7_u8))).expect("ok");
        assert_eq!(value, Some(7));
        assert_eq!(note, "on");
    }

    #[test]
    fn trading_note_off_explains_how_to_turn_it_on() {
        let (value, note): (Option<u8>, _) = trading_note(Ok(None)).expect("ok");
        assert_eq!(value, None);
        assert_eq!(note, "off — set RADAR_TRADE=on to build and price swaps");
    }

    #[test]
    fn trading_note_propagates_the_refusal_message() {
        let result: Result<(Option<u8>, String), String> = trading_note(Err("boom".to_owned()));
        assert_eq!(result, Err("boom".to_owned()));
    }
}
