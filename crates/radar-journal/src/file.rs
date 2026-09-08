// SPDX-License-Identifier: Apache-2.0
//! The file the journal lives in, and what reading it back can establish.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::event::{Correlation, Event, MAX_REDACTED, Outcome, Recorded, SCHEMA_VERSION, Stage};

/// The line terminator, as a byte.
///
/// Named because a byte literal for it cannot be written inline without an
/// escape, and this file is edited by tools that mangle those.
const NEWLINE: u8 = 10;

/// What stopped a journal operation.
///
/// Every variant is a refusal to proceed rather than a thing to log and shrug
/// at. A caller that cannot write its intent must not perform the effect.
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    /// The file could not be read or written.
    #[error("journal {path}: {source}")]
    Io {
        /// Which file.
        path: String,
        /// Why.
        #[source]
        source: std::io::Error,
    },
    /// An event could not be turned into a line.
    #[error("an event could not be serialised: {0}")]
    Serialise(#[from] serde_json::Error),
    /// A caller-supplied diagnostic was longer than [`MAX_REDACTED`].
    ///
    /// Refused rather than truncated: a caller passing something that large is
    /// passing a response body, and the rule is that callers pass a reason.
    #[error("redacted detail is {length} bytes; the bound is {MAX_REDACTED}")]
    RedactedTooLong {
        /// How long it was.
        length: usize,
    },
    /// The event named nothing it is about.
    ///
    /// Findable only by sequence, which is a caller that forgot rather than a
    /// property of the event.
    #[error("an event must correlate to something; this one names nothing")]
    NoCorrelation,
}

/// What reading a journal back established.
///
/// Three states, and they are three different situations for whoever is looking
/// at the box. A `Torn` journal is the ordinary shape of a crash; a `Broken` one
/// means something wrote to this file that was not this journal.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Verified {
    /// Every line parsed, every sequence followed the last, every hash matched.
    Intact {
        /// How many events.
        events: usize,
    },
    /// Every line but the final one is intact, and the final one is incomplete.
    ///
    /// **Not a fault.** A process killed mid-write leaves exactly this, and the
    /// record of everything before it is untouched. Distinguishing it from an
    /// empty history is a requirement rather than a nicety: "nothing was ever
    /// written" and "the last write was interrupted" send an operator to
    /// different places.
    Torn {
        /// How many complete events precede the torn line.
        events: usize,
    },
    /// A gap, a malformed line that is not the last, or a hash that does not
    /// match. A visible fault.
    Broken {
        /// The sequence the fault was found at.
        at: u64,
        /// What was wrong, in words.
        why: String,
    },
}

/// An append-only, hash-chained journal in one file.
///
/// One JSON object per line, the shape [`radar_analyst::log`] uses and for the
/// same reason: a crash mid-write loses the last line rather than the file, and
/// a partial line is detectably partial.
pub struct Journal {
    path: PathBuf,
    sequence: u64,
    previous: String,
}

impl Journal {
    /// Opens the journal at `path`, reading its tail to find where the chain is.
    ///
    /// A journal that does not exist yet is an empty one; the file is created by
    /// the first [`record`](Self::record).
    ///
    /// **A torn final line is resumed from, not repaired.** The complete events
    /// before it are the chain, and the next event continues from the last of
    /// them — so an interrupted write costs the intent it was recording and
    /// nothing else. Rewriting the file to remove the torn line would be this
    /// crate mutating its own append-only record, which is the one thing it must
    /// never do.
    ///
    /// # Errors
    ///
    /// [`JournalError::Io`] if the file exists and cannot be read.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, JournalError> {
        let path = path.into();
        let events = read(&path)?;
        let last = events.last();
        Ok(Self {
            sequence: last.map_or(0, |e| e.sequence),
            previous: last.map_or_else(String::new, |e| e.id.clone()),
            path,
        })
    }

    /// The sequence the next event will carry.
    #[must_use]
    pub const fn next_sequence(&self) -> u64 {
        self.sequence + 1
    }

    /// Writes one event, and returns the receipt an effect needs.
    ///
    /// The sequence, the previous hash and the id are filled in here. A caller
    /// cannot choose them, which is what makes the chain a property of the
    /// journal rather than of everyone who writes to it.
    ///
    /// # Errors
    ///
    /// [`JournalError`] if the event is refused or the write fails. **A caller
    /// that receives one must not perform the effect it was about to perform.**
    #[expect(clippy::too_many_arguments, reason = "the event contract's own fields")]
    pub fn record(
        &mut self,
        stage: Stage,
        outcome: Outcome,
        at: u64,
        correlation: Correlation,
        build: Option<String>,
        versions: Vec<(String, String)>,
        public_reason: Option<String>,
        redacted: Option<String>,
    ) -> Result<Recorded, JournalError> {
        if let Some(detail) = redacted.as_deref()
            && detail.len() > MAX_REDACTED
        {
            return Err(JournalError::RedactedTooLong {
                length: detail.len(),
            });
        }
        if correlation.is_empty() {
            return Err(JournalError::NoCorrelation);
        }

        let mut event = Event {
            schema: SCHEMA_VERSION,
            sequence: self.sequence + 1,
            id: String::new(),
            previous: self.previous.clone(),
            correlation,
            stage,
            outcome,
            at,
            took_ms: None,
            build,
            versions,
            config: None,
            public_reason,
            redacted,
        };
        event.id = event.digest();

        let line = serde_json::to_string(&event)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| JournalError::Io {
                path: self.path.display().to_string(),
                source,
            })?;
        // A torn final write leaves a line with no terminator, and appending
        // straight onto it would splice the two into one unparseable line --
        // destroying the *next* event as well as the torn one, and turning a
        // recoverable crash into a lost intent. Found by the torn-journal test
        // rather than by reasoning: the first version of this function appended
        // unconditionally and the event after a tear vanished.
        //
        // Terminating the torn line is not mutating the record. It appends one
        // byte; the torn line stays torn, stays unparseable, and still reads as
        // `Torn`.
        if ends_mid_line(&self.path)? {
            file.write_all(&[NEWLINE])
                .map_err(|source| JournalError::Io {
                    path: self.path.display().to_string(),
                    source,
                })?;
        }
        // One `writeln!`, so the line and its terminator go to the kernel
        // together and a crash cannot leave a complete line with no newline
        // that the next append then continues.
        writeln!(file, "{line}").map_err(|source| JournalError::Io {
            path: self.path.display().to_string(),
            source,
        })?;
        // `sync_all` before the receipt exists. Without it the effect can
        // happen and the machine can lose power with the intent still in the
        // page cache, which is the exact ordering this crate is for.
        file.sync_all().map_err(|source| JournalError::Io {
            path: self.path.display().to_string(),
            source,
        })?;

        self.sequence = event.sequence;
        self.previous.clone_from(&event.id);
        Ok(Recorded::new(event))
    }

    /// Every complete event in the file, in order.
    ///
    /// # Errors
    ///
    /// [`JournalError::Io`] if the file cannot be read. A file that does not
    /// exist reads as empty.
    pub fn events(&self) -> Result<Vec<Event>, JournalError> {
        read(&self.path)
    }

    /// Walks the chain and says what it found.
    ///
    /// # Errors
    ///
    /// [`JournalError::Io`] if the file cannot be read.
    pub fn verify(&self) -> Result<Verified, JournalError> {
        verify(&self.path)
    }
}

