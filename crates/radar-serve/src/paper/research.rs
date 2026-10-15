// SPDX-License-Identifier: Apache-2.0
//! Authenticated research conversation. No free-text control or broker actions.

use super::{Bridge, Fault, authenticated, browser_write, invoke, unresolved};
use crate::{AppState, Operator};
use axum::{Extension, Json, extract::State, http::{HeaderMap, StatusCode}, response::{IntoResponse, Response}};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};

const VERSION: &str = "alpaca-paper-research-v1";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Message {
    id: String,
    role: String,
    text: String,
    received_at_unix_ms: u64,
    reply_to: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Thread {
    version: String,
    account_id: String,
    as_of_unix_ms: u64,
    available: bool,
    messages: Vec<Message>,
    truncated: bool,
    execution_authority: bool,
    source_claims_verified: bool,
}

fn identity(value: &str) -> bool {
    value.strip_prefix("radar_research_").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.len() <= 80 && suffix.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    })
}

fn project(value: Value) -> Result<Thread, Fault> {
    let thread: Thread = serde_json::from_value(value).map_err(|_| unresolved())?;
    if thread.version != VERSION || thread.account_id.is_empty() || thread.account_id.len() > 64
        || thread.as_of_unix_ms == 0 || thread.execution_authority || thread.source_claims_verified
        || thread.messages.len() > 128 || thread.truncated
        || (!thread.available && !thread.messages.is_empty()) {
        return Err(unresolved());
    }
    let mut ids = BTreeSet::new();
    let mut users = BTreeSet::new();
    let mut replied = BTreeSet::new();
    let mut previous = 0;
    for message in &thread.messages {
        if !identity(&message.id) || !ids.insert(message.id.as_str())
            || message.text.trim().is_empty() || message.text.len() > 4000
            || message.received_at_unix_ms == 0 || message.received_at_unix_ms < previous
            || message.received_at_unix_ms > thread.as_of_unix_ms {
            return Err(unresolved());
        }
        previous = message.received_at_unix_ms;
        match (message.role.as_str(), message.reply_to.as_deref()) {
            ("user", None) => { users.insert(message.id.as_str()); }
            ("assistant", Some(parent)) if users.contains(parent) && replied.insert(parent) => {}
            _ => return Err(unresolved()),
        }
    }
    Ok(thread)
}

pub(super) async fn read(
    proof: Option<Extension<Operator>>,
    Extension(bridge): Extension<Bridge>,
) -> Response {
    let result = async {
        let owner = authenticated(proof)?;
        let raw = invoke(bridge, json!({"action":"research_read",
            "operator":{"email":owner.email,"subject":owner.subject}})).await?;
        project(raw).map(Json)
    }.await;
    result.into_response()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Append {
    nonce: String,
    text: String,
}

#[derive(Serialize)]
struct Submitted {
    thread: Thread,
    reply_status: &'static str,
    detail: Option<&'static str>,
}

pub(super) async fn append(
    State(state): State<Arc<AppState>>,
    proof: Option<Extension<Operator>>,
    Extension(bridge): Extension<Bridge>,
    headers: HeaderMap,
    Json(body): Json<Append>,
) -> Response {
    match submit(state, proof, bridge, headers, body).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => error.into_response(),
    }
}

