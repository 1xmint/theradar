// SPDX-License-Identifier: Apache-2.0
//! The fact sheet: every number the analyst is allowed to say.
//!
//! # This type is the security boundary
//!
//! The model is given this and nothing else, and afterwards every numeric
//! literal in what it wrote is checked back against it. So the set of numbers
//! reachable from here **is** the set of numbers that can be published, and a
//! field added here is a claim authorised.
//!
//! That is the same shape as `radar-signer`'s `verify::check`, which re-decodes
//! the bytes to confirm they match the authorisation rather than trusting the
//! caller's description of them. *The signer re-reads the bytes it signs; the
//! roaster re-reads the numbers it posts.*
//!
//! # Why the numbers are enumerated rather than inferred
//!
//! [`FactSheet::authorised`] lists every value a reply may contain, in every
//! form it may take — a share appears both as its ratio and as its percentage,
//! because a model told "0.251" will reasonably write "25%". Enumerating is
//! deliberate: the alternative is a checker that tries to guess which
//! transformations of a fact are legitimate, and a checker that guesses is one
//! that can be argued into accepting a number nobody measured.

use radar_onchain::budget::Count;
use radar_onchain::{Dossier, LaunchBlock};
use radar_types::Slot;

use crate::baserates::BaseRates;
use crate::clause::{Kind, Voice};
use std::fmt::Write as _;

/// Lamports in one SOL.
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

/// What a fact is a claim about, because one kind is withheld for one mint.
///
/// ADR 0013 constraint 5: the analyst never states its own token's price or
/// market capitalisation. That is enforced here rather than requested of the
/// model — a fact tagged [`About::Price`] is dropped from the sheet for the
/// configured mint **before the model sees it**, so the number is never in the
/// set the fidelity check would authorise.
///
/// **Nothing on the sheet is a price fact today.** Every figure the builder
/// emits is structure, history, depth, cost or population, so the rule has
/// nothing to drop yet. The variant exists so that the first price or
/// market-cap fact anyone adds is withheld for the analyst's own token by
/// construction, rather than by a reviewer remembering the ADR. The residual
/// is stated plainly: [`Fact::exact`] and [`Fact::share`] tag a measurement, so
/// an author adding a market-cap line through them and not through a literal
/// still has to choose the tag. There is no way to make the compiler ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum About {
    /// Structure, history, depth, cost or population -- what the analyst
    /// exists to state, about any token including its own.
    Measurement,
    /// The token's price or market capitalisation, in any unit and any form.
    Price,
}

/// One publishable number, with the words that make it a claim.
#[derive(Clone, Debug, PartialEq)]
pub struct Fact {
    /// What kind of claim this is. Decides whether the self-mint rule drops it.
    pub about: About,
    /// A stable name for the measurement, which survives the sheet.
    ///
    /// The position of a fact in [`FactSheet::facts`] does not: the list is
    /// built conditionally, so the third fact is a different measurement on two
    /// sheets. Anything that has to refer to this fact later — a selection, a
    /// log line, a receipt — refers to this.
    pub kind: crate::clause::Kind,
    /// What it is, in the fact sheet the model reads.
    pub label: String,
    /// How it renders.
    pub rendered: String,
    /// Every numeric value this fact authorises.
    ///
    /// More than one because a single measurement has several honest
    /// renderings: 0.251, 25.1 and 25 are the same fact said three ways, and a
    /// model that picks a different one has not invented anything.
    pub values: Vec<f64>,
    /// The complete sentences this fact may be published as, one per register.
    ///
    /// **Empty means the fact is true and unpublishable.** It is shown to the
    /// model as context it may reason from and given no number it could select,
    /// so a measurement cannot reach a timeline before somebody has written the
    /// sentence that states it. See [`crate::clause`].
    pub clauses: Vec<crate::clause::Clause>,
}

impl Fact {
    /// A measured fact whose only value is the one in its rendering.
    ///
    /// A **measurement**, never a price: a price or market-cap fact is built as
    /// a literal with [`About::Price`], so that the choice is written down where
    /// the self-mint rule can read it.
    ///
    /// Built with no clauses. Add them with [`Fact::saying`]; a fact that never
    /// gets one is context and not copy.
    #[must_use]
    pub fn exact(
        kind: crate::clause::Kind,
        label: impl Into<String>,
        value: f64,
        rendered: impl Into<String>,
    ) -> Self {
        Self {
            about: About::Measurement,
            kind,
            label: label.into(),
            rendered: rendered.into(),
            values: vec![value],
            clauses: Vec::new(),
        }
    }

    /// Adds one vetted clause.
    ///
    /// The whole sentence, written here, by the code that read the measurement:
    /// subject, verb, number, unit, window and limitation. Nothing downstream
    /// completes it.
    #[must_use]
    pub fn saying(mut self, voice: crate::clause::Voice, text: impl Into<String>) -> Self {
        self.clauses.push(crate::clause::Clause::new(voice, text));
        self
    }

    /// A share, authorised as a ratio, a percentage, and the percentage rounded.
    ///
    /// The rounded form is included because a reply that says "a quarter of
    /// them" or "25%" for 25.1% is being *readable*, not inventing. The check
    /// exists to stop fabrication, and a tolerance narrow enough to forbid
    /// ordinary rounding would push every reply to the deterministic template.
    #[must_use]
    pub fn share(kind: crate::clause::Kind, label: impl Into<String>, ratio: f64) -> Self {
        let pct = ratio * 100.0;
        // Precision follows the magnitude, and this is not cosmetic. The
        // strongest finding in 0024 is that launches with one to three
        // recipients graduate instantly **0.02%** of the time; at one decimal
        // place that renders as "0.0%", which reads as *never* rather than as
        // *rare*. A reply that says a thing never happens when it happens two
        // times in twelve thousand is wrong in the direction that gets quoted
        // back at you.
        let rendered = if pct > 0.0 && pct < 0.1 {
            format!("{pct:.2}%")
        } else {
            format!("{pct:.1}%")
        };
        Self {
            about: About::Measurement,
            kind,
            label: label.into(),
            rendered,
            values: vec![
                ratio,
                pct,
                pct.round(),
                (pct * 10.0).round() / 10.0,
                (pct * 100.0).round() / 100.0,
            ],
            clauses: Vec::new(),
        }
    }
}

/// One thing on the sheet that, by the published rule, is a reason to refuse.
///
/// Design 0009 §5, M3: the hunter rank counts, per summoned reply, the refusal
/// signals the fact sheet carried at the time. These are those, as a type
/// rather than a number, so the log says *which* fired and a later rule change
/// can re-score old replies from the record.
///
/// **The model never sees these.** They are not rendered into the sheet and the
/// word "signal" appears nowhere the model reads: the bot states measured
/// facts, and "this is a reason to refuse" is a verdict the facts already
/// carry. The count exists for the leaderboard, and it travels on the reply
/// log entry beside the sheet it was counted from.
///
/// Each variant is read off a fact the sheet states, never inferred from one it
/// could not read: a truncated recipient count, an unmeasured creator and an
/// unseen dev buy are all *no signal*, not a signal of zero (rule 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    /// The launch block's recipient count lands in the snapshot's strongest
    /// band -- the one most enriched for instant graduation -- or above it.
    ///
    /// Read from the snapshot, not named: research 0024 is the record of the
    /// band moving off six, and a constant here would have fired on the wrong
    /// launches from the day it moved.
    LaunchBlockInStrongestBand,
    /// The creator has measured launches and none of them filled over time.
    CreatorNeverGraduatedOrganically,
    /// The creator bought their own token in the launch block.
    CreatorBoughtOwnLaunch,
}

