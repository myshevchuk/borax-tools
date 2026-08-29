//! What a run says about itself.
//!
//! A run produces one stream of events, and the two output modes are
//! two renderings of it: `--json` writes each event as a JSON object on
//! its own line, and the default mode writes the same events as prose.
//! Neither mode learns anything the other does not, so a script and a
//! person are told the same story.
//!
//! Diagnostics are deliberately not events. Progress, warnings, and
//! failures that are about the run rather than about a file go to
//! stderr as [`Diagnostic`]s, which is what keeps stdout parseable line
//! by line.

use std::fmt;
use std::path::PathBuf;

use borax_core::content::ContentHash;
use borax_core::ledger::DuplicateReason;
use borax_core::record::Record;
use serde::{Deserialize, Serialize};

/// The version of the event schema, emitted on every JSON line.
///
/// Consumers pin it: within a major version of borax the shape of an
/// event with a given `event` tag does not change, and a new schema
/// version is how a breaking change announces itself.
pub const SCHEMA: u32 = 1;

/// Something that happened to a file, or to the run as a whole.
///
/// Serialized with an `event` tag naming the variant in kebab-case, so
/// a consumer dispatches on one field and can ignore variants it does
/// not know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Event {
    /// The run is starting. `applying` distinguishes a preview from a
    /// run that will touch the filesystem.
    RunStarted {
        command: String,
        version: String,
        applying: bool,
        /// The external lookup tables the run loaded, in name order.
        /// Rendering is deterministic given a table, but a table is a
        /// file that changes, so a run log without this says what a run
        /// did and not why it did it.
        tables: Vec<TableUsed>,
    },
    /// A lookup found no row. Reported once per distinct table and
    /// input however many files hit it, because what it tells the user
    /// is which line to add to their file, and that is one line however
    /// many documents wanted it.
    LookupMissed { table: String, input: String },
    /// A file's identifier was found and a record fetched for it.
    Resolved {
        path: PathBuf,
        identifier: String,
        /// The whole canonical record, so a consumer of the JSON stream
        /// has what the run resolved rather than only what it looked up.
        /// `borax resolve` exists to emit records; an identifier alone
        /// would send a caller back to the network for what borax
        /// already held.
        ///
        /// Boxed because events accumulate in a `Vec` for the whole
        /// run, where every event pays the size of the largest variant.
        record: Box<Record>,
        source: String,
        /// Which extraction pass supplied the identifier, or `None`
        /// when the caller named it rather than a file carrying it.
        tier: Option<String>,
        /// Whether the content index answered, so the file was neither
        /// opened nor looked up. A response cache hit behind a source
        /// is not visible here and reports `false`.
        cached: bool,
    },
    /// A rename that would happen. Emitted by a preview run only; an
    /// applying run emits [`Event::Renamed`] instead, so no file is
    /// ever reported twice in one run.
    Planned { path: PathBuf, target: PathBuf },
    /// A rename that did happen, with the content hash of the file
    /// that moved.
    ///
    /// The hash is what identifies the file the move was about,
    /// independently of the name it now carries: a reader of the run
    /// log can tell whether what sits at `target` is still what this
    /// event describes. It is required rather than optional because an
    /// applying rename refuses to move a file whose hash it does not
    /// know ([`SkipReason::Unrecordable`]), so a move that happened
    /// always has one.
    Renamed {
        path: PathBuf,
        target: PathBuf,
        hash: ContentHash,
    },
    /// A file the run declined to act on, and why.
    Skipped { path: PathBuf, reason: SkipReason },
    /// An entry written to the master bibliography.
    BibEntry {
        path: PathBuf,
        key: String,
        outcome: String,
    },
    /// A sidecar written beside a resolved file.
    Sidecar { path: PathBuf, target: PathBuf },
    /// One setting of the effective configuration, with the layer that
    /// supplied it. Emitted by `borax config`, one per setting.
    ///
    /// `value` is rendered in TOML value syntax and `origin` in the
    /// wording [`crate::config::Origin`] displays, so a consumer reads
    /// the same two strings a person does.
    ConfigSetting {
        key: String,
        value: String,
        origin: String,
    },
    /// What the response cache holds. `bytes` is the size on disk of
    /// the entries counted.
    CacheStatus {
        root: PathBuf,
        entries: usize,
        bytes: u64,
    },
    /// What clearing the response cache removed.
    CacheCleared {
        root: PathBuf,
        entries: usize,
        bytes: u64,
    },
    /// The collection's ledger was regenerated from what is on disk.
    /// `entries` is how many files the scan found worth recording,
    /// which is the whole of what the ledger holds afterwards rather
    /// than an addition to what it held before.
    LedgerRebuilt { root: PathBuf, entries: usize },
    /// The run is over. Always the last event.
    RunFinished { counts: Counts },
}

