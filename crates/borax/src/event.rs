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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// A file the run declined to act on, and why.
    Skipped { path: PathBuf, reason: SkipReason },
    /// An entry written to the master bibliography.
    BibEntry {
        path: PathBuf,
        key: String,
        outcome: String,
    },
    /// The run is over. Always the last event.
    RunFinished { counts: Counts },
}

/// Why a file was left alone.
///
/// Every variant is a decision borax made deliberately; nothing here is
/// a crash. Serialized with a `kind` tag, nested under the event's
/// `reason` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SkipReason {
    /// Neither extraction pass found an identifier in the file.
    NoIdentifier,
    /// An identifier was found but no source holds it. `attempts`
    /// records what each source said, in the order they were asked.
    Unresolvable { attempts: Vec<Attempt> },
    /// The file's own metadata disagrees with the resolved record, so
    /// the record is probably about a different work.
    Conflict {
        field: String,
        extracted: String,
        resolved: String,
    },
    /// The name the template produced is taken, and the collision
    /// policy is to skip.
    TargetTaken { target: PathBuf },
    /// The file already carries the name the template produced.
    AlreadyNamed,
    /// The file could not be read as a PDF at all.
    Unreadable { message: String },
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
        Event::Skipped { path, reason } => Some(format!(
            "{}: skipped, {}",
            path.display(),
            skipped_because(reason)
        )),
        Event::BibEntry { path, key, outcome } => Some(format!(
            "{}: bibliography entry {key} {outcome}",
            path.display()
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
        } => format!("{field} disagrees (file says {extracted}, record says {resolved})"),
        SkipReason::TargetTaken { target } => format!("{} is taken", target.display()),
        SkipReason::AlreadyNamed => "already carries that name".to_string(),
        SkipReason::Unreadable { message } => format!("unreadable ({message})"),
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
