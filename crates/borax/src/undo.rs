//! Reading back what a run moved, and moving it back.
//!
//! Renaming is the one thing borax does that a user cannot undo by
//! re-running it: the old names are gone. What makes it reversible is
//! the run log — the applying run's own event stream, persisted — which
//! carries for every move the path the file came from, the path it went
//! to, and the content hash it had when it moved.
//!
//! Verification is the point. `borax undo` moves a file back only when
//! it finds the content the log recorded; anything else is reported and
//! left alone, because a wrong guess here overwrites a user's work.

use std::path::PathBuf;

use borax_core::content::ContentHash;
use serde::Deserialize;

use crate::event::{Event, SCHEMA, SkipReason};
use crate::pipeline::Library;
use crate::renaming::Filesystem;

/// One applied rename, as a run log recorded it.
///
/// Carries what undo needs to be sure of itself: where the file came
/// from, where it went, and the content hash it had when it moved.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Move {
    /// Where the file was before the run.
    #[serde(rename = "path")]
    pub from: PathBuf,
    /// Where the run put it.
    #[serde(rename = "target")]
    pub to: PathBuf,
    /// The content hash at the time of the move.
    pub hash: ContentHash,
}

/// Why a run log cannot be replayed at all.
///
/// Distinct from a move that cannot be reverted ([`Unrevertible`]):
/// these are about the log as a whole, and a log borax cannot read is
/// one it must not act on any part of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A line carries a schema version this build does not understand,
    /// so what its fields mean is not knowable here.
    Schema { found: u32 },
    /// A line is not the JSON object every line of a run log is.
    Unreadable { message: String },
}

/// The moves `text` records, oldest first.
///
/// `text` is a run log: one JSON object per line, in the schema
/// [`crate::event::json_line`] writes. Only the lines tagged `renamed`
/// become moves; everything else a run reported about itself is read
/// past.
///
/// Every line's `schema` is checked, not only the ones that become
/// moves, and the first mismatch refuses the whole log: a version borax
/// does not understand may spell a field it does read differently, so
/// there is no line in such a log worth trusting. An event *variant*
/// this build has never heard of is not a refusal — the schema is what
/// promises what a `renamed` line means, and a stream that grew a new
/// variant kept that promise.
///
/// A line that is not JSON at all is [`Refusal::Unreadable`]: a log
/// borax cannot read whole is one it cannot know the extent of, and
/// half an undo is worse than none. Blank lines carry no event and are
/// passed over.
pub fn moves_in(text: &str) -> Result<Vec<Move>, Refusal> {
    let mut moves = Vec::new();

    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|error| Refusal::Unreadable {
                message: error.to_string(),
            })?;

        match value.get("schema").and_then(serde_json::Value::as_u64) {
            Some(schema) if schema == u64::from(SCHEMA) => {}
            found => {
                return Err(Refusal::Schema {
                    found: found.unwrap_or_default() as u32,
                });
            }
        }

        if value.get("event").and_then(serde_json::Value::as_str) != Some(RENAMED) {
            continue;
        }
        moves.push(
            serde_json::from_value(value).map_err(|error| Refusal::Unreadable {
                message: error.to_string(),
            })?,
        );
    }

    Ok(moves)
}

/// The tag a run log's applied renames carry, as
/// [`Event::Renamed`] serializes it.
const RENAMED: &str = "renamed";

/// Why a move could not be reverted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unrevertible {
    /// Nothing is at the recorded new path any more.
    Missing,
    /// Something is there, but not what was moved there.
    ContentChanged,
    /// The original path is occupied, so reverting would overwrite.
    OriginalTaken,
    /// The move itself failed.
    Failed { message: String },
}

/// What undo did about one move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoOutcome {
    /// The file moved from `from` — the path the run had put it at —
    /// back to `to`, the path it started from. The two are the move's
    /// `to` and `from` respectively: an undo runs the move backwards,
    /// so its own from/to are the recorded ones, swapped.
    Reverted { from: PathBuf, to: PathBuf },
    /// The move was left alone, for the stated reason.
    ///
    /// `path` is always the recorded new path, whatever the reason —
    /// including [`Unrevertible::OriginalTaken`], which is about the
    /// other end of the move. It is the one path that identifies which
    /// recorded move is being reported.
    Left { path: PathBuf, reason: Unrevertible },
}

/// Revert `moves`, which are one run's, in the order it applied them.
///
/// They are reverted in reverse, so a run that renamed `a`→`b` and then
/// `b`→`c` unwinds without either step landing on a name the other
/// still holds.
///
/// Each move is verified before it is touched: the file must still be
/// at the recorded new path, hash to the recorded content, and its
/// original path must be free — occupancy read from
/// [`Filesystem::existing`] over the original's own directory, the same
/// namespace planning uses. A move failing any of those is reported as
/// [`UndoOutcome::Left`] and skipped — the run continues, because one
/// file moved away since the rename says nothing about the others.
///
/// Returns outcomes in the order the moves were reverted, which is the
/// reverse of the order they were applied.
pub fn undo_moves(
    moves: &[Move],
    library: &dyn Library,
    filesystem: &dyn Filesystem,
) -> Vec<UndoOutcome> {
    moves
        .iter()
        .rev()
        .map(|recorded| revert(recorded, library, filesystem))
        .collect()
}

/// Verify `recorded` and move its file back, or report why not.
fn revert(recorded: &Move, library: &dyn Library, filesystem: &dyn Filesystem) -> UndoOutcome {
    let left = |reason| UndoOutcome::Left {
        path: recorded.to.clone(),
        reason,
    };

    let Ok(hash) = library.hash(&recorded.to) else {
        return left(Unrevertible::Missing);
    };
    if hash != recorded.hash {
        return left(Unrevertible::ContentChanged);
    }
    if original_taken(recorded, filesystem) {
        return left(Unrevertible::OriginalTaken);
    }

    match filesystem.rename(&recorded.to, &recorded.from) {
        Ok(()) => UndoOutcome::Reverted {
            from: recorded.to.clone(),
            to: recorded.from.clone(),
        },
        Err(error) => left(Unrevertible::Failed {
            message: error.message,
        }),
    }
}

/// Whether anything holds the name `recorded` would move its file back
/// to.
///
/// An original naming no directory or no file is reported as occupied:
/// freedom cannot be established for it, and undo moves nothing it
/// cannot verify.
fn original_taken(recorded: &Move, filesystem: &dyn Filesystem) -> bool {
    let (Some(directory), Some(name)) = (recorded.from.parent(), recorded.from.file_name()) else {
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
