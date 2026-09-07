// SPDX-License-Identifier: Apache-2.0
//! Fact slots: the model chooses facts, and never writes a number.
//!
//! # Why this replaces a scanner
//!
//! [`crate::fidelity`] checks that every numeral in a generated reply appears on
//! the fact sheet. That is a real defence and it has a hole its own
//! documentation describes: a literal passes if **any** authorised value rounds
//! to it at the literal's own precision. `Fact::share` authorises the rounded
//! percentage as well as the ratio, and `FactSheet::authorised` harvests every
//! numeral out of the rendered block — so with fifteen figures on a sheet, most
//! small integers are authorised by *something*.
//!
//! Measured on a real sheet: "3 launches by this creator" passes off a 2.9%
//! population share, and "22.5% of this creator's launches showed no activity"
//! passes because 22.5 is on the sheet as a figure about the population. Both
//! are fabrications about *this coin* assembled out of numbers about something
//! else, and both are exactly the kind of sentence that gets screenshotted.
//! Number **words** are not scanned at all.
//!
//! # What this does instead
//!
//! The model is shown the sheet with each fact numbered, and asked to write
//! prose containing `[F1]`-style tags and **no digits**. Radar substitutes its
//! own rendered string for each tag. The model still decides which facts to use,
//! in what order, with what framing and how sharply — everything that makes a
//! reply worth reading — and it cannot introduce a number, because the only
//! digits in the finished text are ones this crate wrote.
//!
//! That is AGENTS.md §5's ladder: **make it impossible** rather than check for
//! it. [`crate::fidelity`] stays as the second lock, and after substitution it
//! passes by construction — which is the point. A check that can only fail if
//! something upstream is broken is worth keeping precisely because it is silent.
//!
//! # The rules a substitution has to hold
//!
//! 1. **One pass, left to right.** A substituted value is never re-scanned, so a
//!    fact whose rendering happens to contain `[F2]` cannot expand again.
//! 2. **A digit outside a tag rejects the whole reply.** Not stripped: a model
//!    that wrote a number was doing something this design does not permit, and
//!    silently deleting it would leave a sentence missing its subject.
//! 3. **An unknown tag rejects the whole reply.** `[F9]` on a sheet of four
//!    facts is a fact the model believes it was given and was not.
//!
//! All three ship the deterministic template, which is what the account has been
//! posting anyway and is never wrong.

use crate::sheet::FactSheet;

/// Why a tagged reply could not be turned into text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotSubstituted {
    /// The model wrote a digit outside a tag.
    Digits {
        /// The offending run, as written.
        found: String,
    },
    /// The model used a tag that names no fact.
    UnknownTag {
        /// The tag, as written.
        tag: String,
    },
    /// Nothing was left after substitution.
    Empty,
}

impl core::fmt::Display for NotSubstituted {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Digits { found } => write!(f, "wrote the digits {found} outside a tag"),
            Self::UnknownTag { tag } => write!(f, "used {tag}, which names no fact"),
            Self::Empty => write!(f, "substituted to nothing"),
        }
    }
}

/// The tag standing for the whole offered headline.
///
/// Separate from the numbered facts because it is a *sentence* Radar already
/// built and checked, not a value. Without it the headline could not be offered
/// at all: it contains digits, so a model that copied it would be rejected by
/// the digit rule for using the one thing the prompt handed it.
pub const HEADLINE_TAG: &str = "[H]";

/// The sheet as the model sees it, with every fact numbered.
///
/// One-based, because the tags are for a reader rather than an index: `[F0]`
/// reads as a mistake to everybody who is not a programmer, and the model is
/// not one.
#[must_use]
pub fn render_tagged(sheet: &FactSheet) -> String {
    use core::fmt::Write as _;
    let mut out = String::new();
    for (n, fact) in sheet.facts.iter().enumerate() {
        let _ = writeln!(out, "[F{}] {}: {}", n + 1, fact.label, fact.rendered);
    }
    for miss in &sheet.unknown {
        // Deliberately untagged. An unknown has no rendering to substitute, and
        // a tag that expanded to nothing would let the model write a sentence
        // whose subject silently vanished. The model is told to say the absence
        // in its own words, which carry no digits and need none.
        let _ = writeln!(out, "NOT KNOWN (no tag; say it in words): {miss}");
    }
    out
}