/// Why a file was left alone.
///
/// Every variant is a decision borax made deliberately; nothing here is
/// a crash. Serialized with a `kind` tag, nested under the event's
/// `reason` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SkipReason {
    /// Neither extraction pass found an identifier in the file.
    NoIdentifier,
    /// An identifier was found but no source holds it. `attempts`
    /// records what each source said, in the order they were asked.
    Unresolvable { attempts: Vec<Attempt> },
    /// The file's own metadata disagrees with the resolved record, so
    /// the record is probably about a different work.
    ///
    /// `similarity` is how close the two were, from 0.0 to 1.0, and is
    /// always below the threshold that would have cleared them. It is
    /// reported so the skip can be judged: a value near the threshold
    /// says the identifier is probably right and the metadata merely
    /// differs, while one near zero says the file and the record are
    /// about different works.
    Conflict {
        field: String,
        extracted: String,
        resolved: String,
        similarity: f64,
    },
    /// The name the template produced is taken, and the collision
    /// policy is to skip.
    TargetTaken { target: PathBuf },
    /// The file already carries the name the template produced.
    AlreadyNamed,
    /// The file could not be read as a PDF at all.
    Unreadable { message: String },
    /// The record resolved, but the template rendered an empty name
    /// from it — the record is too sparse to name a file.
    Unnameable,
    /// The rename itself failed, after the plan said it would not.
    RenameFailed { message: String },
    /// Bibliography output for the file could not be written.
    BibWriteFailed { message: String },
    /// The record resolved, but the template rendered nothing a
    /// citation key can be made of.
    Unciteable,
    /// Something borax did not write already sits where the file's
    /// sidecar would go, so the sidecar was not written.
    SidecarTaken { target: PathBuf },
    /// The move could not be recorded identifiably, so it was not
    /// made. Every rename in an apply-run log names the content that
    /// moved, and a line that could not say which file it was about
    /// would be a move the log cannot account for afterwards.
    Unrecordable { message: String },
    /// The collection has already admitted this file, by content or by
    /// work. `existing_path` is where the ledger says the file it
    /// duplicates sits, as a full path rather than the
    /// collection-relative one the ledger stores, so the report names
    /// somewhere the reader can go and look.
    Duplicate {
        reason: DuplicateReason,
        existing_path: PathBuf,
    },
}

/// One source's answer during resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub source: String,
    pub error: String,
}

/// One external table a run read, identified well enough to explain a
/// name it produced after the file has been edited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableUsed {
    /// The name templates address it by.
    pub name: String,
    /// The file it was read from, as the run resolved it.
    pub path: PathBuf,
    /// A digest of the bytes it was read with.
    pub digest: String,
}

/// What the run did, in totals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub resolved: usize,
    pub renamed: usize,
    pub skipped: usize,
    /// Distinct lookups that found no row.
    pub unmatched: usize,
}

impl Counts {
    /// Fold `event` into the totals.
    ///
    /// Counts what happened, not what was planned: a preview run
    /// renames nothing and leaves `renamed` at zero however many moves
    /// it described. An event that is about the run rather than about a
    /// file changes nothing.
    pub fn observe(&mut self, event: &Event) {
        match event {
            Event::Resolved { .. } => self.resolved += 1,
            Event::Renamed { .. } => self.renamed += 1,
            Event::Skipped { .. } => self.skipped += 1,
            Event::LookupMissed { .. } => self.unmatched += 1,
            _ => {}
        }
    }
}

/// Which rendering of the event stream to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Prose for a person.
    Human,
    /// One JSON object per line.
    Json,
}

/// `event` rendered in `format`, or `None` when this format has nothing
/// to say about this event.
///
/// Only the human format is ever silent — [`Format::Json`] renders
/// every event, because a consumer that has to reconstruct a run cannot
/// do it from a stream with holes in it.
pub fn render(format: Format, event: &Event) -> Option<String> {
    match format {
        Format::Human => human_line(event),
        Format::Json => Some(json_line(event)),
    }
}

/// `event` as a single line of JSON.
///
/// The object carries `schema` set to [`SCHEMA`] alongside the event's
/// own fields, and contains no newline, so a consumer may split the
/// stream on `\n` before parsing.
pub fn json_line(event: &Event) -> String {
    serde_json::to_string(&Line {
        schema: SCHEMA,
        event,
    })
    .unwrap_or_else(|error| unrenderable(&error.to_string()))
}

/// One line of the JSON stream: the schema version, then the event's own
/// fields hoisted to the same level.
#[derive(Serialize)]
struct Line<'a> {
    schema: u32,
    #[serde(flatten)]
    event: &'a Event,
}

/// The stand-in line for an event that would not serialize.
///
/// A caller writing the stream has nowhere to put an error — losing the
/// line entirely would leave a consumer silently short one event, so the
/// failure is reported in the stream's own shape, under an `event` tag
/// no real event uses.
#[derive(Serialize)]
struct Unrenderable<'a> {
    schema: u32,
    event: &'static str,
    message: &'a str,
}