async fn submit(
    state: Arc<AppState>, proof: Option<Extension<Operator>>, bridge: Bridge,
    headers: HeaderMap, body: Append,
) -> Result<Submitted, Fault> {
    let owner = authenticated(proof)?;
    browser_write(&headers)?;
    if body.nonce.is_empty() || body.nonce.len() > 64
        || !body.nonce.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        || body.text.trim().is_empty() || body.text.len() > 4000 {
        return Err(Fault(StatusCode::BAD_REQUEST, "Use a nonempty research message within 4000 UTF-8 bytes and a valid message nonce."));
    }
    let _permit = Arc::clone(&bridge.research_slots).try_acquire_owned()
        .map_err(|_| Fault(StatusCode::TOO_MANY_REQUESTS, "Research reply is busy; read the thread before another send."))?;
    let operator = json!({"email":owner.email,"subject":owner.subject});
    let user_id = format!("radar_research_u_{}", body.nonce);
    let reply_id = format!("radar_research_a_{}", body.nonce);
    let mut thread = project(invoke(bridge.clone(), json!({"action":"research_add", "operator":operator,
        "message":{"id":user_id,"role":"user","text":body.text,"reply_to":null}})).await?)?;
    let question = thread.messages.iter().find(|m| m.id == user_id && m.role == "user" && m.text == body.text)
        .ok_or_else(unresolved)?;
    if thread.messages.iter().any(|m| m.reply_to.as_deref() == Some(user_id.as_str())) {
        return Ok(Submitted { thread, reply_status: "completed", detail: None });
    }
    // Tail context is explicitly incomplete. The specific question is included
    // even when an idempotent resubmission names an older retained user turn.
    let recent = &thread.messages[thread.messages.len().saturating_sub(6)..];
    let context = json!({"version":VERSION,"account_id":thread.account_id,
        "as_of_unix_ms":thread.as_of_unix_ms,"question":question,"messages":recent,
        "truncated":thread.messages.len() > 6,"source_claims_verified":false,"execution_authority":false}).to_string();
    let result = tokio::task::spawn_blocking(move || {
        state.chat.as_ref().ok_or("No model provider is configured.")
            .and_then(|chat| crate::chat::paper_reply(chat, &context))
    }).await.unwrap_or(Err("The model worker is unavailable."));
    let text = match result {
        Ok(text) => text,
        Err(detail) => return Ok(Submitted { thread, reply_status: "unavailable", detail: Some(detail) }),
    };
    let account = thread.account_id.clone();
    thread = project(invoke(bridge, json!({"action":"research_reply","operator":operator,
        "message":{"id":reply_id,"role":"assistant","text":text,"reply_to":user_id}})).await?)?;
    if thread.account_id != account || !thread.messages.iter().any(|m| m.id == reply_id
        && m.role == "assistant" && m.text == text && m.reply_to.as_deref() == Some(user_id.as_str())) {
        return Err(unresolved());
    }
    Ok(Submitted { thread, reply_status: "completed", detail: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: usize) -> Value {
        json!({"id":format!("radar_research_u_{id}"),"role":"user","text":"An unverified tip",
            "received_at_unix_ms":1,"reply_to":null})
    }

    fn thread() -> Value {
        json!({"version":VERSION,"account_id":"paper-account","as_of_unix_ms":2,
            "available":true,"messages":[user(0)],"truncated":false,
            "execution_authority":false,"source_claims_verified":false})
    }

    #[test]
    fn retained_context_preserves_unknown_and_refuses_authority_schema_or_future_claims() {
        assert!(project(thread()).is_ok());
        for (field, value) in [("version", json!("other")), ("account_id", json!("")),
            ("as_of_unix_ms", json!(0)), ("as_of_unix_ms", json!(-1)),
            ("execution_authority", json!(true)), ("source_claims_verified", json!(true)),
            ("truncated", json!(true)), ("available", json!(false)), ("secret", json!("not projected"))] {
            let mut changed = thread(); changed[field] = value;
            assert!(project(changed).is_err(), "{field}");
        }
        let mut unknown = thread(); unknown["available"] = json!(false); unknown["messages"] = json!([]);
        assert!(!project(unknown).unwrap().available);
        for (field, value) in [("received_at_unix_ms", json!(3)), ("received_at_unix_ms", json!(0)),
            ("text", json!("é".repeat(2001))), ("text", json!(" ")), ("role", json!("system")),
            ("id", json!("radar_research_")), ("id", json!("radar_research_/escape")),
            ("reply_to", json!("radar_research_u_0")), ("secret", json!("not projected"))] {
            let mut changed = thread(); changed["messages"][0][field] = value;
            assert!(project(changed).is_err(), "{field}");
        }
        let mut boundary = thread(); boundary["messages"][0]["text"] = json!("é".repeat(2000));
        boundary["messages"][0]["received_at_unix_ms"] = json!(2);
        assert!(project(boundary).is_ok());
    }

    #[test]
    fn retained_context_is_bounded_and_only_allows_one_reply_to_a_prior_user() {
        let mut full = thread(); full["messages"] = json!((0..128).map(user).collect::<Vec<_>>());
        assert!(project(full.clone()).is_ok());
        full["messages"].as_array_mut().unwrap().push(user(128));
        assert!(project(full).is_err());
        let mut conversation = thread();
        let reply = json!({"id":"radar_research_a_0","role":"assistant","text":"Still a hypothesis",
            "received_at_unix_ms":2,"reply_to":"radar_research_u_0"});
        conversation["messages"].as_array_mut().unwrap().push(reply.clone());
        assert!(project(conversation.clone()).is_ok());
        let mut duplicate = conversation.clone(); duplicate["messages"].as_array_mut().unwrap().push(user(0));
        assert!(project(duplicate).is_err());
        let mut twice = reply.clone(); twice["id"] = json!("radar_research_a_1");
        conversation["messages"].as_array_mut().unwrap().push(twice);
        assert!(project(conversation).is_err());
        let mut orphan = thread(); orphan["messages"] = json!([reply]);
        assert!(project(orphan).is_err());
        let mut backwards = thread(); backwards["messages"] = json!([user(0), user(1)]);
        backwards["messages"][0]["received_at_unix_ms"] = json!(2);
        assert!(project(backwards).is_err());
        assert!(identity(&format!("radar_research_{}", "a".repeat(80))));
        assert!(!identity(&format!("radar_research_{}", "a".repeat(81))));
    }
}
