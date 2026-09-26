// SPDX-License-Identifier: Apache-2.0
//! A small read-only client for CryptoHouse.
//!
//! CryptoHouse is a free public ClickHouse holding the whole Solana chain
//! (ADR 0002). Radar uses it for **bulk extraction into its own store, once** —
//! never on a hot path and never as a live provider lane. After extraction the
//! data is ours and the service going away costs nothing.
//!
//! Being a guest here is a real constraint. The credentials ship in the public
//! web client, so this is a public read endpoint, but that is an implicit
//! invitation rather than an explicit one. Queries are windowed so each one stays
//! well inside the server's sixty-second cap, and the extractor paces itself
//! between them.

use std::time::Duration;

use serde::de::DeserializeOwned;

/// The public endpoint.
pub const ENDPOINT: &str = "https://crypto-clickhouse.clickhouse.com/";
/// The read-only user the public web client uses.
pub const USER: &str = "crypto";

/// A CryptoHouse query failed.
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    /// The request did not complete.
    #[error("cryptohouse transport: {0}")]
    Transport(String),
    /// The server rejected or could not finish the query.
    ///
    /// Two of these mean "the window was too wide" rather than "something is
    /// broken", and the extractor narrows and retries: a sixty-second execution
    /// timeout, and the thousand-row result cap. Both are fixed on the public
    /// endpoint and cannot be raised — the user is `readonly=1`.
    #[error("cryptohouse: {0}")]
    Server(String),
    /// A row did not match the expected shape.
    #[error("cryptohouse row: {0}")]
    Row(#[from] serde_json::Error),
}

/// Marker text every declared-budget exhaustion carries in its message, so a
/// caller can tell "a query ceiling this process chose ran out" apart from
/// every other CryptoHouse failure — the same way [`QueryError::should_narrow`]
/// classifies by substring rather than by a dedicated variant. One constant so
/// every budget that reports exhaustion this way, in whichever module builds
/// it, is found by the same check.
pub const BUDGET_EXHAUSTED_MARKER: &str = "query budget spent";

impl QueryError {
    /// Whether narrowing the window and retrying is worth trying.
    ///
    /// True for the two limits a wide window runs into. Anything else — a bad
    /// identifier, a transport failure — will fail identically on a narrower
    /// window, and retrying would only hammer a public endpoint we are a guest on.
    #[must_use]
    pub fn should_narrow(&self) -> bool {
        matches!(
            self,
            Self::Server(m)
                if m.contains("TIMEOUT_EXCEEDED") || m.contains("TOO_MANY_ROWS_OR_BYTES")
        )
    }

    /// Whether this failure is a declared query budget running out, rather
    /// than a real CryptoHouse refusal.
    ///
    /// A budget-exhausted read must not be mistaken for the endpoint itself
    /// failing: one is this process choosing to stop, the other is the vendor
    /// or the query. Conflating them would make a `consider` run started with
    /// a small budget look, to `radar brief`, exactly like CryptoHouse being
    /// down.
    #[must_use]
    pub fn is_budget_exhausted(&self) -> bool {
        matches!(self, Self::Server(m) if m.contains(BUDGET_EXHAUSTED_MARKER))
    }

    /// Whether this failure is CryptoHouse itself refusing the query because
    /// the shared 120-an-hour allowance is spent, rather than this process's
    /// own declared ceiling ([`Self::is_budget_exhausted`]) or an unrelated
    /// server error.
    ///
    /// The text is the vendor's own, quoted in
    /// [0036](../../../docs/research/0036-the-hourly-consider-run-eats-the-whole-cryptohouse-allowance.md):
    /// `Quota for user 'crypto' for 3600s has been exceeded: queries = 133/120`.
    /// "has been exceeded" is the stable part; the numbers move every hour.
    #[must_use]
    pub fn is_quota_exceeded(&self) -> bool {
        matches!(self, Self::Server(m) if m.contains("has been exceeded"))
    }
}

/// A read-only CryptoHouse client.
///
/// Counts every query it issues and every one CryptoHouse refused for quota,
/// so a caller can log its own totals when a run ends -- 0036's "what was not
/// checked": the per-candidate query cost was read from the call sites, not
/// counted from a log, because nothing counted. Counting here rather than in
/// each caller covers every query any of them issues, including the ones
/// [`crate::narrowing_fetch`] generates by halving a window, without a second
/// counter to keep in step with this one.
pub struct Client {
    endpoint: String,
    agent: ureq::Agent,
    queries: std::cell::Cell<u64>,
    quota_refused: std::cell::Cell<u64>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new(ENDPOINT)
    }
}

