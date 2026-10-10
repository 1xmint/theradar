// SPDX-License-Identifier: Apache-2.0
//! Private owner wallet reads and draft preferences. No execution authority.

use crate::{AppState, customer, privy};
use axum::{
    Extension, Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use radar_types::Address;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub(crate) fn routes() -> Router<Arc<AppState>> {
    routes_with_store(configured_store(
        std::env::var(crate::ledger::STATE_DIR).ok(),
    ))
}

fn configured_store(dir: Option<String>) -> Option<Arc<Store>> {
    dir.filter(|dir| !dir.trim().is_empty())
        .and_then(|dir| Store::at(PathBuf::from(dir)).ok())
        .map(Arc::new)
}

fn routes_with_store(store: Option<Arc<Store>>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/automation/wallet", get(wallet))
        .route("/automation/balance", get(balance))
        .route("/automation/limits", get(limits).post(save))
        .layer(Extension(store))
        .layer(axum::middleware::map_response(private_response))
}

async fn private_response(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "private, no-store".parse().expect("static header"),
    );
    response
}

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"error": message}))).into_response()
}

struct OwnerRefusal(StatusCode, &'static str);

impl IntoResponse for OwnerRefusal {
    fn into_response(self) -> Response {
        refuse(self.0, self.1)
    }
}

fn owner_refuse(status: StatusCode, message: &'static str) -> OwnerRefusal {
    OwnerRefusal(status, message)
}

/// Constructible only after a genuine JWT and authoritative wallet lookup.
pub(crate) struct OwnerWallet {
    did: String,
    wallet: privy::Wallet,
    address: Address,
}

impl OwnerWallet {
    pub(crate) const fn address(&self) -> Address {
        self.address
    }
    fn key(&self) -> String {
        let identity = serde_json::to_vec(&(&self.did, &self.wallet.id, &self.wallet.address))
            .expect("strings serialize");
        let digest = ring::digest::digest(&ring::digest::SHA256, &identity);
        let hash = radar_types::b64::encode(digest.as_ref())
            .replace('/', "_")
            .replace('+', "-");
        format!("owner-preferences-{hash}")
    }
}

async fn owner(
    state: &Arc<AppState>,
    headers: &HeaderMap,
) -> Result<Option<OwnerWallet>, OwnerRefusal> {
    let token = customer::token_from(headers)
        .ok_or_else(|| owner_refuse(StatusCode::UNAUTHORIZED, "Sign in to Privy first."))?;
    let config = state
        .customer
        .config()
        .ok_or_else(|| owner_refuse(StatusCode::SERVICE_UNAVAILABLE, "Privy is not configured."))?
        .clone();
    let state_for_keys = Arc::clone(state);
    let keys = tokio::task::spawn_blocking(move || state_for_keys.customer_keys.get(&config))
        .await
        .map_err(|_| {
            owner_refuse(
                StatusCode::SERVICE_UNAVAILABLE,
                "Could not verify Privy identity.",
            )
        })?
        .map_err(|_| {
            owner_refuse(
                StatusCode::SERVICE_UNAVAILABLE,
                "Could not verify Privy identity.",
            )
        })?;
    let verified = customer::verify(
        &token,
        &keys,
        state.customer.config().expect("configured above"),
        crate::now_unix(),
    )
    .map_err(|_| {
        owner_refuse(
            StatusCode::UNAUTHORIZED,
            "Privy session is invalid or expired. Sign in again.",
        )
    })?;
    let did = verified.did;
    let lookup_did = did.clone();
    let state_for_wallet = Arc::clone(state);
    let looked_up = tokio::task::spawn_blocking(move || {
        state_for_wallet
            .privy
            .as_ref()
            .ok_or(privy::Unavailable::NotConfigured)?
            .wallet_for(&lookup_did)
    })
    .await
    .map_err(|_| {
        owner_refuse(
            StatusCode::BAD_GATEWAY,
            "Could not verify wallet ownership.",
        )
    })?;
    match looked_up {
        Ok(wallet) => {
            let address = wallet.address.parse().map_err(|_| {
                owner_refuse(
                    StatusCode::BAD_GATEWAY,
                    "Privy returned an unreadable wallet address.",
                )
            })?;
            Ok(Some(OwnerWallet {
                did,
                wallet,
                address,
            }))
        }
        Err(privy::Unavailable::NoWallet) => Ok(None),
        Err(_) => Err(owner_refuse(
            StatusCode::BAD_GATEWAY,
            "Could not verify wallet ownership. Balance is unknown.",
        )),
    }
}

async fn wallet(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    match owner(&state, &headers).await {
        Ok(found) => {
            Json(json!({"wallet": found.map(|owner| owner.wallet), "execution_enabled": false}))
                .into_response()
        }
        Err(response) => response.into_response(),
    }
}

async fn balance(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    match owner(&state, &headers).await {
        Ok(Some(owner)) => crate::positions::get_for_owner(state, &owner).await,
        Ok(None) => refuse(
            StatusCode::CONFLICT,
            "Create your embedded Solana wallet first.",
        ),
        Err(response) => response.into_response(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preferences {
    capital_usd: String,
    max_trade_usd: String,
    daily_loss_usd: String,
    autonomous_requested: bool,
    #[serde(default)]
    agent_decides: AgentDecisions,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct AgentDecisions {
    capital_usd: bool,
    max_trade_usd: bool,
    daily_loss_usd: bool,
}

fn manual_amount(input: &str, agent_decides: bool) -> Result<Option<u64>, ()> {
    if agent_decides {
        Ok(None)
    } else {
        micro_usd(input).map(Some).ok_or(())
    }
}

// Fixed decimal arithmetic: no float rounding, exponent or non-finite input.
fn micro_usd(input: &str) -> Option<u64> {
    let (whole, fraction) = input.split_once('.').unwrap_or((input, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 6
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let major = whole.parse::<u64>().ok()?.checked_mul(1_000_000)?;
    let minor = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>().ok()? * 10_u64.pow(6 - u32::try_from(fraction.len()).ok()?)
    };
    major.checked_add(minor).filter(|amount| *amount > 0)
}

impl Preferences {
    fn validate(&self) -> bool {
        match (
            manual_amount(&self.capital_usd, self.agent_decides.capital_usd),
            manual_amount(&self.max_trade_usd, self.agent_decides.max_trade_usd),
            manual_amount(&self.daily_loss_usd, self.agent_decides.daily_loss_usd),
        ) {
            (Ok(capital), Ok(trade), Ok(loss)) => capital.is_none_or(|capital| {
                trade.is_none_or(|trade| trade <= capital)
                    && loss.is_none_or(|loss| loss <= capital)
            }),
            _ => false,
        }
    }
}

struct Store {
    dir: PathBuf,
    writes: Mutex<()>,
}
impl Store {
    fn at(dir: PathBuf) -> Result<Self, crate::ledger::Unusable> {
        crate::ledger::Store::at(&dir)?;
        Ok(Self {
            dir,
            writes: Mutex::new(()),
        })
    }
    fn read(&self, owner: &OwnerWallet) -> Result<Option<Preferences>, ()> {
        match std::fs::read(self.dir.join(format!("{}.json", owner.key()))) {
            Ok(raw) => {
                let preferences: Preferences = serde_json::from_slice(&raw).map_err(|_| ())?;
                if !preferences.validate() {
                    return Err(());
                }
                Ok(Some(preferences))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(()),
        }
    }
    fn write(&self, owner: &OwnerWallet, preferences: &Preferences) -> Result<(), ()> {
        let _lock = self.writes.lock().map_err(|_| ())?;
        crate::ledger::Store::at(&self.dir)
            .map_err(|_| ())?
            .write(&owner.key(), preferences)
            .map_err(|_| ())
    }
}

async fn limits(
    State(state): State<Arc<AppState>>,
    Extension(store): Extension<Option<Arc<Store>>>,
    headers: HeaderMap,
) -> Response {
    let owner = match owner(&state, &headers).await {
        Ok(Some(owner)) => owner,
        Ok(None) => {
            return refuse(
                StatusCode::CONFLICT,
                "Create your embedded Solana wallet first.",
            );
        }
        Err(response) => return response.into_response(),
    };
    let Some(store) = store else {
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "Wallet settings storage is unavailable.",
        );
    };
    match store.read(&owner) {
        Ok(preferences) => {
            Json(json!({"preferences": preferences, "execution_enabled": false})).into_response()
        }
        Err(()) => refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "Saved wallet settings could not be read. Trading remains inactive.",
        ),
    }
}

async fn save(
    State(state): State<Arc<AppState>>,
    Extension(store): Extension<Option<Arc<Store>>>,
    headers: HeaderMap,
    Json(preferences): Json<Preferences>,
) -> Response {
    let owner = match owner(&state, &headers).await {
        Ok(Some(owner)) => owner,
        Ok(None) => {
            return refuse(
                StatusCode::CONFLICT,
                "Create your embedded Solana wallet first.",
            );
        }
        Err(response) => return response.into_response(),
    };
    if !preferences.validate() {
        return refuse(
            StatusCode::BAD_REQUEST,
            "Enter a positive USD amount for each manual option, or select Agent decides. Manual trade and daily loss limits cannot exceed manual capital.",
        );
    }
    let Some(store) = store else {
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "Wallet settings storage is unavailable.",
        );
    };
    if store.write(&owner, &preferences).is_err() {
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "Could not save wallet settings. Trading remains inactive.",
        );
    }
    Json(json!({"preferences": preferences, "execution_enabled": false})).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use ring::{
        rand::SystemRandom,
        signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair},
    };
    use tower::ServiceExt;

    #[test]
    fn wallet_settings_require_an_explicit_writable_directory() {
        let dir = tempfile::tempdir().unwrap();
        assert!(configured_store(Some(dir.path().display().to_string())).is_some());
        assert!(configured_store(None).is_none());
        assert!(configured_store(Some(String::new())).is_none());
        assert!(configured_store(Some("   ".into())).is_none());
        let file = dir.path().join("file");
        std::fs::write(&file, b"not a directory").unwrap();
        assert!(configured_store(Some(file.join("inside").display().to_string())).is_none());
    }

    fn b64(raw: &[u8]) -> String {
        radar_types::b64::encode(raw)
            .replace('+', "-")
            .replace('/', "_")
            .trim_end_matches('=')
            .into()
    }

    fn identity(did: &str) -> (String, customer::Keys) {
        let random = SystemRandom::new();
        let pkcs8 =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &random).unwrap();
        let pair =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &random)
                .unwrap();
        let point = pair.public_key().as_ref();
        let keys = customer::Keys(vec![customer::Jwk {
            kid: "test".into(),
            crv: "P-256".into(),
            x: b64(&point[1..33]),
            y: b64(&point[33..65]),
        }]);
        let claims = json!({"iss":"privy.io", "aud":"test-app", "sub":did, "sid":"test-session", "exp":crate::now_unix() + 3600});
        let input = format!(
            "{}.{}",
            b64(br#"{"alg":"ES256","kid":"test"}"#),
            b64(&serde_json::to_vec(&claims).unwrap())
        );
        let signature = pair.sign(&random, input.as_bytes()).unwrap();
        (format!("{input}.{}", b64(signature.as_ref())), keys)
    }

    fn state(keys: customer::Keys, wallet_body: &str) -> Arc<AppState> {
        struct Reply(String);
        impl privy::Transport for Reply {
            fn get(&self, _: &str, _: &privy::Credentials) -> Result<String, String> {
                Ok(self.0.clone())
            }
        }
        Arc::new(AppState {
            registry: radar_instruments::Registry::new(),
            store: radar_store::Reader::open(std::env::temp_dir()),
            x402: None,
            chat: None,
            access: crate::access::Mode::Off,
            keys: crate::access::KeyCache::new(),
            customer: customer::Mode::Enforce(customer::Config {
                app_id: "test-app".into(),
            }),
            customer_keys: customer::KeyCache::preloaded(keys),
            privy: Some(privy::Client::with_transport(
                privy::Credentials::new("test-app", "test-only"),
                Box::new(Reply(wallet_body.into())),
            )),
            admission: crate::admission::Admission::Closed,
            shares: crate::share::Shares::new(crate::share::Allowance::per_day(1)),
            customer_salt: vec![7; 32],
            linker: crate::link::Linker::new(),
            scoreboard: crate::cache::Cache::new(),
            token: crate::cache::Cache::new(),
            challenges: None,
            market: crate::market::Market::new(),
            market_snapshot: crate::market::SnapshotCache::new(),
            customers: None,
            positions: None,
            trading: None,
            ticker: crate::ticker::Ticker::new(),
            market_ticker: crate::ticker::Ticker::new(),
            market_semaphore: Arc::new(tokio::sync::Semaphore::new(
                crate::MARKET_EVENTS_MAX_CONNECTIONS,
            )),
            market_visitors: Arc::default(),
        })
    }

    async fn call(
        app: Router,
        token: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let request = Request::builder()
            .uri(path)
            .method(if body.is_some() { "POST" } else { "GET" })
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
            .unwrap();
        let reply = app.oneshot(request).await.unwrap();
        assert_eq!(reply.headers()[header::CACHE_CONTROL], "private, no-store");
        let status = reply.status();
        let body = axum::body::to_bytes(reply.into_body(), 8192).await.unwrap();
        (
            status,
            serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null),
        )
    }

    #[tokio::test]
    async fn owner_settings_routes_persist_requested_preferences_but_never_activate_execution() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::at(dir.path().into()).unwrap());
        let (token, keys) = identity("did:privy:alice");
        let app = routes_with_store(Some(Arc::clone(&store))).with_state(state(keys, r#"{"linked_accounts":[{"type":"wallet","chain_type":"solana","connector_type":"embedded","address":"11111111111111111111111111111111","id":"first"}]}"#));
        assert_eq!(
            call(app.clone(), "invalid", "/automation/limits", None)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call(app.clone(), &token, "/automation/limits", None).await,
            (
                StatusCode::OK,
                json!({"preferences":null,"execution_enabled":false})
            )
        );
        let saved = call(
            app.clone(),
            &token,
            "/automation/limits",
            Some(serde_json::to_value(preferences()).unwrap()),
        )
        .await;
        assert_eq!(
            saved,
            (
                StatusCode::OK,
                json!({"preferences": preferences(), "execution_enabled":false})
            )
        );
        assert_eq!(
            call(app.clone(), &token, "/automation/limits", None).await,
            saved
        );
        let mut invalid = preferences();
        invalid.max_trade_usd = "101".into();
        assert_eq!(
            call(
                app.clone(),
                &token,
                "/automation/limits",
                Some(serde_json::to_value(invalid).unwrap())
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(app.clone(), &token, "/automation/limits", None).await,
            saved
        );
        std::fs::write(
            dir.path()
                .join(format!("{}.json", proof("did:privy:alice", "first").key())),
            b"corrupt",
        )
        .unwrap();
        assert_eq!(
            call(app, &token, "/automation/limits", None).await.0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        let (token, keys) = identity("did:privy:alice");
        let no_wallet =
            routes_with_store(Some(store)).with_state(state(keys, r#"{"linked_accounts":[]}"#));
        assert_eq!(
            call(no_wallet.clone(), &token, "/automation/wallet", None).await,
            (
                StatusCode::OK,
                json!({"wallet":null, "execution_enabled":false})
            )
        );
        for path in ["/automation/balance", "/automation/limits"] {
            assert_eq!(
                call(no_wallet.clone(), &token, path, None).await.0,
                StatusCode::CONFLICT
            );
        }
        assert_eq!(
            call(
                no_wallet,
                &token,
                "/automation/limits",
                Some(serde_json::to_value(preferences()).unwrap())
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
    }
    fn proof(did: &str, id: &str) -> OwnerWallet {
        let address = "11111111111111111111111111111111";
        OwnerWallet {
            did: did.into(),
            wallet: privy::Wallet {
                address: address.into(),
                id: Some(id.into()),
                delegated: false,
            },
            address: address.parse().unwrap(),
        }
    }
    fn preferences() -> Preferences {
        Preferences {
            capital_usd: "100".into(),
            max_trade_usd: "10".into(),
            daily_loss_usd: "5".into(),
            autonomous_requested: true,
            agent_decides: AgentDecisions::default(),
        }
    }
    #[test]
    fn only_positive_fixed_precision_amounts_are_accepted() {
        assert_eq!(micro_usd("1.234567"), Some(1_234_567));
        assert_eq!(micro_usd("0.000001"), Some(1));
        assert_eq!(micro_usd("1.2"), Some(1_200_000));
        assert_eq!(micro_usd("18446744073709.551615"), Some(u64::MAX));
        assert_eq!(micro_usd("18446744073709.551616"), None);
        for bad in [
            "",
            "0",
            "0.000000",
            "-1",
            "+1",
            "NaN",
            "inf",
            "1e4",
            " 1",
            ".1",
            "1.0000001",
            "18446744073709551615",
            "1.2.3",
        ] {
            assert_eq!(micro_usd(bad), None, "{bad}");
        }
    }
    #[test]
    fn requested_bounds_cannot_exceed_capital_and_do_not_supply_defaults() {
        let mut prefs = preferences();
        assert!(prefs.validate());
        prefs.max_trade_usd = "100".into();
        prefs.daily_loss_usd = "100".into();
        assert!(prefs.validate());
        prefs.max_trade_usd = "100.000001".into();
        assert!(!prefs.validate());
        prefs.max_trade_usd = "10".into();
        prefs.daily_loss_usd = "100.000001".into();
        assert!(!prefs.validate());
        prefs.daily_loss_usd = String::new();
        assert!(!prefs.validate());
        assert!(serde_json::from_value::<Preferences>(json!({"capital_usd":"1", "max_trade_usd":"1", "daily_loss_usd":"1", "autonomous_requested":true, "wallet":"someone else"})).is_err());
    }
    #[test]
    fn settings_survive_restart_and_are_bound_to_both_identity_and_wallet() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().into()).unwrap();
        let alice = proof("alice", "first");
        assert_eq!(store.read(&alice), Ok(None));
        store.write(&alice, &preferences()).unwrap();
        let reopened = Store::at(dir.path().into()).unwrap();
        assert_eq!(reopened.read(&alice), Ok(Some(preferences())));
        assert_eq!(reopened.read(&proof("bob", "first")), Ok(None));
        assert_eq!(reopened.read(&proof("alice", "second")), Ok(None));
        let mut changed = preferences();
        changed.autonomous_requested = false;
        reopened.write(&alice, &changed).unwrap();
        assert_eq!(store.read(&alice), Ok(Some(changed)));
        std::fs::write(dir.path().join(format!("{}.json", alice.key())), b"corrupt").unwrap();
        assert_eq!(store.read(&alice), Err(()));
        let mut invalid = preferences();
        invalid.max_trade_usd = "101".into();
        std::fs::write(
            dir.path().join(format!("{}.json", alice.key())),
            serde_json::to_vec(&invalid).unwrap(),
        )
        .unwrap();
        assert_eq!(store.read(&alice), Err(()));
    }

    #[test]
    fn every_agent_choice_is_independent_and_manual_values_still_require_bounds() {
        for mask in 0..8 {
            let mut prefs = preferences();
            prefs.agent_decides = AgentDecisions {
                capital_usd: mask & 1 != 0,
                max_trade_usd: mask & 2 != 0,
                daily_loss_usd: mask & 4 != 0,
            };
            assert!(prefs.validate());
            for field in 0..3 {
                let mut changed = prefs.clone();
                match field {
                    0 => changed.capital_usd.clear(),
                    1 => changed.max_trade_usd.clear(),
                    _ => changed.daily_loss_usd.clear(),
                }
                assert_eq!(
                    changed.validate(),
                    mask & (1 << field) != 0,
                    "{mask}/{field}"
                );
            }
            let mut changed = prefs.clone();
            changed.max_trade_usd = "101".into();
            assert_eq!(
                changed.validate(),
                prefs.agent_decides.capital_usd || prefs.agent_decides.max_trade_usd
            );
            changed = prefs.clone();
            changed.daily_loss_usd = "101".into();
            assert_eq!(
                changed.validate(),
                prefs.agent_decides.capital_usd || prefs.agent_decides.daily_loss_usd
            );
        }
    }

    #[test]
    fn agent_choices_survive_restart_and_legacy_settings_remain_manual() {
        let dir = tempfile::tempdir().unwrap();
        let owner = proof("alice", "first");
        let store = Store::at(dir.path().into()).unwrap();
        let mut prefs = preferences();
        prefs.agent_decides = AgentDecisions {
            capital_usd: true,
            max_trade_usd: false,
            daily_loss_usd: true,
        };
        prefs.capital_usd.clear();
        prefs.daily_loss_usd.clear();
        assert!(prefs.validate());
        store.write(&owner, &prefs).unwrap();
        assert_eq!(
            Store::at(dir.path().into()).unwrap().read(&owner),
            Ok(Some(prefs))
        );
        let mut legacy = serde_json::to_value(preferences()).unwrap();
        legacy.as_object_mut().unwrap().remove("agent_decides");
        let parsed: Preferences = serde_json::from_value(legacy).unwrap();
        assert_eq!(parsed.agent_decides, AgentDecisions::default());
        assert!(parsed.validate());
        for invalid in [json!({"capital_usd":"true"}), json!({"other":true})] {
            let mut value = serde_json::to_value(preferences()).unwrap();
            value["agent_decides"] = invalid;
            assert!(serde_json::from_value::<Preferences>(value).is_err());
        }
    }
}
