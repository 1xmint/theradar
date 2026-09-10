// SPDX-License-Identifier: Apache-2.0
//! Keeping a `consider` run, and reading it back at breakfast.
//!
//! [`radar_types::SessionRecord`] is the vocabulary; this is the disk and the
//! page. Two halves, and they are separate on purpose:
//!
//! - [`write`] and [`latest`] put one record per run under `<store>/sessions`,
//!   keyed by watermark then start time, so a directory listing comes back in
//!   the order a reader wants without anything being opened.
//! - [`render`] turns one into the report a person reads. Plain text: the
//!   audience is somebody with a coffee, not a dashboard.
//!
//! # Why the renderer is the dangerous half
//!
//! Every honesty property the record type holds can be undone here by printing
//! a number where the type says there is none. AGENTS §4 rule 9 — *absent is not
//! zero* — is easiest to break in a report, because a zero prints identically
//! whether it was measured or assumed. So the three places a zero could appear
//! and must not are handled by name rather than by formatting:
//!
//! 1. **A window nobody collected.** [`render`] reads
//!    [`WindowCoverage::zero_is_a_measurement`] before it prints a candidate
//!    count, and prints the word *unknown* instead of the figure when the answer
//!    is no.
//! 2. **A holding nobody could price.** [`EquityTotal::Unknown`] prints its
//!    reason and no figure. Not the realised half on its own, which is the
//!    smaller number and the one that would get permission.
//! 3. **A refusal nobody kept.** Every refused count is printed beside the
//!    proposed count, and the funnel's own residual is printed rather than
//!    absorbed.
//!
//! # Why JSON and not a store table
//!
//! One row per run, holding nested tallies whose reason codes outlive the code
//! that raised them. Flattening that into Arrow would either lose the nesting or
//! add a column per reason, and the store's promise is that DuckDB can read its
//! Parquet directly — a table whose shape changes when a strategy gains a
//! refusal is not that. The store keeps the per-candidate
//! [`Decision`](radar_store::Decision) rows; this keeps the run they were taken
//! in.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use radar_types::{
    AccountView, EarliestEntry, EquityTotal, MoneySpent, NoEntryTime, SessionRecord, Visibility,
    WindowCoverage,
};

/// The directory under a store where session records live.
pub const SESSIONS_DIR: &str = "sessions";

/// Where one record is written.
fn path_of(dir: &str, record: &SessionRecord) -> PathBuf {
    Path::new(dir)
        .join(SESSIONS_DIR)
        .join(format!("{}.json", record.run_id))
}

/// Writes one record.
///
/// # Errors
///
/// Returns a message if the directory cannot be created or the file cannot be
/// written. Loud rather than swallowed, for [`write_decisions`]' reason: a run
/// that printed its findings and failed to keep them has not done the job.
///
/// [`write_decisions`]: crate::consider
pub fn write(dir: &str, record: &SessionRecord) -> Result<PathBuf, String> {
    let path = path_of(dir, record);
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    let json = serde_json::to_string_pretty(record)
        .map_err(|e| format!("cannot serialise the session record: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(path)
}

/// Reads one record from a path.
///
/// # Errors
///
/// Returns a message if the file cannot be read or does not parse.
pub fn read(path: &Path) -> Result<SessionRecord, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|e| format!("{} is not a session record: {e}", path.display()))
}

