// SPDX-License-Identifier: Apache-2.0
//! `radar seven-days-later`: the join the analyst is not allowed to make.
//!
//! Design 0009 M4. The analyst daemon does not read the store -- its unit says
//! so, and the recorder's crash must never take the bot with it -- so the one
//! join the daily post needs runs here, on the box, on a timer, the way the
//! creator index is built: replies from the analyst's log, outcomes from the
//! store, written as one file the daemon reads and posts at noon UTC.
//!
//! The rows are for the day **seven days before** the run: what the bot was
//! asked about a week ago, and what the chain has done since.
//!
//! # The bot moved; this file's shape did not
//!
//! Until 2026-09-14 the reply log's [`Entry`](radar_backfill::analyst_log::Entry)
//! and the [`Row`]/[`Rows`] shape below both lived in `radar-analyst`, whose
//! own daemon read the file this command writes and posted from it. That bot
//! now lives in its own repository
//! ([ADR 0024](https://github.com/1xmint/theradar/blob/main/docs/adr/0024-the-bot-stands-alone.md)),
//! and this command still runs on the box on the same timer -- so the
//! `daily/<date>.json` file it writes keeps the exact field names its reader
//! expects, even though nothing in this repository parses it back.

use std::collections::BTreeMap;

use radar_asof::AsOf;
use radar_backfill::analyst_log::Entry;
use radar_store::{GraduationMode, Outcome};
use serde::{Deserialize, Serialize};

/// Seconds in a day.
const DAY: u64 = 86_400;

/// How a coin graduated, as the store measured it.
///
/// The exact shape `radar_analyst::daily::Graduation` used, kept unchanged: a
/// reader of `daily/<date>.json` depends on the field names, not on which
/// crate wrote them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Graduation {
    /// The curve completed within a few slots of the launch.
    Instant,
    /// The curve filled over time.
    Organic,
}

/// One coin the bot was asked about, seven days on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// The coin.
    pub mint: String,
    /// When the bot answered, seconds since the epoch.
    pub asked_at: u64,
    /// The published reply, when there was one.
    pub reply_id: Option<String>,
    /// How it graduated, or `None` for not (as far as the store has seen).
    pub graduation: Option<Graduation>,
    /// Whether no transfer has been seen since the slot the reply was read
    /// at. `None` when the store has no transfer slot for it at all.
    pub quiet_since_reply: Option<bool>,
    /// Held from first fill to last observed price, in basis points, when
    /// both were measured.
    pub held_bps: Option<i64>,
}

/// The day's rows, as this command writes them.
#[expect(
    clippy::struct_field_names,
    reason = "the field is `rows`, matching the on-disk shape a reader still expects"
)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rows {
    /// The day the replies were made, `YYYY-MM-DD`.
    pub asked_on: String,
    /// When the job ran, seconds since the epoch.
    pub built_at: u64,
    /// The store's watermark the outcomes were read at.
    pub watermark_slot: u64,
    /// The coins.
    pub rows: Vec<Row>,
}