/// Everything the analyst may assert about one token.
#[derive(Clone, Debug)]
pub struct FactSheet {
    /// The mint, as text. Not a number, and never checked as one.
    pub mint: String,
    /// The slot every figure was read at.
    pub read_at: Option<Slot>,
    /// The facts, in the order they are shown to the model.
    pub facts: Vec<Fact>,
    /// Creator-supplied strings, kept apart from the facts.
    ///
    /// **Never inlined into the fact list**, because the fact list is what the
    /// model is told is true. These are fenced separately as untrusted, and a
    /// number appearing inside one of them authorises nothing.
    pub untrusted: Vec<(String, String)>,
    /// What could not be read, so the reply can say so plainly.
    ///
    /// "Radar has no record" is a thing the analyst is expected to say, and it
    /// can only say it if the absence survives to here rather than becoming a
    /// default somewhere below.
    pub unknown: Vec<String>,
    /// The refusal signals the facts carry, in a fixed order. See [`Signal`].
    pub signals: Vec<Signal>,
}

impl FactSheet {
    /// Builds the sheet from a dossier and the published base rates.
    ///
    /// `rates` is `None` when the snapshot could not be loaded. That is not a
    /// reason to fall back on remembered numbers: without it the sheet simply
    /// carries no population context, and the reply says less. Rule 8 — a
    /// missing input is a refusal to claim, not a default.
    ///
    /// `self_mint` is the analyst's own token, from `RADAR_SELF_MINT`, or
    /// `None` when no token is special. When the dossier is about that mint,
    /// every [`About::Price`] fact is dropped and the sheet says so
    /// ([`withhold_price`]). Everything else about the token is stated on the
    /// same rule as any other coin — ADR 0013 constraint 6 — which is why this
    /// is one filter and not a separate path.
    #[must_use]
    pub fn build(
        dossier: &Dossier,
        rates: Option<&BaseRates>,
        creators: Option<&crate::creator::CreatorIndex>,
        self_mint: Option<&radar_types::Address>,
    ) -> Self {
        let mut facts = Vec::new();
        let mut untrusted = Vec::new();
        let mut unknown = Vec::new();
        let mut signals = Vec::new();

        if let Some(launch) = &dossier.launch {
            push_launch(&mut facts, &mut untrusted, launch);
            if let Some(rates) = rates {
                push_population(&mut facts, launch.recipients, rates);
                // In the strongest band or above it. An exact count only: a
                // truncated one was decided by Radar's call budget, and a
                // signal read off it would be a signal about the budget.
                if let (Some(exact), Some(strongest)) =
                    (launch.recipients.exact(), rates.strongest_band())
                    && exact >= strongest.lo
                {
                    signals.push(Signal::LaunchBlockInStrongestBand);
                }
            }
            // A buy that was seen. `None` is "could not see", and the sheet
            // already refuses to call that "did not buy".
            if launch.dev_buy_lamports.is_some_and(|l| l > 0) {
                signals.push(Signal::CreatorBoughtOwnLaunch);
            }
        } else {
            unknown.push("the launch block could not be read".to_owned());
        }

        // **The fact that makes one reply differ from another.** The launch
        // block is about the block; three coins launched in the same minute
        // produce the same sentences from it, because the cost line is a
        // constant and most launches sit in the same recipient band. What this
        // creator did before is the part that is about *this* coin, and it is
        // the thing Radar has that nobody else does.
        //
        // **Outside the launch-block arm, and that is the whole point.** It sat
        // inside until 2026-09-06, so a coin whose launch block is past the
        // signature-page budget got no creator history at all -- and that is
        // every coin with real history, which is every coin somebody bothers to
        // ask about. The curve account carries the creator regardless of age,
        // so the launch block is preferred and the curve is the fallback.
        let creator = dossier
            .launch
            .as_ref()
            .map(|l| l.creator)
            .or_else(|| dossier.curve.as_ref().map(|c| c.creator));
        if let (Some(index), Some(address)) = (creators, creator) {
            let creator = address.to_string();
            push_creator(&mut facts, &mut unknown, &creator, index);
            // Measured and none organic. A creator whose launches have not been
            // measured has no record to hold against them.
            if let Some(record) = index.get(&creator)
                && record.measured > 0
                && record.organic == 0
            {
                signals.push(Signal::CreatorNeverGraduatedOrganically);
            }
        }

        if let Some(curve) = &dossier.curve {
            push_curve(&mut facts, curve);
        } else {
            unknown.push("the bonding curve could not be read".to_owned());
        }

        if let Some(count) = dossier.creator_transactions {
            let rendered = format!("{count}");
            facts.push(
                Fact::exact(
                    Kind::CreatorTransactions,
                    "transactions by this creator's address (transactions, not launches)",
                    f64::from(count.lower_bound()),
                    rendered.clone(),
                )
                .saying(
                    Voice::Plain,
                    format!("Radar has seen {rendered} transactions from this creator's address -- transactions, not launches."),
                )
                .saying(
                    Voice::Blunt,
                    format!("That address has {rendered} transactions on it. Transactions, not launches."),
                ),
            );
        }

        // Not inside the launch-block arm above, and deliberately: this is a
        // fact about the venue, not about the coin, so a mint whose launch block
        // could not be read still gets it. It is also what gives the creator's
        // counts a scale -- "none of 150 filled its curve" reads differently
        // once you know what share of everything does.
        if let Some(population) = creators.and_then(|c| c.population) {
            push_measured_population(&mut facts, &population);
        }

        if let Some(rates) = rates {
            push_cost(&mut facts, rates);
        }

        for miss in &dossier.unavailable {
            // **Radar's own phrase, never the raw reason.** `miss.why` is
            // diagnostic text -- "rpc transport: http status: 429", "no account
            // at <mint>" -- and two things are wrong with publishing it.
            //
            // It is an injection surface: a reason that echoes the mint would
            // put the attacker's own base58 into the trusted block, and
            // `authorised` reads numerals out of that block, so a mint chosen to
            // contain "68" would licence 68 as a publishable figure.
            //
            // And it is bad copy. A reader asking about a coin is owed "the
            // launch block could not be read", not an HTTP status. The raw
            // reason stays on the `Dossier` for the operator, where it belongs.
            unknown.push(phrase_for(miss.fact));
        }

        // **Said once, not twice.** Every unreadable fact reaches this list by
        // two routes: the field is `None`, and `Dossier::build` also recorded a
        // reason for it in `unavailable`. Both fire for the same failure, so a
        // reply about a token whose launch block could not be read told the
        // reader so twice, and a token where nothing could be read said four
        // lines that were two.
        //
        // Found by running the thing against a real mint, which is the only
        // place it shows: every fixture in this crate's tests supplies one route
        // or the other, never both, so the duplication was invisible to all of
        // them.
        //
        // Deduplicated rather than removing one route. Keeping both is what
        // guarantees an absent fact is always reported -- if `build` ever stops
        // recording a reason, the `None` branch still speaks, and rule 9 says an
        // absence must never pass silently. Order is preserved because it is the
        // order the reader meets the facts in.
        let mut seen = std::collections::BTreeSet::new();
        unknown.retain(|miss| seen.insert(miss.clone()));

        // Last, after every push, so a price fact added anywhere above is
        // caught. Compared on the parsed address, not on text: the mint a
        // stranger typed has already been parsed by the time a dossier exists,
        // and two spellings of one address must not be two tokens here.
        if self_mint == Some(&dossier.mint) {
            withhold_price(&mut facts);
        }

        Self {
            mint: dossier.mint.to_string(),
            read_at: dossier.read_at,
            facts,
            untrusted,
            unknown,
            signals,
        }
    }