/// The most recent record under a store, or `None` when none has been kept.
///
/// `None` is *nobody kept a run here*, which is a different answer from a run
/// that found nothing — the caller says which, rather than printing an empty
/// report either way.
///
/// # Errors
///
/// Returns a message if the directory exists and cannot be listed, or if the
/// newest record does not parse. A directory that is simply absent is `None`
/// rather than an error: an instance that has never recorded a session is a
/// normal state.
pub fn latest(dir: &str) -> Result<Option<SessionRecord>, String> {
    let sessions = Path::new(dir).join(SESSIONS_DIR);
    if !sessions.is_dir() {
        return Ok(None);
    }
    let mut newest: Option<PathBuf> = None;
    let entries =
        fs::read_dir(&sessions).map_err(|e| format!("cannot list {}: {e}", sessions.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot list {}: {e}", sessions.display()))?;
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        // Lexicographic on the file name, which is what the key is padded for.
        // Reading a modification time instead would order by when a file was
        // touched rather than by the watermark the run was taken at, and those
        // differ the moment anything is copied.
        if newest.as_ref().is_none_or(|best| {
            best.file_name().unwrap_or_default() < path.file_name().unwrap_or_default()
        }) {
            newest = Some(path);
        }
    }
    newest.map(|p| read(&p)).transpose()
}

/// Micro-dollars as a dollar figure, signed.
fn dollars(micro: i64) -> String {
    let sign = if micro < 0 { "-" } else { "" };
    let magnitude = micro.unsigned_abs();
    format!(
        "{sign}${}.{:06}",
        magnitude / 1_000_000,
        magnitude % 1_000_000
    )
}

/// The morning report.
///
/// One string, so the whole page is testable without capturing stdout.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one page of report, written in the order it is read. Splitting it \
              into a function per section would put the reading order in a \
              caller nobody opens, and the sections are not independently \
              useful -- every one of them is a denominator for the ones above."
)]
pub fn render(r: &SessionRecord) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "RADAR — what the last consider run saw");
    let _ = writeln!(out, "run           : {}", r.run_id);
    let _ = writeln!(out, "store         : {}", r.store);
    let _ = writeln!(
        out,
        "evidence ready: slot {} — the watermark everything below was taken as of",
        r.decided_at.get()
    );
    let _ = writeln!(
        out,
        "ran           : {} for {}s",
        radar_types::civil::timestamp_from_seconds(r.started_at_unix),
        r.elapsed_secs()
    );
    let _ = writeln!(
        out,
        "strategy      : {} {}, round trip assumed at {} bps",
        r.strategy, r.strategy_version, r.assumed_round_trip_bps
    );
    let _ = writeln!(out, "exit priced on: {}", r.pricing);
    let _ = writeln!(
        out,
        "build         : {}",
        r.build
            .as_deref()
            .unwrap_or("unknown (built outside release CI)")
    );
    if r.policy_closed {
        let _ = writeln!(
            out,
            "policy        : refuses everything. There was no eligible slot for any
                action below, which is why no entry time is named."
        );
    } else {
        let _ = writeln!(
            out,
            "policy        : CAN AUTHORISE. What is below was judged against a policy
                that is not the shipped one."
        );
    }

    // ---- what the recorder collected ------------------------------------
    out.push_str("\nWHAT THE RECORDER COLLECTED\n");
    for table in &r.coverage.tables {
        match table.state {
            radar_types::CoverageState::NeverAttested => {
                let _ = writeln!(
                    out,
                    "  {:<12}: NOTHING ATTESTS THIS TABLE. Not zero rows — no record says
                  a collection over it was ever run.",
                    table.table
                );
            }
            radar_types::CoverageState::Attested {
                complete_spans,
                measured_empty,
                unfinished,
            } => {
                let _ = writeln!(
                    out,
                    "  {:<12}: {complete_spans} range(s) ran and observed slots, \
                     {measured_empty} ran and measured nothing, {unfinished} did not finish",
                    table.table
                );
            }
        }
    }
    match r.coverage.window {
        WindowCoverage::Attested { from, to } => {
            let _ = writeln!(
                out,
                "  the window this run considered is attested over slots {}–{}.",
                from.get(),
                to.get()
            );
        }
        WindowCoverage::MeasuredEmpty => {
            out.push_str(
                "  the window this run considered was collected and held nothing.
  Zero candidates below is a measurement about the market.\n",
            );
        }
        WindowCoverage::Unattested => {
            out.push_str(
                "  NOTHING ATTESTS THE WINDOW THIS RUN CONSIDERED.
  No completed collection covers these slots, so every count below is a
  fact about the recorder before it is a fact about the market. A zero
  here has not measured anything.\n",
            );
        }
    }

    // ---- the funnel -----------------------------------------------------
    let f = &r.funnel;
    let measured = r.coverage.window.zero_is_a_measurement();
    // The one place a count is withheld. A zero over an unattested window
    // prints as the word, because the figure would read as a finding.
    let count = |n: usize| -> String {
        if n == 0 && !measured {
            "unknown".to_owned()
        } else {
            n.to_string()
        }
    };

    out.push_str("\nWHAT IT LOOKED AT — every stage, including the ones that produced nothing\n");
    let _ = writeln!(
        out,
        "  launches on record       : {}",
        count(f.launches_recorded)
    );
    let _ = writeln!(
        out,
        "  creators on record       : {}",
        count(f.creators_recorded)
    );
    let _ = writeln!(
        out,
        "  inside the {} slot window : {}   <- the denominator",
        r.window_slots,
        count(f.in_window)
    );
    let _ = writeln!(
        out,
        "    no candidate could be built from : {}",
        f.unbuildable
    );
    let _ = writeln!(
        out,
        "    refused by the free tier         : {}",
        f.refused_free
    );
    let _ = writeln!(
        out,
        "    worth a paid look                : {}",
        f.worth_paying_for
    );
    let _ = writeln!(
        out,
        "      examined                       : {}",
        f.paid_examined
    );
    let _ = writeln!(
        out,
        "      not examined, cap of {} ran out : {}   <- worth paying for, nobody looked",
        r.paid_tier_cap, f.deferred_by_cap
    );
    let _ = writeln!(
        out,
        "        refused on launch-block shape: {}",
        f.refused_on_shape
    );
    let _ = writeln!(
        out,
        "        launch block unreadable      : {}   <- the gate was off, not clean",
        f.look_failed
    );
    let _ = writeln!(
        out,
        "        dropped after the exit probe : {}",
        f.dropped_after_probe
    );
    let _ = writeln!(
        out,
        "        passed over after the look   : {}",
        f.passed_paid
    );
    let _ = writeln!(out, "        PROPOSED                     : {}", f.proposed);
    let _ = writeln!(
        out,
        "          authorised by the kernel   : {}",
        f.kernel_authorised
    );
    let _ = writeln!(
        out,
        "          refused by the kernel      : {}",
        f.kernel_refused
    );
    let _ = writeln!(
        out,
        "          never reached the kernel   : {}",
        f.kernel_unseen()
    );

    let leak = f.unaccounted();
    if leak == 0 {
        out.push_str("  every candidate in the window is in exactly one bucket above.\n");
    } else {
        let _ = writeln!(
            out,
            "  {leak} CANDIDATE(S) THIS FUNNEL CANNOT ACCOUNT FOR. A stage lost rows,
  or counted more than reached it. Every rate derived from the numbers
  above is wrong by at least that much.",
        );
    }
    let paid_leak = f.paid_unaccounted();
    if paid_leak != 0 {
        let _ = writeln!(
            out,
            "  {paid_leak} EXAMINED CANDIDATE(S) THE PAID TIER CANNOT ACCOUNT FOR.",
        );
    }

    // ---- refusals -------------------------------------------------------
    out.push_str(
        "\nWHY IT SAID NO — the refusals are the product, so they are the denominator
  (one candidate raises several reasons; these sum past the candidate count)\n",
    );
    let mut section = |title: &str, rows: &[(String, usize)]| {
        if rows.is_empty() {
            let _ = writeln!(out, "  {title}: none raised");
        } else {
            let _ = writeln!(out, "  {title}:");
            for (reason, n) in rows {
                let _ = writeln!(out, "    {n:>6}  {reason}");
            }
        }
    };
    section("free tier", &r.refusals.free_tier);
    section("after the paid look", &r.refusals.paid_tier);
    section("risk kernel", &r.refusals.kernel);
    section("account", &r.refusals.portfolio);

    // ---- spend ----------------------------------------------------------
    out.push_str("\nWHAT IT COST — including the work that produced nothing\n");
    for call in &r.spend.calls {
        let _ = writeln!(
            out,
            "  {:<18}: {} attempted, {} failed, {} answered and changed nothing",
            call.kind, call.attempted, call.failed, call.produced_nothing
        );
    }
    let _ = writeln!(
        out,
        "  total             : {} call(s), {} failed, {} bought nothing",
        r.spend.attempted(),
        r.spend.failed(),
        r.spend.wasted()
    );
    match &r.spend.money {
        MoneySpent::Measured {
            micro_usd,
            rate_table,
        } => {
            let _ = writeln!(
                out,
                "  in money          : {} at rate table {rate_table}",
                dollars(i64::try_from(*micro_usd).unwrap_or(i64::MAX))
            );
        }
        MoneySpent::Unmeasured(why) => {
            let _ = writeln!(
                out,
                "  in money          : UNKNOWN ({why:?}). No provider or model is selected
                      on this instance, so no rate exists to price those calls
                      with. That is not the same as free."
            );
        }
    }

    // ---- timings --------------------------------------------------------
    out.push_str("\nWHEN IT COULD HAVE ACTED\n");
    let _ = writeln!(
        out,
        "  evidence was ready at slot {}, and reasoning took {} ms.",
        r.timings.evidence_ready_at.get(),
        r.timings.reasoning_ms
    );
    if r.timings.candidates.is_empty() {
        out.push_str("  no candidate reached the paid tier, so no fill clock ran.\n");
    }
    for c in &r.timings.candidates {
        let visible = match c.visible_at {
            Visibility::At(slot) => format!("visible from slot {}", slot.get()),
            Visibility::Unattested => "nothing attests when it became visible".to_owned(),
        };
        let entry = match c.earliest_entry {
            EarliestEntry::At(slot) => format!("earliest entry slot {}", slot.get()),
            EarliestEntry::Unknown(NoEntryTime::PolicyRefused) => {
                "no eligible entry — the policy authorises nothing".to_owned()
            }
            EarliestEntry::Unknown(NoEntryTime::LandingLatencyUnmeasured) => {
                "earliest entry UNKNOWN — no build-and-land latency measured here".to_owned()
            }
            EarliestEntry::Unknown(NoEntryTime::VisibilityUnattested) => {
                "earliest entry UNKNOWN — no clock has a start".to_owned()
            }
        };
        let _ = writeln!(
            out,
            "  {}  launched slot {}, {visible}, reasoned {} ms, {entry}",
            c.mint,
            c.launch_slot.get(),
            c.reasoning_ms
        );
    }
    out.push_str(
        "  A fill at the launch price for a decision taken later is a return nobody
  could have had, so no entry slot is named unless all four of evidence,
  reasoning, policy and a landed transaction can be dated.\n",
    );

    // ---- equity ---------------------------------------------------------
    out.push_str("\nWHAT THE ACCOUNT IS WORTH\n");
    let AccountView::Read(e) = &r.equity else {
        // Not a row of zeros. An account nobody could read and one holding
        // nothing are the same picture once a report has flattened them, and
        // only one of them is a measurement.
        if let AccountView::Unreadable { because } = &r.equity {
            let _ = writeln!(
                out,
                "  THE ACCOUNT COULD NOT BE READ: {because}
  Nothing about it is known — not that it is empty, not what it is worth,
  and not what it has already committed."
            );
        }
        return out;
    };
    let _ = writeln!(
        out,
        "  {} holding(s), {} recorded exposure(s) it could not place at all",
        e.holdings, e.unaccounted
    );
    let _ = writeln!(out, "  realised   : {}", dollars(e.realised_micro_usd));
    match e.unrealised {
        radar_types::UnrealisedReport::Known { micro_usd, as_of } => {
            let _ = writeln!(
                out,
                "  unrealised : {} as of slot {}",
                dollars(micro_usd),
                as_of.get()
            );
        }
        radar_types::UnrealisedReport::Unknown(why) => {
            let _ = writeln!(out, "  unrealised : UNKNOWN ({why:?})");
        }
    }
    match e.equity {
        EquityTotal::Known { micro_usd, as_of } => {
            let _ = writeln!(
                out,
                "  EQUITY     : {} as of slot {}",
                dollars(micro_usd),
                as_of.get()
            );
        }
        EquityTotal::Unknown(why) => {
            let _ = writeln!(
                out,
                "  EQUITY     : UNKNOWN ({why:?}). A holding nobody could price does not
               make the total smaller, it removes it. The realised half is
               above and is not the equity."
            );
        }
    }
    let _ = writeln!(
        out,
        "  paid out   : {} lamport(s) in fees, {} in rent, {} in tips
               (lamports, not dollars — converting needs a SOL price this
                record does not hold)",
        e.costs.network_fee_lamports, e.costs.rent_lamports, e.costs.tip_lamports
    );

    out
}

