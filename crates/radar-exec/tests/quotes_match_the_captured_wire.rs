// SPDX-License-Identifier: Apache-2.0
//! The quote parser reads what Jupiter actually sent.
//!
//! Every body here is a real capture from `https://api.jup.ag/swap/v2/build`,
//! taken on 2026-09-09 and committed unedited under `fixtures/` — see that
//! directory's README for how, and for the three places the wire disagreed with
//! Jupiter's own published type. Tests written against a body this crate made
//! up would agree with themselves, which is LEARNINGS 10's shape.
//!
//! No network is touched here and no key is used. `Router::quote` is the thin
//! HTTP wrapper around `Quote::from_response`, which is what these exercise.

use radar_exec::route::{API_KEY_VAR, Credentials, Quote, QuoteRequest, RouteError, Router};
use radar_types::{Address, Asset};

/// The `taker` every capture was taken with. Radar holds no key for it.
const TAKER: &str = "CjfBjFVBs6QRvRTpMdKTBxZ7PZuJvHXWQKGRvR7wFbdz";

fn taker() -> Address {
    TAKER
        .parse()
        .expect("the captured taker is a valid address")
}

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// SOL in, USDC out — the direction the old `build_buy` could express.
#[test]
fn a_sol_to_usdc_capture_becomes_the_quote_it_describes() {
    let request = QuoteRequest::new(Asset::Sol, Asset::Usdc, 100_000_000, taker());
    let quote = Quote::from_response(&fixture("jupiter-build-sol-usdc.json"), &request)
        .expect("the capture is a well-formed answer to this request");

    // The numbers, not merely that it parsed. A parser that returned zeroes
    // would satisfy `is_ok`.
    assert_eq!(quote.in_amount, 100_000_000);
    assert_eq!(quote.out_amount, 10_168_783);
    assert_eq!(quote.worst_out, Some(10_067_096));
    assert_eq!(quote.swap_mode.as_deref(), Some("ExactIn"));

    // 0.0000320221058740248142431269 as a fraction is 0.32 bps, which rounds to
    // nothing. Reported as 0 rather than as absent, and the next test is the
    // reason that distinction is kept.
    assert_eq!(quote.impact_bps, 0);

    assert_eq!(
        quote.venues,
        ["Whirlpool", "Manifest", "AlphaQ", "Raydium", "Whirlpool"],
        "the venue labels in route order"
    );

    // The assets asked about survive, so a caller cannot lose the SOL/wSOL
    // distinction just because Jupiter names both by one mint.
    assert_eq!(quote.input, Asset::Sol);
    assert_eq!(quote.output, Asset::Usdc);
}

/// USDC in, SOL out. The same parser, the reverse direction.
///
/// This is what the task was for: `build_buy(mint, wallet, size_lamports)` could
/// only ever say "SOL in, this mint out". Nothing about this test is a second
/// code path — that is the point.
#[test]
fn the_same_parser_reads_the_reverse_direction() {
    let request = QuoteRequest::new(Asset::Usdc, Asset::Sol, 10_000_000, taker());
    let quote = Quote::from_response(&fixture("jupiter-build-usdc-sol.json"), &request)
        .expect("USDC in and SOL out is a quotable pair");

    assert_eq!(quote.in_amount, 10_000_000);
    assert_eq!(quote.out_amount, 98_346_493);
    assert_eq!(quote.venues, ["HumidiFi"]);
    assert_eq!(quote.input, Asset::Usdc);
    assert_eq!(quote.output, Asset::Sol);
    assert_eq!(quote.impact_bps, 0, "the capture reported \"0\"");
}

/// Wrapped SOL is quotable as either side too, under the same mint as native.
#[test]
fn wrapped_sol_quotes_as_either_side() {
    let out = QuoteRequest::new(Asset::WrappedSol, Asset::Usdc, 100_000_000, taker());
    assert!(Quote::from_response(&fixture("jupiter-build-sol-usdc.json"), &out).is_ok());

    let back = QuoteRequest::new(Asset::Usdc, Asset::WrappedSol, 10_000_000, taker());
    let quote = Quote::from_response(&fixture("jupiter-build-usdc-sol.json"), &back)
        .expect("USDC into wrapped SOL");
    assert_eq!(
        quote.output,
        Asset::WrappedSol,
        "the asset asked for, not the one the mint suggests"
    );
}

/// The catch: an answer about a different pair is refused, not filed.
///
/// Jupiter echoes `inputMint` and `outputMint`. Without this check a quote
/// fetched for one market and returned for another — a retry against a stale
/// URL, a mixed-up cache, a proxy — would be recorded as this market's price,
/// and nothing downstream would ever notice: the numbers are plausible.
#[test]
fn a_quote_for_a_different_pair_is_refused_rather_than_attributed() {
    // The SOL -> USDC body, offered as an answer to the USDC -> SOL question.
    let wrong_way = QuoteRequest::new(Asset::Usdc, Asset::Sol, 10_000_000, taker());
    let err = Quote::from_response(&fixture("jupiter-build-sol-usdc.json"), &wrong_way)
        .expect_err("a quote for the opposite direction must not be accepted");
    assert!(
        matches!(err, RouteError::Malformed(ref m) if m.contains("answered about")),
        "the refusal must name the mismatch, got {err}"
    );

    // And a third mint neither side asked about.
    let unrelated = QuoteRequest::new(
        Asset::spl(Address::new([0x22; 32])),
        Asset::Usdc,
        100_000_000,
        taker(),
    );
    assert!(
        Quote::from_response(&fixture("jupiter-build-sol-usdc.json"), &unrelated).is_err(),
        "an answer about wSOL is not an answer about another mint"
    );
}

