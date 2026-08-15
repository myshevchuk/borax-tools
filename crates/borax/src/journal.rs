//! The record of what a run moved, and how to move it back.
//!
//! Renaming is the one thing borax does that a user cannot undo by
//! re-running it: the old names are gone. The journal is what makes
//! that reversible — an append-only log of every applied rename, with
//! enough in each entry to prove, later, that the file at the new path
//! is still the file that was moved there.
//!
//! Verification is the point. `borax undo` moves a file back only when
//! it finds the content it journaled; anything else is reported and
//! left alone, because a wrong guess here overwrites a user's work.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use borax_core::content::ContentHash;
use serde::{Deserialize, Serialize};

use crate::event::{Event, SkipReason};
use crate::pipeline::Library;
use crate::renaming::Filesystem;

/// The on-disk layout version, used as the last path segment of the
/// default journal directory.
pub const FORMAT_VERSION: &str = "v1";

/// The journal file's name within that directory.
pub const JOURNAL_FILE: &str = "renames.jsonl";

/// Which run an entry belongs to.
///
/// Opaque and only ever compared, never parsed: the journal groups
/// entries by it and `undo` takes the last group. Uniqueness is the
/// caller's to guarantee.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RunId(String);

impl RunId {
    /// A run identifier reading exactly as `value`.
    pub fn new(value: impl Into<String>) -> RunId {
        RunId(value.into())
    }

    /// The identifier as it appears in the journal.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One applied rename.
///
/// Carries what `undo` needs to be sure of itself: where the file came
/// from, where it went, and the content hash it had when it moved. The
/// timestamp is for a human reading the journal; nothing keys on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub run: RunId,
    /// Where the file was before the run.
    pub from: PathBuf,
    /// Where the run put it.
    pub to: PathBuf,
    /// The content hash at the time of the move.
    pub hash: ContentHash,
    /// When the move happened, as the caller chose to render it.
    pub at: String,
}

/// An append-only log of applied renames.
///
/// Reading returns entries oldest first, which is the order they were
/// appended in; `undo` reverses within a run rather than relying on the
/// store to.
pub trait Journal {
    /// Append `entries` in order.
    ///
    /// The whole slice lands or none of it does, so a reader never sees
    /// half a run — a partially journaled run is one `undo` could not
    /// reason about.
    fn append(&self, entries: &[Entry]) -> io::Result<()>;

    /// Every entry, oldest first. An unreadable or absent journal reads
    /// as empty: there is simply nothing to undo.
    fn read(&self) -> Vec<Entry>;
}

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
/// qualifies, which is the signal to run without a journal — and so
/// without `--apply`, since an unjournaled rename is not reversible.
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

/// A [`Journal`] backed by a JSON Lines file.
#[derive(Debug, Clone)]
pub struct FileJournal {
    path: PathBuf,
}

impl FileJournal {
    /// A journal stored at `path`. The file and its parent directory
    /// are created on the first append.
    pub fn new(path: impl Into<PathBuf>) -> FileJournal {
        FileJournal { path: path.into() }
    }

    /// A journal at [`JOURNAL_FILE`] under [`default_state_root`], or
    /// `None` when the environment names no state directory.
    pub fn open_default() -> Option<FileJournal> {
        default_state_root().map(|root| FileJournal::new(root.join(JOURNAL_FILE)))
    }

    /// The file entries are appended to.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Journal for FileJournal {
    /// Appends the entries as one write, each as a line of JSON.
    ///
    /// Lines that cannot be serialized are dropped rather than written
    /// malformed, since a journal that does not parse is worse than one
    /// missing an entry: the first breaks `undo` for the whole run.
    fn append(&self, entries: &[Entry]) -> io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let mut batch = String::new();
        for entry in entries {
            if let Ok(line) = serde_json::to_string(entry) {
                batch.push_str(&line);
                batch.push('\n');
            }
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        // One open, one write: the batch reaches the file whole, so a
        // reader between two appends never sees part of a run.
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?
            .write_all(batch.as_bytes())
    }

