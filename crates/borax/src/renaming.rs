//! Deciding what to call a file, and then calling it that.
//!
//! Renaming is split in two on purpose. Planning is pure: it turns
//! records into names, asks [`borax_core::rename::plan`] to resolve
//! collisions, and produces a decision per file without touching
//! anything. Applying walks that plan and moves files.
//!
//! The split is what makes preview the default rather than a special
//! mode: a preview is the plan, rendered. An applying run computes the
//! identical plan and then acts on it, so what a user sees in a preview
//! is what a later `--apply` does — not a separate code path that hopes
//! to agree.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use borax_core::content::ContentHash;
use borax_core::record::Record;
use borax_core::rename::CollisionPolicy;
use borax_core::template::TemplateTable;

use crate::event::{Counts, Event};
use crate::pipeline::FileRecord;

/// Moving files, and seeing what is already there.
///
/// The seam renaming needs from the filesystem, kept apart from
/// [`crate::pipeline::Library`] because reading a file and moving one
/// are different privileges: a preview run wires an implementation that
/// cannot move anything.
pub trait Filesystem {
    /// The names already present in `directory`, each with its content
    /// hash where that is known and `None` where it is not.
    ///
    /// Keys are file names, not full paths, matching the namespace
    /// [`borax_core::rename::plan`] works in. An unreadable directory
    /// reports as empty: the planner then believes every name is free,
    /// and the rename itself fails safely if it is not.
    fn existing(&self, directory: &Path) -> BTreeMap<String, Option<String>>;

    /// Move `from` to `to`.
    ///
    /// Both paths lie in the same directory. Fails rather than
    /// overwrites when `to` exists — the planner is expected to have
    /// prevented that, and a race that gets past it must not cost a
    /// file.
    fn rename(&self, from: &Path, to: &Path) -> Result<(), RenameError>;
}

/// Why a file could not be moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameError {
    pub message: String,
}

/// What the planner decided for one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedRename {
    /// Move `path` to `target`, which is free.
    Rename { path: PathBuf, target: PathBuf },
    /// `path` already carries the name its record implies, or a
    /// byte-identical file already sits there.
    AlreadyNamed { path: PathBuf },
    /// Leave `path` alone: `target` is taken and the policy is to skip.
    TargetTaken { path: PathBuf, target: PathBuf },
    /// Leave `path` alone: its record renders no usable name.
    Unnameable { path: PathBuf },
}

impl PlannedRename {
    /// The file this decision is about.
    pub fn path(&self) -> &Path {
        match self {
            PlannedRename::Rename { path, .. }
            | PlannedRename::AlreadyNamed { path }
            | PlannedRename::TargetTaken { path, .. }
            | PlannedRename::Unnameable { path } => path,
        }
    }
}

/// The name `record` implies for the file at `path`.
///
/// The template its entry type selects is rendered, the result passed
/// through [`borax_core::sanitize::sanitize`], and `path`'s own
/// extension appended — a rename changes what a file is called, never
/// what it is. `hash` supplies the `sha1` field templates may use, and
/// its absence renders that field empty.
///
/// Returns `None` when the rendered, sanitized name is empty, which is
/// a record too sparse to name a file from.
pub fn target_name(
    path: &Path,
    record: &Record,
    hash: Option<&ContentHash>,
    templates: &TemplateTable,
) -> Option<String> {
    let _ = (path, record, hash, templates);
    todo!("render, sanitize, and re-attach the extension")
}

/// Plan the renames for a batch of resolved files.
///
/// Files are grouped by parent directory and each group planned
/// separately, because collisions are a property of a directory:
/// two files heading for the same name in different folders do not
/// collide, and the planner would wrongly suffix one of them if the
/// batch were planned as a single namespace.
///
/// Within a group, order follows the input, so the suffix a collision
/// receives is deterministic. A file whose record renders no usable
/// name is [`PlannedRename::Unnameable`] and never reaches the
/// planner.
pub fn plan_renames(
    resolved: &[(PathBuf, FileRecord)],
    templates: &TemplateTable,
    policy: CollisionPolicy,
    filesystem: &dyn Filesystem,
) -> Vec<PlannedRename> {
    let _ = (resolved, templates, policy, filesystem);
    todo!("group by directory and plan each group")
}

/// Carry out `plan`, or report what it would do.
///
/// With `apply` false nothing is moved and every rename is reported as
/// [`Event::Planned`]. With `apply` true each rename is performed in
/// plan order and reported as [`Event::Renamed`]; a failure becomes
/// [`crate::event::SkipReason::RenameFailed`] and the batch continues, because one
/// file that cannot be moved says nothing about the next.
///
/// Every variant that does not move a file — [`PlannedRename::AlreadyNamed`],
/// [`PlannedRename::TargetTaken`], [`PlannedRename::Unnameable`] —
/// reports the same way in both modes, since a preview has nothing to
/// add about a file nothing will happen to.
pub fn apply_renames(
    plan: &[PlannedRename],
    filesystem: &dyn Filesystem,
    apply: bool,
) -> Vec<Event> {
    let _ = (plan, filesystem, apply);
    todo!("walk the plan, moving files when applying")
}

/// The totals `events` add up to.
///
/// Counts what happened, not what was planned: a preview run renames
/// nothing and reports `renamed` as zero however many moves it
/// described.
pub fn counts_for(events: &[Event]) -> Counts {
    let _ = events;
    todo!("total the events")
}
