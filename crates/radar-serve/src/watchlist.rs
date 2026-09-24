// SPDX-License-Identifier: Apache-2.0
//! `/v1/customer/watchlist`: the coins a signed-in wallet keeps an eye on.
//!
//! The first route that reads per-wallet state, and so the first behind a
//! [`Tenant`]. Every handler takes one; none takes a wallet address from the
//! request. There is no way to ask for someone else's list here, because there
//! is no parameter to put their address in -- and one smuggled in a query string
//! is refused out loud rather than ignored, so a caller trying it learns that it
//! does not work instead of wondering whose list they got.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::AppState;
use crate::tenant::{Coin, Customers, Tenant, TenantStore, Unavailable, WATCHLIST_LIMIT, refusal};

/// `GET /v1/customer/watchlist`: the calling wallet's coins.
pub(crate) async fn list(State(state): State<Arc<AppState>>, tenant: Tenant, uri: Uri) -> Response {
    with_store(state.customers.as_ref(), &tenant, &uri, |store| {
        answer(&tenant, store.watchlist())
    })
}

/// `PUT /v1/customer/watchlist/{mint}`: adds a coin.
pub(crate) async fn watch(
    State(state): State<Arc<AppState>>,
    tenant: Tenant,
    uri: Uri,
    Path(mint): Path<String>,
) -> Response {
    with_store(
        state.customers.as_ref(),
        &tenant,
        &uri,
        |store| match mint.parse::<Coin>() {
            Ok(coin) => answer(&tenant, store.watch(coin)),
            Err(_) => not_a_coin(&mint),
        },
    )
}

/// `DELETE /v1/customer/watchlist/{mint}`: removes a coin.
pub(crate) async fn unwatch(
    State(state): State<Arc<AppState>>,
    tenant: Tenant,
    uri: Uri,
    Path(mint): Path<String>,
) -> Response {
    with_store(
        state.customers.as_ref(),
        &tenant,
        &uri,
        |store| match mint.parse::<Coin>() {
            Ok(coin) => answer(&tenant, store.unwatch(coin)),
            Err(_) => not_a_coin(&mint),
        },
    )
}

/// Runs `then` against the calling wallet's store, or says why there is none.
fn with_store(
    customers: Option<&Customers>,
    tenant: &Tenant,
    uri: &Uri,
    then: impl FnOnce(&TenantStore<'_>) -> Response,
) -> Response {
    // Checked before anything is read. A query string here can only be someone
    // trying to name a wallet, and answering with the caller's own list would
    // look, to them, like it worked.
    if uri.query().is_some() {
        return refusal(
            StatusCode::BAD_REQUEST,
            "unscoped",
            "this route reads only the signed-in wallet's own list, and takes no parameters",
        );
    }
    let Some(customers) = customers else {
        return refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_configured",
            "this instance keeps no watchlists",
        );
    };
    match customers.store(tenant) {
        Ok(store) => then(&store),
        Err(why) => failed(&why),
    }
}

fn answer(tenant: &Tenant, coins: Result<Vec<Coin>, Unavailable>) -> Response {
    match coins {
        Ok(coins) => Json(json!({
            "wallet": tenant.address().to_string(),
            "coins": coins.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "limit": WATCHLIST_LIMIT,
        }))
        .into_response(),
        Err(why) => failed(&why),
    }
}

fn failed(why: &Unavailable) -> Response {
    let (status, reason) = match why {
        Unavailable::Full => (StatusCode::CONFLICT, "full"),
        Unavailable::Unreadable(_) => (StatusCode::INTERNAL_SERVER_ERROR, "unreadable"),
        Unavailable::Unwritable(_) => (StatusCode::INTERNAL_SERVER_ERROR, "unwritable"),
        Unavailable::Unplaceable => (StatusCode::INTERNAL_SERVER_ERROR, "unplaceable"),
    };
    refusal(status, reason, &why.to_string())
}

fn not_a_coin(mint: &str) -> Response {
    refusal(
        StatusCode::BAD_REQUEST,
        "not_a_coin",
        &format!("{mint} is not a coin address"),
    )
}