impl Client {
    /// A client for the given endpoint.
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(180)))
            // **Do not turn a non-2xx into an error before its body is read.**
            //
            // ClickHouse puts the whole explanation in the body and nothing
            // useful in the status: the thousand-row cap arrives as a bare
            // HTTP 500 whose body names `TOO_MANY_ROWS_OR_BYTES`. With ureq's
            // default, that became `Error::StatusCode(500)` with the body
            // dropped, so [`QueryError::should_narrow`] — which looks for
            // exactly that text — returned false, and a window that needed
            // halving failed outright instead.
            //
            // That is why the `trades` table was empty. `--scope trades` over
            // any window wide enough to be worth running returns far more than
            // a thousand rows, every attempt hit the cap, and every attempt
            // gave up at the first response rather than narrowing. The
            // narrowing code was correct and never ran. Found 2026-09-11.
            .http_status_as_error(false)
            .build();
        Self {
            endpoint: endpoint.into(),
            agent: config.into(),
            queries: std::cell::Cell::new(0),
            quota_refused: std::cell::Cell::new(0),
        }
    }

    /// How many queries this client has sent to CryptoHouse, successful or
    /// not, since it was built.
    ///
    /// Counts every attempt that reached [`Self::query`], including ones a
    /// caller's own [`crate::launch_block::Budget`] or
    /// [`crate::market_tape::Budget`] declined to make -- those never call
    /// this method, so a run capped well under the shared allowance reports
    /// the queries it actually spent, not the ones it was asked for.
    #[must_use]
    pub fn queries_issued(&self) -> u64 {
        self.queries.get()
    }

    /// How many of those CryptoHouse itself refused for quota
    /// ([`QueryError::is_quota_exceeded`]).
    #[must_use]
    pub fn quota_refusals(&self) -> u64 {
        self.quota_refused.get()
    }

    /// Runs a query and deserialises each row.
    ///
    /// Rows come back as `JSONEachRow`, one JSON object per line, which streams
    /// without the server buffering a whole result set.
    ///
    /// # Errors
    ///
    /// Returns [`QueryError`] if the request fails, the server rejects the query,
    /// or a row does not deserialise.
    pub fn query<T: DeserializeOwned>(&self, sql: &str) -> Result<Vec<T>, QueryError> {
        self.queries.set(self.queries.get() + 1);
        let result = self.query_uncounted(sql);
        if let Err(e) = &result
            && e.is_quota_exceeded()
        {
            self.quota_refused.set(self.quota_refused.get() + 1);
        }
        result
    }

    /// The request [`Self::query`] counts before and grades after.
    ///
    /// Split out so the counting is one pair of lines around a call, not
    /// threaded through every early return the request itself has.
    fn query_uncounted<T: DeserializeOwned>(&self, sql: &str) -> Result<Vec<T>, QueryError> {
        // POST with the SQL in the body rather than GET with it in the URL.
        // An outcome batch names four hundred mints, which is roughly 18 KB of
        // query -- well past the URL length most proxies accept, and the failure
        // arrives as a bare 404 that looks like a missing endpoint rather than an
        // oversized request.
        let mut response = match self
            .agent
            .post(&self.endpoint)
            .query("user", USER)
            .content_type("text/plain; charset=utf-8")
            .send(format!("{sql} FORMAT JSONEachRow"))
        {
            Ok(r) => r,
            Err(e) => return Err(QueryError::Transport(e.to_string())),
        };

        // The status is read but never used to decide the outcome on its own:
        // `http_status_as_error(false)` is set precisely so the body arrives
        // whatever the status, because that is where ClickHouse says what
        // happened. It is carried into the message only so an operator can
        // see it.
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| QueryError::Transport(e.to_string()))?;

        if is_error_status(status) {
            return Err(server_error(status, &body, sql));
        }

        parse_rows(&body)
    }
}