    /// Every numeric value a reply may contain.
    ///
    /// Three sources, and the boundary between them is the point:
    ///
    /// 1. **Each fact's declared values**, which carry the honest re-renderings
    ///    a measurement has — 0.251, 25.1 and 25 are one fact said three ways.
    /// 2. **Every numeral in the trusted rendering**, labels included. A label
    ///    says things like "research 0022" and "$20-$200", and those numerals
    ///    were written *by Radar* and shown to the model as true. A model citing
    ///    the band it was given has invented nothing, and a check that caught it
    ///    would reject the most careful replies while passing vaguer ones.
    /// 3. **The slot**, because a reply citing the slot it was read at is doing
    ///    the thing this account exists to do.
    ///
    /// What is **not** a source is [`FactSheet::untrusted`]. That is the whole
    /// boundary: a creator who names their token "99.9% of holders profited"
    /// must not thereby licence 99.9 as a publishable figure. The untrusted
    /// strings are fenced separately and never rendered into this block.
    #[must_use]
    pub fn authorised(&self) -> Vec<f64> {
        let mut values: Vec<f64> = self.facts.iter().flat_map(|f| f.values.clone()).collect();
        values.extend(
            crate::fidelity::literals(&self.render())
                .into_iter()
                .map(|(_, v)| v),
        );
        if let Some(slot) = self.read_at {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a slot is well inside f64's exact integer range and this is a \
                          comparison against a literal the model wrote, not arithmetic"
            )]
            values.push(slot.0 as f64);
        }
        values
    }

    /// The sheet as the model sees it.
    ///
    /// Facts only. The mint, the slot, and the untrusted strings are fenced
    /// separately by [`crate::voice`] so that nothing in this block is
    /// creator-controlled.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for fact in &self.facts {
            let _ = writeln!(out, "{}: {}", fact.label, fact.rendered);
        }
        for miss in &self.unknown {
            let _ = writeln!(out, "NOT KNOWN: {miss}");
        }
        out
    }
}

/// ADR 0013 constraint 5, applied to one sheet.
///
/// Drops every [`About::Price`] fact and says so in the trusted block, so the
/// model is told *why* the figure is absent rather than left to supply one --
/// which it could not do anyway, because a number that is not on the sheet is
/// one the fidelity check refuses. The note carries no digit for that reason:
/// [`FactSheet::authorised`] reads numerals out of the rendered block, and a
/// note citing the ADR by number would authorise that number.
///
/// A function of the facts alone, and separate from [`FactSheet::build`], so
/// it can be tested against a sheet that carries a price fact -- which no
/// dossier produces today.
fn withhold_price(facts: &mut Vec<Fact>) {
    facts.retain(|f| f.about != About::Price);
    facts.push(Fact {
        about: About::Measurement,
        kind: Kind::SelfMintWithheld,
        label: "this token".to_owned(),
        rendered: "the analyst's own. Its price and market capitalisation are never stated, \
                   whoever asks and whatever they are. Say so if it comes up; say nothing \
                   about what it is worth."
            .to_owned(),
        values: Vec::new(),
        // No clause, and this is the strongest case for the empty list being a
        // real mechanism rather than an omission: the note tells the model why
        // a figure is missing, and there is no arrangement of selections that
        // publishes a sentence about the analyst's own price.
        clauses: Vec::new(),
    });
}

/// Radar's own words for a fact it could not read.
///
/// A closed set, so nothing outside this file can put text into the trusted
/// block. An unrecognised fact name gets a generic phrase rather than its raw
/// reason -- the fallback has to be the safe one, because the case it covers is
/// a fact added later by someone who did not read this comment.
fn phrase_for(fact: &str) -> String {
    match fact {
        "launch block" => "the launch block could not be read",
        "curve" => "the bonding curve could not be read",
        "creator history" => "the creator's history could not be read",
        _ => "part of this could not be read",
    }
    .to_owned()
}

fn push_launch(facts: &mut Vec<Fact>, untrusted: &mut Vec<(String, String)>, launch: &LaunchBlock) {
    let recipients = format!("{}", launch.recipients);
    facts.push(
        Fact::exact(
            Kind::LaunchRecipients,
            "distinct token accounts receiving the token in its own launch block \
             (token accounts, NOT owners, NOT people)",
            f64::from(launch.recipients.lower_bound()),
            recipients.clone(),
        )
        // Rule 3 of the old prompt, now unbreakable: the model asked for this
        // clause gets "token accounts" whether or not it remembered the rule,
        // because the noun is not its to choose.
        .saying(
            Voice::Plain,
            format!(
                "The launch block put it into {recipients} token accounts -- accounts, not people."
            ),
        )
        .saying(
            Voice::Blunt,
            format!("It reached {recipients} token accounts at birth. Accounts, not owners."),
        ),
    );
    let transactions = format!("{}", launch.transactions);
    facts.push(
        Fact::exact(
            Kind::LaunchTransactions,
            "transactions in the launch block",
            f64::from(launch.transactions.lower_bound()),
            transactions.clone(),
        )
        .saying(
            Voice::Plain,
            format!("The launch block carried {transactions} transactions."),
        )
        .saying(
            Voice::Blunt,
            format!("One block, {transactions} transactions."),
        ),
    );
    match launch.dev_buy_lamports {
        Some(l) => {
            let sol = format!("{} SOL", render_sol(l));
            facts.push(
                Fact::exact(
                    Kind::DevBuy,
                    "SOL the creator spent buying their own token in the launch block",
                    lamports_as_sol(l),
                    sol.clone(),
                )
                .saying(
                    Voice::Plain,
                    format!("The creator bought {sol} of their own token in the launch block."),
                )
                .saying(Voice::Blunt, format!("The creator's own bid: {sol}.")),
            );
        }
        // Rule 9, and this one is a statement about a person: "did not buy" and
        // "we could not see a buy" are different accusations. The clause says
        // the second, and the model cannot reach for the first, because the only
        // sentence on offer is this one.
        None => facts.push(
            Fact {
                about: About::Measurement,
                kind: Kind::DevBuyUnseen,
                label: "creator's own buy in the launch block".to_owned(),
                rendered: "not found -- absent, NOT zero. Do not say the creator bought nothing."
                    .to_owned(),
                values: Vec::new(),
                clauses: Vec::new(),
            }
            .saying(
                Voice::Plain,
                "No buy by the creator was found in the launch block, which is not the same as none.",
            )
            .saying(
                Voice::Blunt,
                "Radar found no creator buy. Found, not happened.",
            ),
        ),
    }
    untrusted.push(("token name".to_owned(), launch.metadata.name.clone()));
    untrusted.push(("token symbol".to_owned(), launch.metadata.symbol.clone()));
}

