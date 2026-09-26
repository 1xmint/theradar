// SPDX-License-Identifier: Apache-2.0
//! Per-unit CryptoHouse query counts for today, so `radar brief` can show
//! what [0036] found missing: nothing counted the queries each unit actually
//! spent, so the per-candidate cost was read from the call sites rather than
//! from a log.
//!
//! [0036]: ../../../docs/research/0036-the-hourly-consider-run-eats-the-whole-cryptohouse-allowance.md
//!
//! One small atomic state file beside the store, like the follow cursor
//! ([`crate::cursor`]) -- not a new store, because nothing here is queried,
//! joined, replayed, or gated by [`radar_asof::AsOf`]. It is a running tally
//! each unit adds its own run's counts to, and `radar brief` reads back.
//!
//! Text, not JSON, for the same reason [`crate::cursor`] hand-rolls its own
//! date arithmetic rather than pulling in a date crate: this is the only
//! state file in the store's own crate, and it is not worth a `serde_json`
//! dependency for four units and two numbers each.

use std::path::Path;

/// Where the meter lives inside the store.
pub const QUERY_METER_FILE: &str = ".query-meter";

/// One unit's queries and quota refusals so far today.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct UnitTally {
    /// Queries this unit issued to CryptoHouse today, across every run.
    pub queries: u64,
    /// How many of those CryptoHouse refused for quota.
    pub refused: u64,
}

/// Adds one run's counts to `unit`'s tally for `day`.
///
/// `day` is supplied by the caller rather than read from a clock -- every
/// other state file in this tree either carries no time at all or takes it as
/// an argument for the same reason [`crate::cursor`] gives: a caller passing
/// its own notion of "now" is what makes a run reproducible from a recording.
///
/// Rolls to a fresh, empty day when `day` is later than the file's day, the
/// same way a spend meter resets at midnight ([`radar_provider::cost::Meter`]
/// via `roll_to`) -- a tally that never rolled would report yesterday's
/// queries as today's forever.
///
/// # Errors
///
/// Returns the underlying message if the store directory or the file cannot
/// be written, or if the rename fails.
pub fn record(store: &Path, unit: &str, day: &str, queries: u64, refused: u64) -> Result<(), String> {
    std::fs::create_dir_all(store).map_err(|e| e.to_string())?;
    let path = store.join(QUERY_METER_FILE);
    let existing = std::fs::read_to_string(&path).ok();
    let mut units = existing
        .as_deref()
        .and_then(|raw| parse(raw))
        .filter(|(file_day, _)| file_day == day)
        .map_or_else(Vec::new, |(_, units)| units);

    match units.iter_mut().find(|(name, _, _)| name == unit) {
        Some((_, q, r)) => {
            *q += queries;
            *r += refused;
        }
        None => units.push((unit.to_owned(), queries, refused)),
    }

    let body = render(day, &units);
    let temporary = store.join(format!("{QUERY_METER_FILE}.new"));
    std::fs::write(&temporary, body).map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, &path).map_err(|e| e.to_string())
}

/// `unit`'s tally for `day`, or `None` when nothing has recorded against that
/// day yet -- a fresh day, a store from before this file existed, or a unit
/// that has not run today. Rule 9: absent is not zero, so a caller must not
/// render this as `0 queries`, which would claim a unit ran cleanly when
/// nobody has looked.
#[must_use]
pub fn today(store: &Path, unit: &str, day: &str) -> Option<UnitTally> {
    let raw = std::fs::read_to_string(store.join(QUERY_METER_FILE)).ok()?;
    let (file_day, units) = parse(&raw)?;
    if file_day != day {
        return None;
    }
    units
        .into_iter()
        .find(|(name, _, _)| name == unit)
        .map(|(_, queries, refused)| UnitTally { queries, refused })
}

