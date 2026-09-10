// SPDX-License-Identifier: Apache-2.0
//! The chat route.
//!
//! Three crates meet here and the split is the design. [`radar_agent`] decides
//! whether a question may be asked and what the model may see, with no network
//! and no key. [`radar_model`] reaches a provider, with no opinion about
//! policy. This module is the seam, and it is deliberately thin: everything it
//! could get wrong is a thing one of the other two already decided.
//!
//! # What a reply cannot do
//!
//! Two things happen to it. Its prose is rendered, and its **step** is parsed —
//! since [`crate::evidence::Investigation`] landed, a model may ask for a named
//! piece of evidence and may write a typed recommendation.
//!
//! That is a real change and the guarantee is unchanged, so it is worth saying
//! exactly where the guarantee now lives:
//!
//! - **A request is a tool name and one string.** It is checked against the
//!   read-only allowlist by name, and a name that is not on it is refused by
//!   name rather than ignored.
//! - **A recommendation is validated by a deterministic adapter** against the
//!   watermark, the running strategy version and the sources Radar actually
//!   returned. Anything it rejects becomes an abstention.
//! - **An amount is a requested bound, never permission.** The adapter here is
//!   [`radar_agent::Adapter::closed`], whose ceiling is zero, so this route
//!   cannot adopt an `enter` at any size.
//! - **There is still no decision it can reach.** `radar-serve` does not depend
//!   on `radar-risk`'s authorisation path, on `radar-exec` or on `radar-signer`,
//!   and `repo-conformance` holds that. A model fully persuaded by a token name
//!   can write anything into `note` and reach nothing but a `<p>`.
//!
//! # Where the untrusted content comes in
//!
//! The operator's question is placed as a question. Everything Radar looked up
//! in order to answer it — token metadata, a creator's history, social copy — is
//! fenced by [`radar_model::Request::observing`], which escapes the marker
//! before placing it. AGENTS.md rule 4, at the one place in the system where a
//! stranger's text and a language model are in the same buffer.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use radar_agent::{Agent, Unavailable};
use radar_model::Provider;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AppState;
use crate::share;

/// Radar's own framing, and the only text in a system position.
///
/// Short on purpose. A long system prompt full of prohibitions is a prompt that
/// invites negotiation, and the prohibitions that matter here are not enforced
/// by asking: the model has no action tools, every request it makes is checked
/// against the read-only allowlist by name, and every recommendation it writes
/// is validated by a deterministic adapter whose ceiling is zero. What is left
/// for a system prompt to do is set the register — say what Radar is, that a
/// claim without a source is worse than no claim, and that asking for evidence
/// costs a turn.
///
/// The tool list, the watermark and the wire format are **not** here. They are
/// added by [`crate::evidence::framing`] from the allowlist and the store that
/// will actually answer, so a menu cannot drift from what `may_call` admits.
pub const SYSTEM: &str = "You are the reading assistant for Radar, a Solana \
    research recorder. Radar's product is an honest account of what it refused \
    and why, not a profit forecast. The population Radar selects from has a \
    median return around -13% before costs and fewer than one token in ten \
    finishes above a round trip. Answer from the recorded evidence you are \
    given, name the source when you use one, and say plainly when the evidence \
    does not settle the question. You may ask for more evidence by name, a few \
    times, before you answer; a name that is not on your tool list is refused \
    and a request costs a turn, so ask for what would change your answer. \
    Never recommend buying or selling anything: you cannot see a position, a \
    balance or a price, and Radar's trading policy is closed regardless of what \
    you say.";

/// Which strategy a recommendation from this route is about.
///
/// A recommendation naming a different version is refused by the adapter, which
/// is not decoration: reasoning done about last week's strategy, applied to this
/// week's, is a category error that reads as a normal answer.
///
/// The reading assistant does not run a strategy, so this names the route rather
/// than a strategy version — and it is deliberately a value the model is *told*,
/// so that a model inventing one is caught rather than accommodated.
pub const STRATEGY_VERSION: &str = "reading-assistant/1";

/// A question from the operator.
#[derive(Clone, Debug, Deserialize)]
pub struct Ask {
    /// What was typed.
    pub question: String,
}

