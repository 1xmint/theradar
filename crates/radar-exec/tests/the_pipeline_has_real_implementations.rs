// SPDX-License-Identifier: Apache-2.0
//! The pipeline's traits have implementations that are not test stubs.
//!
//! Until 2026-09-01 they did not. `Routing` and `Sending` were implemented only
//! by `FixedRoute` and `CountingSender` inside `pipeline.rs`'s own test module,
//! so the executor could be composed **only** against a fixture — while
//! `route::Router` and `submit::Submitter`, which talk to Jupiter and to an RPC
//! node, sat beside them unconnected.
//!
//! That is exactly the shape [LEARNINGS](../../../LEARNINGS.md) 10 records: a
//! lane whose every stage passes its tests against something the real one would
//! never produce. A live run over 41,254 candidates then raised zero proposals,
//! because a hardcoded probe size made a proposal arithmetically impossible and
//! no fixture had ever been shaped like a real candidate.
//!
//! These tests cannot reach Jupiter or an RPC node, and should not: what they
//! establish is that the trait methods **delegate to the real ones** rather than
//! being present and inert. Both point at an address nothing answers on, so a
//! stub returning success would be visible immediately.
//!
//! One of those guarantees changed on 2026-09-09 and the change is deliberate.
//! `Routing::build_buy` no longer reaches the network, because no Jupiter
//! endpoint returns a transaction `radar-signer` can read any more — see
//! `the_routing_stage_refuses_a_transaction_rather_than_returning_an_unsignable_one`
//! below, and the module documentation of `radar_exec::route`. The router's
//! live path is now `Router::quote`, and that is what is checked for delegation.

use radar_exec::pipeline::{Routing, Sending};
use radar_exec::route::{Credentials, QuoteRequest, RouteError, Router, API_KEY_VAR};
use radar_exec::submit::Submitter;
use radar_types::{Address, Asset};

/// An endpoint that refuses a connection immediately.
///
/// Port 1 on the loopback, deliberately, and not a black-holed address like
/// `192.0.2.1`. That was the first choice and it cost thirty seconds per run:
/// a black hole is not refused, it is waited on, so each of these tests paid a
/// full connect timeout to learn something a refusal says at once.
///
/// Port 1 needs root to bind and nothing does, so a refusal is what arrives.
const NOWHERE: &str = "http://127.0.0.1:1/";

/// Credentials that authorise nothing, for a router that never reaches Jupiter.
fn credentials() -> Credentials {
    Credentials::from_vars(|k| {
        (k == API_KEY_VAR).then(|| "test-key-that-authorises-nothing".to_owned())
    })
    .expect("a supplied key makes credentials")
}

#[test]
fn quoting_reaches_the_real_router() {
    // A stub would answer without touching the network. The real one tries, and
    // fails, which is the observable difference.
    let router = Router::with_endpoint(NOWHERE, credentials());
    let outcome = router.quote(&QuoteRequest::new(
        Asset::Sol,
        Asset::Usdc,
        1_000_000,
        Address::new([0x33; 32]),
    ));
    assert!(
        outcome.is_err(),
        "an unreachable Jupiter must produce an error, not a quote"
    );
}

/// The routing stage refuses to produce a transaction, and says why.
///
/// This test replaced one asserting that `Routing::build_buy` delegated to a
/// real network call. It no longer does, because as of 2026-09-09 there is no
/// Jupiter endpoint that returns a transaction `radar-signer` can read: the
/// Router returns raw instructions and address lookup tables, and `lite-api`'s
/// `asLegacyTransaction` is deprecated. The honest implementation is a refusal.
///
/// So this test is the successor guarantee, and it is a stronger one than "it
/// tried the network": **the pipeline can never be handed an unsignable
/// transaction by this router**, because it is never handed one at all. A
/// regression that started returning bytes here would fail this test.
#[test]
fn the_routing_stage_refuses_a_transaction_rather_than_returning_an_unsignable_one() {
    let router = Router::with_endpoint(NOWHERE, credentials());
    let err = Routing::build_buy(
        &router,
        &Address::new([0x22; 32]),
        &Address::new([0x33; 32]),
        1_000_000,
    )
    .expect_err("the Router API cannot supply a signable transaction");

    // Not `NoRoute`. An operator reading this must not conclude the market was
    // thin and try a different token forever.
    assert!(
        matches!(err, RouteError::Unverifiable(ref m) if m.contains("lookup tables")),
        "the refusal must carry the reason, got {err}"
    );
}

#[test]
fn sending_reaches_the_real_submitter() {
    // Rule 7 lives in `Submitter`: it takes a direct RPC endpoint and never the
    // x402 lane. A stub here would quietly bypass that.
    let submitter = Submitter::new(NOWHERE);
    let outcome = Sending::send(&submitter, "AQAB");
    assert!(
        outcome.is_err(),
        "an unreachable node must produce an error, not a signature"
    );
}

#[test]
fn the_nodes_own_words_survive_to_the_caller() {
    // `Sending` carries whatever the node said rather than a category this crate
    // chose for it. An operator at three in the morning needs the node's words;
    // a label picked in advance by this code tells them less.
    let submitter = Submitter::new(NOWHERE);
    let through_trait = Sending::send(&submitter, "AQAB").expect_err("unreachable");
    let direct = submitter.send("AQAB").expect_err("unreachable");

    assert_eq!(
        through_trait,
        direct.to_string(),
        "the flattened error must be the real one's own message"
    );
    assert!(
        !through_trait.is_empty(),
        "an empty reason is not a reason: {through_trait}"
    );
}
