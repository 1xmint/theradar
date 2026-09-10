// SPDX-License-Identifier: Apache-2.0
//! The request Jupiter would actually receive.
//!
//! `Quote::from_response` is tested against captured bodies elsewhere. This file
//! tests the half that a body cannot show: **what goes out**. A router that
//! built a perfect URL and sent an empty `x-api-key` would pass every parsing
//! test in the suite and be rate-limited as an anonymous caller in production —
//! which is the exact failure `Credentials` exists to prevent, and the one
//! Jupiter will not report, because it answers keyless requests anyway.
//!
//! Both were live mutants: `replace Credentials::header_value -> &str with ""`
//! and `with "xyzzy"` survived the whole suite until this file existed.
//!
//! The server here is a socket on the loopback that answers once and records
//! what it was asked. No real endpoint is contacted and no real key exists.

use std::io::{Read, Write};
use std::time::Duration;

use radar_exec::route::{API_KEY_VAR, Credentials, QuoteRequest, RouteError, Router};
use radar_types::{Address, Asset};

/// The key the test router carries. Not a credential, and it leaves the machine
/// only over a loopback socket this test owns both ends of.
const TEST_KEY: &str = "test-key-that-authorises-nothing";

const CRLF_CRLF: &[u8] = b"\r\n\r\n";

fn credentials() -> Credentials {
    Credentials::from_vars(|k| (k == API_KEY_VAR).then(|| TEST_KEY.to_owned()))
        .expect("a supplied key makes credentials")
}

fn taker() -> Address {
    "CjfBjFVBs6QRvRTpMdKTBxZ7PZuJvHXWQKGRvR7wFbdz"
        .parse()
        .expect("a valid address")
}

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// A loopback server that answers one request and reports what it was asked.
///
/// Returns the endpoint to point a [`Router`] at, and a receiver carrying the
/// raw request head.
fn one_shot(status: u16, reason: &str, body: &str) -> (String, std::sync::mpsc::Receiver<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("an address").port();
    let (tx, rx) = std::sync::mpsc::channel();
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    // `accept` rather than `incoming()`: one request is the whole contract, and
    // a loop that always returns on its first pass is a lie about that.
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut buf = vec![0_u8; 8192];
        let mut n = 0;
        while n < buf.len() {
            let Ok(read) = stream.read(&mut buf[n..]) else {
                break;
            };
            if read == 0 {
                break;
            }
            n += read;
            if buf[..n].windows(4).any(|w| w == CRLF_CRLF) {
                break;
            }
        }
        let _ = tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });

    (format!("http://127.0.0.1:{port}/build"), rx)
}

/// The key is on the request, in the header Jupiter authenticates with.
#[test]
fn the_api_key_is_sent_verbatim_in_the_x_api_key_header() {
    let (endpoint, seen) = one_shot(200, "OK", &fixture("jupiter-build-sol-usdc.json"));
    let router = Router::with_endpoint(endpoint, credentials());

    let quote = router
        .quote(&QuoteRequest::new(
            Asset::Sol,
            Asset::Usdc,
            100_000_000,
            taker(),
        ))
        .expect("the served body is the captured 200");
    assert_eq!(quote.out_amount, 10_168_783, "the live path parses too");

    let request = seen
        .recv_timeout(Duration::from_secs(10))
        .expect("the server saw a request");
    let head = request.to_lowercase();

    assert!(
        head.contains(&format!("x-api-key: {TEST_KEY}")),
        "the key must reach Jupiter verbatim, not empty and not a placeholder. \
         Request head was:\n{request}"
    );

    // And the pair asked about is on the URL, in Jupiter's own parameter names.
    assert!(
        head.contains("inputmint=so11111111111111111111111111111111111111112"),
        "{request}"
    );
    assert!(
        head.contains("outputmint=epjfwdd5aufqssqem2qn1xzybapc8g4weggkzwytdt1v"),
        "{request}"
    );
    // `taker`, not v1's `userPublicKey`. The rename is the migration's one
    // required parameter change, and getting it wrong is a 400 in production.
    assert!(
        head.contains("taker=cjfbjfvbs6qrvrtpmdktbxz7pzujvhxwqkgrvr7wfbdz"),
        "{request}"
    );
    assert!(head.contains("amount=100000000"), "{request}");
}

/// A refusing status is read as a refusal, not parsed as a quote.
///
/// `if status >= 400` is one comparison, and inverting it was a live mutant: a
/// 400 would then be handed to the quote parser, and Jupiter's
/// `{"error":"No routes found"}` would surface as an unreadable-response
/// complaint rather than as the market condition it is.
#[test]
fn a_refusing_status_becomes_a_refusal_rather_than_a_parse_failure() {
    let (endpoint, _seen) = one_shot(
        400,
        "Bad Request",
        &fixture("jupiter-build-400-no-routes.json"),
    );
    let router = Router::with_endpoint(endpoint, credentials());

    let err = router
        .quote(&QuoteRequest::new(
            Asset::Sol,
            Asset::Usdc,
            100_000_000,
            taker(),
        ))
        .expect_err("400 is not a quote");

    assert!(
        matches!(err, RouteError::NoRoute { .. }),
        "a thin market must read as a thin market, not as a broken parser: {err}"
    );
}

/// A rejected key surfaces as an operator problem all the way through.
#[test]
fn a_rejected_key_survives_the_whole_request_path() {
    let (endpoint, _seen) = one_shot(
        401,
        "Unauthorized",
        &fixture("jupiter-build-401-unauthorized.json"),
    );
    let router = Router::with_endpoint(endpoint, credentials());

    let err = router
        .quote(&QuoteRequest::new(
            Asset::Usdc,
            Asset::Sol,
            10_000_000,
            taker(),
        ))
        .expect_err("401 is not a quote");

    assert!(
        matches!(err, RouteError::Unauthorized { status: 401, .. }),
        "got {err}"
    );
}
