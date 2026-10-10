// SPDX-License-Identifier: Apache-2.0
//! Exclusive reservation history, checked across real process death.

use radar_journal::{Correlation, Intent, Journal, OperationError, OperationLog, Verified};
use radar_types::{
    Address, Asset, AssetRole, Balance, Holding, Portfolio, Slot, TokenQuantity, Unvaluable,
    Valuation,
};
use std::io::{BufRead as _, Write as _};

fn account() -> Portfolio {
    let mut p = Portfolio::at(Address::new([7; 32]), Slot(500));
    p.hold(
        Asset::Sol,
        Holding::new(
            AssetRole::Cash,
            Balance::Counted(TokenQuantity::lamports(10_000_000)),
            Valuation::Unknown(Unvaluable::NoPrice),
            Valuation::Unknown(Unvaluable::NoPrice),
        ),
    )
    .expect("balance");
    p
}

fn reserve(log: &mut OperationLog) {
    let id = log
        .propose(
            Intent {
                asset: Asset::Sol,
                amount: TokenQuantity::lamports(4_000_000),
                at: Slot(500),
            },
            1000,
            Correlation {
                mint: Some("test-mint".into()),
                ..Correlation::default()
            },
        )
        .expect("propose");
    log.reserve(&id, &mut account(), 1001).expect("reserve");
}

#[test]
fn a_second_owner_refuses_and_reopening_retains_the_claim() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("operations.jsonl");
    let mut owner = OperationLog::open(&path).expect("owner");
    reserve(&mut owner);
    assert!(OperationLog::open(&path).is_err());
    assert!(OperationLog::open(dir.path().join(".").join("operations.jsonl")).is_err());
    // Read-only audit continues while reservation ownership is held.
    assert_eq!(
        Journal::open(&path)
            .expect("audit")
            .verify()
            .expect("verify"),
        Verified::Intact { events: 2 }
    );
    drop(owner);
    let mut reopened = OperationLog::open(&path).expect("released");
    let mut p = account();
    reopened.rehold(&mut p).expect("rehold");
    assert_eq!(p.free(Asset::Sol).expect("free").raw(), 6_000_000);
    assert_eq!(reopened.outstanding().count(), 1);
}

struct ChildOwner(std::process::Child);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// Invoked only by the real-process test. No CLI or production test hook added.
#[test]
#[ignore = "child process fixture, invoked by process_death_releases_ownership_without_releasing_capital"]
fn child_holds_reservations() {
    let path = std::env::var_os("RADAR_JOURNAL_TEST_PATH").expect("child path");
    let mut log = OperationLog::open(std::path::PathBuf::from(path)).expect("child owner");
    reserve(&mut log);
    println!("LOCKED");
    std::io::stdout().flush().expect("flush");
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .expect("wait for parent");
    drop(log);
}

#[test]
fn process_death_releases_ownership_without_releasing_capital() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("operations.jsonl");
    let child = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "child_holds_reservations",
            "--ignored",
            "--nocapture",
        ])
        .env("RADAR_JOURNAL_TEST_PATH", &path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("child");
    let mut child = ChildOwner(child);
    let mut output = std::io::BufReader::new(child.0.stdout.take().expect("stdout"));
    loop {
        let mut line = String::new();
        assert!(
            output.read_line(&mut line).expect("ready") > 0,
            "child exited without ownership"
        );
        if line.trim() == "LOCKED" {
            break;
        }
    }
    assert!(
        OperationLog::open(&path).is_err(),
        "another process must refuse"
    );
    child.0.kill().expect("simulate abrupt process death");
    child.0.wait().expect("exited");
    let mut reopened = OperationLog::open(&path).expect("OS released ownership");
    let mut p = account();
    reopened.rehold(&mut p).expect("rehold");
    assert_eq!(p.free(Asset::Sol).expect("free").raw(), 6_000_000);
    assert_eq!(reopened.outstanding().count(), 1);
    assert!(
        dir.path().join("operations.jsonl.lock").exists(),
        "do not unlink the lock file"
    );
}

#[test]
fn damaged_and_torn_history_refuse_without_repairing_it() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("operations.jsonl");
    {
        let mut log = OperationLog::open(&path).expect("owner");
        reserve(&mut log);
    }
    let intact = std::fs::read_to_string(&path).expect("read");
    for damaged in [
        intact.replace("4000000", "1000000"),
        format!("{intact}{}\n", intact.lines().last().expect("last")),
        format!("{intact}{{\"unfinished\":"),
        format!("{{\"bad\":true}}\n{intact}"),
    ] {
        std::fs::write(&path, &damaged).expect("damage");
        assert!(matches!(
            OperationLog::open(&path),
            Err(OperationError::HistoryNotIntact(_))
        ));
        assert_eq!(std::fs::read_to_string(&path).expect("unchanged"), damaged);
    }
    // Integrity is relative to the chain present, not a trusted off-host
    // checkpoint: truncating to a complete valid prefix cannot be detected here.
    std::fs::write(
        &path,
        format!("{}\n", intact.lines().next().expect("proposal")),
    )
    .expect("valid prefix");
    assert_eq!(
        OperationLog::open(&path)
            .expect("valid prefix remains indistinguishable")
            .outstanding()
            .count(),
        0
    );
    std::fs::write(&path, &intact).expect("restore");
    assert_eq!(
        OperationLog::open(&path)
            .expect("after failures")
            .outstanding()
            .count(),
        1
    );
}

#[test]
fn lock_open_failure_refuses_without_creating_history() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("operations.jsonl");
    std::fs::create_dir(dir.path().join("operations.jsonl.lock")).expect("unusable lock");
    assert!(OperationLog::open(&path).is_err());
    assert!(!path.exists());
    assert!(OperationLog::open(dir.path().join("missing").join("operations.jsonl")).is_err());
}
