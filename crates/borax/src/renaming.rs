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
use std::fs;
use std::path::{Path, PathBuf};

use borax_core::content::ContentHash;
use borax_core::record::Record;
use borax_core::rename::{CollisionPolicy, PlanInput, PlannedAction, plan};
use borax_core::sanitize::sanitize;
use borax_core::template::{RenderInput, TemplateTable};
use borax_sources::store::hash_file;

use crate::event::{Counts, Event, SkipReason};
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
    /// `target` is the name the record asked for, never a suffixed
    /// candidate — under [`CollisionPolicy::Skip`] nothing is suffixed.
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
/// The template its entry type selects is rendered, `path`'s own
/// extension appended — a rename changes what a file is called, never
/// what it is — and the whole name passed through
/// [`borax_core::sanitize::sanitize`]. The extension goes on before
/// sanitizing rather than after, because sanitizing is what enforces
/// the length limit, and it truncates a component's stem precisely so
/// that its extension survives. Appending afterwards would push a
/// maximal name back over the limit it had just been cut to fit.
///
/// `hash` supplies the `sha1` field templates may use, and its absence
/// renders that field empty.
///
/// Returns `None` when the template renders an empty string, which is
/// a record too sparse to name a file from. The check is on the
/// rendered text, before sanitization: [`borax_core::sanitize::sanitize`]
/// answers `_` for an empty input, so a name that survives it is never
/// evidence that there was one to begin with.
pub fn target_name(
    path: &Path,
    record: &Record,
    hash: Option<&ContentHash>,
    templates: &TemplateTable,
) -> Option<String> {
    let rendered = templates.render(&RenderInput {
        record,
        sha1: hash.map(ContentHash::as_str),
    });
    if rendered.is_empty() {
        return None;
    }

    let named = match path.extension() {
        Some(extension) => format!("{rendered}.{}", extension.to_string_lossy()),
        None => rendered,
    };
    Some(sanitize(&named))
}

