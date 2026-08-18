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
    },
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
    /// A rename that did happen.
    Renamed { path: PathBuf, target: PathBuf },
    /// A rename that was undone: `path` moved back to `target`.
    ///
    /// Distinct from [`Event::Renamed`] even though both describe a
    /// move, because a consumer tracking a library needs to tell a file
    /// arriving from a rename apart from one returning from an undo.
    Reverted { path: PathBuf, target: PathBuf },
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
    /// The move could not be recorded in the undo journal, so it was
    /// not made. A move nothing recorded is one `borax undo` could
    /// never reverse.
    Unjournalable { message: String },
    /// Nothing is at the path the journal says a file was moved to, so
    /// there is nothing to move back.
    Missing,
    /// Something is at the journaled path, but not the file that was
    /// moved there — reverting it would move a stranger.
    ContentChanged,
    /// The path a file would be moved back to is occupied, so
    /// reverting would overwrite whatever now holds it.
    OriginalTaken,
}

/// One source's answer during resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub source: String,
    pub error: String,
}

/// What the run did, in totals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub resolved: usize,
    pub renamed: usize,
    pub skipped: usize,
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
        Event::Renamed { path, target } => Some(format!(
            "{}: renamed to {}",
            path.display(),
            target.display()
        )),
        Event::Reverted { path, target } => Some(format!(
            "{}: moved back to {}",
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
        Event::RunFinished { counts } => Some(format!(
            "{} resolved, {} renamed, {} skipped",
            counts.resolved, counts.renamed, counts.skipped
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
        SkipReason::Missing => "nothing is there to move back".to_string(),
        SkipReason::ContentChanged => "the file there is not the one that was moved".to_string(),
        SkipReason::OriginalTaken => "its original name is taken".to_string(),
        SkipReason::RenameFailed { message } => format!("rename failed ({message})"),
        SkipReason::BibWriteFailed { message } => {
            format!("bibliography output failed ({message})")
        }
        SkipReason::Unciteable => "the record renders an empty citation key".to_string(),
        SkipReason::SidecarTaken { target } => {
            format!("{} exists and borax did not write it", target.display())
        }
        SkipReason::Unjournalable { message } => {
            format!("the move could not be recorded, so it was not made ({message})")
        }
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