    /// Lines that do not parse are skipped, so one corrupt entry costs
    /// its own reversal and not the rest of the journal.
    fn read(&self) -> Vec<Entry> {
        let Ok(contents) = fs::read_to_string(&self.path) else {
            return Vec::new();
        };

        contents
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

/// The entries of the most recent run in `entries`, in the order they
/// were applied.
///
/// "Most recent" is positional, not chronological: the run of the last
/// entry in the journal. Timestamps are a human convenience and a clock
/// that went backwards must not decide what `undo` reverts.
///
/// Selection is by run identifier, not by position alone: every entry
/// carrying the last entry's run is returned, in journal order, even if
/// another run's entries sit between them. A run is undone whole or not
/// at all.
///
/// Empty when the journal is empty.
pub fn last_run(entries: &[Entry]) -> Vec<Entry> {
    let Some(last) = entries.last() else {
        return Vec::new();
    };

    entries
        .iter()
        .filter(|entry| entry.run == last.run)
        .cloned()
        .collect()
}

/// Why an entry could not be reverted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unrevertible {
    /// Nothing is at the journaled new path any more.
    Missing,
    /// Something is there, but not what was moved there.
    ContentChanged,
    /// The original path is occupied, so reverting would overwrite.
    OriginalTaken,
    /// The move itself failed.
    Failed { message: String },
}

/// What `undo` did about one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoOutcome {
    /// The file moved from `from` — the path the run had put it at —
    /// back to `to`, the path it started from. The two are the entry's
    /// `to` and `from` respectively: an undo runs the move backwards,
    /// so its own from/to are the entry's, swapped.
    Reverted { from: PathBuf, to: PathBuf },
    /// The entry was left alone, for the stated reason.
    ///
    /// `path` is always the entry's journaled new path, whatever the
    /// reason — including [`Unrevertible::OriginalTaken`], which is
    /// about the other end of the move. It is the one path that
    /// identifies which journaled move is being reported.
    Left { path: PathBuf, reason: Unrevertible },
}

/// Revert the most recent applied run.
///
/// Entries are reverted in reverse order, so a run that renamed `a`→`b`
/// and then `b`→`c` unwinds without either step landing on a name the
/// other still holds.
///
/// Each entry is verified before it is touched: the file must still be
/// at the journaled new path, hash to the journaled content, and its
/// original path must be free — occupancy read from
/// [`Filesystem::existing`] over the original's own directory, the same
/// namespace planning uses. An entry failing any of those is
/// reported as [`UndoOutcome::Left`] and skipped — the run continues,
/// because one file moved away since the rename says nothing about the
/// others.
///
/// Returns outcomes in the order the entries were reverted, which is
/// the reverse of the order they were applied.
pub fn undo_last(
    journal: &dyn Journal,
    library: &dyn Library,
    filesystem: &dyn Filesystem,
) -> Vec<UndoOutcome> {
    last_run(&journal.read())
        .iter()
        .rev()
        .map(|entry| revert(entry, library, filesystem))
        .collect()
}

/// Verify `entry` and move its file back, or report why not.
fn revert(entry: &Entry, library: &dyn Library, filesystem: &dyn Filesystem) -> UndoOutcome {
    let left = |reason| UndoOutcome::Left {
        path: entry.to.clone(),
        reason,
    };

    let Ok(hash) = library.hash(&entry.to) else {
        return left(Unrevertible::Missing);
    };
    if hash != entry.hash {
        return left(Unrevertible::ContentChanged);
    }
    if original_taken(entry, filesystem) {
        return left(Unrevertible::OriginalTaken);
    }

    match filesystem.rename(&entry.to, &entry.from) {
        Ok(()) => UndoOutcome::Reverted {
            from: entry.to.clone(),
            to: entry.from.clone(),
        },
        Err(error) => left(Unrevertible::Failed {
            message: error.message,
        }),
    }
}

/// Whether anything holds the name `entry` would move its file back to.
///
/// An original naming no directory or no file is reported as occupied:
/// freedom cannot be established for it, and `undo` moves nothing it
/// cannot verify.
fn original_taken(entry: &Entry, filesystem: &dyn Filesystem) -> bool {
    let (Some(directory), Some(name)) = (entry.from.parent(), entry.from.file_name()) else {
        return true;
    };

    filesystem
        .existing(directory)
        .contains_key(name.to_string_lossy().as_ref())
}

/// The event reporting `outcome`.
///
/// A reverted file is an [`Event::Reverted`]; one left alone is an
/// [`Event::Skipped`] carrying the matching reason.
pub fn event_for(outcome: &UndoOutcome) -> Event {
    match outcome {
        UndoOutcome::Reverted { from, to } => Event::Reverted {
            path: from.clone(),
            target: to.clone(),
        },
        UndoOutcome::Left { path, reason } => Event::Skipped {
            path: path.clone(),
            reason: match reason {
                Unrevertible::Missing => SkipReason::Missing,
                Unrevertible::ContentChanged => SkipReason::ContentChanged,
                Unrevertible::OriginalTaken => SkipReason::OriginalTaken,
                Unrevertible::Failed { message } => SkipReason::RenameFailed {
                    message: message.clone(),
                },
            },
        },
    }
}
