// SPDX-License-Identifier: Apache-2.0
//! Operator paper controls over private IPC. No broker keys or order endpoint.

use std::{path::Path, sync::Arc};
use axum::{Extension, Json, Router, extract::DefaultBodyLimit,
    http::{HeaderMap, StatusCode, header}, response::{IntoResponse, Response}, routing::{get, post}};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use crate::{AppState, Operator};

const VERSION: &str = "alpaca-paper-control-v1";
#[cfg(unix)]
const LIMIT: u64 = 4 * 1024 * 1024;
type Caller = dyn Fn(Value) -> Result<Value, ()> + Send + Sync;

#[derive(Clone)]
struct Bridge {
    call: Arc<Caller>,
    slots: Arc<Semaphore>,
}

pub(crate) fn routes() -> Router<Arc<AppState>> {
    routes_with(Bridge { call: Arc::new(|request| socket_call(Path::new("/run/radar-paper-control.sock"), &request)),
        slots: Arc::new(Semaphore::new(4)) })
}

fn routes_with(bridge: Bridge) -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/paper/status", get(status))
        .route("/v1/paper/preferences", post(configure))
        .route("/v1/paper/pause", post(pause))
        .layer(DefaultBodyLimit::max(65536))
        .layer(Extension(bridge))
        .layer(axum::middleware::map_response(private_response))
}

async fn private_response(mut response: Response) -> Response {
    response.headers_mut().insert(header::CACHE_CONTROL, "private, no-store".parse().expect("static header"));
    response
}

fn refusal(code: StatusCode, message: &'static str) -> Response {
    (code, Json(json!({"error":message}))).into_response()
}

fn authenticated(proof: Option<Extension<Operator>>) -> Result<crate::access::Identity, Response> {
    proof.map(|Extension(Operator(identity))| identity)
        .ok_or_else(|| refusal(StatusCode::FORBIDDEN, "Verified Cloudflare operator login required for paper controls."))
}

fn browser_write(headers: &HeaderMap) -> Result<(), Response> {
    // Fetch Metadata is browser-controlled. Together with a non-simple custom
    // header and JSON, a cross-origin page cannot use the operator's cookie.
    // These are CSRF checks, never an alternative to the verified identity.
    if headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) != Some("same-origin")
        || headers.get("x-radar-paper-control").and_then(|v| v.to_str().ok()) != Some("1") {
        return Err(refusal(StatusCode::FORBIDDEN, "Use paper controls from this site's browser page."));
    }
    Ok(())
}

async fn invoke(bridge: Bridge, request: Value) -> Result<Value, Response> {
    let Ok(permit) = Arc::clone(&bridge.slots).try_acquire_owned() else {
        return Err(refusal(StatusCode::TOO_MANY_REQUESTS, "Paper control is busy; reload status before another action."));
    };
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (bridge.call)(request)
    }).await.ok().and_then(Result::ok).ok_or_else(unresolved)
}

