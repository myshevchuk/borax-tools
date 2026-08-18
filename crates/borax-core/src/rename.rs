//! Rename planning: pure decisions about which file goes where.
//!
//! The planner sees a snapshot of the relevant filesystem facts (what
//! exists in the target namespace, with content hashes where known)
//! plus the batch of desired renames, and produces one decision per
//! input, in input order, deterministically. It performs no I/O; the
//! CLI layer gathers the snapshot and executes the plan.
//!
//! Paths here are the `/`-separated relative strings the template
//! engine renders (already sanitized). Collision comparison is
//! case-insensitive on every platform — via Unicode lowercasing, an
//! approximation of Windows/macOS filesystem folding — so a plan made
//! on Linux stays valid after a sync to a case-insensitive filesystem.

use std::collections::{BTreeMap, BTreeSet};

use crate::bib_output::letter_suffix;

/// What to do when a desired target is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionPolicy {
    /// Append the first free deterministic letter suffix to the stem:
    /// `smith2024.pdf` → `smith2024a.pdf`, then `b` … `z`, `aa`, `ab`, …
    Suffix,
    /// Leave the file alone and report the collision.
    Skip,
}

/// Why a file was left unplanned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The desired target (or every candidate the policy allows) is
    /// taken by an existing file or an earlier item in the batch.
    TargetCollision,
}

/// The planner's decision for one input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedAction {
    /// Rename the file to `to` (the desired target, possibly with a
    /// collision suffix).
    Rename { to: String },
    /// Nothing to do: the file already sits at the desired target, or
    /// a byte-identical file (same content hash) already occupies it.
    AlreadyNamed,
    /// Leave the file untouched, for the stated reason.
    Skip { reason: SkipReason },
}

/// One file the batch wants to rename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanInput {
    /// The file's current path in the target namespace (used to exempt
    /// its own slot from collision checks and to report the mapping).
    pub source: String,
    /// The desired target path, as rendered and sanitized.
    pub target: String,
    /// Hash of the file's content (for already-named detection).
    pub content_hash: String,
}

/// One decision of the plan, paired with its input's source path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    pub source: String,
    pub action: PlannedAction,
}

/// A plan in progress, driven one input at a time.
///
/// Holds what a batch call holds between its items: the filesystem
/// snapshot every decision is measured against, and the set of paths
/// already claimed — by a file that was there to begin with, or by an
/// earlier input's target. Carrying claims across calls is what lets a
/// caller interleave planning with other work and still get the plan
/// [`plan`] would have produced for the same inputs in the same order.
///
/// Decisions depend on the snapshot, the policy, and the order inputs
/// arrive. Nothing else: a planner given the same sequence twice answers
/// the same way twice.
#[derive(Debug, Clone)]
pub struct Planner {
    /// The namespace as it was found, path to content hash where known.
    existing: BTreeMap<String, Option<String>>,
    /// Every path spoken for, lowercased: the snapshot's own paths, plus
    /// the target each planned rename took.
    claimed: BTreeSet<String>,
}

impl Planner {
    /// A planner over `existing` — the paths already present in the
    /// namespace inputs will be planned in, each with its content hash
    /// where that is known, keyed as [`plan`] keys them.
    ///
    /// Every path in `existing` starts out claimed.
    pub fn new(existing: BTreeMap<String, Option<String>>) -> Self {
        let _ = existing;
        todo!()
    }

    /// The decision for `item` under `policy`, claiming whatever name it
    /// takes.
    ///
    /// [`PlannedAction::Rename`] claims the path it names — the desired
    /// target, or the suffixed candidate the ladder reached — for every
    /// later call. [`PlannedAction::AlreadyNamed`] and
    /// [`PlannedAction::Skip`] claim nothing the snapshot did not
    /// already hold.
    ///
    /// `item`'s own `source` is exempt from the collision check for the
    /// duration of this call, which is what lets a file change only the
    /// case of its name. The exemption is never extended to a later
    /// input: a source is not treated as vacated by the rename that
    /// leaves it, so no ordering of the executed moves can overwrite
    /// anything.
    pub fn plan(&mut self, item: &PlanInput, policy: CollisionPolicy) -> PlanItem {
        let _ = (item, policy);
        todo!()
    }
}

