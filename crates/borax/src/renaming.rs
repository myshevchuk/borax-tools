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

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use borax_core::content::ContentHash;
use borax_core::record::Record;
use borax_core::rename::{CollisionPolicy, PlanInput, PlannedAction, Planner};
use borax_core::sanitize::sanitize;
use borax_core::tables::Lookups;
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
    /// Keys are file names, not full paths: this answers about one
    /// directory, and the caller is what assembles the namespace
    /// [`borax_core::rename::plan`] compares in — a batch whose targets
    /// reach into subdirectories asks about each of them.
    ///
    /// A directory that is unreadable, or that is not there at all,
    /// reports as empty: the planner then believes every name in it is
    /// free, and the rename itself fails safely if it is not.
    fn existing(&self, directory: &Path) -> BTreeMap<String, Option<String>>;

    /// Move `from` to `to`.
    ///
    /// `to` lies in `from`'s directory or in a subdirectory of it — a
    /// template renders `/` as a directory separator — and any part of
    /// that subdirectory which does not exist is the implementation's
    /// to create.
    ///
    /// Fails rather than overwrites when `to` exists: the planner is
    /// expected to have prevented that, and a race that gets past it
    /// must not cost a file.
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
    lookups: &mut Lookups<'_>,
) -> Option<String> {
    let rendered = templates.render(
        &RenderInput {
            record,
            sha1: hash.map(ContentHash::as_str),
        },
        lookups.tables(),
    );
    for miss in rendered.misses {
        lookups.record(miss);
    }
    if rendered.text.is_empty() {
        return None;
    }

    let named = match path.extension() {
        Some(extension) => format!("{}.{}", rendered.text, extension.to_string_lossy()),
        None => rendered.text,
    };
    Some(sanitize(&named))
}

/// Planning one directory's renames, a file at a time.
///
/// Holds what every decision in the directory is measured against: the
/// names the directory held when this was built, and the names the
/// renames planned since have taken. Files arrive in the order the run
/// gives them and each decision sees every claim its predecessors made,
/// so the suffix a collision receives is the one it would receive from a
/// single call planning the whole directory.
///
/// A template may render a name containing `/`, which
/// [`borax_core::sanitize::sanitize`] keeps as a directory separator, so
/// a target can land in a subdirectory of the directory as well as in the
/// directory itself. Each such subdirectory is looked at the first time a
/// target names it and kept for the rest of the directory: a name is free
/// only if nothing holds it *where the file is going*, and asking only
/// about the directory would let a rename be planned onto a file the
/// planner never saw. A subdirectory that does not exist reads as empty,
/// so a target heading somewhere not yet created finds nothing in its way.
///
/// Paths cross two namespaces here: the planner is handed bare file
/// names and `sub/name` relative paths, matching
/// [`Filesystem::existing`], while every path in a returned decision is
/// full.
pub struct Planning<'a> {
    directory: PathBuf,
    templates: &'a TemplateTable,
    policy: CollisionPolicy,
    filesystem: &'a dyn Filesystem,
    planner: Planner,
    /// The subdirectories already added to the planner's namespace,
    /// relative to `directory`.
    listed: BTreeSet<String>,
}

impl<'a> Planning<'a> {
    /// A plan in progress for files in `directory`, named from
    /// `templates` and resolving collisions by `policy`.
    ///
    /// `directory` is read here, once, and every decision is measured
    /// against what it held at that moment.
    pub fn new(
        directory: &Path,
        templates: &'a TemplateTable,
        policy: CollisionPolicy,
        filesystem: &'a dyn Filesystem,
    ) -> Planning<'a> {
        Planning {
            directory: directory.to_path_buf(),
            templates,
            policy,
            filesystem,
            // Eagerly, unlike the subdirectories below: reading a
            // directory hashes every file in it, and one pass per file
            // planned would cost the square of what one pass per
            // directory does.
            planner: Planner::new(filesystem.existing(directory)),
            listed: BTreeSet::new(),
        }
    }

    /// The decision for the resolved file `file` at `path`.
    ///
    /// `path` is taken to lie in the directory this was built for: only
    /// its file name is planned against, and that directory is what a
    /// returned target is joined to.
    ///
    /// A record that renders no usable name is
    /// [`PlannedRename::Unnameable`] and never reaches the planner — it
    /// claims no name, so it cannot cost a later file its unsuffixed one.
    ///
    /// `lookups` supplies the tables the name's template consults and
    /// collects the misses consulting them produced.
    pub fn plan(
        &mut self,
        path: &Path,
        file: &FileRecord,
        lookups: &mut Lookups<'_>,
    ) -> PlannedRename {
        let path = path.to_path_buf();
        let Some(input) = plan_input(&path, file, self.templates, lookups) else {
            return PlannedRename::Unnameable { path };
        };
        self.reach(&input.target);

        match self.planner.plan(&input, self.policy).action {
            PlannedAction::Rename { to } => PlannedRename::Rename {
                target: self.directory.join(to),
                path,
            },
            PlannedAction::AlreadyNamed => PlannedRename::AlreadyNamed { path },
            PlannedAction::Skip { .. } => PlannedRename::TargetTaken {
                target: self.directory.join(&input.target),
                path,
            },
        }
    }

    /// Add the subdirectory `target` names to the planner's namespace,
    /// unless `target` names none or it is there already.
    ///
    /// Keys are relative to the directory — `sub/Name.pdf` — which is
    /// the namespace [`borax_core::rename::Planner`] compares in and
    /// suffixes within.
    fn reach(&mut self, target: &str) {
        let Some((subdirectory, _)) = target.rsplit_once('/') else {
            return;
        };
        if !self.listed.insert(subdirectory.to_string()) {
            return;
        }
        self.planner.widen(
            self.filesystem
                .existing(&self.directory.join(subdirectory))
                .into_iter()
                .map(|(name, hash)| (format!("{subdirectory}/{name}"), hash))
                .collect(),
        );
    }
}