/// What this creator's other tokens did.
///
/// # Counts, never a rate
///
/// "Nine of forty-one" and "22%" say the same thing to an arithmetician and
/// different things to a reader: the share hides the denominator, and the
/// denominator is the part that decides whether the number means anything.
/// `creator_track_record` computes rates with a minimum sample and a note
/// explaining itself; this publishes what was counted and lets the reader do
/// the division.
///
/// # Absent is not innocent
///
/// A creator the index has never seen launched before Radar was watching. That
/// is said plainly, because a reply that omitted the line would read as a clean
/// record — rule 9 in the direction that flatters, which is the one that gets
/// somebody hurt.
///
/// # Never presented as a good sign
///
/// Research 0011: graduation predicts **volatility, not profit**. Organic
/// graduations end at a median −3,228 bps against −853 for tokens that never
/// graduate. So the graduation count is published as a measurement and the
/// label never suggests it is encouraging.
fn push_creator(
    facts: &mut Vec<Fact>,
    unknown: &mut Vec<String>,
    creator: &str,
    index: &crate::creator::CreatorIndex,
) {
    let Some(record) = index.get(creator) else {
        // One line, no continuation. A `\` continuation in a Rust string keeps
        // the *leading* whitespace of the next line, so this rendered with a
        // run of fourteen spaces in the middle of a published sentence -- which
        // is the sort of thing that looks like a broken bot rather than a
        // careful one.
        unknown.push(
            "this creator has no record here: Radar has been watching since August, so they launched before that, or have not launched again"
                .to_owned(),
        );
        return;
    };

    let launches = record.launches.to_string();
    facts.push(
        Fact::exact(
            Kind::CreatorLaunches,
            "tokens this creator has launched, in Radar's record",
            f64::from(record.launches),
            launches.clone(),
        )
        // "in Radar's record" is in the sentence and not in the model's memory.
        // The window is the part a reader needs to weigh the count, and it is
        // the part a free-writing model drops first.
        .saying(
            Voice::Plain,
            format!("This creator has launched {launches} tokens in Radar's record."),
        )
        .saying(
            Voice::Blunt,
            format!("{launches} launches on this creator, in Radar's record."),
        ),
    );

    // The denominator, always beside the numerator. A gap between launches and
    // measured means the outcome pass has not caught up -- not that those
    // tokens did nothing -- and a share quoted without it would be a share of
    // an unstated population.
    if record.measured == 0 {
        unknown.push("how those launches turned out: none has been measured yet".to_owned());
        return;
    }
    // Every clause below carries its denominator, because that is the number
    // that decides whether the numerator means anything -- and the denominator
    // is what a sentence written for effect leaves out.
    let measured = record.measured.to_string();
    facts.push(
        Fact::exact(
            Kind::CreatorMeasured,
            "of those, how many have been measured",
            f64::from(record.measured),
            measured.clone(),
        )
        .saying(
            Voice::Plain,
            format!("Of those, {measured} have had an outcome measured."),
        )
        .saying(
            Voice::Blunt,
            format!("{measured} of them have been measured."),
        ),
    );
    let organic = record.organic.to_string();
    facts.push(
        Fact::exact(
            Kind::CreatorOrganic,
            "of the measured, how many reached an AMM by filling over time",
            f64::from(record.organic),
            organic.clone(),
        )
        .saying(
            Voice::Plain,
            format!("{organic} of those {measured} filled a curve over time."),
        )
        .saying(
            Voice::Blunt,
            format!("{organic} of {measured} filled a curve the slow way."),
        ),
    );
    let instant = record.instant.to_string();
    facts.push(
        Fact::exact(
            Kind::CreatorInstant,
            "of the measured, how many filled their curve within three slots (capital committed before the token existed, not demand)",
            f64::from(record.instant),
            instant.clone(),
        )
        .saying(
            Voice::Plain,
            format!(
                "{instant} of those {measured} filled inside three slots, which is capital arranged before the token existed."
            ),
        )
        .saying(
            Voice::Blunt,
            format!("{instant} of {measured} filled inside three slots. That is arrangement, not demand."),
        ),
    );
    let stillborn = record.stillborn.to_string();
    facts.push(
        Fact::exact(
            Kind::CreatorStillborn,
            "of the measured, how many showed almost no activity at all",
            f64::from(record.stillborn),
            stillborn.clone(),
        )
        .saying(
            Voice::Plain,
            format!("{stillborn} of those {measured} showed almost no activity at all."),
        )
        .saying(
            Voice::Blunt,
            format!("{stillborn} of {measured} never moved."),
        ),
    );
}

/// The population as **Radar itself measured it**, from the store.
///
/// # Why this is beside the snapshot rather than instead of it
///
/// `push_population` places one coin's recipient count in a distribution that
/// came from outside: a public RPC walking 45 slots, and a SQL endpoint that
/// truncates at a thousand rows. That distribution is the only one available,
/// because the store did not record a launch-block recipient count until
/// ADR 0012 and only rows written after 2026-09-03 carry one.
///
/// The graduation rates are different. Radar has every succeeded launch it ever
/// recorded and every outcome it ever measured, so it can count them rather than
/// sample them — and the creator-index timer already does, every six hours, in
/// the same pass. On these figures the store is the better instrument, and using
/// the sampled ones when the counted ones are on disk would be a choice to be
/// less accurate.
///
/// # The denominator is stated, always
///
/// Every share here is over `measured`, and `measured` is printed beside them.
/// The gap between what was launched and what was measured is Radar's own
/// backlog, and a share quoted without its denominator invites the reader to
/// treat a lag as a finding.
fn push_measured_population(facts: &mut Vec<Fact>, population: &crate::creator::Population) {
    // Rule 9 in one branch: nothing measured is not a population of zeroes. Say
    // that the figure is missing, in Radar's own words, rather than publishing
    // "0% of launches graduate" off an empty denominator.
    let Some(graduated) = population.graduated_share() else {
        facts.push(Fact {
            about: About::Measurement,
            kind: Kind::VenueUnmeasured,
            label: "how the venue as a whole turns out".to_owned(),
            rendered: "NOT AVAILABLE -- no outcome has been measured yet".to_owned(),
            values: Vec::new(),
            // No clause: an absence is context for choosing what to say, and
            // there is nothing here to publish. A sentence about it would be a
            // sentence about Radar's backlog.
            clauses: Vec::new(),
        });
        return;
    };
    let measured = population.measured.to_string();
    facts.push(Fact::exact(
        Kind::VenueMeasured,
        "launches Radar has recorded and measured, which every share below is out of",
        // Lossless below 2^53; these are counts of launches.
        #[expect(
            clippy::cast_precision_loss,
            reason = "counts of launches; 2^53 is six orders of magnitude away"
        )]
        {
            population.measured as f64
        },
        measured.clone(),
    ));
    // The denominator travels inside every share's sentence rather than beside
    // it, so a clause published alone still says what it is a share of.
    let share = Fact::share(
        Kind::VenueGraduated,
        "of every measured launch, how many graduated at all",
        graduated,
    );
    let rendered = share.rendered.clone();
    facts.push(
        share
            .saying(
                Voice::Plain,
                format!("Across the {measured} launches Radar has measured, {rendered} graduated at all."),
            )
            .saying(
                Voice::Blunt,
                format!("{rendered} of {measured} measured launches ever graduated."),
            ),
    );
    if let Some(organic) = population.organic_share() {
        let f = Fact::share(
            Kind::VenueOrganic,
            "of every measured launch, how many filled their curve over time",
            organic,
        );
        let r = f.rendered.clone();
        facts.push(
            f.saying(
                Voice::Plain,
                format!("Of those {measured} measured launches, {r} filled a curve over time."),
            )
            .saying(
                Voice::Blunt,
                format!("{r} of {measured} filled a curve over time."),
            ),
        );
    }
    if let Some(instant) = population.instant_share() {
        let f = Fact::share(
            Kind::VenueInstant,
            "of every measured launch, how many filled inside their own launch block",
            instant,
        );
        let r = f.rendered.clone();
        facts.push(
            f.saying(
                Voice::Plain,
                format!("Of those {measured} measured launches, {r} filled inside their own launch block."),
            )
            .saying(
                Voice::Blunt,
                format!("{r} of {measured} filled inside their own launch block."),
            ),
        );
    }
    if let Some(stillborn) = population.stillborn_share() {
        let f = Fact::share(
            Kind::VenueStillborn,
            "of every measured launch, how many showed almost no activity at all",
            stillborn,
        );
        let r = f.rendered.clone();
        facts.push(
            f.saying(
                Voice::Plain,
                format!(
                    "Of those {measured} measured launches, {r} showed almost no activity at all."
                ),
            )
            .saying(Voice::Blunt, format!("{r} of {measured} never moved.")),
        );
    }
}

fn push_population(facts: &mut Vec<Fact>, recipients: Count, rates: &BaseRates) {
    // A truncated count must not be looked up in a distribution: the band it
    // lands in would be decided by Radar's call budget rather than by the chain.
    let Some(exact) = recipients.exact() else {
        facts.push(Fact {
            about: About::Measurement,
            kind: Kind::BandUnavailable,
            label: "population context for the recipient count".to_owned(),
            rendered: "NOT AVAILABLE -- the count was cut short, so it cannot be \
                       placed in a distribution"
                .to_owned(),
            values: Vec::new(),
            clauses: Vec::new(),
        });
        return;
    };
    let Some(band) = rates.band_for(exact) else {
        return;
    };
    push_band(facts, exact, band);
    push_base_rates(facts, rates);
}