/// The answer, as the interface receives it.
#[derive(Clone, Debug, Serialize)]
pub struct Answered {
    /// What the model said, verbatim and unparsed.
    pub text: String,
    /// Which recorded sources were placed in the prompt.
    ///
    /// The provenance a reader needs. The interface renders an uncited reply
    /// differently from a cited one, because an uncited *claim* has the shape
    /// of a fabrication and a reader must be able to see which they have.
    pub citations: Vec<String>,
    /// Whether anything was consulted at all.
    pub uncited: bool,
    /// The typed recommendation, validated, or the abstention that replaced it.
    ///
    /// Inert. Serialised for a reader and for a later replay; nothing on this
    /// path acts on it, and the risk kernel that could is not in this crate's
    /// path at all.
    pub recommendation: radar_agent::Recommendation,
    /// Every request Radar declined, named.
    ///
    /// Published rather than swallowed. A model spending its turns on refusals
    /// is a prompt problem an operator can only see if the refusals are visible.
    pub refusals: Vec<String>,
    /// How many model calls the answer took.
    pub turns: u32,
    /// Why the investigation ended without a recommendation of its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abstained: Option<radar_agent::Abstained>,
}

/// The longest question that will be considered.
///
/// A prompt is charged by the token and the box is on the internet behind an
/// identity check that could one day be misconfigured. A megabyte pasted into
/// it should cost a refusal, not a bill.
pub const MAX_QUESTION_BYTES: usize = 4_000;

/// Whether a question is too long to send.
///
/// Bytes rather than characters, deliberately. Cost tracks bytes far more
/// closely than it tracks characters, and a limit counted in characters lets
/// four bytes of emoji through for the price of one `a` — which is the shape
/// somebody would use to find the ceiling.
#[must_use]
pub fn overlong(question: &str) -> bool {
    question.len() > MAX_QUESTION_BYTES
}

/// Writes the agent's ledger, so the day's spend survives a restart.
///
/// **A failure here is logged and not propagated**, and that is the one
/// judgement call in this module worth arguing. The alternative is refusing the
/// question because a counter could not be written, which turns a full disk into
/// an outage of the reading assistant.
///
/// It is safe only because the failure is bounded and visible in the other
/// direction: the directory's writability is proven at startup, so reaching this
/// path means the disk filled or the mount changed *while running*, and the
/// worst case is that a restart forgets part of one day's spend against a
/// ceiling that is measured in dollars.
fn record(chat: &Chat, agent: &Agent) {
    if let Err(why) = chat.ledger.write(LEDGER_RECORD, &agent.ledger()) {
        eprintln!(
            "radar-serve: the model ledger could not be written ({why}); a restart              will forget today's spend"
        );
    }
}

/// The name the model meter's state is stored under.
pub const LEDGER_RECORD: &str = "model-ledger";

/// Everything the route needs beyond the store.
///
/// Absent entirely when no provider is configured, which is what makes rule 8
/// structural here: there is no half-built agent to accidentally call.
pub struct Chat {
    /// The policy boundary. Behind a mutex because the meter is the one piece
    /// of state a chat route mutates, and it must not be possible for two
    /// questions in flight to each see the budget before the other spent it.
    pub agent: std::sync::Mutex<Agent>,
    /// How to reach a model.
    pub provider: Box<dyn Provider>,
    /// The same provider, when it is one whose credential can be linked from
    /// the interface.
    ///
    /// A second handle rather than a downcast, because the question "can this be
    /// linked" is answered when the provider is built and should not be
    /// re-derived at the route. `None` on the API-key path, which has nothing to
    /// link: a key is set in a file, not authorised in a browser.
    pub linkable: Option<radar_model::Codex>,
    /// Where the meter's state is written so it survives a restart.
    ///
    /// Not optional. Rule 8 claimed this property and nothing implemented it:
    /// `Agent::restore` had one caller and it was a unit test, so every restart
    /// reset the day's spend to zero and a crash loop under `Restart=always`
    /// would have handed out a fresh allowance per crash.
    pub ledger: crate::ledger::Store,
    /// How the last call went.
    ///
    /// Recorded because a health check that only says "a provider is
    /// configured" is the shape LEARNINGS records repeatedly: a working
    /// component and a dead one reporting the same thing. A credential that
    /// lapsed after a fortnight of inactivity leaves configuration untouched and
    /// every call failing, and this is what makes that visible.
    pub last: std::sync::Mutex<LastCall>,
}