#[cfg(test)]
mod tests {
    use super::{latest, render, write};
    use radar_types::{
        AccountView, CallTally, CandidateTiming, CostsReport, CoverageReport, CoverageState,
        EarliestEntry, EquityReport, EquityTotal, Funnel, MoneySpent, NoEntryTime, Refusals,
        SESSION_SCHEMA, SessionRecord, SignedMicroUsd, Slot, Spend, TableCoverage, Timings,
        UnrealisedReport, Unvaluable, Visibility, WindowCoverage,
    };

    /// A run that collected, looked and refused. The baseline the honesty tests
    /// bend one field of at a time.
    fn a_run() -> SessionRecord {
        SessionRecord {
            schema: SESSION_SCHEMA,
            run_id: SessionRecord::key(Slot(500_000), 1_757_462_400),
            started_at_unix: 1_757_462_400,
            finished_at_unix: 1_757_462_447,
            decided_at: Slot(500_000),
            store: "data".to_owned(),
            window_slots: 216_000,
            paid_tier_cap: 25,
            strategy: "creator_edge".to_owned(),
            strategy_version: "0.1.0".to_owned(),
            assumed_round_trip_bps: 850,
            pricing: "curve".to_owned(),
            policy_closed: true,
            build: None,
            coverage: CoverageReport {
                tables: vec![TableCoverage {
                    table: "launches".to_owned(),
                    state: CoverageState::Attested {
                        complete_spans: 3,
                        measured_empty: 0,
                        unfinished: 0,
                    },
                }],
                window: WindowCoverage::Attested {
                    from: Slot(290_000),
                    to: Slot(499_999),
                },
            },
            funnel: Funnel {
                launches_recorded: 41_254,
                creators_recorded: 9_102,
                in_window: 200,
                unbuildable: 0,
                refused_free: 175,
                worth_paying_for: 25,
                paid_examined: 25,
                deferred_by_cap: 0,
                refused_on_shape: 4,
                look_failed: 1,
                dropped_after_probe: 0,
                passed_paid: 18,
                proposed: 3,
                kernel_authorised: 0,
                kernel_refused: 3,
            },
            refusals: Refusals {
                free_tier: vec![("CreatorUnproven".to_owned(), 175)],
                paid_tier: vec![("CapacityBelowFloor".to_owned(), 18)],
                kernel: vec![("NoAutonomy".to_owned(), 3)],
                portfolio: vec![],
            },
            spend: Spend {
                calls: vec![CallTally {
                    kind: "launch_block".to_owned(),
                    attempted: 25,
                    failed: 1,
                    produced_nothing: 20,
                }],
                money: MoneySpent::default(),
            },
            timings: Timings {
                evidence_ready_at: Slot(500_000),
                reasoning_ms: 41_000,
                candidates: vec![CandidateTiming {
                    mint: "So11111111111111111111111111111111111111112".to_owned(),
                    launch_slot: Slot(499_000),
                    visible_at: Visibility::At(Slot(499_600)),
                    reasoning_ms: 300,
                    earliest_entry: EarliestEntry::Unknown(NoEntryTime::PolicyRefused),
                }],
            },
            equity: AccountView::Read(EquityReport {
                holdings: 0,
                unaccounted: 0,
                realised_micro_usd: 0,
                unrealised: UnrealisedReport::Known {
                    micro_usd: 0,
                    as_of: Slot(500_000),
                },
                equity: EquityTotal::Known {
                    micro_usd: 0,
                    as_of: Slot(500_000),
                },
                costs: CostsReport::default(),
            }),
        }
    }