/// This launch's own band, as a distribution it sits inside.
fn push_band(facts: &mut Vec<Fact>, exact: u32, band: &crate::baserates::Band) {
    // `band.name` is Radar's own label for a range and it contains digits --
    // "10-13 recipients". Those digits are on the sheet because the band's own
    // facts authorise them, and they are inside the clause for the same reason
    // the denominator is: a share of an unnamed population is not a
    // measurement, it is a mood.
    let name = &band.name;
    let f = Fact::share(
        Kind::BandNeverGraduated,
        format!(
            "share of launches that NEVER graduated whose block had {exact} recipients ({name})"
        ),
        band.never_graduated,
    );
    let r = f.rendered.clone();
    facts.push(
        f.saying(
            Voice::Plain,
            format!("Among launches in the {name} band, {r} never graduated at all."),
        )
        .saying(
            Voice::Blunt,
            format!("{r} of the {name} band never graduated."),
        ),
    );
    let f = Fact::share(
        Kind::BandOrganic,
        format!("share of ORGANIC graduations in that band ({name})"),
        band.organic,
    );
    let r = f.rendered.clone();
    facts.push(
        f.saying(
            Voice::Plain,
            format!("Among launches in the {name} band, {r} filled a curve over time."),
        )
        .saying(
            Voice::Blunt,
            format!("{r} of the {name} band filled a curve over time."),
        ),
    );
    let f = Fact::share(
        Kind::BandInstant,
        format!("share of INSTANT graduations in that band ({name})"),
        band.instant,
    );
    let r = f.rendered.clone();
    facts.push(
        f.saying(
            Voice::Plain,
            format!("Among launches in the {name} band, {r} graduated instantly."),
        )
        .saying(
            Voice::Blunt,
            format!("{r} of the {name} band graduated instantly."),
        ),
    );
    let f = Fact::share(
        Kind::BandInstantProbability,
        format!("probability a launch in that band ({name}) graduates instantly"),
        band.p_instant,
    );
    let r = f.rendered.clone();
    facts.push(
        f.saying(
            Voice::Plain,
            format!("A launch in the {name} band graduates instantly {r} of the time."),
        )
        .saying(
            Voice::Blunt,
            format!("Instant graduation in the {name} band: {r}."),
        ),
    );
    // Research 0024 is the record of this multiple moving off six recipients. A
    // clause states it as a dated comparison against the population rate rather
    // than as a property of the band, because it is the ratio of two measured
    // shares and the denominator is the one that moves.
    let times = format!("{:.1}x", band.x_base_instant);
    facts.push(
        Fact::exact(
            Kind::BandTimesBaseRate,
            format!("how many times the base rate that is ({name} band)"),
            band.x_base_instant,
            times.clone(),
        )
        .saying(
            Voice::Plain,
            format!("That is {times} the rate across every launch Radar has measured."),
        )
        .saying(Voice::Blunt, format!("{times} the rate of the field.")),
    );
}

/// The whole field, which is what makes a band figure mean anything.
fn push_base_rates(facts: &mut Vec<Fact>, rates: &BaseRates) {
    let f = Fact::share(
        Kind::BaseInstant,
        "population rate: share of all launches that graduate instantly",
        rates.base_rate_instant,
    );
    let r = f.rendered.clone();
    facts.push(
        f.saying(
            Voice::Plain,
            format!("Across every launch in the snapshot, {r} graduated instantly."),
        )
        .saying(
            Voice::Blunt,
            format!("The whole field graduates instantly {r} of the time."),
        ),
    );
    let f = Fact::share(
        Kind::BaseGraduates,
        "population rate: share of all launches that graduate at all",
        rates.base_rate_graduates,
    );
    let r = f.rendered.clone();
    facts.push(
        f.saying(
            Voice::Plain,
            format!("Across every launch in the snapshot, {r} graduated at all."),
        )
        .saying(
            Voice::Blunt,
            format!("{r} of the whole field ever graduates."),
        ),
    );
}

fn push_curve(facts: &mut Vec<Fact>, curve: &radar_onchain::CurveFacts) {
    facts.push(
        Fact {
            about: About::Measurement,
            kind: Kind::Graduated,
            label: "has the token graduated off the bonding curve".to_owned(),
            rendered: if curve.complete { "yes" } else { "no" }.to_owned(),
            values: Vec::new(),
            clauses: Vec::new(),
        }
        .saying(
            Voice::Plain,
            if curve.complete {
                "It has left the bonding curve and trades on an AMM."
            } else {
                "It is still on its bonding curve."
            },
        )
        .saying(
            Voice::Blunt,
            if curve.complete {
                "Off the curve, on an AMM."
            } else {
                "Still on the curve."
            },
        ),
    );
    // A graduated coin has an empty curve *because it left*, and the two
    // remaining curve facts are both false about it: the capacity is not zero,
    // it is elsewhere, and the curve's fee schedule is not the fee the coin
    // pays. "cannot size into this at all" about a coin trading on an AMM is
    // rule 9 read backwards -- absent taken for zero -- and it is the worst
    // line the sheet could carry, because a graduated coin is exactly the kind
    // of coin people ask the bot about.
    if curve.complete {
        facts.push(
            Fact {
                about: About::Measurement,
                kind: Kind::CapacityAfterGraduation,
                label: "exit capacity".to_owned(),
                rendered:
                    "graduated off the curve; it trades on the AMM, which Radar does not price. \
                     NOT zero, and NOT 'cannot size into this'."
                        .to_owned(),
                values: Vec::new(),
                clauses: Vec::new(),
            }
            .saying(
                Voice::Plain,
                "Radar does not price the AMM it moved to, so it has no exit size for this one.",
            )
            .saying(Voice::Blunt, "Radar cannot size the AMM it moved to."),
        );
        return;
    }
    match curve.capacity_lamports {
        Some(l) => {
            let sol = format!("{} SOL", render_sol(l));
            facts.push(
                Fact::exact(
                    Kind::Capacity,
                    "SOL that can be bought before price moves 1% -- this is RADAR'S OWN \
                     impact budget, NOT a ceiling the venue imposes (research 0022)",
                    lamports_as_sol(l),
                    sol.clone(),
                )
                // Research 0022 reversed the capacity claim: this is Radar's own
                // impact budget, not a ceiling the venue imposes. The clause
                // says whose budget it is, in the sentence, because the earlier
                // wording was read as the venue's limit and that reading was
                // wrong for a year.
                .saying(
                    Voice::Plain,
                    format!("{sol} can be bought before the price moves one percent, on Radar's own impact budget."),
                )
                .saying(
                    Voice::Blunt,
                    format!("{sol} before the price moves one percent, by Radar's budget."),
                ),
            );
        }
        None => facts.push(
            Fact {
                about: About::Measurement,
                kind: Kind::CapacityNone,
                label: "exit capacity".to_owned(),
                rendered: "none -- cannot size into this at all. NOT 'no limit found'.".to_owned(),
                values: Vec::new(),
                clauses: Vec::new(),
            }
            .saying(
                Voice::Plain,
                "No size at all clears Radar's impact budget here.",
            )
            .saying(
                Voice::Blunt,
                "Nothing fits inside Radar's impact budget here.",
            ),
        ),
    }
    push_fee(facts, curve);
}

/// The venue's own fee, which is not the cost of trading and says so.
fn push_fee(facts: &mut Vec<Fact>, curve: &radar_onchain::CurveFacts) {
    if let Some(fees) = &curve.fees {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a basis-point figure is a small integer; this is a comparison value"
        )]
        let rt = fees.round_trip_bps() as f64;
        facts.push(
            Fact {
                about: About::Measurement,
                kind: Kind::VenueFee,
                label: "venue fee, round trip, read from the on-chain schedule".to_owned(),
                rendered: format!(
                    "{rt} bps -- THE VENUE FEE ONLY. The measured all-in round trip is 850 bps. \
                     Never present the fee as the cost of trading."
                ),
                values: vec![rt, rt / 100.0],
                clauses: Vec::new(),
            }
            // "the venue's fee" and "the cost of trading" are different claims
            // and the gap between them is most of the cost. The qualifier is in
            // the sentence rather than in a rule the model is asked to hold.
            .saying(
                Voice::Plain,
                format!("The venue's own round-trip fee, from its on-chain schedule, is {rt} bps, which is not the cost of trading it."),
            )
            .saying(
                Voice::Blunt,
                format!("Venue fee alone: {rt} bps round trip. That is the floor, not the bill."),
            ),
        );
    }
}

