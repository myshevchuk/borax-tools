//! The cache on disk: where it lives, how a key becomes a file, and
//! the index that recognises a file by its contents.
//!
//! [`crate::cache`] defines what a cache is; this module is the one
//! adapter that touches the filesystem. A read degrades to a miss and
//! a write to a reported [`CacheWrite::Failed`] rather than an error,
//! so a cache directory that is unreadable, full, or half-written costs
//! round-trips and never a run.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use borax_core::content::{ContentHash, Hasher};
use borax_core::record::Record;

use crate::cache::{Cache, CacheWrite};

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
/// always misses, and whose every write reports
/// [`CacheWrite::Failed`].
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
    /// partial file.
    ///
    /// Returns [`CacheWrite::Written`] once the entry is in place.
    /// Every failure — a key [`entry_path`] rejects, a record that does
    /// not serialise, a directory that cannot be created, a full disk —
    /// is [`CacheWrite::Failed`] with a message naming it, and leaves
    /// the cache as it was.
    fn put(&self, key: &str, record: &Record) -> CacheWrite {
        let Some(path) = entry_path(&self.root, key) else {
            return CacheWrite::Failed {
                message: format!("invalid cache key {key:?}"),
            };
        };
        let json = match serde_json::to_vec(record) {
            Ok(json) => json,
            Err(error) => {
                return CacheWrite::Failed {
                    message: format!("cannot serialise record: {error}"),
                };
            }
        };
        match write_atomically(&path, &json) {
            Ok(()) => CacheWrite::Written,
            Err(error) => CacheWrite::Failed {
                message: format!("cannot write {}: {error}", path.display()),
            },
        }
    }
}

/// Write `content` to `path`, replacing whatever was there, without
/// ever leaving a partial file behind.
///
/// The bytes go to a temporary file in `path`'s own directory and are
/// renamed over it, which on every supported platform replaces the
/// destination in one step. A reader therefore sees either the previous
/// content or the new content, and a process killed mid-write leaves
/// the previous content intact — the guarantee a plain
/// [`std::fs::write`] does not give, since it truncates first.
///
/// Missing parent directories are created. A failure at any stage
/// removes the temporary and leaves `path` as it was; the error
/// reported is the one that stopped the write.
///
/// The temporary shares a directory with the destination so the rename
/// stays within one filesystem, where it is atomic.
pub fn write_atomically(path: &Path, content: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;

    let temporary = parent.join(temporary_name());
    let written = fs::write(&temporary, content).and_then(|()| fs::rename(&temporary, path));
    if written.is_err() {
        // A temporary that never reached its final name is litter, and
        // nothing else will come looking for it.
        let _ = fs::remove_file(&temporary);
    }
    written
}

/// Create `path` holding `content`, failing rather than replacing a
/// file already there.
///
/// Fails with [`io::ErrorKind::AlreadyExists`] when anything is at
/// `path`, which is left untouched. Missing parent directories are
/// created. The bytes go to a temporary file this call creates
/// exclusively in `path`'s directory, which is then hard-linked to
/// `path`; a link fails when its destination exists, so the file
/// appears whole and in one step, and never over another. Where the
/// filesystem cannot link, the file is created exclusively at `path`
/// and written in place, which still never replaces one but lets a
/// concurrent reader see it short until the write completes.
///
/// No temporary this call created is left behind, and no file it did
/// not create is opened for writing or removed. A failure to remove the
/// temporary after the link succeeded is ignored. When every one of 16
/// temporary names is already taken, the error is not `AlreadyExists`.
pub fn create_atomically(path: &Path, content: &[u8]) -> io::Result<()> {
    create_atomically_with(
        path,
        content,
        &|from, to| fs::hard_link(from, to),
        &temporary_name,
    )
}

/// How many temporary names [`create_atomically_with`] tries before it
/// gives up.
const TEMPORARY_ATTEMPTS: usize = 16;

/// [`create_atomically`], with `link` in place of
/// [`std::fs::hard_link`] and `name` in place of the generated
/// temporary name.
///
/// `name` is called once per temporary name tried, at most 16 times; a
/// name already taken is left alone and the next one tried. An error
/// from `link` other than [`io::ErrorKind::AlreadyExists`] is taken to
/// mean the filesystem cannot link, and the exclusive create is used
/// instead. `link`'s `AlreadyExists` is returned as it is.
pub fn create_atomically_with(
    path: &Path,
    content: &[u8],
    link: &dyn Fn(&Path, &Path) -> io::Result<()>,
    name: &dyn Fn() -> String,
) -> io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;

    let temporary = create_temporary(parent, content, name)?;
    let linked = link(&temporary, path);
    // The temporary was this call's own, and whatever the link did, it
    // is no longer needed under that name.
    let _ = fs::remove_file(&temporary);
    match linked {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(error),
        Err(_) => create_exclusively(path, content),
    }
}

/// A temporary in `directory` holding `content`, created exclusively
/// under the first name `name` gives that is not taken.
///
/// Fails with an error that is not `AlreadyExists` once
/// [`TEMPORARY_ATTEMPTS`] names have all been taken, and with the
/// filesystem's error for any other failure to create or write it. A
/// temporary whose write failed is removed.
fn create_temporary(
    directory: &Path,
    content: &[u8],
    name: &dyn Fn() -> String,
) -> io::Result<PathBuf> {
    for _ in 0..TEMPORARY_ATTEMPTS {
        let temporary = directory.join(name());
        match create_exclusively(&temporary, content) {
            Ok(()) => return Ok(temporary),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other(format!(
        "no free temporary name in {} after {TEMPORARY_ATTEMPTS} attempts",
        directory.display()
    )))
}

/// Create `path` exclusively and write `content` to it.
///
/// Fails with `AlreadyExists`, touching nothing, when anything is at
/// `path`. A file this call created and could not finish writing is
/// removed before the error is returned.
fn create_exclusively(path: &Path, content: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(content).inspect_err(|_| {
        let _ = fs::remove_file(path);
    })
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

    /// Index `record` as the answer for any file hashing to `hash`, and
    /// return what the underlying cache's write returned.
    pub fn put(&self, hash: &ContentHash, record: &Record) -> CacheWrite {
        self.cache.put(&content_key(hash), record)
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