/// Whether an HTTP status means the body is an explanation rather than rows.
///
/// Pulled out beside [`server_error`] and for the same reason: inside
/// [`Client::query`] the only way to exercise the boundary was a live request,
/// so nothing checked which side of 400 each status fell on. A `<` here rather
/// than a `>=` would feed ClickHouse's error text to `parse_rows` and report
/// the failure as a malformed row -- a fact about this build, for something
/// that is a fact about the query.
#[must_use]
const fn is_error_status(status: u16) -> bool {
    status >= 400
}

/// Builds the error for a non-2xx response, from its status and its body.
///
/// **Pure, and separate from [`Client::query`], because the bug it fixes is
/// only visible in what this returns.** The thousand-row cap arrives as a bare
/// HTTP 500 whose body names `TOO_MANY_ROWS_OR_BYTES`, and
/// [`QueryError::should_narrow`] decides whether to halve the window by looking
/// for exactly that text. An earlier version formatted the status and the first
/// hundred characters of the *query* and discarded the body — so the marker was
/// never present, `should_narrow` returned false, and a window that needed
/// halving failed outright. Every `--scope trades` run hit this, which is why
/// the `trades` table was empty. Found 2026-09-11.
///
/// Living inside `query` meant the only way to exercise it was a live request
/// against a public endpoint. Out here it takes a status and a body, so the
/// wrong behaviour can be reapplied in a test.
///
/// The body leads and is truncated at 600 characters: both markers appear near
/// the front of a ClickHouse exception, which can otherwise carry a long stack.
/// The query's first hundred characters follow, because an error that says only
/// "404" could be any of several queries in a batch run.
#[must_use]
fn server_error(status: u16, body: &str, sql: &str) -> QueryError {
    let detail: String = body.trim().chars().take(600).collect();
    let head: String = sql.chars().take(100).collect();
    QueryError::Server(format!("HTTP {status}: {detail} -- rejecting: {head}..."))
}

