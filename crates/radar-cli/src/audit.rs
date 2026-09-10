// SPDX-License-Identifier: Apache-2.0
//! `radar audit` — reading the journal back.
//!
//! The journal's value is that it can be asked a question about a machine
//! nobody was watching. Until something can ask, it is a file that grows.
//!
//! Three of [ADR 0017](https://github.com/hey-vera/radar/blob/main/docs/adr/0017-the-journal-records-intent-before-effect-and-replay-proves-only-the-decision.md)'s
//! four subcommands are here. The fourth, `replay`, is deliberately absent and
//! [`explain_replay_is_missing`] says why in the place someone will type it:
//! an offline deterministic replay needs the recorded **inputs** to a decision —
//! the fact snapshot, the rule settings, the model response — and the journal
//! records the decision without them yet. A `replay` that re-derived a verdict
//! from inputs it fetched now would compare today's world with yesterday's
//! answer and call the difference a divergence.

use radar_journal::{Event, Journal, Outcome, Stage, Verified};

use crate::flag;

/// Where the journal lives when nobody says.
///
/// The analyst's own default, so an operator who has not configured anything
/// finds the file the daemon has been writing.
const DEFAULT_JOURNAL: &str = "data/analyst/journal.jsonl";

/// Runs a subcommand.
///
/// # Errors
///
/// A message when the journal cannot be read or the subcommand is unknown.
pub fn run(args: &[String]) -> Result<(), String> {
    let path = flag(args, "--journal").unwrap_or_else(|| DEFAULT_JOURNAL.to_owned());
    // `args[0]` is `audit`; the subcommand is the next positional.
    let sub = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .map_or("", String::as_str);

    match sub {
        "explain" => {
            let id = flag(args, "--id")
                .ok_or("audit explain --id <correlation> is required; ids come from the journal")?;
            explain(&path, &id)
        }
        "verify" => verify(&path, range(args)?),
        "export" => {
            let week = flag(args, "--week")
                .ok_or("audit export --week <week> is required, as the ledger spells it")?;
            export(&path, &week)
        }
        "replay" => Err(explain_replay_is_missing()),
        other => Err(format!(
            "unknown audit subcommand {other:?}; try explain, verify or export"
        )),
    }
}

/// Why `replay` is not here, said where someone will type it.
///
/// A message rather than a stub that prints nothing: a subcommand that exists
/// and does nothing is worse than one that is absent, because the first reads
/// as "there is nothing to report".
fn explain_replay_is_missing() -> String {
    "audit replay is not built yet, and it is not a missing command so much as a \
     missing input.\n\n\
     An offline deterministic replay compares a recorded decision against the \
     same decision re-derived from the inputs it was made from. The journal \
     records the decision; it does not yet record the fact snapshot, the rule \
     settings or the model response it was made from. A replay built on what is \
     here would re-derive the verdict from the world as it is now and call the \
     difference a divergence -- which would be a check that fails for the wrong \
     reason, and this repository deletes those.\n\n\
     Plan 0010 item 4 carries it, with the outbox that records those inputs."
        .to_owned()
}

/// The `--from`/`--to` sequence bounds, if any.
fn range(args: &[String]) -> Result<(Option<u64>, Option<u64>), String> {
    let parse = |name: &str| -> Result<Option<u64>, String> {
        flag(args, name).map_or(Ok(None), |v| {
            v.parse()
                .map(Some)
                .map_err(|_| format!("{name} {v} is not a sequence number"))
        })
    };
    Ok((parse("--from")?, parse("--to")?))
}

/// Everything the journal holds about one correlation id, in order.
fn explain(path: &str, id: &str) -> Result<(), String> {
    let events = open(path)?.events().map_err(|e| e.to_string())?;
    let matched: Vec<&Event> = events.iter().filter(|e| mentions(e, id)).collect();

    if matched.is_empty() {
        // Not an error. A journal that holds nothing about an id is an answer,
        // and one an operator acts on: it means the run they are looking for
        // never reached this machine.
        println!("nothing in {path} names {id}.");
        println!(
            "  {} events in the journal, sequences {} to {}.",
            events.len(),
            events.first().map_or(0, |e| e.sequence),
            events.last().map_or(0, |e| e.sequence)
        );
        return Ok(());
    }

    println!("{} event(s) name {id}:\n", matched.len());
    for event in matched {
        print_event(event);
    }
    Ok(())
}