/// Parses the meter file: a day on the first line, then one `unit queries
/// refused` triple per line after it.
///
/// `None` for anything that does not match -- an empty file, a torn write
/// caught mid-rename, or a store predating this format. The same shape of
/// answer [`crate::cursor::to_epoch`] gives a line it cannot read: absent,
/// not a default.
fn parse(raw: &str) -> Option<(String, Vec<(String, u64, u64)>)> {
    let mut lines = raw.lines();
    let day = lines.next()?.trim();
    if day.is_empty() {
        return None;
    }
    let mut units = Vec::new();
    for line in lines {
        let mut parts = line.split_whitespace();
        let name = parts.next()?;
        let queries: u64 = parts.next()?.parse().ok()?;
        let refused: u64 = parts.next()?.parse().ok()?;
        units.push((name.to_owned(), queries, refused));
    }
    Some((day.to_owned(), units))
}

/// Renders the meter file, the inverse of [`parse`].
fn render(day: &str, units: &[(String, u64, u64)]) -> String {
    let mut out = format!("{day}\n");
    for (name, queries, refused) in units {
        out.push_str(&format!("{name} {queries} {refused}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_store_has_no_tally_for_today() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(today(dir.path(), "consider", "2026-09-26"), None);
    }

    #[test]
    fn one_run_is_read_back_as_todays_tally() {
        let dir = tempfile::tempdir().expect("tempdir");
        record(dir.path(), "consider", "2026-09-26", 10, 3).expect("record");
        assert_eq!(
            today(dir.path(), "consider", "2026-09-26"),
            Some(UnitTally {
                queries: 10,
                refused: 3
            })
        );
    }

    #[test]
    fn two_runs_the_same_day_accumulate_rather_than_overwrite() {
        // `radar-follow` writes one small tally per window and there can be
        // dozens in a day -- if a second run replaced the first instead of
        // adding to it, the brief would report only the last window's count.
        let dir = tempfile::tempdir().expect("tempdir");
        record(dir.path(), "radar-follow", "2026-09-26", 4, 0).expect("record");
        record(dir.path(), "radar-follow", "2026-09-26", 6, 1).expect("record");
        assert_eq!(
            today(dir.path(), "radar-follow", "2026-09-26"),
            Some(UnitTally {
                queries: 10,
                refused: 1
            })
        );
    }

    #[test]
    fn units_are_tallied_separately() {
        let dir = tempfile::tempdir().expect("tempdir");
        record(dir.path(), "consider", "2026-09-26", 10, 0).expect("record");
        record(dir.path(), "radar-market-tape", "2026-09-26", 32, 2).expect("record");
        assert_eq!(
            today(dir.path(), "consider", "2026-09-26"),
            Some(UnitTally {
                queries: 10,
                refused: 0
            })
        );
        assert_eq!(
            today(dir.path(), "radar-market-tape", "2026-09-26"),
            Some(UnitTally {
                queries: 32,
                refused: 2
            })
        );
    }

    #[test]
    fn a_new_day_rolls_the_tally_rather_than_adding_to_yesterdays() {
        let dir = tempfile::tempdir().expect("tempdir");
        record(dir.path(), "consider", "2026-09-25", 10, 0).expect("record");
        record(dir.path(), "consider", "2026-09-26", 3, 1).expect("record");
        assert_eq!(
            today(dir.path(), "consider", "2026-09-26"),
            Some(UnitTally {
                queries: 3,
                refused: 1
            })
        );
        assert_eq!(today(dir.path(), "consider", "2026-09-25"), None);
    }

    #[test]
    fn a_unit_with_no_record_today_is_none_not_zero() {
        // Rule 9: absent is not zero. A day with other units recorded but not
        // this one must not be read as "ran clean", it must be read as "did
        // not run" -- a different fact `radar brief` has to say differently.
        let dir = tempfile::tempdir().expect("tempdir");
        record(dir.path(), "consider", "2026-09-26", 10, 0).expect("record");
        assert_eq!(today(dir.path(), "radar-follow", "2026-09-26"), None);
    }
}