    /// The record with its account read, for the tests that bend one field of
    /// it. Panics rather than defaulting if the baseline stops carrying one:
    /// a test helper that quietly substitutes an empty account would make the
    /// equity tests pass for the wrong reason.
    fn read_account(r: &mut SessionRecord) -> &mut EquityReport {
        match &mut r.equity {
            AccountView::Read(report) => report,
            AccountView::Unreadable { .. } => panic!("the baseline run reads its account"),
        }
    }

    #[test]
    fn a_run_over_a_window_nobody_collected_says_so_instead_of_reporting_zero() {
        // The first named honesty test. Re-apply the bug -- print the count
        // whatever the coverage says -- and the second assertion fails with a
        // report claiming a measured empty market.
        let mut r = a_run();
        r.coverage.window = WindowCoverage::Unattested;
        r.coverage.tables[0].state = CoverageState::NeverAttested;
        r.funnel = Funnel::default();

        let page = render(&r);
        assert!(
            page.contains("NOTHING ATTESTS THE WINDOW THIS RUN CONSIDERED"),
            "a reader must be told before they read a single count:\n{page}"
        );
        assert!(
            page.contains("inside the 216000 slot window : unknown"),
            "an uncollected window reports unknown, never a zero that reads as a \
             measurement:\n{page}"
        );
        assert!(
            page.contains("NOTHING ATTESTS THIS TABLE"),
            "and the table itself says nobody collected it:\n{page}"
        );
    }