/// Plan a batch of renames against a filesystem snapshot.
///
/// `existing` maps each path already present in the target namespace
/// to its content hash where known (`None` = unknown; unknown never
/// counts as identical). Decisions, in input order:
///
/// - `AlreadyNamed` when the item's source equals its target
///   (byte-equal), or when `existing` holds the target
///   (case-insensitively) with a hash equal to the item's. The item's
///   own `source` entry is exempt here as everywhere: a case-only
///   rename of a file onto its new casing is a `Rename`, not
///   `AlreadyNamed`.
/// - Otherwise the target is free iff no `existing` path and no
///   target claimed by an earlier item equals it case-insensitively.
///   The item's own `source` entry in `existing` is exempt for that
///   item (a file may change only its casing); sources are otherwise
///   *not* treated as vacated — a later item never moves into a slot
///   an earlier rename frees, so no ordering of the executed renames
///   can overwrite anything.
/// - A taken target follows `policy`: `Suffix` tries stem+`a`, `b`,
///   … `z`, `aa`, … (before the extension, the suffix ladder of the
///   bib merge) and claims the first free candidate — identical
///   content elsewhere never short-circuits suffixing, because two
///   distinct files both need names. `Skip` yields
///   `Skip { TargetCollision }`.
///
/// Planned targets (including suffixed ones) are claimed
/// case-insensitively for the rest of the batch. The function is
/// pure and deterministic.
pub fn plan(
    items: &[PlanInput],
    existing: &BTreeMap<String, Option<String>>,
    policy: CollisionPolicy,
) -> Vec<PlanItem> {
    let mut claimed: BTreeSet<String> = existing.keys().map(|path| path.to_lowercase()).collect();
    let mut planned = Vec::with_capacity(items.len());

    for item in items {
        let source_key = item.source.to_lowercase();
        let exempt = existing
            .keys()
            .any(|path| path.to_lowercase() == source_key)
            .then_some(source_key.as_str());
        let target_key = item.target.to_lowercase();

        let action = if item.source == item.target || occupied_by_twin(item, existing, exempt) {
            PlannedAction::AlreadyNamed
        } else if is_free(&target_key, &claimed, exempt) {
            claimed.insert(target_key);
            PlannedAction::Rename {
                to: item.target.clone(),
            }
        } else {
            match policy {
                CollisionPolicy::Suffix => {
                    let mut index = 0;
                    loop {
                        let candidate = with_suffix(&item.target, &letter_suffix(index));
                        let candidate_key = candidate.to_lowercase();
                        if is_free(&candidate_key, &claimed, exempt) {
                            claimed.insert(candidate_key);
                            break PlannedAction::Rename { to: candidate };
                        }
                        index += 1;
                    }
                }
                CollisionPolicy::Skip => PlannedAction::Skip {
                    reason: SkipReason::TargetCollision,
                },
            }
        };

        planned.push(PlanItem {
            source: item.source.clone(),
            action,
        });
    }

    planned
}

/// Whether an existing file other than `item`'s own source sits at its
/// target with the same content.
fn occupied_by_twin(
    item: &PlanInput,
    existing: &BTreeMap<String, Option<String>>,
    exempt: Option<&str>,
) -> bool {
    let target_key = item.target.to_lowercase();
    existing.iter().any(|(path, hash)| {
        let path_key = path.to_lowercase();
        path_key == target_key
            && Some(path_key.as_str()) != exempt
            && hash.as_deref() == Some(item.content_hash.as_str())
    })
}

/// Whether `candidate_key` (an already-lowercased path) is unclaimed,
/// counting the planning item's own slot as free.
fn is_free(candidate_key: &str, claimed: &BTreeSet<String>, exempt: Option<&str>) -> bool {
    !claimed.contains(candidate_key) || exempt == Some(candidate_key)
}

/// Insert `suffix` before `path`'s extension — the last dot of the final
/// `/`-separated component, ignoring a leading dot. A component without
/// one takes the suffix at its end.
fn with_suffix(path: &str, suffix: &str) -> String {
    let component = path.rfind('/').map_or(0, |slash| slash + 1);
    let split = path[component..]
        .rfind('.')
        .filter(|dot| *dot > 0)
        .map_or(path.len(), |dot| component + dot);
    format!("{}{}{}", &path[..split], suffix, &path[split..])
}
