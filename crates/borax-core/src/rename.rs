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

use std::collections::BTreeMap;

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
    let _ = (items, existing, policy);
    todo!("plan the batch")
}
