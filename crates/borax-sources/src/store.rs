//! The cache on disk: where it lives, how a key becomes a file, and
//! the index that recognises a file by its contents.
//!
//! [`crate::cache`] defines what a cache is; this module is the one
//! adapter that touches the filesystem. Every operation degrades to a
//! miss rather than an error, so a cache directory that is unreadable,
//! full, or half-written costs round-trips and never a run.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use borax_core::content::{ContentHash, Hasher};
use borax_core::record::Record;

use crate::cache::Cache;

/// The on-disk layout version, used as the last path segment of the
/// default root.
///
/// Entries are serialized [`Record`]s, so a change to that model makes
/// existing entries unreadable. Bumping this moves the cache to a fresh
/// directory instead, leaving the old one to be cleared.
pub const FORMAT_VERSION: &str = "v1";

/// The borax cache directory implied by `lookup`, which answers the way
/// [`std::env::var_os`] does.
///
/// The candidates are tried in order and the first whose value is a
/// non-empty absolute path is taken: on Unix `XDG_CACHE_HOME`, then
/// `HOME` (as `HOME/.cache`); on Windows `LOCALAPPDATA`, then
/// `XDG_CACHE_HOME`. A variable that is unset, empty, or relative is
/// skipped rather than fatal.
///
/// The returned path ends in `borax/<FORMAT_VERSION>` and is neither
/// created nor checked for existence.
///
/// Returns `None` when no candidate qualifies, which is the signal to
/// run without persistence rather than to fail.
pub fn cache_root(lookup: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
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

/// The variables that may name a cache directory, in the order they are
/// tried, each with what to append to its value.
#[cfg(not(windows))]
const CANDIDATES: &[(&str, Option<&str>)] = &[("XDG_CACHE_HOME", None), ("HOME", Some(".cache"))];

/// The variables that may name a cache directory, in the order they are
/// tried, each with what to append to its value.
#[cfg(windows)]
const CANDIDATES: &[(&str, Option<&str>)] = &[("LOCALAPPDATA", None), ("XDG_CACHE_HOME", None)];

/// [`cache_root`] applied to this process's environment.
pub fn default_cache_root() -> Option<PathBuf> {
    cache_root(|name| std::env::var_os(name))
}

/// Where the entry for `key` lives under `root`.
///
/// The key is used as a relative path with `.json` appended, so
/// `crossref/doi/10.1038-171737a0` becomes
/// `<root>/crossref/doi/10.1038-171737a0.json`.
///
/// Returns `None` for any key that is not a plain relative path of
/// `a-z`, `0-9`, `.`, `-`, `_` segments separated by `/`: an empty key,
/// an empty, `.` or `..` segment, a leading or trailing `/`, a
/// backslash, or any other character. A key therefore cannot address a
/// file outside `root`, whatever produced it.
pub fn entry_path(root: &Path, key: &str) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    let mut segments = key.split('/').peekable();

    while let Some(segment) = segments.next() {
        if !is_safe_segment(segment) {
            return None;
        }

        match segments.peek() {
            Some(_) => path.push(segment),
            None => path.push(format!("{segment}.json")),
        }
    }

    Some(path)
}

/// Whether `segment` names one directory or file below the root and
/// nothing else.
fn is_safe_segment(segment: &str) -> bool {
    !matches!(segment, "" | "." | "..")
        && segment
            .chars()
            .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '.' | '-' | '_'))
}

/// The key a record resolved from a file with hash `hash` is stored
/// under.
///
/// Formed as `content/<hash>`, which shares the store with the
/// identifier-keyed entries without colliding: no source is named
/// `content`.
pub fn content_key(hash: &ContentHash) -> String {
    format!("content/{hash}")
}

/// A [`Cache`] backed by a directory of JSON files.
///
/// The directory is created lazily, on the first successful write. A
/// [`FileCache`] over a path that cannot be created is a cache that
/// always misses.
#[derive(Debug, Clone)]
pub struct FileCache {
    root: PathBuf,
}

impl FileCache {
    /// A cache storing entries under `root`.
    pub fn new(root: impl Into<PathBuf>) -> FileCache {
        FileCache { root: root.into() }
    }

    /// A cache under [`default_cache_root`], or `None` when the
    /// environment names no cache directory.
    pub fn open_default() -> Option<FileCache> {
        default_cache_root().map(FileCache::new)
    }

    /// The directory entries are stored under.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Remove the root directory and everything in it.
    ///
    /// Succeeds when the directory is already absent. Errors are
    /// returned rather than swallowed: a `borax cache clear` that could
    /// not clear anything has to say so.
    pub fn clear(&self) -> io::Result<()> {
        match fs::remove_dir_all(&self.root) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

impl Cache for FileCache {
    /// The record stored under `key`, or `None` when the entry is
    /// absent, unreadable, holds a key that is not a valid path, or
    /// does not parse as a record.
    fn get(&self, key: &str) -> Option<Record> {
        let bytes = fs::read(entry_path(&self.root, key)?).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Store `record` under `key`, creating the parent directories.
    ///
    /// The entry is written to a temporary file in its destination
    /// directory and renamed over the final path, so a concurrent
    /// reader sees either the previous entry or the new one and never a
    /// partial file. Every failure — an invalid key, a directory that
    /// cannot be created, a full disk — leaves the cache as it was.
    fn put(&self, key: &str, record: &Record) {
        let Some(path) = entry_path(&self.root, key) else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        let Ok(json) = serde_json::to_vec(record) else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }

        let temporary = parent.join(temporary_name());
        if fs::write(&temporary, &json).is_ok() && fs::rename(&temporary, &path).is_ok() {
            return;
        }

        // A temporary that never reached its final name is litter, and
        // nothing else will come looking for it.
        let _ = fs::remove_file(&temporary);
    }
}

/// How many temporary entry files this process has named so far.
static TEMPORARIES: AtomicU64 = AtomicU64::new(0);

/// A file name no concurrent writer will choose: another process
/// differs in its id, another thread in its draw from the counter.
fn temporary_name() -> String {
    format!(
        ".tmp-{}-{}",
        std::process::id(),
        TEMPORARIES.fetch_add(1, Ordering::Relaxed)
    )
}

/// Records indexed by the content hash of the file they were resolved
/// from.
///
/// A file that has been resolved once is answered for under any name,
/// as long as its bytes are unchanged — which is what lets a second run
/// over a renamed file skip both extraction and the network. Wrapping a
/// [`Cache`] rather than owning storage means the index is persistent
/// or in-memory exactly as the cache it is given is.
#[derive(Debug)]
pub struct ContentIndex<C> {
    cache: C,
}

impl<C: Cache> ContentIndex<C> {
    /// An index storing its entries in `cache`.
    pub fn new(cache: C) -> ContentIndex<C> {
        ContentIndex { cache }
    }

    /// The record indexed for `hash`, if any.
    pub fn get(&self, hash: &ContentHash) -> Option<Record> {
        self.cache.get(&content_key(hash))
    }

    /// Index `record` as the answer for any file hashing to `hash`.
    pub fn put(&self, hash: &ContentHash, record: &Record) {
        self.cache.put(&content_key(hash), record);
    }
}

/// The [`ContentHash`] of the file at `path`.
///
/// The file is read in chunks, so hashing costs constant memory
/// whatever its size. Returns the underlying [`io::Error`] when the
/// file cannot be opened or read.
pub fn hash_file(path: &Path) -> io::Result<ContentHash> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; 64 * 1024];

    loop {
        match file.read(&mut buffer)? {
            0 => return Ok(hasher.finish()),
            read => hasher.update(&buffer[..read]),
        }
    }
}
