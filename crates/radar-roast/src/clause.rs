// SPDX-License-Identifier: Apache-2.0
//! Whole clauses: the model chooses sentences, and writes none of them.
//!
//! # What this closes that the tag layer did not
//!
//! `tags.rs` (removed by this change, and its argument kept below) bound the
//! **value**. The model wrote prose with `[F1]` in it, Radar substituted its own
//! rendering, and a digit outside a tag threw the reply away. That made every
//! published *digit* one Radar measured.
//!
//! It did not make every published *claim* one Radar measured. A `Fact` bound a
//! label and a rendering; nothing bound the subject, the window, the unit, the
//! comparison or the negation in the prose around the tag. So
//!
//! ```text
//!   "only [F3] of coins like this one ever recover"
//! ```
//!
//! substituted cleanly, passed the digit refusal, passed the fidelity check, and
//! is a fabricated claim carrying an authorised figure. At the substitution
//! boundary it is indistinguishable from an honest sentence, because the only
//! thing that boundary can see is the number.
//!
//! # What the model chooses now
//!
//! - **Which** facts to publish, out of the ones that carry a vetted clause.
//! - **In what order.**
//! - **Which [`Voice`] variant** of each clause to use.
//!
//! That is genuinely most of what makes a reply pointed: deciding that the
//! creator's record matters more than the capacity here, and putting the count
//! that damns it second. What the model no longer does is write a subject, a
//! number, a unit, a window, a negation, a comparison or a verdict. Every one of
//! those is in a [`Clause`], written in code, by the same file that read the
//! measurement.
//!
//! AGENTS.md §5's ladder: this is level 1. The alternative was a checker that
//! reads generated prose and decides whether its subject matches the
//! measurement, which is a natural-language judgement — wrong in both
//! directions, and its false positives would spend the credibility of every
//! check standing beside it.
//!
//! # Why the connector between two clauses is a space and nothing else
//!
//! The obvious next thing is a table of dry connectors — "but", "and yet",
//! "despite that" — picked in code so the model still cannot write one. Every
//! one of those words asserts a **relationship** between the two clauses it
//! joins, and a relationship between two measurements is a comparison. ADR 0016
//! forbids the model from writing a comparison; a connector table that does not
//! know which two facts it sits between cannot write one honestly either. So the
//! clauses are complete sentences and the connector is a single space, and this
//! paragraph is here so the next person to reach for "but" reads the reason
//! first.
//!
//! # A fact with no clause cannot be published
//!
//! [`Fact::clauses`](crate::sheet::Fact::clauses) is allowed to be empty, and an
//! empty one is not selectable — it is not given a number in the sheet the model
//! reads, and [`parse`] has nothing that could name it. That is how a
//! measurement stays unpublishable while remaining visible: the withheld
//! self-mint note, the "could not be read" lines, and any family admitted to the
//! sheet before its wording has been reviewed. **Adding a measurement and
//! publishing it are two separate acts**, and this is the mechanism that keeps
//! them separate rather than a rule someone remembers.

use crate::sheet::{Fact, FactSheet};

