//! Where a run's record goes, and what it is called.
//!
//! A run log is the run's event stream written to a file: the same
//! versioned JSON Lines the `--json` format prints, so the record a
//! reviewer reads and the stream a script parses are one format rather
//! than two.
//!
//! Everything here is about placement and naming. Deciding those apart
//! from the writing is what lets the rules — which run keeps a log,
//! where it lands, and which run cannot go ahead without one — be
//! settled before the first event exists.

use std::ffi::OsString;
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

/// The on-disk layout version, used as the last path segment of the
/// state directory.
///
/// Everything borax keeps outside a collection lives under it, so
/// bumping it is how a layout change leaves the old one where it lies
/// rather than reading it as the new one.
pub const FORMAT_VERSION: &str = "v1";

/// The borax state directory implied by `lookup`, which answers the way
/// [`std::env::var_os`] does.
///
/// The candidates are tried in order and the first whose value is a
/// non-empty absolute path is taken: on Unix `XDG_STATE_HOME`, then
/// `HOME` (as `HOME/.local/state`); on Windows `LOCALAPPDATA`, then
/// `XDG_STATE_HOME`. A variable that is unset, empty, or relative is
/// skipped rather than fatal.
///
/// The returned path ends in `borax/<FORMAT_VERSION>` and is neither
/// created nor checked for existence. `None` when no candidate
/// qualifies, which is the signal that a run outside a collection has
/// nowhere to record itself — and so cannot apply, since a rename
/// nothing recorded is not reversible.
pub fn state_root(lookup: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let mut root = CANDIDATES.iter().find_map(|(name, suffix)| {
        let base = PathBuf::from(lookup(name)?);
        if !base.is_absolute() {
            return None;
        }

        Some(match suffix {
            Some(suffix) => base.join(suffix),
            None => base,
        })
    })?;

    root.push("borax");
    root.push(FORMAT_VERSION);
    Some(root)
}

/// The variables that may name a state directory, in the order they are
/// tried, each with what to append to its value.
#[cfg(not(windows))]
const CANDIDATES: &[(&str, Option<&str>)] =
    &[("XDG_STATE_HOME", None), ("HOME", Some(".local/state"))];

/// The variables that may name a state directory, in the order they are
/// tried, each with what to append to its value.
#[cfg(windows)]
const CANDIDATES: &[(&str, Option<&str>)] = &[("LOCALAPPDATA", None), ("XDG_STATE_HOME", None)];

/// [`state_root`] applied to this process's environment.
pub fn default_state_root() -> Option<PathBuf> {
    state_root(|name| std::env::var_os(name))
}

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
/// that the run changes something — `bib` and `ledger rebuild` both do
/// — but that renaming is the one thing borax does that leaves no
/// account of itself in the files afterwards: a bibliography file and
/// the ledger can each be produced again from the collection, while
/// the name a file used to carry is nowhere but this log. A run whose
/// log is only a record loses a record when the disk refuses it; an
/// apply run whose log is missing has moved files that nothing now
/// describes, which is why it is the one run that would rather not
/// happen at all.
///
/// Widening this to "the run mutates something" would make an
/// unwritable disk abort a `bib` run whose output is derivable anyway.
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
///   and it goes under `state_root`: a rename applied in a downloads
///   directory needs a record as much as one in a collection, while a
///   best-effort record of a run there is worth less than the directory
///   it would need.
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
