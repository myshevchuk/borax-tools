//! Remembering what a service already told us.
//!
//! The cache is an accounting convenience, never an authority: losing
//! it costs network round-trips and nothing else. It is what makes
//! re-running borax over a directory nearly free, which the spec
//! requires — and what keeps a re-run from hammering services that are
//! doing borax a favour by answering at all.

use std::collections::BTreeMap;
use std::sync::Mutex;

use borax_core::identifier::Identifier;
use borax_core::record::Record;

use crate::source::{Source, SourceError, SourceName};

/// The filename-safe key a source's answer about an identifier is
/// stored under.
///
/// Formed as `<source>/<kind>/<slug>` where `slug` is the identifier's
/// string form with every character outside `a-z`, `0-9`, `.`, `-` and
/// `_` replaced by `-`, lowercased. Two different identifiers can
/// therefore never share a key by accident of punctuation, and the key
/// is safe as a path on every platform.
///
/// The source is part of the key because sources disagree: Crossref's
/// answer about a DOI is not OpenAlex's, and a cache that conflated
/// them would make fallback results sticky.
pub fn key(source: SourceName, identifier: &Identifier) -> String {
    let _ = (source, identifier);
    todo!("derive a cache key")
}

/// A store of records that a source previously returned.
///
/// Implementations are adapters over a directory; [`MemoryCache`] is
/// the in-process one used by tests and by runs that disable
/// persistence.
pub trait Cache {
    /// The record stored under `key`, if any. A failure to read is
    /// reported as a miss: a broken cache must never fail a run.
    fn get(&self, key: &str) -> Option<Record>;

    /// Store `record` under `key`. Failures are silent for the same
    /// reason.
    fn put(&self, key: &str, record: &Record);
}

/// A [`Cache`] held in memory for the life of the process.
#[derive(Debug, Default)]
pub struct MemoryCache {
    entries: Mutex<BTreeMap<String, Record>>,
}

impl MemoryCache {
    /// An empty cache.
    pub fn new() -> MemoryCache {
        MemoryCache::default()
    }

    /// How many records are held.
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .map(|entries| entries.len())
            .unwrap_or(0)
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Cache for MemoryCache {
    fn get(&self, key: &str) -> Option<Record> {
        self.entries.lock().ok()?.get(key).cloned()
    }

    fn put(&self, key: &str, record: &Record) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(key.to_string(), record.clone());
        }
    }
}

/// A [`Source`] that answers from a [`Cache`] before asking the source
/// it wraps.
///
/// Wrapping rather than building caching into each client keeps the
/// clients ignorant of it and makes "was the network touched?" a
/// property tests can assert directly.
#[derive(Debug)]
pub struct Cached<S, C> {
    source: S,
    cache: C,
}

impl<S, C> Cached<S, C> {
    /// Wrap `source` so its answers are served from, and stored in,
    /// `cache`.
    pub fn new(source: S, cache: C) -> Cached<S, C> {
        Cached { source, cache }
    }
}

impl<S: Source, C: Cache> Source for Cached<S, C> {
    fn name(&self) -> SourceName {
        self.source.name()
    }

    fn supports(&self, identifier: &Identifier) -> bool {
        self.source.supports(identifier)
    }

    /// A cache hit is returned without consulting the wrapped source.
    /// On a miss the source is asked, and a successful answer is
    /// stored; failures are never cached, so a service that was down
    /// is asked again next time rather than remembered as broken.
    fn fetch(&self, identifier: &Identifier) -> Result<Record, SourceError> {
        let _ = (identifier, &self.source, &self.cache);
        todo!("serve from cache, else fetch and store")
    }
}