fn push_cost(facts: &mut Vec<Fact>, rates: &BaseRates) {
    let kernel = format!("{} bps", rates.round_trip_kernel);
    facts.push(
        Fact::exact(
            Kind::RoundTripKernel,
            "measured all-in round trip Radar's kernel assumes, on fresh launches",
            rates.round_trip_kernel,
            kernel.clone(),
        )
        .saying(
            Voice::Plain,
            format!("A round trip on a fresh launch costs {kernel} all in, as Radar's kernel measures it."),
        )
        .saying(Voice::Blunt, format!("{kernel} to get in and out, all in.")),
    );
    let bar = format!("{} bps", rates.round_trip_bar);
    facts.push(
        Fact::exact(
            Kind::RoundTripBar,
            "expected edge a strategy must clear before one trade is worth making",
            rates.round_trip_bar,
            bar.clone(),
        )
        .saying(
            Voice::Plain,
            format!(
                "A strategy has to clear {bar} of expected edge before one trade is worth making."
            ),
        )
        .saying(
            Voice::Blunt,
            format!("Clear {bar} of edge or do not trade."),
        ),
    );
    for band in &rates.cost_bands {
        let size = &band.band;
        let rendered = format!("{} bps ({:.1}%)", band.round_trip, band.round_trip / 100.0);
        facts.push(
            Fact {
                about: About::Measurement,
                kind: Kind::CostBand,
                label: format!("round trip for a position of {size}"),
                rendered: rendered.clone(),
                values: vec![band.round_trip, band.round_trip / 100.0],
                clauses: Vec::new(),
            }
            .saying(
                Voice::Plain,
                format!("A position of {size} pays {rendered} to go round."),
            )
            .saying(Voice::Blunt, format!("{size} costs {rendered} round trip.")),
        );
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "lamports are converted only for a comparison against a literal the model \
              wrote; the rendered figure comes from integer arithmetic in `render_sol`"
)]
fn lamports_as_sol(lamports: u64) -> f64 {
    lamports as f64 / LAMPORTS_PER_SOL as f64
}

