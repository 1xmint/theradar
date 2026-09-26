// SPDX-License-Identifier: Apache-2.0
//! A minimal local HTTP stub for driving [`crate::cryptohouse::Client`]
//! through a real response, test-only.
//!
//! This crate is a guest on the public CryptoHouse endpoint (0036) and its
//! tests must never hit it, but `Client`'s query and quota-refusal counters
//! only move in response to an actual HTTP round trip. A tiny local server is
//! the way to exercise that counting without a network dependency or a real
//! endpoint.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

/// Starts a server that answers each accepted connection with the next
/// `(status, body)` pair in `responses`, in order, then exits.
///
/// Every response is sent with `Connection: close`, so [`ureq`]'s agent opens
/// a fresh connection for each query rather than reusing one -- which is what
/// lets a plain `accept` loop serve exactly `responses.len()` queries, one
/// per connection, in the order the client issues them.
///
/// Returns the endpoint URL to point a [`crate::cryptohouse::Client`] at.
pub(crate) fn start_server(responses: Vec<(u16, &'static str)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
    let addr = listener.local_addr().expect("local addr");
    thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            // The request bodies here are short SQL text, comfortably inside
            // one TCP segment, so one read is enough to let the client's
            // write complete before the response goes back.
            let mut buf = [0u8; 8192];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    format!("http://{addr}/")
}