    #[test]
    fn a_collected_window_that_held_nothing_reports_its_zero_as_a_measurement() {
        // The other direction, and it has to hold or the first test passes by
        // refusing to print anything. A completed collection that found nothing
        // is a fact about the market and is reported as one.
        let mut r = a_run();
        r.coverage.window = WindowCoverage::MeasuredEmpty;
        r.funnel = Funnel::default();

        let page = render(&r);
        assert!(
            page.contains("inside the 216000 slot window : 0"),
            "a measured empty window prints its zero:\n{page}"
        );
        assert!(page.contains("Zero candidates below is a measurement about the market"));
    }

    #[test]
    fn an_unvaluable_holding_leaves_no_equity_figure_in_the_report() {
        // The second named honesty test. Re-apply the bug -- print the realised
        // half as the equity when the unrealised one is unknown -- and this
        // fails on the dollar figure it finds.
        let mut r = a_run();
        let account = read_account(&mut r);
        account.holdings = 1;
        account.realised_micro_usd = 4_000_000;
        account.unrealised = UnrealisedReport::Unknown(Unvaluable::NoPrice);
        account.equity = EquityTotal::of(SignedMicroUsd(4_000_000), account.unrealised);

        let page = render(&r);
        let equity_line = page
            .lines()
            .find(|l| l.contains("EQUITY"))
            .expect("the report states an equity line either way");
        assert!(
            equity_line.contains("UNKNOWN"),
            "an unpriceable holding removes the total:\n{equity_line}"
        );
        assert!(
            !equity_line.contains('$'),
            "and no dollar figure may appear on it — $4.00 is the number that \
             would get permission:\n{equity_line}"
        );
        assert!(
            page.contains("does not\n               make the total smaller, it removes it"),
            "the reader is told which way the absence goes:\n{page}"
        );
    }

