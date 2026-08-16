//! Looking into the response cache, and emptying it.
//!
//! The cache is the reason a second run over the same library costs no
//! network at all, and the reason a wrong answer keeps being wrong
//! until someone clears it. `borax cache` exists for both halves: it
//! reports what is stored, and it removes it.
//!
//! Storage belongs to [`borax_sources::store`]; what lives here is the
//! counting and the events.

use std::io;
use std::path::{Path, PathBuf};

use crate::event::Event;

/// What the cache holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheStats {
    /// The directory the entries were counted under, reported whether
    /// or not it exists.
    pub root: PathBuf,
    /// How many cache entries are stored.
    pub entries: usize,
    /// What those entries occupy on disk, in bytes.
    pub bytes: u64,
}

/// Count the entries stored under `root`.
///
/// Every regular file below `root`, at any depth, is one entry: the
/// store spreads entries over subdirectories, and how deeply is its
/// business rather than this count's.
///
/// A `root` that does not exist is not a failure — it reports zero
/// entries, which is what an unused cache holds. Other I/O failures
/// are returned, because a `borax cache` that could not read the cache
/// must not report it as empty.
pub fn inspect(root: &Path) -> io::Result<CacheStats> {
    todo!("walk the tree and total the regular files")
}

/// Remove everything under `root`.
///
/// Returns what was removed, counted before the removal, so the run can
/// report what clearing cost. A `root` that does not exist clears
/// successfully and reports zero.
pub fn clear(root: &Path) -> io::Result<CacheStats> {
    todo!("count, then remove the tree")
}

/// The event reporting `stats` as the cache's current contents.
pub fn status_event(stats: &CacheStats) -> Event {
    todo!("render as CacheStatus")
}

/// The event reporting `stats` as what clearing removed.
pub fn cleared_event(stats: &CacheStats) -> Event {
    todo!("render as CacheCleared")
}