/// A stable name for a measurement.
///
/// A selection, a log line and (item 5b) a receipt all have to refer to a fact
/// after the sheet that produced it is gone. An index into `FactSheet::facts` is
/// not that reference: the list is built conditionally — a creator with no
/// measured launches contributes four fewer facts — so `F3` means different
/// things on two sheets, and a stored `F3` means nothing at all.
///
/// Non-exhaustive because adding a measurement is expected and must not be a
/// breaking change for a reader of old records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Kind {
    /// Distinct token accounts credited in the launch block.
    LaunchRecipients,
    /// Transactions in the launch block.
    LaunchTransactions,
    /// SOL the creator spent on their own token in the launch block.
    DevBuy,
    /// No creator buy was found. Absent, not zero.
    DevBuyUnseen,
    /// Tokens this creator has launched, in Radar's record.
    CreatorLaunches,
    /// How many of those have had an outcome measured.
    CreatorMeasured,
    /// Of the measured, how many filled a curve over time.
    CreatorOrganic,
    /// Of the measured, how many filled inside three slots.
    CreatorInstant,
    /// Of the measured, how many showed almost no activity.
    CreatorStillborn,
    /// Transactions by the creator's address. Transactions, not launches.
    CreatorTransactions,
    /// The venue has no measured outcome at all.
    VenueUnmeasured,
    /// Launches Radar has recorded and measured.
    VenueMeasured,
    /// Share of measured launches that graduated at all.
    VenueGraduated,
    /// Share that filled a curve over time.
    VenueOrganic,
    /// Share that filled inside their own launch block.
    VenueInstant,
    /// Share that showed almost no activity.
    VenueStillborn,
    /// The recipient count was truncated, so no band applies.
    BandUnavailable,
    /// Share of that band's launches that never graduated.
    BandNeverGraduated,
    /// Share of that band's launches that graduated over time.
    BandOrganic,
    /// Share of that band's launches that graduated instantly.
    BandInstant,
    /// Probability a launch in that band graduates instantly.
    BandInstantProbability,
    /// How many times the base rate that probability is.
    BandTimesBaseRate,
    /// Population rate: share of all launches graduating instantly.
    BaseInstant,
    /// Population rate: share of all launches graduating at all.
    BaseGraduates,
    /// Whether the token has left the bonding curve.
    Graduated,
    /// Capacity is not measurable because the token graduated.
    CapacityAfterGraduation,
    /// SOL that can be bought before price moves one percent.
    Capacity,
    /// No capacity at all could be sized.
    CapacityNone,
    /// The venue's round-trip fee, from the on-chain schedule.
    VenueFee,
    /// The all-in round trip Radar's kernel assumes.
    RoundTripKernel,
    /// The edge a strategy must clear before a trade is worth making.
    RoundTripBar,
    /// The round trip for one position size.
    CostBand,
    /// The note that this token's price is never stated.
    SelfMintWithheld,
}

/// Which register a clause is written in.
///
/// Two, deliberately. A variant is a sentence somebody has to author and
/// review; a long list of them is a long list of unreviewed sentences wearing
/// the authority of reviewed ones. Two is enough for the model to have a real
/// choice about tone and small enough that every one of them has been read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Voice {
    /// States the measurement and stops.
    Plain,
    /// The same measurement, said shorter and harder. Never a different claim.
    Blunt,
}

impl Voice {
    /// The word the model writes to choose this variant.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Blunt => "blunt",
        }
    }

    /// Reads the word back. Anything else is not a voice.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "plain" => Some(Self::Plain),
            "blunt" => Some(Self::Blunt),
            _ => None,
        }
    }
}

/// One complete sentence, written in code.
///
/// Subject, verb, number, unit, window and limitation are all in `text`. There
/// is no hole in it for a model to fill, which is the entire point: a sentence
/// with a hole is a sentence whose claim was decided somewhere other than here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clause {
    /// The register this variant speaks in.
    pub voice: Voice,
    /// The sentence, complete, ending in its own punctuation.
    pub text: String,
}

impl Clause {
    /// Builds one.
    #[must_use]
    pub fn new(voice: Voice, text: impl Into<String>) -> Self {
        Self {
            voice,
            text: text.into(),
        }
    }
}

/// One fact the model may name, and the number it is named by.
///
/// The number is a position in *this* rendering, not a [`Kind`] and not an
/// index into `FactSheet::facts` — it counts only the selectable facts, so the
/// model is never shown a number it is not allowed to write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offered {
    /// What the model writes: the `n` in `F<n>`.
    pub number: usize,
    /// Where the fact lives in [`FactSheet::facts`].
    pub at: usize,
}

