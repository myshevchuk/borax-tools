//! The cache on disk: where it lives, how a key becomes a file, and
//! the index that recognises a file by its contents.
//!
//! [`crate::cache`] defines what a cache is, and [`IndexStore`] where
//! the content index's facts are kept; this module is the one adapter
//! that touches the filesystem. A read degrades to a miss and
//! a write to a reported [`CacheWrite::Failed`] rather than an error,
//! so a cache directory that is unreadable, full, or half-written costs
//! round-trips and never a run.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use borax_core::content::{ContentHash, Hasher};
use borax_core::identifier::Identifier;
use borax_core::index::{
    Fact, IndexEntry, extracted_from_json, extracted_to_json, operator_from_json, operator_to_json,
};
use borax_core::library::IdentificationEvidence;
use borax_core::record::Record;

use crate::cache::{Cache, CacheWrite};

/// The response cache's on-disk layout version, used as the last path
/// segment of the default cache root.
///
/// Entries are serialized [`Record`]s, so a change to that model makes
/// existing entries unreadable. Bumping this moves the cache to a fresh
/// directory instead, leaving the old one to be cleared.
pub const FORMAT_VERSION: &str = "v1";

/// The content index's on-disk layout version, used as the last path
/// segment of the default data root.
pub const INDEX_FORMAT_VERSION: &str = "v2";

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
    root_from(CANDIDATES, FORMAT_VERSION, lookup)
}