/// Parses a `JSONEachRow` body, surfacing a server exception as an error.
///
/// ClickHouse reports failures as a normal-looking row containing `exception`,
/// so a parser that only looked at the HTTP status would treat a timeout as an
/// empty result — and an empty result from a backfill is a silent gap.
///
/// # Errors
///
/// Returns [`QueryError::Server`] if the body carries an exception, or
/// [`QueryError::Row`] if a line does not deserialise.
pub fn parse_rows<T: DeserializeOwned>(body: &str) -> Result<Vec<T>, QueryError> {
    let mut out = Vec::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line)
            && let Some(e) = v.get("exception").and_then(serde_json::Value::as_str)
        {
            return Err(QueryError::Server(e.to_owned()));
        }
        out.push(serde_json::from_str(line)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize, PartialEq, Eq, Debug)]
    struct Row {
        n: String,
    }

    #[test]
    fn rows_parse_one_per_line() {
        let rows: Vec<Row> = parse_rows("{\"n\":\"1\"}\n{\"n\":\"2\"}\n").expect("parses");
        assert_eq!(rows, vec![Row { n: "1".into() }, Row { n: "2".into() }]);
    }

    #[test]
    fn the_row_cap_narrows_the_window_like_a_timeout_does() {
        // The public endpoint caps results at a thousand rows and will not let a
        // readonly user raise it, so overflow is a routine signal to ask for less
        // rather than a failure.
        let body = "{\"exception\": \"Code: 396. DB::Exception: Limit for result exceeded,                     max rows: 1.00 thousand (TOO_MANY_ROWS_OR_BYTES)\"}";
        assert!(
            parse_rows::<Row>(body)
                .expect_err("must error")
                .should_narrow()
        );
    }

    /// 400 is the boundary, and both sides of it are pinned.
    ///
    /// ClickHouse answers a rejected query with a non-2xx whose body carries
    /// the explanation, and a success with rows. Reading 400 as success feeds
    /// the explanation to `parse_rows`, which then reports a fact about this
    /// build for something that is a fact about the query; reading 399 as a
    /// failure throws away rows that arrived.
    #[test]
    fn four_hundred_is_where_a_body_stops_being_rows() {
        assert!(!is_error_status(200));
        assert!(!is_error_status(204));
        assert!(!is_error_status(399), "399 is not an error status");
        assert!(is_error_status(400), "400 is");
        assert!(is_error_status(500));
    }

    /// The row cap as it actually arrives: a bare HTTP 500 with the marker in
    /// the body.
    ///
    /// Reapply the bug by having `server_error` ignore `body` — format the
    /// status and the query alone, as it did before 2026-09-11 — and this
    /// fails while
    /// `the_row_cap_narrows_the_window_like_a_timeout_does` still passes,
    /// because that one feeds the body in directly and never goes near a
    /// status code. That gap is the whole reason `--scope trades` never
    /// worked.
    #[test]
    fn a_row_cap_reported_as_http_500_still_narrows() {
        let body = "Code: 396. DB::Exception: Limit for result exceeded, max rows: 1.00 thousand (TOO_MANY_ROWS_OR_BYTES) (version 26.4.1.2212)";
        let err = server_error(500, body, "SELECT mint FROM solana.token_transfers");
        assert!(
            err.should_narrow(),
            "a 500 carrying TOO_MANY_ROWS_OR_BYTES must narrow, not fail: {err}"
        );
    }

    /// A timeout reported the same way, for the same reason.
    #[test]
    fn a_timeout_reported_as_http_500_still_narrows() {
        let body = "Code: 159. DB::Exception: Timeout exceeded (TIMEOUT_EXCEEDED)";
        let err = server_error(500, body, "SELECT 1");
        assert!(err.should_narrow(), "{err}");
    }

    /// And a genuine mistake still does not, because narrowing it would only
    /// hammer a public endpoint with the same broken query.
    #[test]
    fn a_bad_identifier_does_not_narrow_however_it_is_reported() {
        let body =
            "Code: 47. DB::Exception: Unknown expression identifier `mnit` (UNKNOWN_IDENTIFIER)";
        let err = server_error(404, body, "SELECT mnit FROM solana.token_transfers");
        assert!(!err.should_narrow(), "{err}");
    }

    /// The status reaches the operator, and so does which query failed.
    #[test]
    fn the_error_names_the_status_and_the_query_that_failed() {
        let err = server_error(
            500,
            "Code: 396 ...",
            "SELECT mint FROM solana.token_transfers",
        );
        let text = err.to_string();
        assert!(text.contains("500"), "{text}");
        assert!(text.contains("solana.token_transfers"), "{text}");
    }

    #[test]
    fn a_server_exception_is_an_error_rather_than_an_empty_result() {
        // ClickHouse returns failures as a normal-looking row. Treating that as
        // zero rows would write a silent gap into the store, and a gap in a
        // backfill is indistinguishable from a quiet market.
        let body =
            "{\"exception\": \"Code: 159. DB::Exception: Timeout exceeded (TIMEOUT_EXCEEDED)\"}";
        let err = parse_rows::<Row>(body).expect_err("must error");
        assert!(err.should_narrow(), "{err}");
    }

    #[test]
    fn a_query_error_is_not_retried_by_narrowing() {
        // A bad identifier fails identically on a narrower window; retrying would
        // only hammer an endpoint we are a guest on.
        let body = "{\"exception\": \"Code: 47. DB::Exception: Unknown identifier\"}";
        assert!(
            !parse_rows::<Row>(body)
                .expect_err("must error")
                .should_narrow()
        );
    }

    #[test]
    fn an_empty_body_is_zero_rows_not_an_error() {
        let rows: Vec<Row> = parse_rows("").expect("parses");
        assert!(rows.is_empty());
    }

    #[test]
    fn a_budget_exhaustion_message_is_recognised() {
        // Any budget that reports exhaustion through the shared marker must be
        // found this way, whichever module built it and whatever the rest of
        // the sentence says.
        let err = QueryError::Server(
            "consider query budget spent for this run; remaining candidates were not considered"
                .to_owned(),
        );
        assert!(err.is_budget_exhausted(), "{err}");
    }

    #[test]
    fn an_ordinary_server_error_is_not_mistaken_for_budget_exhaustion() {
        // A real CryptoHouse refusal must not be read as this process's own
        // declared ceiling -- that would hide the endpoint actually failing
        // behind "the budget ran out", which is a different fix.
        let body = "Code: 159. DB::Exception: Timeout exceeded (TIMEOUT_EXCEEDED)";
        let err = server_error(500, body, "SELECT 1");
        assert!(!err.is_budget_exhausted(), "{err}");
    }

    /// The exact sentence 0036 quoted from production, so the detector is
    /// pinned to what the vendor actually sends rather than to a guess.
    #[test]
    fn the_measured_quota_message_is_recognised() {
        let err = QueryError::Server(
            "Quota for user 'crypto' for 3600s has been exceeded: queries = 133/120".to_owned(),
        );
        assert!(err.is_quota_exceeded(), "{err}");
    }

    /// A budget this process declared for itself must not also count as
    /// CryptoHouse refusing it -- one is a run choosing to stop, the other is
    /// the vendor's shared allowance actually running out, and `radar brief`
    /// must be able to tell them apart per unit.
    #[test]
    fn a_declared_budget_exhaustion_is_not_a_quota_refusal() {
        let err = QueryError::Server(
            "consider query budget spent for this run; remaining candidates were not considered"
                .to_owned(),
        );
        assert!(!err.is_quota_exceeded(), "{err}");
    }

    /// An unrelated server error is neither kind of allowance running out.
    #[test]
    fn an_ordinary_server_error_is_not_a_quota_refusal() {
        let body = "Code: 159. DB::Exception: Timeout exceeded (TIMEOUT_EXCEEDED)";
        let err = server_error(500, body, "SELECT 1");
        assert!(!err.is_quota_exceeded(), "{err}");
    }

    /// A client counts every attempt whether it succeeds or fails, and counts
    /// a quota refusal as both a query and a refusal -- not a refusal instead
    /// of a query, which would make "queries issued" undercount what actually
    /// reached the endpoint.
    #[test]
    fn a_client_against_no_server_still_counts_the_attempt() {
        // No live request is made in this crate's tests (0036's own rule: a
        // guest on a public endpoint does not hammer it from a test suite),
        // so this exercises the counting through a client pointed at a port
        // nothing listens on -- a transport failure, counted as one query and
        // zero quota refusals.
        let client = Client::new("http://127.0.0.1:0/");
        let result: Result<Vec<serde_json::Value>, QueryError> = client.query("SELECT 1");
        assert!(result.is_err());
        assert_eq!(client.queries_issued(), 1);
        assert_eq!(client.quota_refusals(), 0);
    }

    /// Two queries against a local stub: `queries_issued` must read the real
    /// count, not a constant.
    ///
    /// Kills `queries_issued` mutated to return the literal `1` -- the other
    /// test in this file that checks it (`a_client_against_no_server_still_
    /// counts_the_attempt`) only ever drives one query, so a client stuck
    /// returning `1` would pass it too. It also kills `+` mutated to `*` or
    /// `-` in `Client::query`'s counting line: `*` never leaves zero (`0*1`
    /// stays `0`), and `-` wraps to `u64::MAX` on the first call, so neither
    /// reaches `2`.
    #[test]
    fn queries_issued_counts_every_query_not_just_the_first() {
        let endpoint = crate::test_support::start_server(vec![
            (200, "{\"n\":\"1\"}\n"),
            (200, "{\"n\":\"2\"}\n"),
        ]);
        let client = Client::new(endpoint);

        let first: Vec<Row> = client.query("SELECT 1").expect("first query succeeds");
        let second: Vec<Row> = client.query("SELECT 2").expect("second query succeeds");

        assert_eq!(first, vec![Row { n: "1".into() }]);
        assert_eq!(second, vec![Row { n: "2".into() }]);
        assert_eq!(client.queries_issued(), 2);
    }

    /// A response CryptoHouse itself refuses for quota must move
    /// `quota_refusals` off zero.
    ///
    /// Kills `quota_refusals` mutated to return the literal `0` -- every other
    /// test in this file that checks it only ever sees a genuine zero, so a
    /// client stuck returning `0` would pass them too.
    #[test]
    fn quota_refusals_counts_a_quota_exceeded_response() {
        let endpoint = crate::test_support::start_server(vec![(
            500,
            "Quota for user 'crypto' for 3600s has been exceeded: queries = 121/120",
        )]);
        let client = Client::new(endpoint);

        let result: Result<Vec<Row>, QueryError> = client.query("SELECT 1");

        let err = result.expect_err("a quota-exceeded response must be an error");
        assert!(err.is_quota_exceeded(), "{err}");
        assert_eq!(client.queries_issued(), 1);
        assert_eq!(client.quota_refusals(), 1);
    }
}