/// Every fact on the sheet that carries at least one vetted clause.
///
/// The order is the sheet's order, and the numbering is dense over the facts
/// that survive the filter: a sheet whose second fact has no clause offers
/// `F1`, `F2`, `F3` and not `F1`, `F3`, `F4`. A gap in the numbering is an
/// invitation to guess what fell in it.
#[must_use]
pub fn offered(sheet: &FactSheet) -> Vec<Offered> {
    sheet
        .facts
        .iter()
        .enumerate()
        .filter(|(_, fact)| !fact.clauses.is_empty())
        .enumerate()
        .map(|(number, (at, _))| Offered {
            number: number + 1,
            at,
        })
        .collect()
}

/// One choice: a fact, and which of its clauses to say it with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pick {
    /// Where the fact lives in [`FactSheet::facts`].
    pub at: usize,
    /// Which register.
    pub voice: Voice,
}

/// An accepted model answer: an ordered list of clauses to say.
///
/// Cannot be built outside this module, so nothing downstream can assemble a
/// reply out of picks that never went through [`parse`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    picks: Vec<Pick>,
}

impl Selection {
    /// The picks, in the order the reply says them.
    #[must_use]
    pub fn picks(&self) -> &[Pick] {
        &self.picks
    }
}

/// The most clauses one reply may carry.
///
/// Three sentences was the old prompt's ceiling and it is kept: a reply is read
/// on a timeline, and the fourth sentence is the one nobody reaches.
pub const MOST_CLAUSES: usize = 3;

/// Why a model answer was not a selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotSelected {
    /// A line that is not a pick.
    ///
    /// Carries the line as written, so an operator reading the log sees what
    /// the model actually did rather than "invalid".
    Unparsed(String),
    /// A fact number that is not on offer.
    NoSuchFact(usize),
    /// A register that does not exist.
    NoSuchVoice(String),
    /// The same fact twice. Saying one measurement in two registers is not two
    /// facts, and the second is the sentence that reads as padding.
    Repeated(usize),
    /// More picks than [`MOST_CLAUSES`].
    TooMany(usize),
    /// Nothing was selected at all.
    Empty,
}

impl core::fmt::Display for NotSelected {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unparsed(line) => write!(f, "wrote {line:?}, which is not a selection"),
            Self::NoSuchFact(n) => write!(f, "chose F{n}, which is not on the sheet"),
            Self::NoSuchVoice(v) => write!(f, "asked for the {v:?} voice, which does not exist"),
            Self::Repeated(n) => write!(f, "chose F{n} twice"),
            Self::TooMany(n) => write!(f, "chose {n} clauses, and {MOST_CLAUSES} is the most"),
            Self::Empty => write!(f, "chose nothing"),
        }
    }
}

impl std::error::Error for NotSelected {}

/// Reads the model's answer as a selection, or refuses the whole answer.
///
/// # The grammar, entire
///
/// One pick per line, `F<number>.<voice>`, blank lines ignored. That is all of
/// it. There is no prose position, no comment position and no free text the
/// parser tolerates, because a parser that skips what it does not understand is
/// a parser that lets the model write a sentence nobody reads.
///
/// A single bad line refuses the **whole** answer rather than being dropped.
/// Dropping it would publish a reply the model did not choose: the remaining
/// clauses in an order that made sense with the fourth one in it.
///
/// # Errors
///
/// [`NotSelected`], naming what the model did: wrote something that is not a
/// pick, named a fact that is not on offer, asked for a register nobody
/// authored for that fact, repeated a fact, chose more than [`MOST_CLAUSES`], or
/// chose nothing. Every one of them ships the deterministic template.
pub fn parse(answer: &str, sheet: &FactSheet) -> Result<Selection, NotSelected> {
    let on_offer = offered(sheet);
    let mut picks: Vec<Pick> = Vec::new();
    let mut chosen: Vec<usize> = Vec::new();

    for line in answer.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let pick = read_line(line)?;
        let (number, voice) = pick;
        let Some(offer) = on_offer.iter().find(|o| o.number == number) else {
            return Err(NotSelected::NoSuchFact(number));
        };
        if chosen.contains(&number) {
            return Err(NotSelected::Repeated(number));
        }
        // A fact whose clauses do not include this register. Not the same
        // failure as an unknown register, and worth telling apart in a log: one
        // is the model inventing a word, the other is it asking for a sentence
        // nobody has written yet.
        if !sheet.facts[offer.at]
            .clauses
            .iter()
            .any(|c| c.voice == voice)
        {
            return Err(NotSelected::NoSuchVoice(voice.tag().to_owned()));
        }
        chosen.push(number);
        picks.push(Pick {
            at: offer.at,
            voice,
        });
    }

    if picks.is_empty() {
        return Err(NotSelected::Empty);
    }
    if picks.len() > MOST_CLAUSES {
        return Err(NotSelected::TooMany(picks.len()));
    }
    Ok(Selection { picks })
}

