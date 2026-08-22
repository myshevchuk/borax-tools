//! Where a run's record goes, and what it is called.
//!
//! A run log is the run's event stream written to a file: the same
//! versioned JSON Lines the `--json` format prints, so the record a
//! reviewer reads, the stream a script parses, and the log `borax undo`
//! will replay are one format rather than three.
//!
//! Everything here is about placement and naming. Deciding those apart
//! from the writing is what lets the rules — which run keeps a log,
//! where it lands, and which run cannot go ahead without one — be
//! settled before the first event exists.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cli::Command;
use crate::ledger::ACCOUNTING_DIR;

/// The directory run logs are kept in: under the collection's
/// [`ACCOUNTING_DIR`], or directly under the state root for a run that
/// belongs to no collection.
pub const RUNS_DIR: &str = "runs";

/// The extension every run log carries, naming the format its lines
/// are in.
const LOG_EXTENSION: &str = "jsonl";

/// The name a run log carries: `<stamp>-<command>-<dry|apply>.jsonl`.
///
/// `timestamp` keeps only its letters and digits, so a stamp written in
/// ISO 8601's extended form (`2024-01-01T00:00:00Z`) becomes the basic
/// form the name wants and no separator of any spelling can reach the
/// filename — a `:` alone makes a name Windows will not create.
/// `command` is the subcommand as [`Command::name`] gives it, with the
/// space of a two-word name closed up to a `-`.
///
/// The timestamp leads so that a directory listing sorts by time, and
/// the `dry`/`apply` suffix trails so a preview sorts directly above
/// the run that applied it.
pub fn log_name(timestamp: &str, command: &str, applying: bool) -> String {
    let stamp: String = timestamp
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    format!(
        "{stamp}-{}-{}.{LOG_EXTENSION}",
        command.replace(' ', "-"),
        phase(applying)
    )
}

/// What a run log's name says about whether the run was applying.
fn phase(applying: bool) -> &'static str {
    match applying {
        true => "apply",
        false => "dry",
    }
}

/// Where a run's log goes, and whether the run depends on it.
pub struct Destination {
    pub path: PathBuf,
    /// Whether the run has to abandon itself when the log cannot be
    /// written. See [`mandatory`].
    pub mandatory: bool,
}

/// Whether `command`'s log has to exist for the run to go ahead.
///
/// True for `rename --apply` and for nothing else. The reason is not
/// that the run changes something — `bib`, `undo` and `ledger rebuild`
/// all do — but that `borax undo` reads this log to reverse the run,
/// and what undo reverses is renames. A run whose log is only a record
/// loses a record when the disk refuses it; an apply run whose log is
/// missing has moved files nothing can bring back, which is why it is
/// the one run that would rather not happen at all.
///
/// Widening this to "the run mutates something" would make an
/// unwritable disk abort a `bib` run that had nothing to undo.
pub(crate) fn mandatory(command: &Command) -> bool {
    matches!(command, Command::Rename { apply: true, .. })
}

/// Where `command`'s run log goes, or `None` when it keeps none.
///
/// `applying` is what the run reports of itself and decides the name's
/// suffix; `enabled` is the `run-log` setting. `timestamp` is the run's
/// own, as [`log_name`] takes it.
///
/// The rules, in the order they apply:
///
/// - a mandatory log ([`mandatory`]) ignores `enabled`, since the
///   setting suppresses records and this one is not merely a record;
/// - a collection root takes the log, under its [`ACCOUNTING_DIR`], so
///   the run's record travels with the files it is about;
/// - outside a collection only a mandatory log has anywhere else to go,
///   and it goes under `state_root`: `borax undo` has to work in a
///   downloads directory, while a best-effort record of a run there is
///   worth less than the directory it would need.
///
/// `None` for an apply run with neither root is a refusal rather than a
/// run without a log — the caller is what turns it into one, since only
/// the caller knows the run has not started.
pub fn destination(
    command: &Command,
    applying: bool,
    enabled: bool,
    timestamp: &str,
    collection_root: Option<&Path>,
    state_root: Option<&Path>,
) -> Option<Destination> {
    let mandatory = mandatory(command);
    if !enabled && !mandatory {
        return None;
    }

    let directory = match (collection_root, state_root) {
        (Some(root), _) => root.join(ACCOUNTING_DIR).join(RUNS_DIR),
        (None, Some(state)) if mandatory => state.join(RUNS_DIR),
        (None, _) => return None,
    };

    Some(Destination {
        path: directory.join(log_name(timestamp, command.name(), applying)),
        mandatory,
    })
}

/// Open `destination` for writing, creating the directories above it.
///
/// The file is truncated if something is already there, which only a
/// second run within the same second could have put there.
pub(crate) fn create(destination: &Destination) -> io::Result<fs::File> {
    if let Some(parent) = destination.path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::File::create(&destination.path)
}

/// The most recent apply-run log: the collection's, and failing that
/// the state root's.
///
/// The collection is searched first and wins outright rather than by
/// being newer. A run in a collection is a run about that collection,
/// and a stray apply log left in the state directory by work somewhere
/// else must not be what an undo here reverses.
///
/// Dry-run logs are not candidates at all: they record a run that moved
/// nothing, so there is nothing in one to reverse.
pub fn latest_apply_log(
    collection_root: Option<&Path>,
    state_root: Option<&Path>,
) -> Option<PathBuf> {
    [
        collection_root.map(|root| root.join(ACCOUNTING_DIR).join(RUNS_DIR)),
        state_root.map(|root| root.join(RUNS_DIR)),
    ]
    .into_iter()
    .flatten()
    .find_map(|directory| latest_apply_in(&directory))
}

/// The apply log in `directory` whose name sorts last, or `None` when
/// there is none — including when `directory` is not there at all,
/// which is what a collection that has never had an applied run looks
/// like.
///
/// Every name begins with a fixed-width timestamp ([`log_name`]), so
/// the greatest name is the most recent run. Sorting by name rather
/// than by modification time is what keeps the answer stable across a
/// copy, a restore, or a checkout, none of which preserve mtimes.
fn latest_apply_in(directory: &Path) -> Option<PathBuf> {
    fs::read_dir(directory)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_apply_log(path))
        .max()
}

/// Whether `path` names a log of a run that applied.
///
/// The tail is built from the same pieces [`log_name`] ends with, so
/// what is recognised here cannot drift from what is written there.
fn is_apply_log(path: &Path) -> bool {
    let applied = format!("-{}.{LOG_EXTENSION}", phase(true));
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with(&applied))
}