/// The measurement that decides whether this route could ever be signed.
#[test]
fn every_captured_route_runs_through_lookup_tables_the_signer_refuses() {
    let out = QuoteRequest::new(Asset::Sol, Asset::Usdc, 100_000_000, taker());
    let quote =
        Quote::from_response(&fixture("jupiter-build-sol-usdc.json"), &out).expect("parses");
    assert_eq!(quote.lookup_tables, 5);
    assert!(
        !quote.signer_could_read(),
        "a route through lookup tables names accounts the signer cannot see"
    );

    // Asked with maxAccounts=20 — the smallest account set Jupiter would give —
    // and still not zero. That is why this branch does not try to assemble a
    // legacy transaction and call it supported.
    let back = QuoteRequest::new(Asset::Usdc, Asset::Sol, 10_000_000, taker());
    let quote =
        Quote::from_response(&fixture("jupiter-build-usdc-sol.json"), &back).expect("parses");
    assert_eq!(quote.lookup_tables, 1);
    assert!(!quote.signer_could_read());
}

/// Jupiter's own refusal words, from the captured bodies.
#[test]
fn the_captured_refusals_classify_as_what_they_are() {
    let no_routes = RouteError::from_status(
        400,
        &fixture("jupiter-build-400-no-routes.json"),
        "SomeMint",
        1_000,
    );
    assert_eq!(
        no_routes,
        RouteError::NoRoute {
            mint: "SomeMint".to_owned(),
            size_lamports: 1_000,
        },
        "a thin market is a market condition"
    );

    let rejected = RouteError::from_status(
        401,
        &fixture("jupiter-build-401-unauthorized.json"),
        "SomeMint",
        1_000,
    );
    assert!(
        matches!(rejected, RouteError::Unauthorized { status: 401, ref body } if body == "Unauthorized"),
        "a rejected key is an operator problem and must not read as a thin market: got {rejected}"
    );

    // 400 with "No routes found" is read from `error`; 401 from `message`.
    // Jupiter uses both key names, so both are read, and neither is guessed.
    let odd = RouteError::from_status(500, "<html>gateway</html>", "SomeMint", 1_000);
    assert!(
        matches!(odd, RouteError::Unavailable(ref m) if m.contains("500")),
        "got {odd}"
    );
}

/// A missing impact is maximal, and a missing floor is absent rather than zero.
#[test]
fn what_jupiter_did_not_say_is_never_read_as_zero() {
    // A minimal body in the wire's own shape, with the two optional fields
    // dropped. AGENTS rule 9: absent is not zero, unknown is not safe.
    let body = r#"{
        "inputMint": "So11111111111111111111111111111111111111112",
        "outputMint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        "inAmount": "100000000",
        "outAmount": "10168783",
        "swapMode": "ExactIn",
        "routePlan": []
    }"#;
    let request = QuoteRequest::new(Asset::Sol, Asset::Usdc, 100_000_000, taker());
    let quote = Quote::from_response(body, &request).expect("the required fields are all here");

    assert_eq!(
        quote.impact_bps,
        u32::MAX,
        "an unstated price impact must price as the worst case, not the best"
    );
    assert_eq!(
        quote.worst_out, None,
        "an unstated floor is unstated; 0 would be a claim that nothing is guaranteed"
    );
    assert_eq!(quote.lookup_tables, 0, "a null map is no tables");
}

/// A zero out is a refusal, not a quote of nothing.
#[test]
fn a_zero_output_is_no_route_rather_than_a_free_trade() {
    let body = r#"{
        "inputMint": "So11111111111111111111111111111111111111112",
        "outputMint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        "inAmount": "100000000",
        "outAmount": "0",
        "priceImpactPct": "0",
        "routePlan": []
    }"#;
    let request = QuoteRequest::new(Asset::Sol, Asset::Usdc, 100_000_000, taker());
    let err = Quote::from_response(body, &request).expect_err("zero out is not a price");
    assert!(matches!(err, RouteError::NoRoute { .. }), "got {err}");
}

/// Without the key there is no router, so there is nothing to quote with.
///
/// The deny-by-default rule, checked at the only place it can be bypassed.
/// Jupiter answers keyless requests — every fixture here proves it — so this
/// refusal is Radar's, and it has to be Radar's.
#[test]
fn no_key_means_no_router_at_all() {
    assert!(
        Credentials::from_vars(|_| None).is_none(),
        "an absent key must not produce credentials"
    );
    assert!(
        Router::from_env().is_none() || std::env::var(API_KEY_VAR).is_ok(),
        "a router exists from the environment only when the key does"
    );

    // And with a key, a router can be built without touching the network.
    let credentials = Credentials::from_vars(|k| {
        (k == API_KEY_VAR).then(|| "test-key-that-authorises-nothing".to_owned())
    })
    .expect("a supplied key makes credentials");
    let rendered = format!("{:?}", Router::new(credentials));
    assert!(
        !rendered.contains("test-key-that-authorises-nothing"),
        "the key must not survive into a formatter: {rendered}"
    );
}