impl Rows {
    /// Writes a day's file, via a sibling and a rename.
    ///
    /// # Errors
    ///
    /// The I/O error.
    pub fn write(&self, path: &str) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = format!("{path}.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }
}

/// The `YYYY-MM-DD` of a moment.
#[must_use]
pub fn date_of(secs: u64) -> String {
    radar_types::civil::date_from_days(i64::try_from(secs / DAY).unwrap_or(i64::MAX))
}

/// Where a day's rows live. The same path
/// `radar_analyst::daily::paths_for` derived.
#[must_use]
fn rows_path(daily_dir: &str, date: &str) -> String {
    format!("{daily_dir}/{date}.json")
}

/// Runs the command.
///
/// `--store <dir>`, `--analyst-dir <dir>` (default `data/analyst`), and
/// `--today <seconds>` for a test that wants a fixed clock.
///
/// # Errors
///
/// A message when the store cannot be read or the file cannot be written.
pub fn run(args: &[String]) -> Result<(), String> {
    let reader = crate::store_of(args)?;
    let analyst_dir =
        crate::flag(args, "--analyst-dir").unwrap_or_else(|| "data/analyst".to_owned());
    let today = match crate::flag(args, "--today") {
        Some(t) => t.parse::<u64>().map_err(|e| format!("--today: {e}"))?,
        None => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    };

    let log = radar_backfill::analyst_log::latest(&format!("{analyst_dir}/replies.jsonl"))
        .unwrap_or_default();
    let watermark = reader
        .watermark()
        .map_err(|e| format!("cannot read the watermark: {e}"))?
        .ok_or("the store holds no events, so there is nothing to look back on")?;
    let outcomes = reader
        .read_outcomes(AsOf::at(watermark))
        .map_err(|e| format!("cannot read outcomes: {e}"))?;

    let rows = build(&log, &outcomes, today, watermark.get());
    let daily_dir = format!("{analyst_dir}/daily");
    std::fs::create_dir_all(&daily_dir).map_err(|e| format!("cannot create {daily_dir}: {e}"))?;
    let path = rows_path(&daily_dir, &date_of(today));
    rows.write(&path)
        .map_err(|e| format!("cannot write {path}: {e}"))?;
    println!(
        "{} replies from {} joined against {} outcomes at slot {}, written to {path}",
        rows.rows.len(),
        rows.asked_on,
        outcomes.len(),
        watermark.get()
    );
    Ok(())
}

/// The rows for the day seven days before `today`. Pure.
///
/// Only **published** X replies are looked back on: a dry-run answer was never
/// a public call and has nothing to age. The latest outcome per mint is used;
/// a mint the store has not measured gets a row with every field unknown,
/// which the post counts as a coin asked about and nothing else.
#[must_use]
pub fn build(log: &[Entry], outcomes: &[Outcome], today: u64, watermark_slot: u64) -> Rows {
    let asked_day = (today / DAY).saturating_sub(7);
    let (from, to) = (asked_day * DAY, (asked_day + 1) * DAY);

    let mut latest: BTreeMap<String, &Outcome> = BTreeMap::new();
    for o in outcomes {
        let key = o.mint.to_string();
        if latest
            .get(&key)
            .is_none_or(|have| o.measured_at > have.measured_at)
        {
            latest.insert(key, o);
        }
    }

    let rows = log
        .iter()
        .filter(|e| e.at >= from && e.at < to && e.reply_id.is_some())
        .filter_map(|e| {
            let mint = e.mint.clone()?;
            let outcome = latest.get(&mint);
            Some(Row {
                graduation: outcome.and_then(|o| o.graduation_mode()).map(|m| match m {
                    GraduationMode::Instant => Graduation::Instant,
                    GraduationMode::Organic => Graduation::Organic,
                }),
                // No transfer at or after the slot the reply was read at.
                //
                // **The outcome has to be newer than the reply**, and it usually
                // is not. The store measures a token at one hour, six hours and
                // a day after launch and then never again, so for any coin that
                // was already older than a day when somebody asked about it —
                // which is most coins people ask about — the latest outcome was
                // recorded *before* the reply. Its `last_transfer_slot` is
                // therefore from before the reply too, `last <= read` is true by
                // arithmetic, and the post said the coin "had no transfer since
                // we answered" about a coin trading on the AMM that afternoon.
                //
                // A public statement about a named coin, false for a structural
                // reason, on the account's flagship post. `None` here is "cannot
                // say", which is what the store actually knows.
                quiet_since_reply: match (e.read_at_slot, outcome) {
                    (Some(read), Some(o)) if o.measured_at.get() > read => {
                        o.last_transfer_slot.map(|last| last.get() <= read)
                    }
                    _ => None,
                },
                held_bps: outcome.and_then(|o| o.held_to_end_gain_bps()),
                mint,
                asked_at: e.at,
                reply_id: e.reply_id.clone(),
            })
        })
        .collect();

    Rows {
        asked_on: date_of(from),
        built_at: today,
        watermark_slot,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_types::{Address, Slot};

    fn entry(mint: [u8; 32], at: u64, reply: Option<&str>, slot: Option<u64>) -> Entry {
        Entry {
            at,
            mention_id: format!("m-{}", mint[0]),
            mint: Some(Address::new(mint).to_string()),
            read_at_slot: slot,
            reply_id: reply.map(str::to_owned),
        }
    }

    fn outcome(
        mint: [u8; 32],
        measured_at: u64,
        graduated_after: Option<u64>,
        last_transfer: Option<u64>,
        first: Option<u64>,
        last: Option<u64>,
    ) -> Outcome {
        Outcome {
            mint: Address::new(mint),
            measured_at: Slot(measured_at),
            launch_slot: Slot(1_000),
            first_transfer_slot: Some(Slot(1_000)),
            last_transfer_slot: last_transfer.map(Slot),
            transfers: 3,
            unique_senders: 2,
            unique_receivers: 2,
            graduated_at: graduated_after.map(|d| Slot(1_000 + d)),
            first_price: first,
            last_price: last,
            peak_price: None,
            trough_price: None,
            window_peak_price: None,
            window_trough_price: None,
            vwap: None,
            fills: 0,
        }
    }

    const TODAY: u64 = 20_701 * DAY + 11 * 3_600; // 2026-09-05 11:00 UTC

    #[test]
    fn the_rows_are_last_weeks_published_replies_joined_to_the_latest_outcome() {
        let week_ago = TODAY - 7 * DAY;
        let log = vec![
            entry([1; 32], week_ago, Some("r1"), Some(2_000)), // organic, quiet
            entry([2; 32], week_ago + 10, Some("r2"), Some(2_000)), // instant, traded since
            entry([3; 32], week_ago + 20, None, Some(2_000)),  // dry run: not public
            entry([4; 32], week_ago + 30, Some("r4"), None),   // no slot: cannot say quiet
            entry([5; 32], week_ago - DAY, Some("r5"), Some(2_000)), // eight days ago
            entry([6; 32], TODAY - DAY, Some("r6"), Some(2_000)), // yesterday
            // The day's edges: the first second of the next day is out, the
            // last second of the day is in. Re-applied `<` as `<=` and r7 is
            // counted.
            entry([7; 32], (week_ago / DAY + 1) * DAY, Some("r7"), Some(2_000)),
            entry(
                [8; 32],
                (week_ago / DAY + 1) * DAY - 1,
                Some("r8"),
                Some(2_000),
            ),
        ];
        let outcomes = vec![
            outcome([1; 32], 5_000, Some(900), Some(1_900), Some(100), Some(50)),
            // Two measurements of mint 2: the later one has the later transfer.
            outcome([2; 32], 4_000, Some(1), Some(1_500), Some(100), Some(120)),
            outcome([2; 32], 6_000, Some(1), Some(2_500), Some(100), Some(130)),
            outcome([4; 32], 5_000, None, Some(1_700), None, None),
        ];
        let rows = build(&log, &outcomes, TODAY, 6_000);
        assert_eq!(rows.asked_on, "2026-08-29");
        assert_eq!(rows.watermark_slot, 6_000);
        let mints: Vec<&str> = rows
            .rows
            .iter()
            .map(|r| r.reply_id.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(mints, ["r1", "r2", "r4", "r8"], "{rows:?}");

        let r1 = &rows.rows[0];
        assert_eq!(r1.graduation, Some(Graduation::Organic));
        assert_eq!(r1.quiet_since_reply, Some(true));
        assert_eq!(r1.held_bps, Some(-5_000));

        let r2 = &rows.rows[1];
        assert_eq!(r2.graduation, Some(Graduation::Instant));
        assert_eq!(
            r2.quiet_since_reply,
            Some(false),
            "the later measurement wins"
        );
        assert_eq!(r2.held_bps, Some(3_000));

        let r4 = &rows.rows[2];
        assert_eq!(r4.graduation, None);
        assert_eq!(r4.quiet_since_reply, None, "no reply slot: cannot say");
        assert_eq!(r4.held_bps, None);
    }

    #[test]
    fn a_mint_the_store_never_measured_is_a_row_with_nothing_known() {
        // Re-applied by dropping unmeasured mints: the row disappears and the
        // day under-counts what the bot was asked about.
        let week_ago = TODAY - 7 * DAY;
        let log = vec![entry([9; 32], week_ago, Some("r9"), Some(2_000))];
        let rows = build(&log, &[], TODAY, 1);
        assert_eq!(rows.rows.len(), 1);
        assert_eq!(rows.rows[0].graduation, None);
        assert_eq!(rows.rows[0].quiet_since_reply, None);
        assert_eq!(rows.rows[0].held_bps, None);
    }

    #[test]
    fn an_outcome_older_than_the_reply_cannot_say_the_coin_went_quiet() {
        // The false statement this post was going to publish about named coins.
        //
        // The store measures at one hour, six hours and a day after launch and
        // then never again. So for any coin already older than a day when
        // somebody asked about it -- most coins people ask about -- the latest
        // outcome predates the reply, `last_transfer_slot <= read_at_slot` is
        // true by arithmetic, and the post said "had no transfer since we
        // answered" about a coin trading on the AMM that afternoon.
        let asked_at = 7 * DAY + 100;
        let today = 14 * DAY + 100;
        let log = [entry([1u8; 32], asked_at, Some("r1"), Some(500_000))];
        let stale = [outcome([1u8; 32], 400_000, None, Some(390_000), None, None)];

        let rows = build(&log, &stale, today, 600_000);
        assert_eq!(rows.rows.len(), 1);
        assert_eq!(
            rows.rows[0].quiet_since_reply, None,
            "an outcome measured before the reply says nothing about after it"
        );
    }

    #[test]
    fn an_outcome_newer_than_the_reply_can_say_it() {
        // The other half. A filter that answered `None` for everything would
        // pass the test above and make the clause unreachable -- and the post
        // would quietly stop reporting the one thing it is for.
        let asked_at = 7 * DAY + 100;
        let today = 14 * DAY + 100;
        let log = [entry([1u8; 32], asked_at, Some("r1"), Some(500_000))];

        // Measured after the reply, last transfer before it: genuinely quiet.
        let quiet = [outcome([1u8; 32], 700_000, None, Some(490_000), None, None)];
        assert_eq!(
            build(&log, &quiet, today, 800_000).rows[0].quiet_since_reply,
            Some(true)
        );

        // Measured after the reply, and it moved since: not quiet.
        let moved = [outcome([1u8; 32], 700_000, None, Some(510_000), None, None)];
        assert_eq!(
            build(&log, &moved, today, 800_000).rows[0].quiet_since_reply,
            Some(false)
        );
    }

    #[test]
    fn rows_write_via_a_sibling_and_a_rename_and_the_path_matches_the_old_layout() {
        let dir = std::env::temp_dir().join(format!("radar-seven-days-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let dir_str = dir.to_string_lossy().into_owned();

        assert_eq!(date_of(20_701 * DAY + 12 * 3_600), "2026-09-05");
        let path = rows_path(&dir_str, "2026-09-05");
        assert_eq!(path, format!("{dir_str}/2026-09-05.json"));

        let rows = Rows {
            asked_on: "2026-08-29".to_owned(),
            built_at: 1_788_600_000,
            watermark_slot: 444_505_805,
            rows: vec![Row {
                mint: "M".to_owned(),
                asked_at: 1,
                reply_id: Some("r".to_owned()),
                graduation: None,
                quiet_since_reply: None,
                held_bps: None,
            }],
        };
        rows.write(&path).expect("write");
        let text = std::fs::read_to_string(&path).expect("read back");
        let back: Rows = serde_json::from_str(&text).expect("parses");
        assert_eq!(back, rows);
        assert!(
            !std::path::Path::new(&format!("{path}.tmp")).exists(),
            "the temporary file must not survive"
        );
    }
}
