// SPDX-License-Identifier: Apache-2.0
//! Reading the public reply bot's log as a file.
//!
//! Until 2026-09-14 this crate read `<dir>/replies.jsonl` through
//! `radar_analyst::log`, the bot's own crate. That day the bot moved into its
//! own repository ([ADR 0024](https://github.com/1xmint/theradar/blob/main/docs/adr/0024-the-bot-stands-alone.md)
//! there), and `radar-analyst` was deleted from this one.
//!
//! This is the one remaining reader of that file: this crate's own seven-day
//! checkpoint, and `radar-cli`'s `brief` and `seven-days-later`, which reuse it
//! rather than each carrying a second parser of the same format -- two things
//! that can drift, the way `radar_store::cursor`'s own docs argue against.
//!
//! The on-disk line carries more than [`Entry`] does — the fact sheet, the
//! reply text, the refusal signals — and this struct silently ignores the
//! rest on read, the way `serde` ignores an unknown field by default. The
//! format itself is unchanged: this reads exactly what the bot still writes.

use std::collections::HashMap;

use serde::Deserialize;

/// One answered — or refused — mention, as much of it as Radar reads.
#[derive(Clone, Debug, Deserialize)]
pub struct Entry {
    /// When, as seconds since the epoch.
    pub at: u64,
    /// The mention's id on the platform. `publish` appends twice per reply —
    /// once before anything is said, once after — sharing one id, and
    /// [`latest`] folds on it.
    pub mention_id: String,
    /// The mint, when one was resolved.
    pub mint: Option<String>,
    /// The slot the facts were read at.
    #[serde(default)]
    pub read_at_slot: Option<u64>,
    /// The published reply's id, when one was actually sent. `None` for a dry
    /// run or a refusal.
    pub reply_id: Option<String>,
}

/// Reads a log back.
///
/// Skips lines that will not parse rather than failing the read: a log with
/// one torn line at the end of a crashed write is still the record of
/// everything before it.
///
/// # Errors
///
/// The underlying I/O error if the file cannot be read at all.
pub fn read(path: &str) -> std::io::Result<Vec<Entry>> {
    let text = std::fs::read_to_string(path)?;
    Ok(text
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect())
}

/// The log, with each mention's last word.
///
/// `publish` appends twice for one reply — once before it says anything, once
/// after, to record the platform's id or why there is none — so a reader
/// counting raw lines counts intents, not replies. Ordered by first
/// appearance.
///
/// # Errors
///
/// The underlying I/O error if the file cannot be read at all.
pub fn latest(path: &str) -> std::io::Result<Vec<Entry>> {
    let all = read(path)?;
    let mut order: Vec<String> = Vec::new();
    let mut newest: HashMap<String, Entry> = HashMap::new();
    for entry in all {
        if !newest.contains_key(&entry.mention_id) {
            order.push(entry.mention_id.clone());
        }
        newest.insert(entry.mention_id.clone(), entry);
    }
    Ok(order
        .into_iter()
        .filter_map(|id| newest.remove(&id))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(mention_id: &str, at: u64, mint: Option<&str>, reply_id: Option<&str>) -> String {
        line_with_slot(mention_id, at, mint, Some(2_000), reply_id)
    }

    fn line_with_slot(
        mention_id: &str,
        at: u64,
        mint: Option<&str>,
        read_at_slot: Option<u64>,
        reply_id: Option<&str>,
    ) -> String {
        serde_json::json!({
            "at": at,
            "mention_id": mention_id,
            "summoner": "s",
            "mint": mint,
            "read_at_slot": read_at_slot,
            "fact_sheet": "",
            "reply": "",
            "fellback": null,
            "reply_id": reply_id,
        })
        .to_string()
    }

    #[test]
    fn a_missing_log_is_empty_rather_than_an_error_from_read() {
        let dir = std::env::temp_dir().join(format!("radar-alog-missing-{}", std::process::id()));
        let path = dir.join("nowhere.jsonl");
        assert!(read(path.to_str().expect("a path")).is_err());
    }

    #[test]
    fn only_the_fields_this_crate_reads_are_kept_and_extra_fields_are_ignored() {
        // The on-disk line carries far more than `Entry` does. `serde` ignores
        // the rest silently, which is the whole point: reading the same file
        // the bot still writes without carrying its other crates.
        let dir = std::env::temp_dir().join(format!("radar-alog-fields-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("replies.jsonl");
        std::fs::write(&path, line("m1", 100, Some("MINT1"), Some("r1"))).expect("write");

        let entries = read(path.to_str().expect("a path")).expect("read");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].at, 100);
        assert_eq!(entries[0].mention_id, "m1");
        assert_eq!(entries[0].mint.as_deref(), Some("MINT1"));
        assert_eq!(entries[0].reply_id.as_deref(), Some("r1"));
        assert_eq!(entries[0].read_at_slot, Some(2_000));
    }

    #[test]
    fn a_line_written_before_read_at_slot_existed_still_parses() {
        // `#[serde(default)]`: an absent field is `None`, not a parse failure.
        let dir = std::env::temp_dir().join(format!("radar-alog-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("replies.jsonl");
        let old = r#"{"at":1,"mention_id":"m","summoner":"s","mint":null,"fact_sheet":"","reply":"","fellback":null,"reply_id":null}"#;
        std::fs::write(&path, old).expect("write");
        let entries = read(path.to_str().expect("a path")).expect("read");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].read_at_slot, None);
    }

    #[test]
    fn a_torn_final_line_does_not_lose_the_rest() {
        let dir = std::env::temp_dir().join(format!("radar-alog-torn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("replies.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\n{{\"at\": 200, \"mention_i",
                line("m1", 100, None, None)
            ),
        )
        .expect("write");

        let entries = read(path.to_str().expect("a path")).expect("read");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn latest_folds_the_two_lines_publish_writes_per_reply() {
        let dir = std::env::temp_dir().join(format!("radar-alog-fold-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("replies.jsonl");
        let text = format!(
            "{}\n{}\n",
            line("m1", 100, Some("MINT1"), None),
            line("m1", 100, Some("MINT1"), Some("r1")),
        );
        std::fs::write(&path, text).expect("write");

        let entries = latest(path.to_str().expect("a path")).expect("latest");
        assert_eq!(entries.len(), 1, "the two lines fold to one reply");
        assert_eq!(entries[0].reply_id.as_deref(), Some("r1"));
    }
}