/// The outcome of the most recent model call.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize)]
#[serde(tag = "last_call", rename_all = "snake_case")]
pub enum LastCall {
    /// Nothing has been asked since this process started.
    ///
    /// Not a failure and not a success. Saying so plainly beats reporting it as
    /// either: a restart makes this the normal state, so alarming on it would
    /// alarm on every deploy, and reporting it as healthy would call an
    /// untested provider working.
    #[default]
    Never,
    /// The last call succeeded.
    Ok,
    /// The last call failed, and this is why.
    Failed {
        /// The refusal, as the provider gave it.
        why: String,
    },
}

/// Answers a question, or says why it cannot.
///
/// # Errors
///
/// Never returns `Err`; every failure is a status and a JSON body, because this
/// is answering a browser.
pub async fn ask(
    State(state): State<Arc<AppState>>,
    // Present only when the guard verified a *customer* token. Absent for an
    // operator, and absent today for everyone, because no customer
    // authenticator is configured and every request falls back to the operator
    // check.
    //
    // `Option`, so the route keeps working for the operator it currently serves
    // rather than refusing everyone the day this landed.
    customer: Option<axum::Extension<crate::customer::Customer>>,
    Json(body): Json<Ask>,
) -> Response {
    let Some(chat) = state.chat.as_ref() else {
        // Not 503. An unconfigured route does not exist, the same way the
        // unconfigured paid routes do not: a surface that announces its own
        // shape is halfway to one that serves.
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response();
    };

    let question = body.question.trim();
    if question.is_empty() {
        return refuse(StatusCode::BAD_REQUEST, "ask a question");
    }
    if overlong(question) {
        return refuse(
            StatusCode::PAYLOAD_TOO_LARGE,
            &format!("questions are limited to {MAX_QUESTION_BYTES} bytes"),
        );
    }

    let day = today_utc();

    // A customer's share of the day, charged before the global budget is
    // touched.
    //
    // The order matters: reserving globally first and then refusing here would
    // hold a commitment for a call that never goes out, and `Meter::ledger`
    // records in-flight commitments — so a refused customer would show up as
    // spend nobody made.
    //
    // An operator has no `Customer` extension and is not metered here. There is
    // one of him, he pays for the instance, and the global budget already bounds
    // what he can spend.
    if let Some(axum::Extension(who)) = customer.as_ref()
        && let Err(why) = state.shares.charge(&who.did, &state.customer_salt, day)
    {
        {
            return match why {
                share::Refused::Spent { allowance } => refuse(
                    StatusCode::TOO_MANY_REQUESTS,
                    &format!(
                        "you have asked {allowance} questions today, which is this \
                         instance's per-customer limit. It resets at midnight UTC."
                    ),
                ),
                // 503 rather than 429: nothing the customer does fixes this, and
                // a rate-limit status would have them waiting for a window that
                // is never going to open.
                share::Refused::Unconfigured => refuse(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "this instance has no per-customer question allowance configured, \
                     so no customer may spend its model budget",
                ),
                share::Refused::NoSubject(_) => refuse(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "this instance cannot meter customers, so it will not spend on \
                     their behalf",
                ),
            };
        }
    }

    // The meter is held for the whole investigation rather than per turn.
    // Two questions in flight would otherwise interleave their reservations
    // against one budget, and this box is one browser tab with a retry button.
    let Ok(mut agent) = chat.agent.lock() else {
        return refuse(StatusCode::SERVICE_UNAVAILABLE, "the meter is poisoned");
    };

    let started = std::time::Instant::now();
    let investigation = crate::evidence::Investigation {
        registry: &state.registry,
        store: &state.store,
        bounds: radar_agent::Bounds::SHIPPED,
        strategy_version: STRATEGY_VERSION,
    };
    let session = crate::evidence::Session {
        provider: chat.provider.as_ref(),
        day,
        elapsed: &|| u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        record: &|agent| record(chat, agent),
    };
    let outcome =
        tokio::task::block_in_place(|| investigation.run(SYSTEM, question, &mut agent, &session));
    drop(agent);

    // A refusal from the boundary itself keeps its own status. 402 says the
    // operator has spent the day's budget and tomorrow will work; 503 says
    // something is broken now. Collapsing them into a 200 carrying an
    // abstention would be how a spent budget gets diagnosed as a bad answer.
    if let Some(radar_agent::Abstained::Unavailable(why)) = &outcome.abstained {
        if let Ok(mut last) = chat.last.lock() {
            *last = match why {
                Unavailable::Unreachable(detail) => LastCall::Failed {
                    why: detail.clone(),
                },
                _ => LastCall::Ok,
            };
        }
        return unavailable(why);
    }

    if let Ok(mut last) = chat.last.lock() {
        *last = LastCall::Ok;
    }

    // The instruments Radar actually invoked, not names the model chose to
    // write down. A citation here can be re-run.
    let citations: Vec<String> = outcome
        .facts
        .iter()
        .map(|fact| fact.source().to_owned())
        .collect();
    Json(Answered {
        text: outcome.note,
        uncited: citations.is_empty(),
        citations,
        recommendation: outcome.recommendation,
        refusals: outcome.refusals,
        turns: outcome.turns,
        abstained: outcome.abstained,
    })
    .into_response()
}

