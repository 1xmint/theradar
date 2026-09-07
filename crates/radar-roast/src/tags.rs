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
    let mut out = String::new();
    let bytes: Vec<char> = written.chars().collect();
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];
        if c == '[' {
            if let Some((tag, after)) = read_tag(&bytes, i) {
                // Resolved against the sheet, and the result is **appended**
                // rather than re-scanned. Rule 1: a fact whose rendering
                // contains a bracket cannot expand a second time.
                match resolve(&tag, sheet, headline) {
                    Some(text) => out.push_str(&text),
                    None => return Err(NotSubstituted::UnknownTag { tag }),
                }
                i = after;
                continue;
            }
            // Not a tag: an ordinary bracket, kept.
            out.push(c);
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            // Rule 2. The whole run is reported, so an operator reading the log
            // sees the number the model tried to write rather than its first
            // digit.
            let mut found = String::new();
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == '.') {
                found.push(bytes[j]);
                j += 1;
            }
            return Err(NotSubstituted::Digits { found });
        }
        out.push(c);
        i += 1;
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
    let mut i = at + 1;
    if chars.get(i) == Some(&'H') && chars.get(i + 1) == Some(&']') {
        return Some((HEADLINE_TAG.to_owned(), i + 2));
    }
    if chars.get(i) != Some(&'F') {
        return None;
    }
    i += 1;
    let start = i;
    while chars.get(i).is_some_and(char::is_ascii_digit) {
        i += 1;
    }
    if i == start || chars.get(i) != Some(&']') {
        return None;
    }
    let digits: String = chars[start..i].iter().collect();
    Some((format!("[F{digits}]"), i + 1))
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
}