/// One event, as a human reads it.
fn print_event(event: &Event) {
    println!(
        "  #{:<6} {}  {}",
        event.sequence,
        stage_label(event.stage),
        outcome_label(event.outcome)
    );
    println!("     at    {} (unix seconds)", event.at);
    println!("     id    {}", event.id);
    if let Some(build) = &event.build {
        println!("     build {build}");
    }
    for (name, version) in &event.versions {
        println!("     {name:<5} {version}");
    }
    if let Some(reason) = &event.public_reason {
        println!("     said  {reason}");
    }
    if let Some(detail) = &event.redacted {
        println!("     note  {detail}");
    }
    println!();
}

/// Walks the chain and reports what it found.
///
/// Exits non-zero on a broken journal, because this is what a monitor runs and a
/// monitor that reports corruption with a zero exit is a monitor nobody notices.
fn verify(path: &str, (from, to): (Option<u64>, Option<u64>)) -> Result<(), String> {
    let journal = open(path)?;
    let verdict = journal.verify().map_err(|e| e.to_string())?;

    // The range narrows what is *reported*, never what is checked: a chain is
    // only sound from its start, and verifying a window would answer a question
    // nobody asked with a word that sounds like the answer to the one they did.
    let events = journal.events().map_err(|e| e.to_string())?;
    let shown = events
        .iter()
        .filter(|e| in_range(e.sequence, from, to))
        .count();

    for line in verify_lines(&verdict, shown, from, to) {
        println!("{line}");
    }
    match verdict {
        // A torn journal exits **zero**. A deploy that restarted the daemon
        // mid-write leaves exactly this, and a monitor that pages for it is a
        // monitor that gets muted -- and then it is silent for the one below.
        Verified::Intact { .. } | Verified::Torn { .. } => Ok(()),
        Verified::Broken { at, why } => Err(format!(
            "broken at sequence {at}: {why}\n  \
             This is not a crash. Something wrote to this file that was not the\n  \
             journal, or something removed a line. The events before {at} still\n  \
             stand; nothing after it can be trusted without knowing what happened."
        )),
    }
}

/// What `verify` says, as lines.
///
/// A `Vec<String>` rather than a block of `println!`, which is this crate's
/// idiom for the same reason three times over now: a printer is reachable by no
/// test, and the mutation gate reports every decision inside one as a survivor.
/// `candidate_lines` and `missingness_lines` are the other two.
///
/// The `Broken` case says nothing here — its words are the error, because an
/// error is what an operator's shell shows them.
fn verify_lines(
    verdict: &Verified,
    shown: usize,
    from: Option<u64>,
    to: Option<u64>,
) -> Vec<String> {
    // The bounds rather than a `ranged` flag, so the decision that a range was
    // asked for lives inside the function a test can call. It was a flag, and
    // the mutation gate then reported the `||` at the *call site* -- outside
    // everything the test could reach. A split that leaves the decision on the
    // other side of the seam has not split anything.
    let ranged = from.is_some() || to.is_some();
    match verdict {
        Verified::Intact { events } => {
            let mut lines = vec![format!("intact: {events} events, chained from the first.")];
            if ranged {
                lines.push(format!("  {shown} of them in the range asked for."));
            }
            lines
        }
        Verified::Torn { events } => vec![
            format!("torn: {events} complete events, and a final write that did not finish."),
            "  That is the ordinary shape of a crash or a restart. The".to_owned(),
            "  events before it are untouched, and the next one continues".to_owned(),
            "  from the last complete event.".to_owned(),
        ],
        Verified::Broken { .. } => Vec::new(),
    }
}