/// The day the meter accounts against.
///
/// Public because the binary needs the same day when it builds the agent, and
/// two functions computing "today" independently is how a meter starts a day
/// behind its own ledger.
///
/// Whole days since the epoch, in UTC. The meter takes the day as an argument
/// rather than reading a clock — that is what makes it replayable — so the
/// impurity lives here, at the edge, where it is one line.
#[must_use]
pub fn today_utc() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or_default()
}

/// A refusal with a reason a person can act on.
pub(crate) fn refuse(status: StatusCode, why: &str) -> Response {
    (status, Json(json!({ "error": why }))).into_response()
}

/// Maps a policy refusal to a status.
///
/// Separated because the distinction is the useful part: 402 says the operator
/// has spent the day's budget and tomorrow will work, 503 says something is
/// broken now. Collapsing them into one status is how a spent budget gets
/// diagnosed as an outage.
fn unavailable(why: &Unavailable) -> Response {
    let status = match why {
        Unavailable::NoProvider | Unavailable::NoBudget => StatusCode::NOT_FOUND,
        Unavailable::OverBudget => StatusCode::PAYMENT_REQUIRED,
        Unavailable::Unreachable(_) => StatusCode::SERVICE_UNAVAILABLE,
    };
    refuse(status, &why.to_string())
}

/// What `radar brief` reads to decide whether the agent is healthy.
///
/// Reports in both directions. A check that can only say "ok" is the failure
/// LEARNINGS records repeatedly: a healthy component and a dead one printing
/// the same thing.
#[must_use]
pub fn status(chat: Option<&Chat>) -> serde_json::Value {
    match chat {
        None => json!({ "configured": false }),
        Some(chat) => {
            let ledger = chat.agent.lock().ok().map(|a| a.ledger());
            let last = chat.last.lock().map_or_else(
                |_| LastCall::Failed {
                    why: "the record is poisoned".to_owned(),
                },
                |l| l.clone(),
            );
            json!({
                "configured": true,
                "provider": chat.provider.name(),
                "linkable": chat.linkable.is_some(),
                "estimate_micro_usd": chat.provider.estimate().get(),
                "spent_micro_usd": ledger.as_ref().map(|l| l.spent),
                "tools": chat.agent.lock().ok().map_or(0, |a| a.allowlist().len()),
                "last": last,
            })
        }
    }
}