/// What the planner needs to know about one resolved file, or `None`
/// when the file has no name to plan from.
fn plan_input(
    path: &Path,
    file: &FileRecord,
    templates: &TemplateTable,
    lookups: &mut Lookups<'_>,
) -> Option<PlanInput> {
    Some(PlanInput {
        source: path.file_name()?.to_string_lossy().into_owned(),
        target: target_name(path, &file.record, file.hash.as_ref(), templates, lookups)?,
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
/// receives is deterministic.
///
/// A group is a [`Planning`] driven across its files, so a batch and a
/// caller feeding one file at a time decide the same way by construction.
/// Everything a decision rests on — how a name is rendered, when a
/// collision is suffixed, which subdirectories are looked at — is stated
/// there.
///
/// `lookups` supplies the tables the names' templates consult and
/// collects the misses consulting them produced.
pub fn plan_renames(
    resolved: &[(PathBuf, FileRecord)],
    templates: &TemplateTable,
    policy: CollisionPolicy,
    filesystem: &dyn Filesystem,
    lookups: &mut Lookups<'_>,
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
        let mut planning = Planning::new(directory, templates, policy, filesystem);
        for index in members {
            let (path, file) = &resolved[index];
            decisions[index] = Some(planning.plan(path, file, lookups));
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
///
/// `hashes` pairs with `plan` by position and carries the content hash
/// of the file each decision is about. A decision with no hash is not
/// applied ([`Applying::carry_out`]); a position `hashes` does not
/// reach counts as having none.
///
/// An [`Applying`] driven across `plan`, so a caller feeding one
/// decision at a time acts the same way.
pub fn apply_renames(
    plan: &[PlannedRename],
    filesystem: &dyn Filesystem,
    apply: bool,
    hashes: &[Option<ContentHash>],
) -> Vec<Event> {
    let mut applying = Applying::new(filesystem, apply);
    plan.iter()
        .enumerate()
        .map(|(position, decision)| {
            applying.carry_out(decision, hashes.get(position).cloned().flatten())
        })
        .collect()
}

/// Carrying out a plan, a decision at a time.
///
/// A caller feeding decisions in one at a time and one handing over the
/// whole plan ([`apply_renames`]) get the same treatment of each, since
/// nothing a decision does carries over to the next.
pub struct Applying<'a> {
    filesystem: &'a dyn Filesystem,
    apply: bool,
}

impl<'a> Applying<'a> {
    /// A run over `filesystem` that moves files when `apply` is set and
    /// otherwise only says what it would move.
    pub fn new(filesystem: &'a dyn Filesystem, apply: bool) -> Applying<'a> {
        Applying { filesystem, apply }
    }

    /// Carry out `decision`, or report what it would do, and say what
    /// happened. `hash` is the content hash of the file it is about.
    ///
    /// [`PlannedRename::Rename`] is [`Event::Planned`] while previewing
    /// and [`Event::Renamed`] once carried out; a move the filesystem
    /// refuses is [`crate::event::SkipReason::RenameFailed`] and the run
    /// goes on, because one file that cannot be moved says nothing about
    /// the next. Every variant that moves nothing reports the same way in
    /// both modes, since a preview has nothing to add about a file
    /// nothing will happen to.
    ///
    /// An applying rename of a file whose `hash` is `None` is not made,
    /// and reports [`crate::event::SkipReason::Unrecordable`]: the hash
    /// is what identifies the file a recorded move was about, so a line
    /// written without one would name two paths and nothing that ties
    /// them to a file. Moving anyway would leave the collection with a
    /// rename its own log cannot account for. The batch goes on — one
    /// file borax cannot describe says nothing about the next. A
    /// preview never looks at `hash`, having no move to describe.
    pub fn carry_out(&mut self, decision: &PlannedRename, hash: Option<ContentHash>) -> Event {
        match decision {
            PlannedRename::Rename { path, target } if self.apply => {
                let Some(hash) = hash else {
                    return Event::Skipped {
                        path: path.clone(),
                        reason: SkipReason::Unrecordable {
                            message: "the file's content hash is unknown".to_string(),
                        },
                    };
                };
                match self.filesystem.rename(path, target) {
                    Ok(()) => Event::Renamed {
                        path: path.clone(),
                        target: target.clone(),
                        hash,
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
        }
    }
}

/// The totals `events` add up to.
///
/// A fold over [`Counts::observe`], which is also what a run streaming
/// its events counts them with: a caller holding the whole stream as a
/// value and a caller watching it go past reach the same totals because
/// they add them up the same way.
///
/// Counts what happened, not what was planned: a preview run renames
/// nothing and reports `renamed` as zero however many moves it
/// described.
pub fn counts_for(events: &[Event]) -> Counts {
    let mut counts = Counts::default();
    for event in events {
        counts.observe(event);
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

        // A template may put a file in a subdirectory of the one it is
        // in, and the subdirectory is borax's to create: a name is not
        // a place until there is somewhere to put it. Creating it is
        // additive — an existing directory is left as it is, and no file
        // is touched — so it is safe to do before the link that is the
        // step which must not overwrite anything.
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(failed)?;
        }

        fs::hard_link(from, to).map_err(failed)?;
        // An unlink that fails leaves the file under both names rather
        // than backing the link out: removing `to` would be the only link
        // left if `from` disappeared under us, and a duplicate costs a
        // second run where that would cost the file.
        fs::remove_file(from).map_err(failed)
    }
}
