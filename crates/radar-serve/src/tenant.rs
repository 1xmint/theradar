// SPDX-License-Identifier: Apache-2.0
//! The signed-in wallet, and the only per-wallet storage it can reach.
//!
//! Decisions 1 and 2 of [plan 0012](../../../docs/plans/0012-the-public-trading-panel.md),
//! built as task 9-11-0011 of it and item 1 of plan 0013's Phase C. This is the
//! data-isolation boundary: what one wallet keeps here, no other wallet can
//! read, and **nobody reads it through Radar but that wallet** -- the operator
//! included. The owner decided that on 2026-09-23. An operator view would be a
//! second way in to the one boundary this module exists to hold.
//!
//! # How an unscoped read is made impossible
//!
//! - A [`Tenant`] holds a verified wallet address in a private field. The only
//!   way to make one is [`Tenant::verify`], which checks a session token against
//!   the server's secret. There is no `FromStr`, no `Deserialize` and no public
//!   field, so a handler cannot build one from anything a caller sent.
//! - A [`TenantStore`] is made only from a `Tenant`, by [`Customers::store`].
//!   **No method on it takes a wallet address.** A coin is a [`Coin`], a type no
//!   wallet address converts into, so "read wallet B's list" is not a sentence
//!   this API can say.
//! - A handler asks for a `Tenant` as an extractor. The guard is the only thing
//!   that puts one on a request, and it does so only after the session verified
//!   and the wallet was admitted. Missing means refused, before any read.
//!
//! # Two places this departs from plan 0012's text, and why
//!
//! 0012 says a `Tenant`'s constructor takes the guard's `Customer` extension.
//! `Customer`'s fields are public, so any handler could write
//! `Customer { did: "<wallet B>".into(), .. }` and get wallet B's tenant. That
//! version passes 0012's rubric and still leaks. Taking a token that has to
//! verify closes it.
//!
//! 0012 says the per-wallet directory reuses `radar-store`'s writer. That writer
//! appends Parquet rows partitioned by slot; a watchlist has no slot and needs
//! removals. What 0012 chose the directory *for* is kept -- a developer who
//! forgets to scope reads a folder that does not exist rather than every
//! wallet's rows -- and the file inside it is plain JSON, written the way
//! [`crate::ledger`] writes. It lives under `RADAR_STATE_DIR`, which
//! `radar-serve` already proves writable at start, rather than beside the store's
//! tables, which other processes write.
//!
//! # Why no watermark
//!
//! AGENTS §4 rule 3 gates reads of what Radar *observed*. A watchlist is the
//! wallet's own instruction, not an observation of the chain, and a replay has
//! nothing to read it against. It is not cached, so the rule's sharpest edge --
//! a cached value served past the watermark it was stored at -- does not arise.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use axum::Json;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use radar_customer::session::{self, Invalid};
use radar_types::Address;
use serde_json::json;

/// A wallet that proved it is the one calling.
///
/// ```compile_fail
/// // Not from a struct literal: the fields are private.
/// let _ = radar_serve::tenant::Tenant {
///     address: radar_types::Address::new([0; 32]),
///     expires_at: u64::MAX,
/// };
/// ```
///
/// ```compile_fail
/// // Not from a string, a path segment or a query parameter.
/// let _: radar_serve::tenant::Tenant = "11111111111111111111111111111111".parse().unwrap();
/// ```
///
/// ```compile_fail
/// // Not from a request body.
/// let _: radar_serve::tenant::Tenant = serde_json::from_str("{}").unwrap();
/// ```
///
/// ```compile_fail
/// // Not from an address someone already holds.
/// let _ = radar_serve::tenant::Tenant::from(radar_types::Address::new([0; 32]));
/// ```
#[derive(Clone, Debug)]
pub struct Tenant {
    address: Address,
    expires_at: u64,
}

impl Tenant {
    /// The wallet a session token names, if the token verifies now.
    ///
    /// The one constructor. It takes a token rather than an address because a
    /// token is the only thing a caller sends that the caller cannot write for
    /// someone else: the tag is keyed on this instance's secret.
    ///
    /// # Errors
    ///
    /// [`Invalid`], exactly as [`session::verify`] reports it.
    pub fn verify(token: &str, secret: &[u8], now: u64) -> Result<Self, Invalid> {
        let session = session::verify(token, secret, now)?;
        Ok(Self {
            address: session.address,
            expires_at: session.expires_at,
        })
    }

    /// The wallet.
    #[must_use]
    pub const fn address(&self) -> &Address {
        &self.address
    }

    /// When the session ends, in seconds since the epoch.
    #[must_use]
    pub const fn expires_at(&self) -> u64 {
        self.expires_at
    }
}