/// The same, with everything an unauthenticated reader has no business knowing
/// removed.
///
/// # What is taken out, and why each one
///
/// `/health` is public. It carried the whole of [`status`], and two of those
/// fields are operational secrets rather than health:
///
/// - **`spent_micro_usd`** says how much of the day's budget is left. That is
///   directly useful to somebody trying to spend it: the cheapest attack on this
///   account is to exhaust its allowance, and this field is the scoreboard for
///   it.
/// - **`last.why`** is the provider's own refusal text, verbatim. A platform
///   lock reason, a policy notice or an upstream error body ends up there, and
///   publishing it hands a stranger the account's operational state in the
///   provider's own words.
///
/// What stays is enough to answer *is it working*: configured, which provider,
/// and whether the last call succeeded — as a word, not as a reason.
/// `radar brief` reads exactly that, so the monitor keeps working with no
/// credential, and an operator who wants the figures reads `/v1/store`.
#[must_use]
pub fn public_status(chat: Option<&Chat>) -> serde_json::Value {
    let full = status(chat);
    if full.get("configured").and_then(serde_json::Value::as_bool) != Some(true) {
        return full;
    }
    let last = match full.get("last").and_then(|l| l.get("last_call")) {
        Some(v) => v.clone(),
        // `LastCall` serialises `Never` without a `last_call` field on some
        // shapes; an absent value is reported as absent rather than as a
        // success, which is the same rule the rest of this file follows.
        None => serde_json::Value::Null,
    };
    json!({
        "configured": true,
        "provider": full.get("provider"),
        "last": { "last_call": last },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_model::{Answer, Request, Unreachable};
    use radar_types::MicroUsd;

    /// A provider that never reaches anything, so the seam can be tested
    /// without a key, a subprocess or a network.
    #[derive(Debug)]
    struct Stub(Result<Answer, Unreachable>);

    impl Provider for Stub {
        fn name(&self) -> &'static str {
            "stub"
        }
        fn estimate(&self) -> MicroUsd {
            MicroUsd(1_000)
        }
        fn ask(&self, _: &Request) -> Result<Answer, Unreachable> {
            self.0.clone()
        }
    }

    fn chat(outcome: Result<Answer, Unreachable>) -> Chat {
        let mut allowlist = radar_agent::Allowlist::new();
        allowlist.allow("creator_history");
        Chat {
            agent: std::sync::Mutex::new(Agent::new(
                radar_agent::Config {
                    budget: radar_agent::Budget {
                        per_call_max: MicroUsd(10_000),
                        daily_max: MicroUsd(20_000),
                    },
                    allowlist,
                },
                today_utc(),
            )),
            provider: Box::new(Stub(outcome)),
            linkable: None,
            last: std::sync::Mutex::new(LastCall::Never),
            ledger: crate::ledger::Store::at(&std::env::temp_dir().join("radar-chat-test-ledger"))
                .expect("a writable scratch directory"),
        }
    }

    #[test]
    fn the_system_prompt_never_invites_a_recommendation() {
        // The register matters more than the prohibitions: a model asked what
        // to buy, by an operator, with no tools, will still answer -- and an
        // answer that reads as advice is the failure mode of this whole
        // feature, because it is the one a reader would act on.
        assert!(SYSTEM.contains("Never recommend buying or selling"));
        assert!(SYSTEM.contains("-13%"), "the base rate is in the framing");
        // And it says the one thing about the shape of the conversation that a
        // model would otherwise get wrong. This used to be "you cannot request
        // more evidence"; since the investigative loop landed it is the
        // opposite, and a prompt still carrying the old sentence would teach a
        // model not to use the turns it has been given.
        assert!(SYSTEM.contains("may ask for more evidence by name"));
        assert!(
            !SYSTEM.contains("cannot request more evidence"),
            "the pre-loop instruction is gone, not merely contradicted"
        );
    }

    #[test]
    fn a_spent_budget_and_a_broken_provider_are_different_statuses() {
        // Collapsing these is how a spent budget gets diagnosed as an outage at
        // three in the morning. 402 means tomorrow will work.
        assert_eq!(
            unavailable(&Unavailable::OverBudget).status(),
            StatusCode::PAYMENT_REQUIRED
        );
        assert_eq!(
            unavailable(&Unavailable::Unreachable("the CLI exited".to_owned())).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        // And an unconfigured agent does not exist rather than being broken.
        assert_eq!(
            unavailable(&Unavailable::NoProvider).status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn a_status_report_says_configured_or_not_rather_than_only_ok() {
        // Both directions. A check that can only say "ok" is a healthy
        // component and a dead one printing the same thing, which LEARNINGS
        // records three times.
        assert_eq!(status(None)["configured"], false);
        let chat = chat(Ok(Answer {
            text: "hello".to_owned(),
            cost: None,
        }));
        let live = status(Some(&chat));
        assert_eq!(live["configured"], true);
        assert_eq!(live["provider"], "stub");
        assert_eq!(live["tools"], 1);
        assert_eq!(live["spent_micro_usd"], 0);
    }

    #[test]
    fn a_provider_that_reports_no_cost_is_charged_the_estimate() {
        // Rule 9, at the seam. A subscription never reports a cost, and a call
        // charged as zero is a call the meter never counts -- so the budget
        // never runs out and a chat box left open in a loop runs forever.
        let chat = chat(Ok(Answer {
            text: "hello".to_owned(),
            cost: None,
        }));
        let mut agent = chat.agent.lock().expect("fresh");
        let commitment = agent.begin(MicroUsd(1_000), today_utc()).expect("fits");
        agent.settle(commitment, MicroUsd(1_000));
        assert_eq!(agent.ledger().spent, 1_000, "the estimate, not nothing");
    }

    #[test]
    fn a_failed_call_releases_its_reservation() {
        // A provider that failed did not charge. Holding the estimate would let
        // a flapping provider exhaust a budget it never spent.
        let chat = chat(Err(Unreachable::NoContact("no such binary".to_owned())));
        let mut agent = chat.agent.lock().expect("fresh");
        let commitment = agent.begin(MicroUsd(1_000), today_utc()).expect("fits");
        agent.abandon(commitment);
        assert_eq!(agent.ledger().spent, 0);
    }

    #[test]
    fn the_length_limit_counts_bytes_rather_than_characters() {
        // Truncating would send a mangled question to a metered provider and
        // charge for the answer. The limit exists because a prompt is charged
        // by the token.
        assert!(!overlong(&"x".repeat(MAX_QUESTION_BYTES)), "exactly at it");
        assert!(overlong(&"x".repeat(MAX_QUESTION_BYTES + 1)), "one past it");

        // Four bytes per character: comfortably under the limit counted one
        // way, comfortably over it counted the other.
        let emoji = "\u{1f680}".repeat(MAX_QUESTION_BYTES / 2);
        assert!(emoji.chars().count() < MAX_QUESTION_BYTES);
        assert!(overlong(&emoji), "counted in bytes, this is over");
    }

    #[test]
    fn the_day_advances_and_is_not_a_constant() {
        // The meter's day is an argument precisely so it can be replayed, which
        // means the impurity is this one function -- and a `today` stuck at zero
        // would make the daily ceiling a lifetime one.
        //
        // A range known in advance rather than a lower bound. `>` alone passes
        // for any arithmetic that grows -- seconds instead of days multiplies
        // by 86,400 and is still "greater than 20,000", which is a meter whose
        // day never repeats and whose ceiling therefore never binds.
        let today = today_utc();
        assert!(
            (20_000..30_000).contains(&today),
            "days since 1970 is ~20,700 in 2026 and ~30,000 in 2052; got {today}"
        );
    }

    #[test]
    fn an_unconfigured_agent_says_the_same_thing_to_everybody() {
        // Nothing to hide, and nothing to differ about. A public view that
        // diverged here would leak the one bit it is meant to publish.
        assert_eq!(status(None), public_status(None));
        assert_eq!(public_status(None), json!({ "configured": false }));
    }

    #[test]
    fn the_public_view_drops_the_spend_and_the_refusal_text() {
        // Finding H9. `/health` is reachable by anybody and it carried both.
        //
        // The spend is the scoreboard for whoever is trying to exhaust the
        // day's budget, which is the cheapest attack on this account. The
        // refusal text is the provider's own words about this account — a
        // platform lock reason or a policy notice, published.
        let chat = chat(Err(radar_model::Unreachable::NoContact("x".to_owned())));
        *chat.last.lock().expect("not poisoned") = LastCall::Failed {
            why: "your account is temporarily locked (code 326)".to_owned(),
        };
        let full = status(Some(&chat));
        let public = public_status(Some(&chat));

        // The full view still has them, because the operator needs them.
        assert!(full.get("spent_micro_usd").is_some());
        assert!(full.to_string().contains("326"));

        // The public one has neither, by any route: asserted against the whole
        // serialised body rather than field by field, because a nested field
        // added later would otherwise slip through a per-field check.
        let rendered = public.to_string();
        assert!(!rendered.contains("326"), "{rendered}");
        assert!(!rendered.contains("locked"), "{rendered}");
        assert!(!rendered.contains("spent_micro_usd"), "{rendered}");
        assert!(!rendered.contains("estimate_micro_usd"), "{rendered}");
    }

    #[test]
    fn the_public_view_still_says_whether_the_last_call_worked() {
        // The other half, and the one that keeps `radar brief` working without a
        // credential: a view that published nothing would make the monitor
        // permanently unable to see, which alarms for ever and is then ignored.
        let good = chat(Err(radar_model::Unreachable::NoContact("x".to_owned())));
        *good.last.lock().expect("not poisoned") = LastCall::Ok;
        let ok = public_status(Some(&good));
        assert_eq!(ok["configured"], json!(true));
        assert_eq!(ok["last"]["last_call"], json!("ok"));

        let failing = chat(Err(radar_model::Unreachable::NoContact("x".to_owned())));
        *failing.last.lock().expect("not poisoned") = LastCall::Failed {
            why: "whatever it was".to_owned(),
        };
        let bad = public_status(Some(&failing));
        assert_eq!(bad["last"]["last_call"], json!("failed"));
    }
}
