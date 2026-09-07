// SPDX-License-Identifier: Apache-2.0
//! Talking to the signer.
//!
//! # Why this is a socket and not a child process
//!
//! The property that matters is that the executor cannot read the key file. If
//! the executor spawned the signer, the child would inherit the executor's user,
//! so the key would have to be readable by the executor's user — and the
//! separation would be decorative.
//!
//! So the signer runs as its own user, and the executor reaches it over a Unix
//! socket whose ownership and mode systemd sets. With `Accept=yes` on the socket
//! unit, systemd hands each accepted connection to a fresh signer instance on
//! **stdin and stdout**, which is exactly the interface `radar-signer` already
//! speaks. No code in the signer knows a socket exists; the kernel and systemd
//! do the access control, which is the right place for it.
//!
//! See `deploy/radar-signer.socket`.
//!
//! # Why the transport is a trait bound
//!
//! [`StreamSigner`] works over anything readable and writable, so the protocol
//! is tested against an ordinary pipe on any platform. A Unix socket is then one
//! small constructor, and the part that could be wrong — the framing — is the
//! part that is covered.

use std::io::{BufRead as _, BufReader, Read, Write};

use radar_risk::Authorization;

use crate::pipeline::{Bounds, Signing};

/// A signer reached over a byte stream.
///
/// One request, one line; one response, one line. Newline-delimited JSON,
/// because the framing has to be something both ends agree on without a length
/// negotiation that could disagree.
pub struct StreamSigner<S> {
    stream: std::cell::RefCell<Framed<S>>,
}

struct Framed<S> {
    reader: BufReader<S>,
    writer: S,
}

impl<S: Read + Write> StreamSigner<S> {
    /// Wraps a duplex stream. `read` and `write` are usually clones of one
    /// socket.
    pub fn new(read: S, write: S) -> Self {
        Self {
            stream: std::cell::RefCell::new(Framed {
                reader: BufReader::new(read),
                writer: write,
            }),
        }
    }
}

impl<S: Read + Write> Signing for StreamSigner<S> {
    fn sign(
        &self,
        authorization: &Authorization,
        transaction: &str,
        bounds: Bounds,
    ) -> Result<String, Vec<String>> {
        let request = serde_json::json!({
            // The signer serves two kinds of request now -- a local signature
            // and a Privy authorization signature -- and the tag is required
            // with no default. An untagged request is refused rather than
            // assumed to be this one, so a half-updated deployment stops
            // signing instead of guessing.
            "sign": "local",
            "authorization": authorization,
            "transaction": transaction,
            // The signer needs a slot to check expiry against and has no RPC
            // of its own, so the caller supplies one. This was `0` until
            // 2026-09-07 and zero is inside every window, which made both the
            // expiry check and ADR 0008's staleness check unreachable from
            // production. Sent explicitly rather than omitted so the signer's
            // own field is never optional, which is how a missing value becomes
            // a default that passes.
            "now_slot": bounds.now.get(),
            // The authorization is in micro-USD and the transaction in
            // lamports. The signer has no price feed, so the conversion is
            // stated here and the signer holds us to it. It can only narrow.
            "max_lamports": bounds.max_lamports,
        });

        let mut framed = self.stream.borrow_mut();
        if writeln!(framed.writer, "{request}").is_err() || framed.writer.flush().is_err() {
            return Err(vec!["signer is not reachable".to_owned()]);
        }

        let mut line = String::new();
        if framed.reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            // No answer is a refusal, never a pass. A signer that died must not
            // become a signer that agreed.
            return Err(vec!["signer did not answer".to_owned()]);
        }

        let Ok(response) = serde_json::from_str::<serde_json::Value>(&line) else {
            return Err(vec!["signer answered unreadably".to_owned()]);
        };

        match response.get("outcome").and_then(serde_json::Value::as_str) {
            Some("signed") => response
                .get("transaction")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .ok_or_else(|| vec!["signer reported success with no transaction".to_owned()]),
            _ => Err(response
                .get("reasons")
                .and_then(serde_json::Value::as_array)
                .map_or_else(
                    || vec!["signer refused without saying why".to_owned()],
                    |rs| {
                        rs.iter()
                            .filter_map(|r| r.as_str().map(ToOwned::to_owned))
                            .collect()
                    },
                )),
        }
    }
}

/// Connects to the signer's Unix socket.
///
/// # Errors
///
/// Returns the connection error. A signer that cannot be reached refuses
/// everything, which is the correct behaviour — but the caller should know the
/// difference between "refused" and "absent", so this is an error rather than a
/// silently refusing client.
#[cfg(unix)]
pub fn connect(
    path: &std::path::Path,
) -> std::io::Result<StreamSigner<std::os::unix::net::UnixStream>> {
    let stream = std::os::unix::net::UnixStream::connect(path)?;
    let read = stream.try_clone()?;
    Ok(StreamSigner::new(read, stream))
}