/// Why the guard did not attach a [`Tenant`], carried to the handler.
///
/// Without it the handler could only say "no session", and an expired session
/// and a tampered one would read the same as never having signed in -- three
/// answers that want three different responses from the person reading them.
#[derive(Clone, Debug)]
pub(crate) struct SessionRefused(pub(crate) Invalid);

impl<S: Send + Sync> FromRequestParts<S> for Tenant {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Response> {
        if let Some(tenant) = parts.extensions.get::<Self>() {
            return Ok(tenant.clone());
        }
        if let Some(SessionRefused(why)) = parts.extensions.get::<SessionRefused>() {
            return Err(refused(why));
        }
        if parts
            .extensions
            .get::<crate::customer::Customer>()
            .is_some()
        {
            // Signed in, by email rather than by wallet. Privy's identity carries
            // no address, and a watchlist belongs to a wallet.
            return Err(refusal(
                StatusCode::FORBIDDEN,
                "not_a_wallet",
                "a watchlist belongs to a wallet, and this session is not a wallet's; sign in with your wallet",
            ));
        }
        Err(no_session())
    }
}

/// The refusal for a request carrying no wallet session at all.
pub(crate) fn no_session() -> Response {
    refusal(
        StatusCode::FORBIDDEN,
        "no_session",
        "no wallet session on this request; sign in with your wallet",
    )
}

/// The refusal for a wallet session that did not verify.
///
/// Expiry is named, because the answer to it is "sign in again". A bad tag and a
/// malformed token share a reason and do not say which half was wrong, for the
/// reason [`Invalid::BadTag`] gives: telling a forger which half to keep is help.
pub(crate) fn refused(why: &Invalid) -> Response {
    match why {
        // The server's own configuration, not the caller's token. Not named in
        // detail: the length of a secret is nobody's business but the operator's.
        Invalid::SecretTooShort { .. } => refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "sessions_unavailable",
            "this instance cannot check wallet sessions",
        ),
        Invalid::Expired { .. } => refusal(
            StatusCode::FORBIDDEN,
            "session_expired",
            &format!("{why}; sign in with your wallet again"),
        ),
        Invalid::Malformed | Invalid::BadTag => refusal(
            StatusCode::FORBIDDEN,
            "session_invalid",
            &format!("{why}; sign in with your wallet again"),
        ),
    }
}

pub(crate) fn refusal(status: StatusCode, reason: &str, message: &str) -> Response {
    (status, Json(json!({ "error": message, "reason": reason }))).into_response()
}

/// The most coins one wallet may keep.
///
/// A bound rather than none, because any wallet can sign in and a list with no
/// ceiling is disk anyone can fill. A hundred is more than a screen shows.
pub const WATCHLIST_LIMIT: usize = 100;

/// A coin, by its mint address.
///
/// Its own type so that [`TenantStore`]'s methods take no [`Address`]: a wallet
/// address cannot be handed to them by mistake. Parsing someone else's wallet
/// address *as* a coin is possible, and harmless -- it lands on the caller's own
/// list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Coin(Address);

impl core::str::FromStr for Coin {
    type Err = radar_types::AddressParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

impl core::fmt::Display for Coin {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

/// Why a watchlist could not be read or changed.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Unavailable {
    /// The file exists and could not be read or parsed.
    ///
    /// **Not** an empty list. Reporting it as one would tell a wallet it is
    /// watching nothing when Radar could not look -- rule 9 on a screen.
    #[error("Radar could not read this watchlist: {0}")]
    Unreadable(String),
    /// The change could not be saved.
    #[error("Radar could not save this watchlist: {0}")]
    Unwritable(String),
    /// Adding would pass [`WATCHLIST_LIMIT`].
    #[error("a watchlist holds at most {WATCHLIST_LIMIT} coins; remove one first")]
    Full,
    /// The wallet's address did not render as a safe folder name.
    ///
    /// Cannot happen for a 32-byte address. Reported rather than panicking,
    /// because "it cannot happen" is the sentence that precedes it happening.
    #[error("Radar could not place this wallet's storage")]
    Unplaceable,
}

/// Where every wallet's folder lives: `<RADAR_STATE_DIR>/customers/`.
#[derive(Debug)]
pub struct Customers {
    root: PathBuf,
    /// Held across a read-change-write, so two changes from one wallet at once
    /// cannot both read the old list and have the second write erase the first.
    /// One lock for every wallet: changes are rare and each is one small file.
    writing: Mutex<()>,
}

impl Customers {
    /// Opens the directory named by `RADAR_STATE_DIR`, proving it writable.
    ///
    /// # Errors
    ///
    /// [`crate::ledger::Unusable`] when unset or unwritable.
    pub fn open(get: &impl Fn(&str) -> Option<String>) -> Result<Self, crate::ledger::Unusable> {
        let state = get(crate::ledger::STATE_DIR)
            .filter(|v| !v.trim().is_empty())
            .ok_or(crate::ledger::Unusable::NotConfigured)?;
        Self::at(&Path::new(state.trim()).join("customers"))
    }