/// Replaces every tag with Radar's own rendering of that fact.
///
/// # Errors
///
/// [`NotSubstituted`] when the model wrote a digit outside a tag, used a tag
/// that names no fact, or produced nothing.
pub fn substitute(
    written: &str,
    sheet: &FactSheet,
    headline: Option<&str>,
) -> Result<String, NotSubstituted> {
    // # Why this is a `for` and not a `while` over a cursor
    //
    // It was a cursor, and the mutation gate reported **five timeouts** across
    // three shards. Every one of them was a mutant that stopped the cursor
    // advancing, and cargo-mutants cannot tell an infinite loop from a slow one
    // — the justfile's own note says a timeout is `inconclusive`, never a pass,
    // and shard 3 failed on a timeout with no survivors at all.
    //
    // A `for` over a fixed iterator cannot loop forever whatever a mutant does
    // to the body. `consumed` is the tag's remaining characters, skipped rather
    // than jumped over: mutated to add instead of subtract it skips more, which
    // is wrong and terminates and is therefore *catchable*. That is AGENTS.md
    // §5's ladder applied to a check rather than to a behaviour — the hang was
    // made impossible instead of tested for.
    let chars: Vec<char> = written.chars().collect();
    let mut out = String::new();
    let mut consumed = 0usize;

    for (i, &c) in chars.iter().enumerate() {
        if consumed > 0 {
            consumed -= 1;
            continue;
        }
        if c == '[' {
            if let Some((tag, after)) = read_tag(&chars, i) {
                // Resolved against the sheet, and the result is **appended**
                // rather than re-scanned. Rule 1: a fact whose rendering
                // contains a bracket cannot expand a second time.
                match resolve(&tag, sheet, headline) {
                    Some(text) => out.push_str(&text),
                    None => return Err(NotSubstituted::UnknownTag { tag }),
                }
                // The tag spans `i..after`; this iteration has consumed the
                // character at `i`, so the rest are skipped.
                consumed = after.saturating_sub(i).saturating_sub(1);
                continue;
            }
            // Not a tag: an ordinary bracket, kept.
            out.push(c);
            continue;
        }
        if c.is_ascii_digit() {
            // Rule 2. The whole run is reported, so an operator reading the log
            // sees the number the model tried to write rather than its first
            // digit. `take_while` rather than an index walk, for the same reason
            // the outer loop is a `for`.
            let found: String = chars[i..]
                .iter()
                .take_while(|d| d.is_ascii_digit() || **d == '.')
                .collect();
            return Err(NotSubstituted::Digits { found });
        }
        out.push(c);
    }

    if out.trim().is_empty() {
        return Err(NotSubstituted::Empty);
    }
    Ok(out)
}

/// Reads a tag starting at `at`, returning it and the index after it.
///
/// Strict: `[F` followed by at least one digit followed by `]`, or exactly
/// `[H]`. Anything else is not a tag, which is what makes `[F[F1]]` resolve to
/// the inner tag inside two literal brackets rather than to something nested.
fn read_tag(chars: &[char], at: usize) -> Option<(String, usize)> {
    let opener = at + 1;
    if chars.get(opener) == Some(&'H') && chars.get(opener + 1) == Some(&']') {
        return Some((HEADLINE_TAG.to_owned(), opener + 2));
    }
    if chars.get(opener) != Some(&'F') {
        return None;
    }
    let start = opener + 1;
    // Counted rather than walked, for the reason `substitute`'s outer loop is a
    // `for`: a manual cursor is one mutation away from never advancing, and a
    // hang is reported as `inconclusive` rather than caught. `take_while`
    // cannot loop forever whatever is done to it.
    let digits: String = chars
        .get(start..)?
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let end = start + digits.chars().count();
    if digits.is_empty() || chars.get(end) != Some(&']') {
        return None;
    }
    Some((format!("[F{digits}]"), end + 1))
}