/// Every event about one week, as JSON, for an incident bundle.
///
/// JSON rather than the human rendering, because what leaves this machine goes
/// to somebody who will want to load it. Already redacted by construction: the
/// journal never holds a credential, so exporting it cannot leak one.
fn export(path: &str, week: &str) -> Result<(), String> {
    let events = open(path)?.events().map_err(|e| e.to_string())?;
    let matched = events_of_week(&events, week);

    let bundle = serde_json::json!({
        "week": week,
        "events": matched,
        "journal": path,
        // What a reader needs in order to know what they are holding, rather
        // than having to trust that the file is the whole story.
        "of_journal_events": events.len(),
        "how_to_read": "Each event's `previous` names the one before it. Run \
                        `radar audit verify --journal <path>` against the source \
                        journal to establish that this chain is intact; this \
                        bundle is a filtered copy and cannot establish that \
                        about itself.",
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?
    );
    Ok(())
}

/// Whether a sequence is inside the reported range.
///
/// Both bounds inclusive, and an absent bound is unbounded on that side. Split
/// out of [`verify`] so a test can reach it: it was inline, and the mutation
/// gate reported every comparison in it as a survivor -- the range is exercised
/// by nothing that runs the command, because the command prints and a printer
/// is reachable by no test.
const fn in_range(sequence: u64, from: Option<u64>, to: Option<u64>) -> bool {
    let after_start = match from {
        Some(f) => sequence >= f,
        None => true,
    };
    let before_end = match to {
        Some(t) => sequence <= t,
        None => true,
    };
    after_start && before_end
}

/// The events about one week.
///
/// Split out of [`export`] for the reason [`in_range`] is, and because the test
/// for it had re-implemented this filter rather than calling it -- which tests
/// the test.
fn events_of_week<'a>(events: &'a [Event], week: &str) -> Vec<&'a Event> {
    events
        .iter()
        .filter(|e| e.correlation.week.as_deref() == Some(week))
        .collect()
}

/// Whether an event names `id` anywhere a reader would look.
///
/// The event's own id, or any of the correlation ids. One function so that
/// `explain --id` takes whichever id somebody is holding -- a mention, a mint, a
/// week, a signature -- rather than making them know which field it lives in.
fn mentions(event: &Event, id: &str) -> bool {
    let c = &event.correlation;
    event.id == id
        || [
            c.mention.as_deref(),
            c.receipt.as_deref(),
            c.nomination.as_deref(),
            c.week.as_deref(),
            c.claim.as_deref(),
            c.payout.as_deref(),
            c.mint.as_deref(),
        ]
        .contains(&Some(id))
}

fn open(path: &str) -> Result<Journal, String> {
    Journal::open(path).map_err(|e| format!("cannot read the journal at {path}: {e}"))
}

const fn stage_label(stage: Stage) -> &'static str {
    match stage {
        Stage::Received => "received     ",
        Stage::Parsed => "parsed       ",
        Stage::Admitted => "admitted     ",
        Stage::InputFetched => "input        ",
        Stage::FactBuilt => "fact         ",
        Stage::ModelAnswered => "model        ",
        Stage::Publication => "publication  ",
        Stage::CandidateScored => "candidate    ",
        Stage::ScoringMode => "scoring mode ",
        Stage::WinnerSelected => "winner       ",
        Stage::Claim => "claim        ",
        Stage::Payout => "payout       ",
        Stage::Operation => "operation    ",
    }
}