/// The first of `candidates` whose variable `lookup` gives a non-empty
/// absolute path, with its suffix, `borax` and `version` appended.
fn root_from(
    candidates: &[(&str, Option<&str>)],
    version: &str,
    lookup: impl Fn(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    let mut root = candidates.iter().find_map(|(name, suffix)| {
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
    root.push(version);
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

/// The borax data directory implied by `lookup`, which answers the way
/// [`std::env::var_os`] does: where the content index lives.
///
/// The candidates are tried in order and the first whose value is a
/// non-empty absolute path is taken: on Unix `XDG_DATA_HOME`, then
/// `HOME` (as `HOME/.local/share`); on Windows `APPDATA`, then
/// `XDG_DATA_HOME`. A variable that is unset, empty, or relative is
/// skipped rather than fatal.
///
/// The returned path ends in `borax/<INDEX_FORMAT_VERSION>` and is
/// neither created nor checked for existence.
///
/// Returns `None` when no candidate qualifies, which is the signal to
/// keep the index in memory for the run rather than to fail.
pub fn data_root(lookup: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    root_from(DATA_CANDIDATES, INDEX_FORMAT_VERSION, lookup)
}

/// The variables that may name a data directory, in the order they are
/// tried, each with what to append to its value.
#[cfg(not(windows))]
const DATA_CANDIDATES: &[(&str, Option<&str>)] =
    &[("XDG_DATA_HOME", None), ("HOME", Some(".local/share"))];

/// The variables that may name a data directory, in the order they are
/// tried, each with what to append to its value.
#[cfg(windows)]
const DATA_CANDIDATES: &[(&str, Option<&str>)] = &[("APPDATA", None), ("XDG_DATA_HOME", None)];

/// [`data_root`] applied to this process's environment.
pub fn default_data_root() -> Option<PathBuf> {
    data_root(|name| std::env::var_os(name))
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

/// The key the extracted fact of the entry for `hash` is kept under:
/// `content/<hash>/extracted`, the file `content/<hash>/extracted.json`
/// under a [`FileCache`]'s root.
pub fn extracted_key(hash: &ContentHash) -> String {
    format!("content/{hash}/extracted")
}

/// The key the operator fact of the entry for `hash` is kept under:
/// `content/<hash>/operator`, the file `content/<hash>/operator.json`
/// under a [`FileCache`]'s root.
pub fn operator_key(hash: &ContentHash) -> String {
    format!("content/{hash}/operator")
}

/// Where content-index facts are kept: text by key, each fact whole.
///
/// `Sync` for the reason [`Cache`] is: resolution shares one index
/// across a pool of threads.
pub trait IndexStore: Sync {
    /// The text of the fact under `key`: `None` when there is none, and
    /// `Some(Err(message))` when one is there and cannot be read.
    fn read_fact(&self, key: &str) -> Option<Result<String, String>>;

    /// Replace the fact under `key` whole with `text`, atomically, and
    /// say whether it was written.
    ///
    /// A failure is returned as [`CacheWrite::Failed`], never raised,
    /// and leaves the fact as it was.
    fn write_fact(&self, key: &str, text: &str) -> CacheWrite;
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

impl IndexStore for FileCache {
    /// The text of the file [`entry_path`] gives for `key`. A key it
    /// rejects, a file that cannot be read for any reason but its
    /// absence, and bytes that are not UTF-8 are `Some(Err(_))`.
    fn read_fact(&self, key: &str) -> Option<Result<String, String>> {
        let Some(path) = entry_path(&self.root, key) else {
            return Some(Err(format!("invalid index key {key:?}")));
        };
        match fs::read(&path) {
            Ok(bytes) => Some(
                String::from_utf8(bytes)
                    .map_err(|_| format!("{} is not UTF-8 text", path.display())),
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => Some(Err(format!("cannot read {}: {error}", path.display()))),
        }
    }

    /// Write `text` to the file [`entry_path`] gives for `key` through
    /// [`write_atomically`], creating the entry's directory. A key it
    /// rejects and every write failure are [`CacheWrite::Failed`].
    fn write_fact(&self, key: &str, text: &str) -> CacheWrite {
        let Some(path) = entry_path(&self.root, key) else {
            return CacheWrite::Failed {
                message: format!("invalid index key {key:?}"),
            };
        };
        match write_atomically(&path, text.as_bytes()) {
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

/// What is known about files by the content hash of their bytes.
///
/// A file that has been resolved once is recognised under any name, as
/// long as its bytes are unchanged. Each hash's entry holds
/// identification evidence as two facts, read and written through an
/// [`IndexStore`]: the extracted identifier and the operator's
/// evidence, each replaced whole by its own writer and never merged
/// with the other. The index is persistent or in-memory exactly as the
/// store it is given is.
///
/// Over a [`Cache`], the index also keeps a record per hash under
/// [`content_key`].
#[derive(Debug)]
pub struct ContentIndex<S> {
    store: S,
}

impl<S> ContentIndex<S> {
    /// An index keeping its entries in `store`.
    pub fn new(store: S) -> ContentIndex<S> {
        ContentIndex { store }
    }
}

impl<S: Cache> ContentIndex<S> {
    /// The record kept for `hash`, if any.
    pub fn get(&self, hash: &ContentHash) -> Option<Record> {
        self.store.get(&content_key(hash))
    }

    /// Keep `record` for any file hashing to `hash`, and return what the
    /// underlying cache's write returned.
    pub fn put(&self, hash: &ContentHash, record: &Record) -> CacheWrite {
        self.store.put(&content_key(hash), record)
    }
}

impl<S: IndexStore> ContentIndex<S> {
    /// Both facts of `hash`'s entry, each parsed. A fact that cannot be
    /// read, or whose text does not parse, is [`Fact::Unreadable`].
    pub fn entry(&self, hash: &ContentHash) -> IndexEntry {
        IndexEntry {
            extracted: self.fact(&extracted_key(hash), extracted_from_json),
            operator: self.fact(&operator_key(hash), operator_from_json),
        }
    }

    /// Replace `hash`'s extracted fact with `identifier`, leaving its
    /// operator fact as it is, and return what the store's write
    /// returned.
    pub fn put_extracted(&self, hash: &ContentHash, identifier: &Identifier) -> CacheWrite {
        self.store
            .write_fact(&extracted_key(hash), &extracted_to_json(identifier))
    }

    /// Replace `hash`'s operator fact with `evidence`, leaving its
    /// extracted fact as it is, and return what the store's write
    /// returned.
    ///
    /// Evidence whose origin is not an operator's (`supplied` or
    /// `overridden`) is [`CacheWrite::Failed`], and nothing is written.
    pub fn put_operator(
        &self,
        hash: &ContentHash,
        evidence: &IdentificationEvidence,
    ) -> CacheWrite {
        if !evidence.origin.is_operator() {
            return CacheWrite::Failed {
                message: format!(
                    "evidence of origin {} is not an operator's",
                    evidence.origin
                ),
            };
        }
        self.store
            .write_fact(&operator_key(hash), &operator_to_json(evidence))
    }

    /// The fact under `key`, parsed by `parse`.
    fn fact<T, E: std::fmt::Display>(
        &self,
        key: &str,
        parse: impl Fn(&str) -> Result<T, E>,
    ) -> Fact<T> {
        match self.store.read_fact(key) {
            None => Fact::Absent,
            Some(Err(message)) => Fact::Unreadable(message),
            Some(Ok(text)) => match parse(&text) {
                Ok(value) => Fact::Held(value),
                Err(error) => Fact::Unreadable(error.to_string()),
            },
        }
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