#[cfg(test)]
mod tests {
    use radar_risk::Action;
    use radar_types::{Address, MicroUsd, Slot};

    use super::*;

    /// A stream that replays a canned answer and records what was written.
    struct Canned {
        answer: std::io::Cursor<Vec<u8>>,
        written: Vec<u8>,
    }

    impl Canned {
        fn new(answer: &str) -> Self {
            Self {
                answer: std::io::Cursor::new(answer.as_bytes().to_vec()),
                written: Vec::new(),
            }
        }
    }

    impl Read for Canned {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.answer.read(buf)
        }
    }

    impl Write for Canned {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.written.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// A shared buffer holding everything the client wrote.
    type Written = std::rc::Rc<std::cell::RefCell<Vec<u8>>>;

    /// A stream like [`Canned`], except the bytes written are kept where the
    /// test can read them. `Canned` drops them, which is why the request itself
    /// went unasserted for as long as it did.
    struct Recording(Written, std::io::Cursor<Vec<u8>>);

    impl Recording {
        fn new(written: &Written, answer: &[u8]) -> Self {
            Self(
                std::rc::Rc::clone(written),
                std::io::Cursor::new(answer.to_vec()),
            )
        }
    }

    impl Read for Recording {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.1.read(buf)
        }
    }

    impl Write for Recording {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn authorization() -> Authorization {
        Authorization {
            nonce: "n".to_owned(),
            mint: Address::new([2u8; 32]),
            action: Action::Buy,
            max_notional: MicroUsd::from_dollars(10.0),
            expires_after: Slot(1_150),
            needs_operator_signature: false,
        }
    }

    fn signer(answer: &str) -> StreamSigner<Canned> {
        StreamSigner::new(Canned::new(answer), Canned::new(""))
    }

    /// Bounds a test does not care about.
    const fn bounds() -> Bounds {
        Bounds {
            now: Slot(1_100),
            max_lamports: 20_000_000,
        }
    }

    #[test]
    fn a_signed_answer_yields_the_transaction() {
        let s =
            signer(r#"{"outcome":"signed","signature":"sig","wallet":"w","transaction":"dHg="}"#);
        assert_eq!(
            s.sign(&authorization(), "unsigned", bounds()),
            Ok("dHg=".to_owned())
        );
    }

    #[test]
    fn a_refusal_carries_its_reasons_through() {
        let s = signer(r#"{"outcome":"refused","reasons":["mint absent","expired"]}"#);
        assert_eq!(
            s.sign(&authorization(), "unsigned", bounds()),
            Err(vec!["mint absent".to_owned(), "expired".to_owned()])
        );
    }

    #[test]
    fn a_silent_signer_is_a_refusal_not_a_pass() {
        // The failure mode worth being certain about: a signer that died must
        // never become a signer that agreed.
        assert!(
            signer("")
                .sign(&authorization(), "unsigned", bounds())
                .is_err()
        );
    }

    #[test]
    fn an_unreadable_answer_is_a_refusal() {
        assert!(
            signer("not json\n")
                .sign(&authorization(), "x", bounds())
                .is_err()
        );
    }

    #[test]
    fn a_success_with_no_transaction_is_a_refusal() {
        // Nothing to submit is not a success, however the answer is labelled.
        let s = signer(r#"{"outcome":"signed","signature":"sig","wallet":"w"}"#);
        assert!(s.sign(&authorization(), "unsigned", bounds()).is_err());
    }

    #[test]
    fn an_unlabelled_answer_is_a_refusal() {
        // Anything other than an explicit success is a refusal, so a signer
        // speaking a future protocol version fails closed.
        assert!(
            signer("{}\n")
                .sign(&authorization(), "x", bounds())
                .is_err()
        );
    }

    #[test]
    fn the_request_carries_the_slot_and_the_size_the_executor_meant() {
        // The bug this replaces: `now_slot` was the literal `0`, which is inside
        // every expiry window, so neither the signer's expiry check nor ADR
        // 0008's staleness check could ever fire from production. A test that
        // only asserted the answer came back would not have seen it, so this
        // asserts what went *out*.
        let written = Written::default();

        let answer = r#"{"outcome":"signed","signature":"s","wallet":"w","transaction":"dHg="}"#;
        let s = StreamSigner::new(
            Recording::new(&written, answer.as_bytes()),
            Recording::new(&written, b""),
        );
        s.sign(&authorization(), "unsigned", bounds())
            .expect("the canned answer is a signature");

        let sent = String::from_utf8(written.borrow().clone()).expect("utf-8");
        let value: serde_json::Value = serde_json::from_str(sent.trim()).expect("one JSON line");
        assert_eq!(value["now_slot"], 1_100, "the real head, not zero");
        assert_eq!(value["max_lamports"], 20_000_000);
    }
}