fn unresolved() -> Response {
    refusal(StatusCode::SERVICE_UNAVAILABLE,
        "Paper control unavailable or outcome unresolved. Reload status before another action; do not retry blindly.")
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Preferences {
    capital_usd: String,
    max_trade_usd: String,
    daily_loss_usd: String,
    daily_loss_enabled: bool,
    agent_decides: Choices,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Choices {
    capital_usd: bool,
    max_trade_usd: bool,
    daily_loss_usd: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configure {
    generation: u64,
    preferences: Preferences,
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pause { generation: u64 }

#[derive(Deserialize, Serialize)]
struct Account { id: String, cash: String, equity: String, currency: String, status: String }

#[derive(Deserialize, Serialize)]
struct Position { symbol: String, qty: String, side: String, market_value: Option<String> }

#[derive(Deserialize, Serialize)]
struct Clock { timestamp: String, is_open: bool, next_open: String, next_close: String }

#[derive(Deserialize, Serialize)]
struct Effective {
    enabled: bool,
    max_capital_microusd: u64,
    max_order_microusd: u64,
    max_daily_loss_microusd: Option<u64>,
}

#[derive(Deserialize, Serialize)]
struct Status {
    version: String,
    environment: String,
    account_id: String,
    generation: u64,
    preferences: Option<Preferences>,
    handover_enabled: bool,
    effective_policy: Option<Effective>,
    account: Account,
    positions: Option<Vec<Position>>,
    market_clock: Option<Clock>,
    execution_authority: bool,
    agent_lifecycle_verified: bool,
    profit_measured: bool,
}

async fn status(proof: Option<Extension<Operator>>, Extension(bridge): Extension<Bridge>) -> Response {
    if let Err(response) = authenticated(proof) { return response; }
    let value = match invoke(bridge, json!({"action":"status"})).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Ok(report) = serde_json::from_value::<Status>(value) else { return unresolved(); };
    if report.version != VERSION || report.environment != "paper" || report.execution_authority
        || report.agent_lifecycle_verified || report.profit_measured || report.account_id != report.account.id
        || report.account.currency != "USD" || report.account.status != "ACTIVE"
        || report.effective_policy.is_none() != (report.generation == 0)
        || (report.handover_enabled && (report.generation == 0 || report.preferences.is_none()))
        || report.effective_policy.as_ref().is_some_and(|p| p.enabled != report.handover_enabled) {
        return unresolved();
    }
    // Project only selected fields; never forward a future vendor/config field.
    Json(report).into_response()
}

async fn configure(proof: Option<Extension<Operator>>, Extension(bridge): Extension<Bridge>,
    headers: HeaderMap, Json(change): Json<Configure>) -> Response {
    let operator = match authenticated(proof) { Ok(value) => value, Err(response) => return response };
    if let Err(response) = browser_write(&headers) { return response; }
    change_call(bridge, change.generation, json!({"action":"configure", "generation":change.generation,
        "preferences":change.preferences, "enabled":change.enabled,
        "operator":{"email":operator.email,"subject":operator.subject}}), !change.enabled).await
}

async fn pause(proof: Option<Extension<Operator>>, Extension(bridge): Extension<Bridge>,
    headers: HeaderMap, Json(change): Json<Pause>) -> Response {
    let operator = match authenticated(proof) { Ok(value) => value, Err(response) => return response };
    if let Err(response) = browser_write(&headers) { return response; }
    change_call(bridge, change.generation, json!({"action":"pause", "generation":change.generation,
        "operator":{"email":operator.email,"subject":operator.subject}}), true).await
}

#[derive(Deserialize, Serialize)]
struct Changed {
    version: String, environment: String, generation: u64, handover_enabled: bool,
    execution_authority: bool, pending_orders_canceled: bool, positions_liquidated: bool,
}

async fn change_call(bridge: Bridge, generation: u64, request: Value, closing: bool) -> Response {
    let value = match invoke(bridge, request).await { Ok(value) => value, Err(response) => return response };
    let Ok(report) = serde_json::from_value::<Changed>(value) else { return unresolved(); };
    if report.version != VERSION || report.environment != "paper" || report.execution_authority
        || report.pending_orders_canceled || report.positions_liquidated
        || generation.checked_add(1) != Some(report.generation) || (closing && report.handover_enabled) {
        return unresolved();
    }
    Json(report).into_response()
}

#[cfg(unix)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope { result: Option<Value>, error: Option<String> }

fn socket_call(path: &Path, request: &Value) -> Result<Value, ()> {
    #[cfg(unix)]
    {
        use std::io::{BufRead as _, Read as _, Write as _};
        use std::time::Duration;
        // std UnixStream has no connect deadline. A full local listen backlog
        // must not strand a blocking worker or its concurrency permit forever.
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|_| ())?;
        let stream = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), tokio::net::UnixStream::connect(path)).await
        }).map_err(|_| ())?.map_err(|_| ())?;
        let mut stream = stream.into_std().map_err(|_| ())?;
        stream.set_nonblocking(false).map_err(|_| ())?;
        stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(|_| ())?;
        stream.set_read_timeout(Some(Duration::from_secs(80))).map_err(|_| ())?;
        let mut raw = serde_json::to_vec(request).map_err(|_| ())?;
        raw.push(b'\n');
        if raw.len() > 65536 { return Err(()); }
        stream.write_all(&raw).map_err(|_| ())?;
        let mut raw = Vec::new();
        std::io::BufReader::new(stream.take(LIMIT + 1)).read_until(b'\n', &mut raw).map_err(|_| ())?;
        if raw.len() as u64 > LIMIT || !raw.ends_with(b"\n") { return Err(()); }
        let packet: Envelope = serde_json::from_slice(&raw).map_err(|_| ())?;
        if packet.error.is_some() { return Err(()); }
        packet.result.ok_or(())
    }
    #[cfg(not(unix))]
    { let _ = (path, request); Err(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt as _;
    use std::sync::Mutex;
    use tower::ServiceExt as _;

    fn preferences() -> Value {
        json!({"capital_usd":"50", "max_trade_usd":"10", "daily_loss_usd":"",
            "daily_loss_enabled":false, "agent_decides":{"capital_usd":false,
                "max_trade_usd":false,"daily_loss_usd":true}})
    }

    fn report() -> Value {
        json!({"version":VERSION,"environment":"paper","account_id":"paper-account","generation":7,
            "preferences":preferences(),"handover_enabled":false,
            "effective_policy":{"enabled":false,"max_capital_microusd":50000000,
                "max_order_microusd":10000000,"max_daily_loss_microusd":null},
            "account":{"id":"paper-account","cash":"100000","equity":"100000","currency":"USD","status":"ACTIVE",
                "private_field":"private-sentinel"},"positions":[],
            "market_clock":{"timestamp":"2026-10-13T15:00:00Z","is_open":true,
                "next_open":"2026-10-14T13:30:00Z","next_close":"2026-10-13T20:00:00Z"},
            "execution_authority":false,"agent_lifecycle_verified":false,"profit_measured":false,
            "future_config":"private-sentinel"})
    }

    fn changed(request: &Value) -> Value {
        json!({"version":VERSION,"environment":"paper","generation":request["generation"].as_u64().unwrap() + 1,
            "handover_enabled":request.get("enabled").cloned().unwrap_or(json!(false)),
            "execution_authority":false,"pending_orders_canceled":false,"positions_liquidated":false})
    }

    fn router(call: Arc<Caller>, off: bool) -> (Router, &'static str, Arc<Semaphore>) {
        let (token, config, keys) = crate::access::tests::operator_fixture();
        let state = Arc::new(AppState {
            registry: radar_instruments::Registry::new(),
            store: radar_store::Reader::open(std::env::temp_dir().join("radar-paper-route-test")),
            x402:None, chat:None,
            access:if off { crate::access::Mode::Off } else { crate::access::Mode::Enforce(config) },
            keys, customer:crate::customer::Mode::Off, customer_keys:crate::customer::KeyCache::new(),
            privy:None, admission:crate::admission::Admission::Open,
            shares:crate::share::Shares::new(crate::share::Allowance::per_day(100)), customer_salt:vec![7;32],
            linker:crate::link::Linker::new(), scoreboard:crate::cache::Cache::new(), token:crate::cache::Cache::new(),
            challenges:None, market:crate::market::Market::new(), market_snapshot:crate::market::SnapshotCache::new(),
            customers:None, positions:None, trading:None, ticker:crate::ticker::Ticker::new(),
            market_ticker:crate::ticker::Ticker::new(), market_semaphore:Arc::new(Semaphore::new(4)),
            market_visitors:Arc::default(),
        });
        let slots = Arc::new(Semaphore::new(1));
        let app = routes_with(Bridge {call, slots:Arc::clone(&slots)})
            .layer(axum::middleware::from_fn_with_state(Arc::clone(&state), crate::guard))
            .with_state(state);
        (app, token, slots)
    }

    fn request(path: &str, token: Option<&str>, body: Option<Value>) -> Request<Body> {
        let mut request = Request::builder().uri(path);
        if let Some(token) = token { request = request.header("cf-access-jwt-assertion", token); }
        if let Some(body) = body {
            request.method("POST").header("content-type","application/json")
                .header("sec-fetch-site","same-origin").header("x-radar-paper-control","1")
                .body(Body::from(body.to_string())).unwrap()
        } else { request.body(Body::empty()).unwrap() }
    }

    async fn answer(app: Router, request: Request<Body>) -> (StatusCode, Value) {
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-store");
        let code = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn genuine_access_proof_reaches_status_and_private_fields_are_projected_out() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let retained = Arc::clone(&calls);
        let (app, token, _) = router(Arc::new(move |request| { retained.lock().unwrap().push(request); Ok(report()) }), false);
        let (code, value) = answer(app, request("/v1/paper/status", Some(token), None)).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(value["account"]["cash"], "100000");
        assert_eq!(value["generation"], 7);
        assert_eq!(value["agent_lifecycle_verified"], false);
        assert!(!value.to_string().contains("private-sentinel"));
        assert_eq!(*calls.lock().unwrap(), vec![json!({"action":"status"})]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn access_off_missing_and_forged_assertions_never_reach_control() {
        for off in [false, true] {
            let calls = Arc::new(Mutex::new(0));
            let retained = Arc::clone(&calls);
            let (app, token, _) = router(Arc::new(move |_| { *retained.lock().unwrap() += 1; Ok(report()) }), off);
            for assertion in [None, Some("forged"), off.then_some(token)] {
                let mut request = request("/v1/paper/status", assertion, None);
                request.headers_mut().insert("cf-access-authenticated-user-email", "owner@example.com".parse().unwrap());
                let response = app.clone().oneshot(request).await.unwrap();
                assert_eq!(response.status(), StatusCode::FORBIDDEN);
            }
            assert_eq!(*calls.lock().unwrap(), 0);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn configure_and_pause_derive_actor_from_verified_jwt_and_keep_generation() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let retained = Arc::clone(&calls);
        let (app, token, _) = router(Arc::new(move |request| {
            retained.lock().unwrap().push(request.clone()); Ok(changed(&request))
        }), false);
        for (path, body, action) in [
            ("/v1/paper/preferences", json!({"generation":7,"preferences":preferences(),"enabled":true}), "configure"),
            ("/v1/paper/pause", json!({"generation":8}), "pause"),
        ] {
            let mut request = request(path, Some(token), Some(body.clone()));
            request.headers_mut().insert("cf-access-authenticated-user-email", "forged@example.com".parse().unwrap());
            let (code, value) = answer(app.clone(), request).await;
            assert_eq!(code, StatusCode::OK);
            assert_eq!(value["generation"], body["generation"].as_u64().unwrap() + 1);
            let forwarded = calls.lock().unwrap().last().unwrap().clone();
            assert_eq!(forwarded["action"], action);
            assert_eq!(forwarded["operator"], json!({"email":"joshfair2@gmail.com","subject":"user-1"}));
            assert_eq!(forwarded["generation"], body["generation"]);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn browser_csrf_and_strict_change_schema_refuse_before_ipc() {
        let calls = Arc::new(Mutex::new(0));
        let retained = Arc::clone(&calls);
        let (app, token, _) = router(Arc::new(move |_| { *retained.lock().unwrap() += 1; Ok(report()) }), false);
        for (name, value) in [("sec-fetch-site", None), ("sec-fetch-site",Some("cross-site")),
            ("sec-fetch-site",Some("same-site")), ("x-radar-paper-control",None), ("x-radar-paper-control",Some("0"))] {
            let mut request = request("/v1/paper/pause", Some(token), Some(json!({"generation":7})));
            request.headers_mut().remove(name);
            if let Some(value) = value { request.headers_mut().insert(name, value.parse().unwrap()); }
            assert_eq!(answer(app.clone(), request).await.0, StatusCode::FORBIDDEN);
        }
        for body in [json!({"generation":true}), json!({"generation":-1}),
            json!({"generation":7,"operator":{"email":"forged"}}), json!({"generation":7,"action":"submit"})] {
            let response = app.clone().oneshot(request("/v1/paper/pause", Some(token), Some(body))).await.unwrap();
            assert!(response.status().is_client_error());
        }
        let mut body = json!({"generation":7,"enabled":false,"preferences":preferences()});
        body["preferences"]["agent_decides"]["other"] = json!(true);
        assert!(answer(app, request("/v1/paper/preferences", Some(token), Some(body))).await.0.is_client_error());
        assert_eq!(*calls.lock().unwrap(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unavailable_or_live_or_inconsistent_status_is_never_a_zero_balance_or_success() {
        let mut wrong = vec![Value::Null];
        for (field, value) in [("environment",json!("live")), ("version",json!("wrong")),
            ("execution_authority",json!(true)), ("agent_lifecycle_verified",json!(true)),
            ("profit_measured",json!(true)), ("account_id",json!("other")), ("handover_enabled",json!(true))] {
            let mut report = report(); report[field] = value; wrong.push(report);
        }
        let mut missing = report(); missing.as_object_mut().unwrap().remove("account"); wrong.push(missing);
        for (field, value) in [("currency", "EUR"), ("status", "SUSPENDED")] {
            let mut value_report = report(); value_report["account"][field] = json!(value); wrong.push(value_report);
        }
        for field in ["generation", "preferences"] {
            let mut value = report(); value["handover_enabled"] = json!(true);
            value["effective_policy"]["enabled"] = json!(true);
            value[field] = if field == "generation" { json!(0) } else { Value::Null }; wrong.push(value);
        }
        let mut missing = report(); missing["effective_policy"] = Value::Null; wrong.push(missing);
        for value in wrong {
            let (app, token, _) = router(Arc::new(move |_| Ok(value.clone())), false);
            let (code, body) = answer(app, request("/v1/paper/status", Some(token), None)).await;
            assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE);
            assert!(body.get("account").is_none());
        }
        let (app, token, _) = router(Arc::new(|_| Err(())), false);
        assert_eq!(answer(app, request("/v1/paper/status", Some(token), None)).await.0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn invalid_change_acknowledgements_and_busy_or_oversized_requests_are_not_retried() {
        for field in ["execution_authority", "pending_orders_canceled", "positions_liquidated", "handover_enabled", "generation", "version", "environment"] {
            let (app, token, _) = router(Arc::new(move |request| {
                let mut reply = changed(&request);
                reply[field] = match field { "generation" => json!(7), "version" | "environment" => json!("wrong"), _ => json!(true) }; Ok(reply)
            }), false);
            assert_eq!(answer(app, request("/v1/paper/pause", Some(token), Some(json!({"generation":7})))).await.0,
                StatusCode::SERVICE_UNAVAILABLE);
        }
        let calls = Arc::new(Mutex::new(0)); let retained = Arc::clone(&calls);
        let (app, token, slots) = router(Arc::new(move |_| { *retained.lock().unwrap() += 1; Ok(report()) }), false);
        let permit = slots.acquire().await.unwrap();
        assert_eq!(answer(app.clone(), request("/v1/paper/status", Some(token), None)).await.0, StatusCode::TOO_MANY_REQUESTS);
        drop(permit);
        let response = app.oneshot(request("/v1/paper/pause", Some(token), Some(json!({"generation":7,"large":"x".repeat(65536)})))).await.unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(*calls.lock().unwrap(), 0);
    }

    #[test]
    fn actual_private_socket_requires_bounded_complete_envelopes_and_never_exposes_errors() {
        #[cfg(unix)]
        {
            use std::io::{BufRead as _, Write as _};
            use std::os::unix::net::UnixListener;
            let prefix = b"{\"result\":{\"padding\":\"";
            let suffix = b"\"}}\n";
            let mut oversized = prefix.to_vec();
            oversized.extend(vec![b'x'; usize::try_from(LIMIT + 1).unwrap() - prefix.len() - suffix.len()]);
            oversized.extend(suffix);
            for (raw, accepted) in [(b"{\"result\":{\"ok\":true}}\n".to_vec(),true),
                (b"{\"result\":{\"ok\":true}}".to_vec(),false),
                (b"{\"error\":\"private-sentinel\"}\n".to_vec(),false),
                (b"{\"result\":{},\"error\":\"private-sentinel\"}\n".to_vec(),false),
                (b"{\"other\":true}\n".to_vec(),false), (b"invalid\n".to_vec(),false),
                (oversized,false)] {
                let temporary = tempfile::tempdir().unwrap(); let path = temporary.path().join("control.sock");
                let listener = UnixListener::bind(&path).unwrap();
                let server = std::thread::spawn(move || {
                    let (mut stream, _) = listener.accept().unwrap();
                    let mut line = String::new();
                    std::io::BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap();
                    assert_eq!(line, "{\"action\":\"status\"}\n");
                    let _ = stream.write_all(&raw);
                });
                let reply = socket_call(&path, &json!({"action":"status"}));
                assert_eq!(reply.is_ok(), accepted);
                if accepted { assert_eq!(reply.unwrap(), json!({"ok":true})); }
                server.join().unwrap();
            }
            assert!(socket_call(Path::new("/radar-test-missing.sock"), &json!({"action":"status"})).is_err());
        }
        #[cfg(not(unix))]
        assert!(socket_call(Path::new("unused"), &json!({"action":"status"})).is_err());
    }
}