/// Whether the file has content that does not end in a newline.
///
/// Which is to say: whether the last write was interrupted. An empty file is
/// `false` — there is no torn line to terminate.
///
/// Its only caller has already opened the file with `create(true)`, so a
/// missing file is not a case here. It was special-cased anyway in the first
/// version, and the mutation gate reported the branch as a survivor — correctly,
/// because nothing can reach it. A read that fails now is an error, which is
/// what it should be: if the file we just created cannot be read, the next thing
/// this function's caller would do is append to it.
fn ends_mid_line(path: &Path) -> Result<bool, JournalError> {
    let bytes = fs::read(path).map_err(|source| JournalError::Io {
        path: path.display().to_string(),
        source,
    })?;
    Ok(bytes.last().is_some_and(|b| *b != NEWLINE))
}

/// Reads the complete events out of a journal file.
///
/// A final line that will not parse is dropped, because that is what a crash
/// mid-write leaves and the events before it are still the record. A line that
/// will not parse **anywhere else** is dropped here too and reported by
/// [`verify`] — this function's job is to hand back what is readable, and
/// deciding whether the file is sound is a different question asked by a
/// different function.
fn read(path: &Path) -> Result<Vec<Event>, JournalError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(JournalError::Io {
                path: path.display().to_string(),
                source,
            });
        }
    };
    Ok(text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}

/// Walks a journal file and says whether it is intact, torn, or broken.
fn verify(path: &Path) -> Result<Verified, JournalError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Verified::Intact { events: 0 });
        }
        Err(source) => {
            return Err(JournalError::Io {
                path: path.display().to_string(),
                source,
            });
        }
    };

    let lines: Vec<&str> = text.lines().collect();
    let mut previous = String::new();
    let mut events = 0usize;

    // The expected sequence comes from a zipped range rather than a variable
    // this loop advances. Same reason `split` in `radar-research` stopped using
    // a cursor: a hand-advanced counter is a hang waiting for a mutant, and
    // `cargo mutants` reports a hang as an inconclusive shard rather than as
    // the survivor it is. Clippy asks for this shape too.
    for (expected, (index, line)) in (1u64..).zip(lines.iter().enumerate()) {
        let last = index + 1 == lines.len();
        let Ok(event) = serde_json::from_str::<Event>(line) else {
            if last {
                // The ordinary shape of a crash. Everything before it stands.
                return Ok(Verified::Torn { events });
            }
            return Ok(Verified::Broken {
                at: expected,
                why: "a line that is not the last will not parse".to_owned(),
            });
        };
        if event.sequence != expected {
            return Ok(Verified::Broken {
                at: expected,
                why: format!("expected sequence {expected}, found {}", event.sequence),
            });
        }
        if event.previous != previous {
            return Ok(Verified::Broken {
                at: expected,
                why: "the previous-event hash does not match the event before it".to_owned(),
            });
        }
        if event.id != event.digest() {
            return Ok(Verified::Broken {
                at: expected,
                why: "the event's own hash does not match its contents".to_owned(),
            });
        }
        previous = event.id;
        events += 1;
    }

    Ok(Verified::Intact { events })
}