    #[test]
    fn the_refused_and_the_proposed_are_both_on_the_page() {
        // The third named honesty test. A run that considered two hundred and
        // proposed three reports two hundred. Re-apply the bug -- drop the
        // refusal rows, or report only what survived -- and this fails.
        let page = render(&a_run());

        assert!(
            page.contains("inside the 216000 slot window : 200"),
            "the denominator is the window, not the survivors:\n{page}"
        );
        assert!(page.contains("refused by the free tier         : 175"));
        assert!(page.contains("refused on launch-block shape: 4"));
        assert!(page.contains("PROPOSED                     : 3"));
        assert!(page.contains("refused by the kernel      : 3"));
        assert!(
            page.contains("175  CreatorUnproven"),
            "and every refusal reason keeps its own count:\n{page}"
        );
        assert!(page.contains("3  NoAutonomy"));
        assert!(
            page.contains("every candidate in the window is in exactly one bucket"),
            "a funnel that balances says so, so that one that does not is visible:\n{page}"
        );
    }

    #[test]
    fn a_funnel_that_lost_candidates_says_so_on_the_page() {
        // The residual is printed rather than absorbed. Re-apply the bug --
        // compute the refused count as the remainder instead of counting it --
        // and this leak becomes invisible.
        let mut r = a_run();
        r.funnel.refused_free = 100;
        let page = render(&r);
        assert!(
            page.contains("75 CANDIDATE(S) THIS FUNNEL CANNOT ACCOUNT FOR"),
            "{page}"
        );
    }

    #[test]
    fn a_pass_nobody_priced_reports_no_bill_rather_than_a_free_one() {
        let page = render(&a_run());
        assert!(page.contains("in money          : UNKNOWN"));
        assert!(page.contains("That is not the same as free"), "{page}");
        assert!(
            page.contains("25 attempted, 1 failed, 20 answered and changed nothing"),
            "the calls that bought nothing are charged and printed:\n{page}"
        );
    }

    #[test]
    fn a_closed_policy_names_no_entry_slot_at_all() {
        let page = render(&a_run());
        assert!(page.contains("no eligible entry — the policy authorises nothing"));
        assert!(
            page.contains("visible from slot 499600"),
            "availability time is printed beside the launch slot, not instead of \
             it:\n{page}"
        );
        assert!(page.contains("launched slot 499000"));
    }

    #[test]
    fn a_record_round_trips_through_disk_and_the_newest_comes_back() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_str().expect("utf-8 path").to_owned();

        assert_eq!(
            latest(&root).expect("an empty store lists"),
            None,
            "nobody kept a run here, which is not the same as a run that found \
             nothing"
        );

        let older = a_run();
        let mut newer = a_run();
        newer.decided_at = Slot(600_000);
        newer.run_id = SessionRecord::key(Slot(600_000), 1_757_462_400);

        write(&root, &older).expect("writes");
        write(&root, &newer).expect("writes");

        let back = latest(&root).expect("lists").expect("a record is there");
        assert_eq!(
            back, newer,
            "the newest watermark comes back, not the first"
        );
    }
}