    /// Opens a customers directory at a path, proving it writable.
    ///
    /// # Errors
    ///
    /// [`crate::ledger::Unusable::NotWritable`] when it cannot be written.
    pub fn at(root: &Path) -> Result<Self, crate::ledger::Unusable> {
        // The ledger's own proof, reused: it creates the directory and writes a
        // probe, which is the check that tells a read-only mount apart.
        crate::ledger::Store::at(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            writing: Mutex::new(()),
        })
    }

    /// The storage this wallet, and only this wallet, may use.
    ///
    /// # Errors
    ///
    /// [`Unavailable::Unplaceable`] if the address renders as anything but a
    /// plain base58 name.
    pub fn store(&self, tenant: &Tenant) -> Result<TenantStore<'_>, Unavailable> {
        let rendered = tenant.address.to_string();
        let name = segment(&rendered).ok_or(Unavailable::Unplaceable)?;
        Ok(TenantStore {
            customers: self,
            dir: self.root.join(name),
        })
    }
}

/// The base58 alphabet Solana addresses are written in.
const BASE58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// `rendered`, if it is safe to use as one folder name.
///
/// Base58 of 32 bytes is 32 to 44 characters from an alphabet with no
/// separator, no dot and no drive letter's colon, so this refuses nothing a real
/// address produces. It exists for the address that is not real.
fn segment(rendered: &str) -> Option<&str> {
    let safe = (32..=44).contains(&rendered.len()) && rendered.bytes().all(|b| BASE58.contains(&b));
    safe.then_some(rendered)
}

/// One wallet's storage. Made only by [`Customers::store`].
#[derive(Debug)]
pub struct TenantStore<'a> {
    customers: &'a Customers,
    dir: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Watchlist {
    coins: Vec<String>,
}