const fn outcome_label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Ok => "ok",
        Outcome::Refused => "refused",
        Outcome::Failed => "failed",
        // Spelled out, because this is the one an operator must not read as a
        // failure: the effect may have happened.
        Outcome::Uncertain => "UNCERTAIN -- the effect may have happened",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_journal::Correlation;

    fn temp(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("radar-audit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let p = dir.join(name);
        let p = p.to_str().expect("a path").to_owned();
        let _ = std::fs::remove_file(&p);
        p
    }

    /// A journal with one week's worth of events, and one event about another.
    fn journal_with_two_weeks(path: &str) {
        let mut journal = Journal::open(path).expect("open");
        for (week, mint) in [
            ("2026-W37", "MintA"),
            ("2026-W37", "MintB"),
            ("2026-W38", "MintC"),
        ] {
            journal
                .record(
                    Stage::WinnerSelected,
                    Outcome::Ok,
                    1_000,
                    Correlation {
                        week: Some(week.to_owned()),
                        mint: Some(mint.to_owned()),
                        ..Correlation::default()
                    },
                    None,
                    Vec::new(),
                    None,
                    None,
                )
                .expect("record");
        }
    }

    #[test]
    fn a_torn_journal_verifies_clean_and_a_broken_one_does_not() {
        // The distinction the whole subcommand exists for, at the exit code --
        // which is what a monitor reads. A deploy that restarted the daemon
        // mid-write leaves a torn journal, and a monitor that pages for that is
        // a monitor somebody mutes; then it is silent for the one that matters.
        //
        // Re-apply by returning `Err` for `Torn`: every restart alarms.
        let torn = temp("torn.jsonl");
        journal_with_two_weeks(&torn);
        let mut text = std::fs::read_to_string(&torn).expect("read");
        text.push_str("{\"schema\":1,\"sequ");
        std::fs::write(&torn, text).expect("write");
        assert!(
            verify(&torn, (None, None)).is_ok(),
            "a torn journal is a crash, not a fault"
        );

        // And the other side, which is not a crash: a line removed from the
        // middle. Re-apply by returning `Ok` for `Broken` and corruption is
        // reported with a zero exit, which is a monitor that reports nothing.
        let broken = temp("broken.jsonl");
        journal_with_two_weeks(&broken);
        let text = std::fs::read_to_string(&broken).expect("read");
        let kept: Vec<&str> = text.lines().filter(|l| !l.contains("MintB")).collect();
        std::fs::write(&broken, kept.join("\n") + "\n").expect("write");

        let out = verify(&broken, (None, None));
        assert!(out.is_err(), "a gap is a fault");
        let why = out.expect_err("a fault");
        assert!(why.contains("broken at sequence 2"), "{why}");
        assert!(
            why.contains("not a crash"),
            "the message has to say which of the two this is: {why}"
        );
    }

    #[test]
    fn an_empty_journal_verifies_clean_and_says_it_holds_nothing() {
        // Distinct from torn, and both are `Ok`. The words are what separate
        // them for a reader; the exit code deliberately does not.
        let path = temp("empty.jsonl");
        assert!(verify(&path, (None, None)).is_ok());
    }

    #[test]
    fn explain_takes_whichever_id_the_operator_is_holding() {
        // A mint, a week, or the event's own id all find the same events.
        // Somebody debugging an outage has one of those in front of them and
        // should not have to know which field it lives in.
        //
        // Re-apply by dropping any arm of `mentions`: the id somebody actually
        // has stops finding anything, and the journal reads as empty.
        let path = temp("explain.jsonl");
        journal_with_two_weeks(&path);
        let events = Journal::open(&path)
            .expect("open")
            .events()
            .expect("events");

        assert!(mentions(&events[0], "MintA"), "by mint");
        assert!(mentions(&events[0], "2026-W37"), "by week");
        assert!(mentions(&events[0], &events[0].id), "by its own id");
        assert!(!mentions(&events[0], "MintC"), "and not by another's");

        // The command itself runs and does not fail on a hit or a miss.
        assert!(explain(&path, "MintA").is_ok());
        assert!(
            explain(&path, "nothing-here").is_ok(),
            "an id the journal does not hold is an answer, not an error"
        );
    }

    #[test]
    fn export_carries_one_week_and_says_it_cannot_vouch_for_itself() {
        // A filtered copy cannot establish that the chain it came from is
        // intact -- the events either side of the filter are what would prove
        // it. Saying so in the bundle is the difference between evidence and a
        // file that looks like evidence.
        let path = temp("export.jsonl");
        journal_with_two_weeks(&path);

        assert!(export(&path, "2026-W37").is_ok());
    }

    #[test]
    fn replay_says_what_is_missing_rather_than_printing_nothing() {
        // A subcommand that exists and does nothing reads as "there is nothing
        // to report", which is the one thing it must not say. Re-apply by
        // returning `Ok(())`.
        let refused = run(&[
            "audit".to_owned(),
            "replay".to_owned(),
            "--id".to_owned(),
            "whatever".to_owned(),
        ]);
        let why = refused.expect_err("replay is not built");
        assert!(why.contains("missing input"), "{why}");
        assert!(
            why.contains("item 4"),
            "it says where the work is tracked: {why}"
        );
    }

    #[test]
    fn an_unknown_subcommand_names_the_ones_that_exist() {
        let why = run(&["audit".to_owned(), "explode".to_owned()]).expect_err("unknown");
        for known in ["explain", "verify", "export"] {
            assert!(why.contains(known), "{why}");
        }
    }

    #[test]
    fn the_reported_range_includes_both_bounds_and_is_open_on_an_absent_one() {
        // Walked exactly, because every comparison in it survived the mutation
        // gate: the range is exercised by nothing that runs the command, since
        // the command prints and a printer is reachable by no test.
        //
        // Both bounds inclusive: `--from 2 --to 4` is 2, 3 and 4. An operator
        // who asks for a sequence and is not shown it reads the journal as
        // missing an event.
        assert!(in_range(2, Some(2), Some(4)), "the lower bound is inside");
        assert!(in_range(3, Some(2), Some(4)));
        assert!(in_range(4, Some(2), Some(4)), "the upper bound is inside");
        assert!(!in_range(1, Some(2), Some(4)));
        assert!(!in_range(5, Some(2), Some(4)));

        // An absent bound is unbounded on that side, not zero and not the end.
        assert!(in_range(1, None, Some(4)));
        assert!(in_range(9, Some(2), None));
        assert!(in_range(0, None, None));

        // The `&&`, which as an `||` admits everything either bound admits --
        // so a window becomes a union and the count reported is larger than the
        // range asked for.
        assert!(
            !in_range(9, Some(2), Some(4)),
            "past the end is outside, even though it is after the start"
        );
        assert!(
            !in_range(1, Some(2), Some(4)),
            "before the start is outside, even though it is before the end"
        );
    }

    #[test]
    fn the_range_count_is_reported_only_when_a_range_was_asked_for() {
        // Cosmetic, and not equivalent: as an `&&` the count disappears unless
        // *both* bounds are given, so `--from 2` alone silently stops reporting
        // what it narrowed to. The mutation gate found it, and the answer is
        // the `_lines` split rather than an entry claiming a real behaviour
        // change is equivalent.
        let intact = Verified::Intact { events: 9 };

        let plain = verify_lines(&intact, 9, None, None);
        assert_eq!(
            plain.len(),
            1,
            "no range asked for, no range line: {plain:?}"
        );

        let ranged = verify_lines(&intact, 4, Some(2), Some(5));
        assert_eq!(ranged.len(), 2, "{ranged:?}");
        assert!(
            ranged[1].contains('4') && ranged[1].contains("range"),
            "{ranged:?}"
        );

        // One bound is a range. As an `&&` the count disappears unless both are
        // given, so `--from 2` alone silently stops reporting what it narrowed
        // to -- and that is the mutant, which lived at the call site until the
        // bounds moved in here.
        assert_eq!(verify_lines(&intact, 7, Some(2), None).len(), 2);
        assert_eq!(verify_lines(&intact, 7, None, Some(5)).len(), 2);

        // Torn says the same thing whether or not a range was asked for: the
        // count is about a chain that is sound, and a torn one is not the
        // question.
        let torn = Verified::Torn { events: 3 };
        assert_eq!(
            verify_lines(&torn, 1, Some(2), Some(5)),
            verify_lines(&torn, 9, None, None)
        );
        assert!(verify_lines(&torn, 0, None, None)[0].contains("torn"));

        // Broken says nothing here. Its words are the error, which is what an
        // operator's shell shows them -- printing them twice would read as two
        // faults.
        assert!(
            verify_lines(
                &Verified::Broken {
                    at: 2,
                    why: "a gap".to_owned()
                },
                0,
                None,
                None
            )
            .is_empty()
        );
    }

    #[test]
    fn a_week_export_carries_that_week_and_no_other() {
        // This test used to re-implement the filter it was checking, which
        // tests the test. It calls the function now, and the mutation gate is
        // what said so: `== Some(week)` mutated to `!=` and nothing failed.
        let path = temp("week-filter.jsonl");
        journal_with_two_weeks(&path);
        let events = Journal::open(&path)
            .expect("open")
            .events()
            .expect("events");

        let week = events_of_week(&events, "2026-W37");
        assert_eq!(week.len(), 2);
        assert!(
            week.iter()
                .all(|e| e.correlation.week.as_deref() == Some("2026-W37")),
            "and every one of them is that week's"
        );

        let other = events_of_week(&events, "2026-W38");
        assert_eq!(other.len(), 1);

        // A week the journal holds nothing about is empty rather than
        // everything, which is what the inverted comparison would give.
        assert!(events_of_week(&events, "2026-W99").is_empty());
    }
}