/// Reads one line into a fact number and a register.
///
/// Split out because it is the decision, and the function it was inside walks a
/// sheet — so a test could only reach this through a whole `FactSheet`, and the
/// mutation gate could only report that something in `parse` survived.
fn read_line(line: &str) -> Result<(usize, Voice), NotSelected> {
    let unparsed = || NotSelected::Unparsed(line.to_owned());
    let rest = line.strip_prefix('F').ok_or_else(unparsed)?;
    let (number, voice) = rest.split_once('.').ok_or_else(unparsed)?;
    // `parse::<usize>` accepts a leading `+`, which would make `F+1` a second
    // spelling of `F1`. One spelling per pick: two spellings is two log lines
    // that have to be recognised as the same choice later.
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return Err(unparsed());
    }
    let number = number.parse::<usize>().map_err(|_| unparsed())?;
    let voice = Voice::from_tag(voice).ok_or_else(|| NotSelected::NoSuchVoice(voice.to_owned()))?;
    Ok((number, voice))
}

/// Joins the selected clauses into the reply.
///
/// The connector is a space. See the module header for why it is not a word.
#[must_use]
pub fn assemble(selection: &Selection, sheet: &FactSheet) -> String {
    selection
        .picks()
        .iter()
        .filter_map(|pick| {
            sheet.facts[pick.at]
                .clauses
                .iter()
                .find(|c| c.voice == pick.voice)
                .map(|c| c.text.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The sheet as the model reads it.
///
/// Selectable facts carry their number and every register they can be said in.
/// Everything else goes below, marked as context — the model needs to know that
/// the creator has no record here in order to decide that the launch block is
/// the story, and it must not be able to publish that line until somebody has
/// written the sentence.
#[must_use]
pub fn render_for_selection(sheet: &FactSheet) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    out.push_str("SELECTABLE -- write F<number>.<voice>, one per line:\n");
    for offer in offered(sheet) {
        let fact = &sheet.facts[offer.at];
        let _ = writeln!(out, "F{}  ({})", offer.number, fact.label);
        for clause in &fact.clauses {
            let _ = writeln!(out, "    .{:<6} {}", clause.voice.tag(), clause.text);
        }
    }

    let context: Vec<&Fact> = sheet
        .facts
        .iter()
        .filter(|f| f.clauses.is_empty())
        .collect();
    if !context.is_empty() {
        out.push_str("\nCONTEXT -- true, and NOT publishable. Use it to choose, never to say:\n");
        for fact in context {
            let _ = writeln!(out, "  {}: {}", fact.label, fact.rendered);
        }
    }
    if !sheet.unknown.is_empty() {
        out.push_str("\nNOT KNOWN -- also not publishable:\n");
        for line in &sheet.unknown {
            let _ = writeln!(out, "  {line}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet_of(facts: Vec<Fact>) -> FactSheet {
        FactSheet {
            mint: "MintMintMint".to_owned(),
            read_at: None,
            facts,
            untrusted: Vec::new(),
            unknown: Vec::new(),
            signals: Vec::new(),
        }
    }

    fn selectable(kind: Kind, label: &str, text: &str) -> Fact {
        Fact::exact(kind, label, 1.0, "1")
            .saying(Voice::Plain, text)
            .saying(Voice::Blunt, text)
    }

    #[test]
    fn a_fact_with_no_clause_is_not_offered_and_cannot_be_named() {
        // The mechanism that keeps "measured" and "publishable" apart. The
        // withheld self-mint note, every "could not be read" line, and any
        // family admitted before its wording is reviewed all live here.
        let sheet = sheet_of(vec![
            selectable(Kind::LaunchRecipients, "recipients", "Six token accounts."),
            // No clause: on the sheet, never on the timeline.
            Fact::exact(Kind::SelfMintWithheld, "withheld", 0.0, "n/a"),
        ]);

        let on_offer = offered(&sheet);
        assert_eq!(on_offer.len(), 1, "only the one with a clause is offered");
        assert_eq!(on_offer[0].number, 1);

        assert_eq!(
            parse("F2.plain", &sheet),
            Err(NotSelected::NoSuchFact(2)),
            "the unpublishable fact has no number the model could write"
        );
    }

    #[test]
    fn the_numbering_is_dense_over_the_selectable_facts() {
        // A gap invites a guess about what fell in it. The second fact here has
        // no clause, so the third selectable fact is F2 and there is no F3.
        let sheet = sheet_of(vec![
            selectable(Kind::LaunchRecipients, "a", "A."),
            Fact::exact(Kind::SelfMintWithheld, "gap", 0.0, "n/a"),
            selectable(Kind::LaunchTransactions, "b", "B."),
        ]);

        let numbers: Vec<usize> = offered(&sheet).iter().map(|o| o.number).collect();
        assert_eq!(numbers, vec![1, 2]);

        // And F2 is the third fact, not the second.
        assert_eq!(offered(&sheet)[1].at, 2);
    }

    #[test]
    fn the_grammar_admits_a_pick_and_refuses_everything_else() {
        assert_eq!(read_line("F1.plain"), Ok((1, Voice::Plain)));
        assert_eq!(read_line("F12.blunt"), Ok((12, Voice::Blunt)));

        // Prose. The whole failure this layer exists to close: a model that
        // writes a sentence must not have that sentence published.
        assert!(matches!(
            read_line("only 3% of coins like this ever recover"),
            Err(NotSelected::Unparsed(_))
        ));
        // A pick with something appended is not a pick.
        assert!(matches!(
            read_line("F1.plain -- and this one is dead"),
            Err(NotSelected::NoSuchVoice(_))
        ));
        // The old tag spelling, which is now a sentence fragment and not a
        // selection.
        assert!(matches!(read_line("[F1]"), Err(NotSelected::Unparsed(_))));
        assert!(matches!(read_line("F1"), Err(NotSelected::Unparsed(_))));
        assert!(matches!(
            read_line("F.plain"),
            Err(NotSelected::Unparsed(_))
        ));
        assert!(matches!(
            read_line("1.plain"),
            Err(NotSelected::Unparsed(_))
        ));
        // One spelling per pick.
        assert!(matches!(
            read_line("F+1.plain"),
            Err(NotSelected::Unparsed(_))
        ));
        assert!(matches!(
            read_line("F1.savage"),
            Err(NotSelected::NoSuchVoice(_))
        ));
    }

    #[test]
    fn one_bad_line_refuses_the_whole_answer() {
        // Not dropped. Dropping it publishes a reply the model did not choose:
        // the surviving clauses in an order that was chosen with the dropped one
        // still in it.
        let sheet = sheet_of(vec![
            selectable(Kind::LaunchRecipients, "a", "A."),
            selectable(Kind::LaunchTransactions, "b", "B."),
        ]);
        assert!(matches!(
            parse("F1.plain\nand it is a rug\nF2.blunt", &sheet),
            Err(NotSelected::Unparsed(_))
        ));
    }

    #[test]
    fn a_fact_is_never_said_twice() {
        let sheet = sheet_of(vec![selectable(Kind::LaunchRecipients, "a", "A.")]);
        assert_eq!(
            parse("F1.plain\nF1.blunt", &sheet),
            Err(NotSelected::Repeated(1))
        );
    }

    #[test]
    fn an_empty_or_overlong_answer_is_refused() {
        let facts: Vec<Fact> = (0..5)
            .map(|_| selectable(Kind::LaunchRecipients, "a", "A."))
            .collect();
        let sheet = sheet_of(facts);
        assert_eq!(parse("", &sheet), Err(NotSelected::Empty));
        assert_eq!(parse("   \n\n  ", &sheet), Err(NotSelected::Empty));
        assert_eq!(
            parse("F1.plain\nF2.plain\nF3.plain\nF4.plain", &sheet),
            Err(NotSelected::TooMany(4))
        );
        assert!(parse("F1.plain\nF2.plain\nF3.plain", &sheet).is_ok());
    }

    #[test]
    fn the_reply_is_the_clauses_in_the_order_chosen() {
        let sheet = sheet_of(vec![
            Fact::exact(Kind::LaunchRecipients, "a", 1.0, "1")
                .saying(Voice::Plain, "First, plainly.")
                .saying(Voice::Blunt, "First, hard."),
            Fact::exact(Kind::LaunchTransactions, "b", 2.0, "2")
                .saying(Voice::Plain, "Second, plainly."),
        ]);

        let selection = parse("F2.plain\nF1.blunt", &sheet).expect("a selection");
        assert_eq!(
            assemble(&selection, &sheet),
            "Second, plainly. First, hard."
        );
    }

    #[test]
    fn what_is_not_known_reaches_the_model_and_an_empty_section_never_does() {
        // Rule 9 in the one place the model can act on it. An unknown is
        // something the reply is expected to *say* -- via a clause, when one is
        // written -- and the model can only choose to lead on an absence if it
        // is shown the absence.
        //
        // The other half is that the section is absent rather than empty when
        // nothing is unknown. A heading with nothing under it reads to a model
        // as a list it failed to receive.
        let mut sheet = sheet_of(vec![selectable(Kind::LaunchRecipients, "a", "A.")]);
        assert!(
            !render_for_selection(&sheet).contains("NOT KNOWN"),
            "an empty section was added"
        );

        sheet
            .unknown
            .push("the bonding curve could not be read".to_owned());
        let rendered = render_for_selection(&sheet);
        assert!(rendered.contains("NOT KNOWN"), "{rendered}");
        assert!(rendered.contains("the bonding curve could not be read"));
    }

    #[test]
    fn a_refusal_says_what_the_model_actually_did() {
        // The line an operator reads in `radar roast`'s output, and the only
        // thing that tells "wrote a sentence" apart from "asked for a register
        // nobody authored". Those want different fixes -- the prompt, and the
        // clause list -- so an empty or uniform message costs a diagnosis.
        assert!(
            NotSelected::Unparsed("it is a rug".to_owned())
                .to_string()
                .contains("it is a rug"),
            "the offending line is quoted back"
        );
        for (why, wanted) in [
            (NotSelected::NoSuchFact(9), "F9"),
            (NotSelected::Repeated(2), "F2"),
            (NotSelected::NoSuchVoice("savage".to_owned()), "savage"),
            (NotSelected::TooMany(4), "4"),
            (NotSelected::Empty, "nothing"),
        ] {
            let said = why.to_string();
            assert!(said.contains(wanted), "{why:?} said {said:?}");
        }
    }

    #[test]
    fn a_register_nobody_wrote_for_that_fact_is_refused() {
        // Different from an unknown register, and the parser says which: one is
        // the model inventing a word, the other is it asking for a sentence that
        // has not been authored.
        let sheet = sheet_of(vec![
            Fact::exact(Kind::LaunchRecipients, "a", 1.0, "1").saying(Voice::Plain, "Only plain."),
        ]);
        assert_eq!(
            parse("F1.blunt", &sheet),
            Err(NotSelected::NoSuchVoice("blunt".to_owned()))
        );
    }
}