impl TenantStore<'_> {
    /// The coins this wallet watches, oldest first. Empty when it never saved any.
    ///
    /// # Errors
    ///
    /// [`Unavailable::Unreadable`] when a saved list exists and cannot be read.
    pub fn watchlist(&self) -> Result<Vec<Coin>, Unavailable> {
        let raw = match std::fs::read_to_string(self.file()) {
            Ok(raw) => raw,
            // Never saved: the folder does not exist, which is an empty list and
            // the one place "missing" means "none".
            Err(why) if why.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(why) => return Err(Unavailable::Unreadable(why.to_string())),
        };
        let list: Watchlist =
            serde_json::from_str(&raw).map_err(|why| Unavailable::Unreadable(why.to_string()))?;
        list.coins
            .iter()
            .map(|coin| {
                coin.parse()
                    .map_err(|_| Unavailable::Unreadable(format!("{coin} is not a coin address")))
            })
            .collect()
    }

    /// Adds a coin, and returns the list after. Adding one already there changes
    /// nothing.
    ///
    /// # Errors
    ///
    /// [`Unavailable`]: the list could not be read, is full, or could not be saved.
    pub fn watch(&self, coin: Coin) -> Result<Vec<Coin>, Unavailable> {
        self.change(|coins| {
            if coins.contains(&coin) {
                return Ok(());
            }
            if coins.len() >= WATCHLIST_LIMIT {
                return Err(Unavailable::Full);
            }
            coins.push(coin);
            Ok(())
        })
    }

    /// Removes a coin, and returns the list after. Removing one not there changes
    /// nothing.
    ///
    /// # Errors
    ///
    /// [`Unavailable`]: the list could not be read or could not be saved.
    pub fn unwatch(&self, coin: Coin) -> Result<Vec<Coin>, Unavailable> {
        self.change(|coins| {
            coins.retain(|c| *c != coin);
            Ok(())
        })
    }

    fn change(
        &self,
        edit: impl FnOnce(&mut Vec<Coin>) -> Result<(), Unavailable>,
    ) -> Result<Vec<Coin>, Unavailable> {
        // A poisoned lock means another change panicked mid-way. The file is
        // still whole -- it is only ever replaced by a rename -- so carrying on
        // is safe, and refusing every later change would not be.
        let _held = self
            .customers
            .writing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Read first, and refuse on an unreadable list rather than starting a
        // fresh one: a new one-coin list written over a damaged file would erase
        // everything the wallet had saved.
        let mut coins = self.watchlist()?;
        edit(&mut coins)?;
        let unwritable = |why: std::io::Error| Unavailable::Unwritable(why.to_string());
        std::fs::create_dir_all(&self.dir).map_err(unwritable)?;
        let encoded = serde_json::to_vec(&Watchlist {
            coins: coins.iter().map(ToString::to_string).collect(),
        })
        .map_err(|why| Unavailable::Unwritable(why.to_string()))?;
        // Written aside and renamed, so a process killed mid-write leaves the
        // previous list rather than half of one.
        let temporary = self.dir.join("watchlist.json.tmp");
        std::fs::write(&temporary, encoded).map_err(unwritable)?;
        std::fs::rename(&temporary, self.file()).map_err(unwritable)?;
        Ok(coins)
    }

    fn file(&self) -> PathBuf {
        self.dir.join("watchlist.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = &[7u8; 32];
    const NOW: u64 = 1_788_000_000;

    fn tenant(byte: u8) -> Tenant {
        let token = session::issue(&Address::new([byte; 32]), SECRET, NOW).expect("issues");
        Tenant::verify(&token, SECRET, NOW + 1).expect("verifies")
    }

    fn coin(byte: u8) -> Coin {
        Coin(Address::new([byte; 32]))
    }

    fn customers() -> (tempfile::TempDir, Customers) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let customers = Customers::at(&dir.path().join("customers")).expect("writable");
        (dir, customers)
    }

    #[test]
    fn a_tenant_is_the_wallet_its_session_names() {
        let t = tenant(3);
        assert_eq!(*t.address(), Address::new([3u8; 32]));
        assert_eq!(t.expires_at(), NOW + session::LIFETIME_SECONDS);
    }

    #[test]
    fn a_session_that_does_not_verify_makes_no_tenant() {
        let token = session::issue(&Address::new([3u8; 32]), SECRET, NOW).expect("issues");
        assert_eq!(
            Tenant::verify(&token, &[9u8; 32], NOW + 1).err(),
            Some(Invalid::BadTag)
        );
        assert!(matches!(
            Tenant::verify(&token, SECRET, NOW + session::LIFETIME_SECONDS),
            Err(Invalid::Expired { .. })
        ));
    }

    async fn extract(
        extensions: impl FnOnce(&mut axum::http::Extensions),
    ) -> Result<Tenant, Response> {
        let (mut parts, ()) = axum::http::Request::new(()).into_parts();
        extensions(&mut parts.extensions);
        Tenant::from_request_parts(&mut parts, &()).await
    }

    async fn reason(refused: Response) -> (StatusCode, String) {
        use http_body_util::BodyExt;
        let status = refused.status();
        let bytes = refused
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        (
            status,
            body["reason"].as_str().expect("a reason").to_owned(),
        )
    }

    #[tokio::test]
    async fn a_handler_gets_the_guards_tenant_and_nothing_else_makes_one() {
        let t = extract(|e| {
            e.insert(tenant(4));
        })
        .await
        .expect("the guard's tenant");
        assert_eq!(*t.address(), Address::new([4u8; 32]));

        // Signed in by email: a customer, and not a wallet.
        let privy = extract(|e| {
            e.insert(crate::customer::Customer {
                did: "did:privy:someone".into(),
                session: "0".into(),
            });
        })
        .await
        .expect_err("a Privy customer is not a wallet");
        assert_eq!(
            reason(privy).await,
            (StatusCode::FORBIDDEN, "not_a_wallet".into())
        );

        let nothing = extract(|_| {}).await.expect_err("no tenant");
        assert_eq!(
            reason(nothing).await,
            (StatusCode::FORBIDDEN, "no_session".into())
        );

        let expired = extract(|e| {
            e.insert(SessionRefused(Invalid::Expired { by_seconds: 5 }));
        })
        .await
        .expect_err("expired");
        assert_eq!(
            reason(expired).await,
            (StatusCode::FORBIDDEN, "session_expired".into())
        );
    }

    #[tokio::test]
    async fn an_instance_that_cannot_check_sessions_says_so_rather_than_blaming_the_token() {
        let refused = refused(&Invalid::SecretTooShort { got: 0 });
        assert_eq!(
            reason(refused).await,
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "sessions_unavailable".into()
            )
        );
        let malformed = refused_reason(&Invalid::Malformed).await;
        assert_eq!(malformed, (StatusCode::FORBIDDEN, "session_invalid".into()));
    }

    async fn refused_reason(why: &Invalid) -> (StatusCode, String) {
        reason(refused(why)).await
    }

    #[test]
    fn opening_needs_a_configured_state_directory() {
        assert!(matches!(
            Customers::open(&|_| None),
            Err(crate::ledger::Unusable::NotConfigured)
        ));
        assert!(matches!(
            Customers::open(&|_| Some("  ".into())),
            Err(crate::ledger::Unusable::NotConfigured)
        ));
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().to_string_lossy().into_owned();
        Customers::open(&|_| Some(path.clone())).expect("writable");
        assert!(
            dir.path().join("customers").is_dir(),
            "lists live under customers/"
        );
    }

    #[test]
    fn one_wallets_watchlist_is_invisible_to_another() {
        // Rubric item 7. Re-apply by building every store's folder from one
        // fixed name instead of the tenant's address: B then reads A's coin.
        let (_dir, customers) = customers();
        let a = customers.store(&tenant(1)).expect("placed");
        let b = customers.store(&tenant(2)).expect("placed");

        a.watch(coin(9)).expect("saves");

        assert_eq!(a.watchlist(), Ok(vec![coin(9)]));
        assert_eq!(
            b.watchlist(),
            Ok(Vec::new()),
            "wallet B must not see wallet A's coin"
        );
        b.unwatch(coin(9)).expect("saves");
        assert_eq!(
            a.watchlist(),
            Ok(vec![coin(9)]),
            "wallet B removing a coin must not touch wallet A's list"
        );
    }

    #[test]
    fn the_folder_builder_refuses_anything_but_a_plain_address() {
        // Rubric item 6. Re-apply by making `segment` return its input.
        let hostile = [
            "",
            ".",
            "..",
            "../../etc/passwd",
            "..\\..\\windows",
            "/absolute",
            "C:\\drive",
            "C:",
            "a/b",
            "a\\b",
            "11111111111111111111111111111111/..",
            "1111111111111111111111111111111\0",
            "0OIl0OIl0OIl0OIl0OIl0OIl0OIl0OIl", // 32 characters, none of them base58
            "1111111111111111111111111111111",  // 31: too short for 32 bytes
            "111111111111111111111111111111111111111111111", // 45: too long
        ];
        for input in hostile {
            assert_eq!(segment(input), None, "{input:?} must be refused");
        }
        for byte in [0u8, 1, 0x7f, 0xff] {
            let real = Address::new([byte; 32]).to_string();
            assert_eq!(
                segment(&real),
                Some(real.as_str()),
                "a real address is a name"
            );
        }
    }

    #[test]
    fn watching_twice_keeps_one_and_the_list_is_bounded() {
        let (_dir, customers) = customers();
        let store = customers.store(&tenant(1)).expect("placed");
        store.watch(coin(1)).expect("saves");
        assert_eq!(store.watch(coin(1)), Ok(vec![coin(1)]));

        for byte in 2..=u8::try_from(WATCHLIST_LIMIT).expect("fits") {
            store.watch(coin(byte)).expect("room");
        }
        assert_eq!(store.watchlist().map(|l| l.len()), Ok(WATCHLIST_LIMIT));
        assert_eq!(store.watch(coin(200)), Err(Unavailable::Full));
        // Already there is not "adding", so a full list still accepts it.
        assert_eq!(store.watch(coin(1)).map(|l| l.len()), Ok(WATCHLIST_LIMIT));
    }

    #[test]
    fn unwatching_removes_only_that_coin() {
        let (_dir, customers) = customers();
        let store = customers.store(&tenant(1)).expect("placed");
        store.watch(coin(1)).expect("saves");
        store.watch(coin(2)).expect("saves");
        assert_eq!(store.unwatch(coin(1)), Ok(vec![coin(2)]));
        assert_eq!(store.watchlist(), Ok(vec![coin(2)]));
    }

    #[test]
    fn a_damaged_list_is_reported_and_never_overwritten() {
        // Rule 9. Re-apply by treating a parse failure as an empty list: the
        // read says "watching nothing" and the next add erases the file.
        let (_dir, customers) = customers();
        let store = customers.store(&tenant(1)).expect("placed");
        store.watch(coin(1)).expect("saves");
        std::fs::write(store.file(), b"{ not json").expect("damages");

        assert!(matches!(store.watchlist(), Err(Unavailable::Unreadable(_))));
        assert!(matches!(
            store.watch(coin(2)),
            Err(Unavailable::Unreadable(_))
        ));
        assert_eq!(
            std::fs::read(store.file()).expect("still there"),
            b"{ not json",
            "a damaged list must be left for someone to look at"
        );
    }
}