/// Renders lamports as SOL by integer arithmetic.
///
/// `radar-types` keeps money integral on purpose, and a printed figure that has
/// silently rounded through a float is exactly what this account must not
/// publish.
fn render_sol(lamports: u64) -> String {
    format!(
        "{}.{:04}",
        lamports / LAMPORTS_PER_SOL,
        (lamports % LAMPORTS_PER_SOL) / 100_000
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_precision_switch_is_exactly_where_0024_needs_it() {
        // The rule is `> 0.0 && < 0.1` renders two decimals, everything else
        // one. Both comparisons are one character from being wrong and every
        // mutation of them survived, because nothing tested either edge.
        //
        // What is at stake is 0024's strongest finding: one to three recipients
        // graduate instantly 0.02% of the time. At one decimal that is "0.0%",
        // which reads as *never* rather than *rare* -- wrong in the direction
        // that gets quoted back at you.
        assert_eq!(
            Fact::share(Kind::LaunchRecipients, "x", 0.000_2).rendered,
            "0.02%"
        );
        assert_eq!(
            Fact::share(Kind::LaunchRecipients, "x", 0.000_5).rendered,
            "0.05%"
        );

        // Zero is not "0.00%". It is genuinely zero, and the two-decimal form is
        // for small-but-real, so the lower bound is exclusive.
        assert_eq!(
            Fact::share(Kind::LaunchRecipients, "x", 0.0).rendered,
            "0.0%"
        );

        // And a tenth of a percent is the upper bound, also exclusive: 0.1% has
        // no hidden precision to show.
        assert_eq!(
            Fact::share(Kind::LaunchRecipients, "x", 0.001).rendered,
            "0.1%"
        );

        // Ordinary magnitudes are unaffected.
        assert_eq!(
            Fact::share(Kind::LaunchRecipients, "x", 0.251).rendered,
            "25.1%"
        );
    }

    #[test]
    fn every_unreadable_fact_names_which_fact_it_was() {
        // Each arm is a different sentence in a published reply. Deleting any of
        // them falls through to the generic phrase, which is safe but says less,
        // and nothing noticed -- three arms, three survivors.
        assert_eq!(
            phrase_for("launch block"),
            "the launch block could not be read"
        );
        assert_eq!(phrase_for("curve"), "the bonding curve could not be read");
        assert_eq!(
            phrase_for("creator history"),
            "the creator's history could not be read"
        );
        // The fallback is deliberately the safe one, for a fact added later by
        // someone who did not read the comment above it.
        assert_eq!(
            phrase_for("something new"),
            "part of this could not be read"
        );
    }

    #[test]
    fn lamports_convert_to_sol_at_the_documented_rate() {
        // Used only to compare against a figure a model wrote, which is why it
        // is a float at all -- but a wrong conversion there authorises a wrong
        // number in a public reply.
        assert!((lamports_as_sol(LAMPORTS_PER_SOL) - 1.0).abs() < 1e-9);
        assert!((lamports_as_sol(LAMPORTS_PER_SOL / 2) - 0.5).abs() < 1e-9);
        assert!((lamports_as_sol(3 * LAMPORTS_PER_SOL) - 3.0).abs() < 1e-9);
        assert!((lamports_as_sol(0) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn a_share_authorises_its_ordinary_roundings_and_nothing_else() {
        let f = Fact::share(Kind::LaunchRecipients, "x", 0.251);
        assert!(f.values.iter().any(|v| (*v - 0.251).abs() < 1e-9));
        assert!(f.values.iter().any(|v| (*v - 25.1).abs() < 1e-9));
        assert!(f.values.iter().any(|v| (*v - 25.0).abs() < 1e-9));
        // Not a number a reader would call a rounding of 25.1%.
        assert!(!f.values.iter().any(|v| (*v - 68.0).abs() < 1e-9));
        assert_eq!(f.rendered, "25.1%");
    }

    #[test]
    fn sol_renders_by_integer_arithmetic() {
        assert_eq!(render_sol(1_000_000_000), "1.0000");
        assert_eq!(render_sol(303_000_000), "0.3030");
        assert_eq!(render_sol(0), "0.0000");
    }

    #[test]
    fn an_untrusted_name_is_never_a_fact() {
        // The separation this type exists for: a number inside a token's *name*
        // must authorise nothing, or a creator could licence their own figures
        // by putting them in the name.
        let sheet = FactSheet {
            mint: "M".to_owned(),
            read_at: None,
            facts: Vec::new(),
            untrusted: vec![("token name".to_owned(), "99999 percent safe".to_owned())],
            unknown: Vec::new(),
            signals: Vec::new(),
        };
        assert!(sheet.authorised().is_empty());
        assert!(!sheet.render().contains("99999"));
    }

    /// A market-cap fact, which nothing builds today and which is exactly what
    /// the rule exists to catch when something does.
    fn a_price_fact() -> Fact {
        Fact {
            about: About::Price,
            kind: Kind::SelfMintWithheld,
            clauses: Vec::new(),
            label: "market capitalisation when read".to_owned(),
            rendered: "69000 USD".to_owned(),
            values: vec![69_000.0, 69.0],
        }
    }

    #[test]
    fn a_price_fact_is_withheld_and_the_sheet_says_why() {
        // ADR 0013 constraint 5. The figure must leave the authorised set, not
        // merely the rendering: a price the model may not see but may still
        // cite is a price the fidelity check would let through.
        //
        // Re-apply the bug by deleting the `retain` in `withhold_price`: the
        // 69000 stays authorised and this fails on the first assertion.
        let mut facts = vec![
            Fact::exact(Kind::LaunchRecipients, "recipients", 6.0, "6"),
            a_price_fact(),
        ];
        withhold_price(&mut facts);
        let sheet = FactSheet {
            mint: "M".to_owned(),
            read_at: None,
            facts,
            untrusted: Vec::new(),
            unknown: Vec::new(),
            signals: Vec::new(),
        };

        let authorised = sheet.authorised();
        assert!(
            !authorised.iter().any(|v| (*v - 69_000.0).abs() < 1e-9),
            "the market cap survived withholding: {authorised:?}"
        );
        assert!(
            !authorised.iter().any(|v| (*v - 69.0).abs() < 1e-9),
            "a rendering of the market cap survived: {authorised:?}"
        );
        assert!(
            sheet.facts.iter().all(|f| f.about != About::Price),
            "a price fact is still on the sheet: {:?}",
            sheet.facts
        );

        // Withholding must not become silence, and must not cost the answer.
        // Rule 9: an absence that goes unmentioned reads as reassurance, or
        // here as coyness -- the model is told why the figure is not there.
        let rendered = sheet.render();
        assert!(rendered.contains("never stated"), "{rendered}");
        assert!(
            authorised.iter().any(|v| (*v - 6.0).abs() < 1e-9),
            "the measured fact was lost with the price: {authorised:?}"
        );

        // And the note itself authorises nothing. `authorised` reads numerals
        // out of the rendered block, so a note that cited the ADR by number
        // would licence that number. Only the recipient count remains.
        assert!(
            authorised.iter().all(|v| (*v - 6.0).abs() < 1e-9),
            "the note put a number into the authorised set: {authorised:?}"
        );
    }

    #[test]
    fn a_graduated_coin_still_gets_its_creators_history_and_says_where_it_trades() {
        // The shape of every coin worth asking about: enough signatures that
        // `oldest_launch` refuses to guess a launch block, and a curve that has
        // graduated. Until 2026-09-06 this produced the worst sheet in the
        // system -- no creator history at all, because the lookup sat inside
        // the launch-block arm, and "exit capacity: none, cannot size into this
        // at all" about a coin trading perfectly well on an AMM.
        //
        // Re-apply either half to see this fail: move the `creator` lookup back
        // inside `if let Some(launch)`, or delete `push_curve`'s early return.
        let mut d = dossier_for([7u8; 32]);
        d.launch = None;
        d.curve = Some(radar_onchain::CurveFacts {
            creator: radar_types::Address::new([9u8; 32]),
            complete: true,
            real_sol_reserves: 0,
            capacity_lamports: None,
            fees: None,
        });
        d.unavailable.push(radar_onchain::dossier::Unavailable {
            fact: "launch block",
            why: "this token has more history than the page budget allows".to_owned(),
        });

        let sheet = FactSheet::build(&d, None, Some(&index_with(record(150, 0))), None);
        let rendered = sheet.render();

        // The creator's record survived the missing launch block.
        assert!(rendered.contains("150"), "no creator history: {rendered}");
        assert!(
            sheet
                .signals
                .contains(&Signal::CreatorNeverGraduatedOrganically),
            "the signal was lost with the launch block: {:?}",
            sheet.signals
        );
        // And the curve says where the coin went rather than that it is stuck.
        assert!(rendered.contains("trades on the AMM"), "{rendered}");
        // The claim, not the word: the replacement line names the old phrasing
        // in order to forbid it, so a bare substring would match itself.
        assert!(
            !rendered.contains("none -- cannot size into this at all"),
            "{rendered}"
        );
        assert!(!rendered.contains("venue fee"), "{rendered}");
    }

    /// A dossier about one mint and nothing else, so what the sheet says is
    /// decided by the mint alone.
    fn dossier_for(mint: [u8; 32]) -> Dossier {
        Dossier {
            mint: radar_types::Address::new(mint),
            read_at: None,
            launch: None,
            curve: None,
            creator_transactions: None,
            unavailable: Vec::new(),
            calls: 0,
            elapsed_ms: 0,
        }
    }

    /// A launch block with the three things the signals read.
    fn launch(
        recipients: radar_onchain::budget::Count,
        dev_buy_lamports: Option<u64>,
    ) -> radar_onchain::launch::LaunchBlock {
        radar_onchain::launch::LaunchBlock {
            slot: Slot(444_007_820),
            creator: radar_types::Address::new([9u8; 32]),
            recipients,
            transactions: radar_onchain::budget::Count::Exactly(4),
            dev_buy_lamports,
            metadata: radar_onchain::launch::Metadata {
                name: "x".to_owned(),
                symbol: "X".to_owned(),
                uri: String::new(),
            },
        }
    }

    /// A snapshot whose strongest band is `lo..=hi`, beside a weaker one at
    /// one to three so that "strongest" is a comparison and not the only row.
    fn rates_strongest(lo: u32, hi: u32) -> BaseRates {
        let band = |name: &str, lo, hi, x| crate::baserates::Band {
            name: name.to_owned(),
            lo,
            hi,
            fires_on: 0.0,
            never_graduated: 0.0,
            organic: 0.0,
            instant: 0.0,
            p_instant: 0.0,
            x_base_instant: x,
        };
        BaseRates {
            measured_on: "2026-09-03".to_owned(),
            aftermath: None,
            launches: 1,
            base_rate_graduates: 0.0,
            base_rate_instant: 0.0,
            bands: vec![
                band("one to three", 1, 3, 0.0),
                band("strong", lo, hi, 10.1),
            ],
            round_trip_kernel: 0.0,
            round_trip_bar: 0.0,
            cost_bands: Vec::new(),
        }
    }

    fn index_with(record: crate::creator::Record) -> crate::creator::CreatorIndex {
        let mut creators = std::collections::BTreeMap::new();
        creators.insert(radar_types::Address::new([9u8; 32]).to_string(), record);
        crate::creator::CreatorIndex {
            watermark_slot: 444_343_109,
            built_at: 1_788_000_000,
            population: None,
            creators,
        }
    }

    fn record(measured: u32, organic: u32) -> crate::creator::Record {
        crate::creator::Record {
            launches: measured + 1,
            measured,
            organic,
            instant: measured.saturating_sub(organic),
            stillborn: 0,
        }
    }

    #[test]
    fn the_three_signals_fire_together_in_a_fixed_order_and_the_model_sees_none_of_them() {
        // Design 0009 M3. Twelve recipients in a snapshot whose strongest band
        // is ten to thirteen; a dev buy that was seen; a creator with three
        // measured launches and no organic graduation.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(
            radar_onchain::budget::Count::Exactly(12),
            Some(30_000_000),
        ));
        let rates = rates_strongest(10, 13);
        let index = index_with(record(3, 0));
        let sheet = FactSheet::build(&dossier, Some(&rates), Some(&index), None);
        assert_eq!(
            sheet.signals,
            [
                Signal::LaunchBlockInStrongestBand,
                Signal::CreatorBoughtOwnLaunch,
                Signal::CreatorNeverGraduatedOrganically,
            ]
        );
        // Not rendered. The model is shown facts, and the word would be a
        // verdict handed to it.
        let rendered = sheet.render();
        assert!(!rendered.to_lowercase().contains("signal"), "{rendered}");
    }

    #[test]
    fn above_the_strongest_band_counts_and_below_it_does_not() {
        // "Ten to thirteen or above": 14 is above and fires; 9 is below and
        // does not; 13 and 10 are the edges. Re-applied `>=` as `>`: 10 stops
        // firing and this fails.
        let rates = rates_strongest(10, 13);
        for (recipients, fires) in [(9, false), (10, true), (13, true), (14, true), (40, true)] {
            let mut dossier = dossier_for([3u8; 32]);
            dossier.launch = Some(launch(
                radar_onchain::budget::Count::Exactly(recipients),
                None,
            ));
            let sheet = FactSheet::build(&dossier, Some(&rates), None, None);
            assert_eq!(
                sheet.signals.contains(&Signal::LaunchBlockInStrongestBand),
                fires,
                "{recipients} recipients"
            );
        }
    }

    #[test]
    fn the_strongest_band_is_read_from_the_snapshot_and_moves_with_it() {
        // Research 0024's lesson as a test: the same twelve recipients fire
        // when the snapshot's strongest band is ten to thirteen and do not
        // when it is exactly six -- and six then does.
        let mut twelve = dossier_for([3u8; 32]);
        twelve.launch = Some(launch(radar_onchain::budget::Count::Exactly(12), None));
        let mut six = dossier_for([4u8; 32]);
        six.launch = Some(launch(radar_onchain::budget::Count::Exactly(6), None));

        let at_six = rates_strongest(6, 6);
        assert!(
            FactSheet::build(&twelve, Some(&at_six), None, None)
                .signals
                .contains(&Signal::LaunchBlockInStrongestBand),
            "twelve is above six and fires: 'or above' is the rule"
        );
        assert!(
            FactSheet::build(&six, Some(&at_six), None, None)
                .signals
                .contains(&Signal::LaunchBlockInStrongestBand)
        );
        let at_ten = rates_strongest(10, 13);
        assert!(
            !FactSheet::build(&six, Some(&at_ten), None, None)
                .signals
                .contains(&Signal::LaunchBlockInStrongestBand),
            "six is below ten and does not fire once the band has moved"
        );
    }

    #[test]
    fn what_could_not_be_read_is_no_signal_rather_than_a_signal_of_zero() {
        // Rule 9, three ways. A truncated recipient count was decided by the
        // call budget; a creator with nothing measured has no record to hold
        // against them; a dev buy that was not seen is not a dev buy of zero.
        // And no snapshot means no band to be strongest.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(radar_onchain::budget::Count::AtLeast(40), None));
        let rates = rates_strongest(10, 13);
        let unmeasured = index_with(record(0, 0));
        let sheet = FactSheet::build(&dossier, Some(&rates), Some(&unmeasured), None);
        assert_eq!(sheet.signals, []);

        // A dev buy of exactly zero lamports, if a chain ever reported one, is
        // a buy that was seen and was nothing -- also no signal.
        let mut zero = dossier_for([3u8; 32]);
        zero.launch = Some(launch(radar_onchain::budget::Count::Exactly(12), Some(0)));
        assert_eq!(
            FactSheet::build(&zero, None, None, None).signals,
            [],
            "no snapshot, no band; a zero buy is not a buy"
        );

        // A creator with a measured organic graduation is not "never".
        let mut organic = dossier_for([3u8; 32]);
        organic.launch = Some(launch(radar_onchain::budget::Count::Exactly(2), None));
        let graduated = index_with(record(3, 1));
        assert_eq!(
            FactSheet::build(&organic, Some(&rates), Some(&graduated), None).signals,
            []
        );

        // No launch block at all: nothing to count.
        assert_eq!(
            FactSheet::build(
                &dossier_for([5u8; 32]),
                Some(&rates),
                Some(&graduated),
                None
            )
            .signals,
            []
        );
    }

    #[test]
    fn the_curve_facts_reach_the_sheet() {
        // `push_curve` replaced with nothing survived mutation testing on
        // 2026-09-05: no test asserted that a curve the dossier read appears on
        // the sheet. A sheet that silently says less is LEARNINGS 5 in a
        // published reply -- an absence that reads as fine.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.curve = Some(radar_onchain::CurveFacts {
            creator: radar_types::Address::new([9u8; 32]),
            complete: false,
            real_sol_reserves: 6_186_150_833,
            capacity_lamports: Some(303_000_000),
            fees: None,
        });
        let rendered = FactSheet::build(&dossier, None, None, None).render();
        assert!(
            rendered.contains("has the token graduated off the bonding curve: no"),
            "{rendered}"
        );
        assert!(rendered.contains("0.3030 SOL"), "{rendered}");

        // Graduated, and no depth at all: both are statements, not blanks.
        let mut done = dossier_for([3u8; 32]);
        done.curve = Some(radar_onchain::CurveFacts {
            creator: radar_types::Address::new([9u8; 32]),
            complete: true,
            real_sol_reserves: 0,
            capacity_lamports: None,
            fees: None,
        });
        let rendered = FactSheet::build(&done, None, None, None).render();
        assert!(
            rendered.contains("has the token graduated off the bonding curve: yes"),
            "{rendered}"
        );
        assert!(rendered.contains("cannot size into this"), "{rendered}");
    }

    const SNAPSHOT: &str = include_str!("../../../docs/research/data/0024-base-rates.json");

    #[test]
    fn the_cost_facts_reach_the_sheet() {
        // `push_cost` replaced with nothing survived the same run. The cost line
        // is the fact GOAL.md says leads every reply, and nothing pinned that it
        // was there at all.
        let rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        let sheet = FactSheet::build(&dossier_for([3u8; 32]), Some(&rates), None, None);
        let rendered = sheet.render();
        assert!(
            rendered.contains(
                "expected edge a strategy must clear before one trade is worth making: 456 bps"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("round trip Radar's kernel assumes, on fresh launches: 850 bps"),
            "{rendered}"
        );
        assert!(
            rendered.contains("round trip for a position of $20-$200: 456 bps (4.6%)"),
            "{rendered}"
        );
        // Authorised in both renderings, so a reply quoting 4.6% is not refused
        // as a fabrication of a figure the sheet stated.
        let authorised = sheet.authorised();
        assert!(authorised.iter().any(|v| (*v - 456.0).abs() < 1e-9));
        assert!(authorised.iter().any(|v| (*v - 4.56).abs() < 1e-9));
    }

    #[test]
    fn the_measured_population_reaches_the_sheet() {
        // `push_measured_population` replaced with nothing survived the same
        // run. This is the denominator every creator count is read against;
        // without it "none of 150 filled its curve" has no scale.
        let index = crate::creator::CreatorIndex {
            watermark_slot: 444_374_676,
            built_at: 1_788_000_000,
            population: Some(crate::creator::Population {
                launches: 508_814,
                measured: 506_991,
                organic: 9_060,
                instant: 5_222,
                stillborn: 116_608,
            }),
            creators: std::collections::BTreeMap::new(),
        };
        let rendered = FactSheet::build(&dossier_for([3u8; 32]), None, Some(&index), None).render();
        assert!(
            rendered.contains(
                "launches Radar has recorded and measured, which every share below is out of: 506991"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "of every measured launch, how many showed almost no activity at all: 23.0%"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("of every measured launch, how many graduated at all: "),
            "{rendered}"
        );

        // Nothing measured is not a population of zeroes -- rule 9. The figure
        // is said to be missing, in Radar's words, rather than published as 0%.
        let empty = crate::creator::CreatorIndex {
            population: Some(crate::creator::Population::default()),
            ..index
        };
        let rendered = FactSheet::build(&dossier_for([3u8; 32]), None, Some(&empty), None).render();
        assert!(
            rendered.contains("NOT AVAILABLE -- no outcome has been measured yet"),
            "{rendered}"
        );
        assert!(!rendered.contains("0.0%"), "{rendered}");
    }

    #[test]
    fn the_rule_applies_to_the_configured_mint_and_to_no_other() {
        // Three sheets, one dossier. The comparison in `build` is the whole of
        // "is this the analyst's own token", and it is one character from
        // applying to every coin or to none.
        let own = radar_types::Address::new([3u8; 32]);
        let other = radar_types::Address::new([4u8; 32]);
        let dossier = dossier_for([3u8; 32]);

        let withheld = FactSheet::build(&dossier, None, None, Some(&own));
        assert!(
            withheld.render().contains("never stated"),
            "the configured mint must be told apart: {}",
            withheld.render()
        );

        // Another coin is answered like any other, with no mention of the rule.
        // A note on every sheet would make every reply about the analyst's own
        // token, which is the opposite of constraint 6.
        let stranger = FactSheet::build(&dossier, None, None, Some(&other));
        assert!(
            !stranger.render().contains("never stated"),
            "{}",
            stranger.render()
        );

        // No token configured: no token is special. Rule 8 is not touched --
        // absence means the rule has nothing to apply to, not that a default
        // mint is assumed.
        let unconfigured = FactSheet::build(&dossier, None, None, None);
        assert!(
            !unconfigured.render().contains("never stated"),
            "{}",
            unconfigured.render()
        );
    }
}