/// The text a tag stands for, or `None` if it names nothing.
fn resolve(tag: &str, sheet: &FactSheet, headline: Option<&str>) -> Option<String> {
    if tag == HEADLINE_TAG {
        return headline.map(ToOwned::to_owned);
    }
    let n: usize = tag.strip_prefix("[F")?.strip_suffix(']')?.parse().ok()?;
    // One-based, and zero is not a fact. `[F0]` is an off-by-one somebody would
    // otherwise never see, because it would silently resolve to the first fact.
    let index = n.checked_sub(1)?;
    sheet.facts.get(index).map(|f| f.rendered.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::Fact;

    fn sheet() -> FactSheet {
        FactSheet {
            mint: "MintOne".to_owned(),
            read_at: Some(radar_types::Slot(444_007_820)),
            facts: vec![
                Fact::exact("tokens this creator has launched", 3.0, "3"),
                Fact::exact("measured all-in round trip", 850.0, "850 bps"),
            ],
            untrusted: Vec::new(),
            unknown: vec!["the launch block could not be read".to_owned()],
            signals: Vec::new(),
        }
    }

    #[test]
    fn every_fact_is_numbered_from_one() {
        // One-based because the tags are for a reader. `[F0]` reads as a mistake
        // to everybody who is not a programmer, and the model is not one.
        let rendered = render_tagged(&sheet());
        assert!(rendered.starts_with("[F1] tokens this creator has launched: 3\n"));
        assert!(rendered.contains("[F2] measured all-in round trip: 850 bps"));
    }

    #[test]
    fn an_unknown_carries_no_tag_because_it_has_nothing_to_substitute() {
        // A tag that expanded to nothing would let the model write a sentence
        // whose subject silently vanished — which reads as a measurement and is
        // not one. The absence is said in words instead, and words carry no
        // digits.
        let rendered = render_tagged(&sheet());
        assert!(rendered.contains("NOT KNOWN (no tag; say it in words):"));
        assert!(!rendered.contains("[F3]"));
    }

    #[test]
    fn a_tag_becomes_radars_own_wording() {
        let out = substitute("[F1] launches, round trip [F2].", &sheet(), None)
            .expect("both tags resolve");
        assert_eq!(out, "3 launches, round trip 850 bps.");
    }

    #[test]
    fn a_digit_outside_a_tag_is_refused_and_the_whole_run_is_reported() {
        // Not stripped. A model that wrote a number was doing something this
        // design does not permit, and deleting it would leave a sentence
        // missing its subject.
        //
        // The whole run rather than its first character, because an operator
        // reading the log should see the number the model tried to publish.
        assert_eq!(
            substitute("[F1] launches and 97.1% of them died", &sheet(), None),
            Err(NotSubstituted::Digits {
                found: "97.1".to_owned()
            })
        );
    }

    #[test]
    fn a_tag_naming_no_fact_is_refused() {
        // A model that believes it was given a ninth fact was not.
        assert_eq!(
            substitute("[F9] launches", &sheet(), None),
            Err(NotSubstituted::UnknownTag {
                tag: "[F9]".to_owned()
            })
        );
        // And zero is not a fact. Without the `checked_sub` this would silently
        // resolve to the first one, which is an off-by-one nobody would ever
        // see.
        assert_eq!(
            substitute("[F0] launches", &sheet(), None),
            Err(NotSubstituted::UnknownTag {
                tag: "[F0]".to_owned()
            })
        );
    }

    #[test]
    fn a_substituted_value_is_never_scanned_again() {
        // Rule 1, at the level it is enforced. The output is built by appending,
        // so a fact whose rendering contains tag-shaped text cannot expand a
        // second time — which is the shape every template-injection bug has.
        let mut sheet = sheet();
        sheet.facts[0] = Fact::exact("tokens this creator has launched", 3.0, "[F2]");
        assert_eq!(
            substitute("look: [F1]", &sheet, None).expect("resolves once"),
            "look: [F2]"
        );
    }

    #[test]
    fn a_tag_inside_a_tag_resolves_the_inner_one_and_nothing_else() {
        // `[F` followed by something that is not a digit is not a tag, so the
        // outer brackets are ordinary characters. Worth pinning because a
        // reader's first instinct is that nesting must mean something.
        assert_eq!(
            substitute("[F[F1]]", &sheet(), None).expect("the inner tag"),
            "[F3]"
        );
    }

    #[test]
    fn a_bracket_that_is_not_a_tag_survives_as_a_bracket() {
        // The check must not be so tight that ordinary punctuation trips it.
        assert_eq!(
            substitute("[note] a thing [F1] [F] [Fx] [", &sheet(), None).expect("resolves"),
            "[note] a thing 3 [F] [Fx] ["
        );
    }

    #[test]
    fn the_headline_tag_resolves_only_when_a_headline_was_offered() {
        assert_eq!(
            substitute("[H] and nothing more", &sheet(), Some("Three launches.")).expect("ok"),
            "Three launches. and nothing more"
        );
        // Offered nothing, used anyway: refused rather than dropped, for the
        // same reason an unknown fact tag is.
        assert_eq!(
            substitute("[H] and nothing more", &sheet(), None),
            Err(NotSubstituted::UnknownTag {
                tag: "[H]".to_owned()
            })
        );
    }

    #[test]
    fn a_reply_that_substitutes_to_nothing_is_empty_rather_than_published() {
        assert_eq!(
            substitute("   ", &sheet(), None),
            Err(NotSubstituted::Empty)
        );
        assert_eq!(substitute("", &sheet(), None), Err(NotSubstituted::Empty));
    }

    #[test]
    fn a_tag_split_by_an_invisible_character_is_refused_rather_than_repaired() {
        // Why substitution runs *before* `render::for_publication`. Cleaning
        // first would repair `[F\u{200b}1]` into a working tag — which is the
        // model's text deciding what the tag was. Refusing is the direction that
        // ships the template.
        let broken = substitute("[F\u{200b}1] launches", &sheet(), None);
        assert_eq!(
            broken,
            Err(NotSubstituted::Digits {
                found: "1".to_owned()
            }),
            "a broken tag is a digit outside a tag, which is a refusal"
        );
    }

    #[test]
    fn a_reply_with_no_tags_and_no_digits_is_left_alone() {
        // The model is allowed to say something entirely qualitative. Words like
        // "none" and "both" are exactly what the prompt asks for where a figure
        // would be wrong.
        assert_eq!(
            substitute(
                "Nothing here was measured, and the absence is the fact.",
                &sheet(),
                None
            )
            .expect("no tags is fine"),
            "Nothing here was measured, and the absence is the fact."
        );
    }

    #[test]
    fn a_digit_run_at_the_very_end_is_reported_whole() {
        // The boundary the old index walk got wrong: `j < len` mutated to `<=`
        // survived because no test drove the run to the end of the input. It is
        // a `take_while` now and has no index to compare, but the case is worth
        // a test on its own terms — a model that ends a sentence on a figure is
        // the most likely way this fires.
        assert_eq!(
            substitute("launches 97", &sheet(), None),
            Err(NotSubstituted::Digits {
                found: "97".to_owned()
            })
        );
        assert_eq!(
            substitute("5", &sheet(), None),
            Err(NotSubstituted::Digits {
                found: "5".to_owned()
            })
        );
    }

    #[test]
    fn a_tag_is_skipped_by_exactly_its_own_length() {
        // `consumed` is what replaced the cursor jump. Too few and the tag's
        // own digits leak out as a fabricated number; too many and the
        // characters after it vanish from the reply.
        //
        // Both directions are asserted here because they fail differently: the
        // first is a wrong refusal, the second is a sentence missing its end.
        assert_eq!(
            substitute("[F1] launches, and that is all", &sheet(), None).expect("resolves"),
            "3 launches, and that is all"
        );
        assert_eq!(
            substitute("[F10]", &sheet(), None),
            Err(NotSubstituted::UnknownTag {
                tag: "[F10]".to_owned()
            }),
            "a two-digit tag is read whole"
        );
        // Two tags back to back, with nothing between them to absorb an
        // off-by-one.
        assert_eq!(
            substitute("[F1][F2]", &sheet(), None).expect("resolves"),
            "3850 bps"
        );
        // And the headline tag, which is a different length again.
        assert_eq!(
            substitute("[H]x", &sheet(), Some("Three.")).expect("resolves"),
            "Three.x"
        );
    }

    #[test]
    fn each_refusal_says_something_only_it_could_say() {
        // `Display` is what `radar roast` prints and what the daemon logs, and
        // it is the line somebody reads when the fifty tagged replies are being
        // checked by hand. Mutated to render the empty string, every other test
        // here still passed -- so this asserts each variant names its own
        // failure rather than merely being non-empty.
        let digits = NotSubstituted::Digits {
            found: "97.1".to_owned(),
        }
        .to_string();
        assert!(digits.contains("97.1"), "{digits}");
        assert!(digits.contains("digits"), "{digits}");

        let unknown = NotSubstituted::UnknownTag {
            tag: "[F9]".to_owned(),
        }
        .to_string();
        assert!(unknown.contains("[F9]"), "{unknown}");
        assert!(unknown.contains("no fact"), "{unknown}");

        let empty = NotSubstituted::Empty.to_string();
        assert!(empty.contains("nothing"), "{empty}");

        // And no two of them read the same, which is the property a caller
        // relies on when it puts one in a log line.
        assert_ne!(digits, unknown);
        assert_ne!(unknown, empty);
    }
}
