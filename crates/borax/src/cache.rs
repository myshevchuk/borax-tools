//! Looking into the response cache, and emptying it.
//!
//! The cache is the reason a second run over the same library costs no
//! network at all, and the reason a wrong answer keeps being wrong
//! until someone clears it. `borax cache` exists for both halves: it
//! reports what is stored, and it removes it.
//!
//! Storage belongs to [`borax_sources::store`]; what lives here is the
//! counting and the events.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use borax_sources::store::FileCache;

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
    let mut stats = CacheStats {
        root: root.to_path_buf(),
        entries: 0,
        bytes: 0,
    };
    count(root, &mut stats)?;
    Ok(stats)
}

/// Add every regular file below `directory` to `stats`.
///
/// A directory that is not there is counted as empty, which is what
/// makes a missing root zero rather than a failure, and what keeps a
/// subdirectory removed while the walk runs from ending it.
///
/// Metadata is read without following symlinks, so a link is neither a
/// file nor a directory here: it is not a stored entry, and it cannot
/// send the walk round a loop.
fn count(directory: &Path, stats: &mut CacheStats) -> io::Result<()> {
    let listing = match fs::read_dir(directory) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };

    for entry in listing {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            count(&entry.path(), stats)?;
        } else if metadata.is_file() {
            stats.entries += 1;
            stats.bytes += metadata.len();
        }
    }
    Ok(())
}

/// Remove everything under `root`.
///
/// Returns what was removed, counted before the removal, so the run can
/// report what clearing cost. A `root` that does not exist clears
/// successfully and reports zero.
pub fn clear(root: &Path) -> io::Result<CacheStats> {
    let stats = inspect(root)?;
    FileCache::new(root).clear()?;
    Ok(stats)
}

/// The event reporting `stats` as the cache's current contents.
pub fn status_event(stats: &CacheStats) -> Event {
    Event::CacheStatus {
        root: stats.root.clone(),
        entries: stats.entries,
        bytes: stats.bytes,
    }
}

/// The event reporting `stats` as what clearing removed.
pub fn cleared_event(stats: &CacheStats) -> Event {
    Event::CacheCleared {
        root: stats.root.clone(),
        entries: stats.entries,
        bytes: stats.bytes,
    }
}