fn unrenderable(message: &str) -> String {
    serde_json::to_string(&Unrenderable {
        schema: SCHEMA,
        event: "unrenderable",
        message,
    })
    .unwrap_or_else(|_| format!(r#"{{"schema":{SCHEMA},"event":"unrenderable"}}"#))
}

/// `event` as a line of prose, or `None` when the event is not worth
/// saying aloud.
///
/// [`Event::RunStarted`] is the only silent one: a person watching a
/// rename wants the renames, not a restatement of the command they
/// just typed. The JSON stream keeps it regardless.
pub fn human_line(event: &Event) -> Option<String> {
    match event {
        Event::RunStarted { .. } => None,
        Event::Resolved {
            path,
            identifier,
            source,
            cached,
            ..
        } => Some(format!(
            "{}: resolved {identifier} via {source}{}",
            path.display(),
            if *cached { " (cached)" } else { "" }
        )),
        Event::Planned { path, target } => Some(format!(
            "{}: would rename to {}",
            path.display(),
            target.display()
        )),
        Event::Renamed { path, target, .. } => Some(format!(
            "{}: renamed to {}",
            path.display(),
            target.display()
        )),
        Event::Skipped { path, reason } => Some(format!(
            "{}: skipped, {}",
            path.display(),
            skipped_because(reason)
        )),
        Event::BibEntry { path, key, outcome } => Some(format!(
            "{}: bibliography entry {key} {outcome}",
            path.display()
        )),
        Event::Sidecar { path, target } => Some(format!(
            "{}: sidecar written to {}",
            path.display(),
            target.display()
        )),
        Event::ConfigSetting { key, value, origin } => Some(format!("{key} = {value}  # {origin}")),
        Event::CacheStatus {
            root,
            entries,
            bytes,
        } => Some(format!(
            "{}: {entries} entries, {bytes} bytes",
            root.display()
        )),
        Event::CacheCleared {
            root,
            entries,
            bytes,
        } => Some(format!(
            "{}: cleared {entries} entries, {bytes} bytes",
            root.display()
        )),
        Event::LookupMissed { table, input } => Some(format!("{table}: no row for {input:?}")),
        Event::LedgerRebuilt { root, entries } => Some(format!(
            "{}: rebuilt with {entries} entries",
            root.display()
        )),
        // A zero count of unmatched lookups is left out rather than
        // written as a zero: the JSON summary carries it either way,
        // and a run that looked nothing up has nothing to say about
        // tables it never consulted.
        Event::RunFinished { counts } => Some(format!(
            "{} resolved, {} renamed, {} skipped{}",
            counts.resolved,
            counts.renamed,
            counts.skipped,
            match counts.unmatched {
                0 => String::new(),
                unmatched => format!(", {unmatched} unmatched"),
            }
        )),
    }
}

/// The clause following `skipped,` in a human line.
fn skipped_because(reason: &SkipReason) -> String {
    match reason {
        SkipReason::NoIdentifier => "no identifier found".to_string(),
        SkipReason::Unresolvable { attempts } => {
            let said: Vec<String> = attempts
                .iter()
                .map(|attempt| format!("{}: {}", attempt.source, attempt.error))
                .collect();
            match said.is_empty() {
                true => "no source had a record".to_string(),
                false => format!("no source had a record ({})", said.join("; ")),
            }
        }
        SkipReason::Conflict {
            field,
            extracted,
            resolved,
            similarity,
        } => format!(
            "{field} disagrees {}% (file says {extracted}, record says {resolved})",
            (similarity * 100.0).round()
        ),
        SkipReason::TargetTaken { target } => format!("{} is taken", target.display()),
        SkipReason::AlreadyNamed => "already carries that name".to_string(),
        SkipReason::Unreadable { message } => format!("unreadable ({message})"),
        SkipReason::Unnameable => "the record renders an empty name".to_string(),
        SkipReason::RenameFailed { message } => format!("rename failed ({message})"),
        SkipReason::BibWriteFailed { message } => {
            format!("bibliography output failed ({message})")
        }
        SkipReason::Unciteable => "the record renders an empty citation key".to_string(),
        SkipReason::SidecarTaken { target } => {
            format!("{} exists and borax did not write it", target.display())
        }
        SkipReason::Unrecordable { message } => {
            format!("the move could not be recorded, so it was not made ({message})")
        }
        SkipReason::Duplicate {
            reason: DuplicateReason::Content,
            existing_path,
        } => format!("same bytes already archived at {}", existing_path.display()),
        SkipReason::Duplicate {
            reason: DuplicateReason::Work,
            existing_path,
        } => format!(
            "same work already archived at {} (different file)",
            existing_path.display()
        ),
    }
}

/// How much a diagnostic matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Warning,
    Error,
}

/// A message about the run rather than about a file.
///
/// Diagnostics go to stderr in both output formats and are never part
/// of the event stream, so `--json` stdout stays parseable even when a
/// run has plenty to complain about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: Level,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    /// `warning: <message>` or `error: <message>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.level {
            Level::Warning => write!(f, "warning: {}", self.message),
            Level::Error => write!(f, "error: {}", self.message),
        }
    }
}