/// What the planner needs to know about one resolved file, or `None`
/// when the file has no name to plan from.
fn plan_input(path: &Path, file: &FileRecord, templates: &TemplateTable) -> Option<PlanInput> {
    Some(PlanInput {
        source: path.file_name()?.to_string_lossy().into_owned(),
        target: target_name(path, &file.record, file.hash.as_ref(), templates)?,
        content_hash: file
            .hash
            .as_ref()
            .map_or("", ContentHash::as_str)
            .to_string(),
    })
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
/// planner — it claims no name, so it cannot cost a later file its
/// unsuffixed one.
///
/// Paths cross the two namespaces here: the planner is handed bare
/// file names, matching [`Filesystem::existing`], while every path in
/// the returned decisions is full — the input path as given, and a
/// target of its directory joined to the planned name.
pub fn plan_renames(
    resolved: &[(PathBuf, FileRecord)],
    templates: &TemplateTable,
    policy: CollisionPolicy,
    filesystem: &dyn Filesystem,
) -> Vec<PlannedRename> {
    let mut groups: BTreeMap<&Path, Vec<usize>> = BTreeMap::new();
    for (index, (path, _)) in resolved.iter().enumerate() {
        let directory = path.parent().unwrap_or(Path::new(""));
        groups.entry(directory).or_default().push(index);
    }

    // Decisions are parked at their input position, so grouping — which
    // visits directories in sorted order — cannot reorder the result.
    let mut decisions: Vec<Option<PlannedRename>> = vec![None; resolved.len()];
    for (directory, members) in groups {
        let mut inputs = Vec::with_capacity(members.len());
        let mut planned_indices = Vec::with_capacity(members.len());
        for index in members {
            let (path, file) = &resolved[index];
            match plan_input(path, file, templates) {
                Some(input) => {
                    inputs.push(input);
                    planned_indices.push(index);
                }
                None => {
                    decisions[index] = Some(PlannedRename::Unnameable { path: path.clone() });
                }
            }
        }

        let items = plan(&inputs, &filesystem.existing(directory), policy);
        for ((item, input), index) in items.into_iter().zip(&inputs).zip(planned_indices) {
            let path = resolved[index].0.clone();
            decisions[index] = Some(match item.action {
                PlannedAction::Rename { to } => PlannedRename::Rename {
                    path,
                    target: directory.join(to),
                },
                PlannedAction::AlreadyNamed => PlannedRename::AlreadyNamed { path },
                PlannedAction::Skip { .. } => PlannedRename::TargetTaken {
                    path,
                    target: directory.join(&input.target),
                },
            });
        }
    }

    decisions.into_iter().flatten().collect()
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
    plan.iter()
        .map(|decision| match decision {
            PlannedRename::Rename { path, target } if apply => {
                match filesystem.rename(path, target) {
                    Ok(()) => Event::Renamed {
                        path: path.clone(),
                        target: target.clone(),
                    },
                    Err(error) => Event::Skipped {
                        path: path.clone(),
                        reason: SkipReason::RenameFailed {
                            message: error.message,
                        },
                    },
                }
            }
            PlannedRename::Rename { path, target } => Event::Planned {
                path: path.clone(),
                target: target.clone(),
            },
            PlannedRename::AlreadyNamed { path } => Event::Skipped {
                path: path.clone(),
                reason: SkipReason::AlreadyNamed,
            },
            PlannedRename::TargetTaken { path, target } => Event::Skipped {
                path: path.clone(),
                reason: SkipReason::TargetTaken {
                    target: target.clone(),
                },
            },
            PlannedRename::Unnameable { path } => Event::Skipped {
                path: path.clone(),
                reason: SkipReason::Unnameable,
            },
        })
        .collect()
}

/// The totals `events` add up to.
///
/// Counts what happened, not what was planned: a preview run renames
/// nothing and reports `renamed` as zero however many moves it
/// described.
pub fn counts_for(events: &[Event]) -> Counts {
    let mut counts = Counts::default();
    for event in events {
        match event {
            Event::Resolved { .. } => counts.resolved += 1,
            Event::Renamed { .. } => counts.renamed += 1,
            Event::Skipped { .. } => counts.skipped += 1,
            _ => {}
        }
    }
    counts
}

/// A [`Filesystem`] backed by the real filesystem.
///
/// Reads a directory to see what is in it, and moves files with
/// [`std::fs`].
#[derive(Debug, Clone, Copy)]
pub struct RealFilesystem;

impl Filesystem for RealFilesystem {
    /// Every regular file in `directory`, each under its content hash.
    ///
    /// Hashing the whole directory costs one pass over its files, which
    /// is the same order as the run itself: a batch already hashes every
    /// file it was given. What it buys is the identical-content case —
    /// a file whose target name is taken by a byte-identical file is
    /// already named rather than blocked.
    ///
    /// A directory that cannot be read reports empty, and a file whose
    /// hash cannot be taken reports `None` for it, so neither failure
    /// ends the run.
    fn existing(&self, directory: &Path) -> BTreeMap<String, Option<String>> {
        let Ok(listing) = fs::read_dir(directory) else {
            return BTreeMap::new();
        };

        listing
            .flatten()
            .filter(|entry| entry.metadata().is_ok_and(|metadata| metadata.is_file()))
            .map(|entry| {
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    hash_file(&entry.path())
                        .ok()
                        .map(|hash| hash.as_str().to_string()),
                )
            })
            .collect()
    }

    /// Move `from` to `to` without ever overwriting `to`.
    ///
    /// Done as a link followed by an unlink rather than as a rename:
    /// [`std::fs::rename`] replaces an existing destination silently on
    /// Unix, and the one thing this must not do is lose the file that
    /// was already there. [`std::fs::hard_link`] fails when `to` exists,
    /// and it fails in the kernel rather than after a check, so nothing
    /// can slip in between.
    ///
    /// The cost is that the two names both exist for an instant, and a
    /// process killed in that instant leaves the file under both. That
    /// is recoverable; an overwrite is not.
    fn rename(&self, from: &Path, to: &Path) -> Result<(), RenameError> {
        let failed = |error: std::io::Error| RenameError {
            message: error.to_string(),
        };

        fs::hard_link(from, to).map_err(failed)?;
        // An unlink that fails leaves the file under both names rather
        // than undoing the link: removing `to` would be the only link
        // left if `from` disappeared under us, and a duplicate costs a
        // second run where that would cost the file.
        fs::remove_file(from).map_err(failed)
    }
}
