#![allow(clippy::unwrap_used)]

use std::cell::Cell;
use std::rc::Rc;

use borax_core::identifier::{ArxivId, Doi, Identifier, Pmid};
use borax_core::record::{EntryType, Record};
use borax_sources::cache::{Cache, Cached, MemoryCache, key};
use borax_sources::dispatch::resolve;
use borax_sources::source::{Source, SourceError, SourceName};

fn doi(value: &str) -> Identifier {
    Identifier::Doi(Doi::parse(value).unwrap())
}

fn arxiv(value: &str) -> Identifier {
    Identifier::Arxiv(ArxivId::parse(value).unwrap())
}

fn pmid(value: &str) -> Identifier {
    Identifier::Pmid(Pmid::parse(value).unwrap())
}

fn doi_identifier() -> Identifier {
    doi("10.1038/171737a0")
}

fn other_doi_identifier() -> Identifier {
    doi("10.1021/jacs.4c01234")
}

fn article_with_doi(value: &str) -> Record {
    Record {
        title: Some("On the Structure of Borax".to_string()),
        doi: Some(Doi::parse(value).unwrap()),
        ..Record::new(EntryType::Article)
    }
}

/// A [`Source`] whose name, support answer, and canned response are
/// fixed at construction. The call counter is shared through an `Rc`
/// so a test can still read it after the source has been moved into a
/// [`Cached`].
struct FakeSource {
    name: SourceName,
    supports: bool,
    response: Result<Record, SourceError>,
    calls: Rc<Cell<usize>>,
}

impl Source for FakeSource {
    fn name(&self) -> SourceName {
        self.name
    }

    fn supports(&self, _identifier: &Identifier) -> bool {
        self.supports
    }

    fn fetch(&self, _identifier: &Identifier) -> Result<Record, SourceError> {
        self.calls.set(self.calls.get() + 1);
        self.response.clone()
    }
}

/// A [`Cache`] borrowing a [`MemoryCache`] rather than owning it, so a
/// test can inspect the cache after handing it to a [`Cached`], which
/// otherwise takes ownership of whatever cache it wraps.
struct SharedCache<'a> {
    cache: &'a MemoryCache,
}

impl<'a> SharedCache<'a> {
    fn new(cache: &'a MemoryCache) -> Self {
        SharedCache { cache }
    }
}

impl Cache for SharedCache<'_> {
    fn get(&self, key: &str) -> Option<Record> {
        self.cache.get(key)
    }

    fn put(&self, key: &str, record: &Record) {
        self.cache.put(key, record);
    }
}

// --- key() ---

#[test]
fn key_is_deterministic() {
    let id = doi_identifier();
    assert_eq!(
        key(SourceName::Crossref, &id),
        key(SourceName::Crossref, &id)
    );
}

#[test]
fn key_includes_the_source() {
    let id = doi_identifier();
    assert_ne!(
        key(SourceName::Crossref, &id),
        key(SourceName::OpenAlex, &id)
    );
}

#[test]
fn key_differs_across_identifier_kinds() {
    let doi_key = key(SourceName::Crossref, &doi_identifier());
    let arxiv_key = key(SourceName::Crossref, &arxiv("1706.03762"));
    let pmid_key = key(SourceName::Crossref, &pmid("13054692"));

    assert_ne!(doi_key, arxiv_key);
    assert_ne!(doi_key, pmid_key);
    assert_ne!(arxiv_key, pmid_key);
}

#[test]
fn key_differs_across_doi_values() {
    let a = key(SourceName::Crossref, &doi_identifier());
    let b = key(SourceName::Crossref, &other_doi_identifier());
    assert_ne!(a, b);
}

#[test]
fn key_for_a_doi_is_filename_safe() {
    let k = key(SourceName::Crossref, &doi_identifier());

    assert!(k.chars().all(|c| c.is_ascii_lowercase()
        || c.is_ascii_digit()
        || matches!(c, '.' | '-' | '_' | '/')));
    assert!(!k.chars().any(|c| c.is_ascii_uppercase()));
    assert!(!k.contains(':'));
    assert!(!k.chars().any(char::is_whitespace));
}

#[test]
fn key_for_an_old_style_arxiv_id_has_no_embedded_slash_in_its_slug() {
    let k = key(SourceName::Arxiv, &arxiv("math.GT/0309136"));

    assert!(k.chars().all(|c| c.is_ascii_lowercase()
        || c.is_ascii_digit()
        || matches!(c, '.' | '-' | '_' | '/')));
    assert!(!k.chars().any(|c| c.is_ascii_uppercase()));
    assert!(!k.contains(':'));
    assert!(!k.chars().any(char::is_whitespace));

    let segments: Vec<&str> = k.split('/').collect();
    assert_eq!(
        segments.len(),
        3,
        "expected <source>/<kind>/<slug>, got {k:?}"
    );
}

#[test]
fn key_distinguishes_dois_that_differ_only_in_punctuation() {
    let dash = key(SourceName::Crossref, &doi("10.1000/a-b"));
    let dot = key(SourceName::Crossref, &doi("10.1000/a.b"));
    assert_ne!(dash, dot);
}

// --- Cache trait / MemoryCache ---

#[test]
fn memory_cache_new_is_empty() {
    let cache = MemoryCache::new();
    assert!(cache.is_empty());
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.get("k"), None);
}

#[test]
fn memory_cache_put_then_get_round_trips() {
    let cache = MemoryCache::new();
    let record = article_with_doi("10.1038/171737a0");

    cache.put("k", &record);

    assert_eq!(cache.get("k"), Some(record));
}

#[test]
fn memory_cache_put_twice_under_same_key_overwrites() {
    let cache = MemoryCache::new();
    let first = article_with_doi("10.1038/171737a0");
    let second = article_with_doi("10.1021/jacs.4c01234");

    cache.put("k", &first);
    cache.put("k", &second);

    assert_eq!(cache.get("k"), Some(second));
    assert_eq!(cache.len(), 1);
}

#[test]
fn memory_cache_distinct_keys_accumulate() {
    let cache = MemoryCache::new();
    cache.put("a", &article_with_doi("10.1038/171737a0"));
    cache.put("b", &article_with_doi("10.1021/jacs.4c01234"));

    assert_eq!(cache.len(), 2);
}

#[test]
fn memory_cache_get_for_absent_key_is_none() {
    let cache = MemoryCache::new();
    cache.put("a", &article_with_doi("10.1038/171737a0"));

    assert_eq!(cache.get("absent"), None);
}

// --- Cached<S, C> ---

#[test]
fn cached_fetch_miss_then_hit_calls_wrapped_source_once() {
    let memory = MemoryCache::new();
    let calls = Rc::new(Cell::new(0));
    let record = article_with_doi("10.1038/171737a0");
    let fake = FakeSource {
        name: SourceName::Crossref,
        supports: true,
        response: Ok(record.clone()),
        calls: calls.clone(),
    };
    let cached = Cached::new(fake, SharedCache::new(&memory));
    let id = doi_identifier();

    let first = cached.fetch(&id).unwrap();
    let second = cached.fetch(&id).unwrap();

    assert_eq!(first, record);
    assert_eq!(second, record);
    assert_eq!(calls.get(), 1);
}

#[test]
fn cached_fetch_populates_the_cache_on_success() {
    let memory = MemoryCache::new();
    let fake = FakeSource {
        name: SourceName::Crossref,
        supports: true,
        response: Ok(article_with_doi("10.1038/171737a0")),
        calls: Rc::new(Cell::new(0)),
    };
    let cached = Cached::new(fake, SharedCache::new(&memory));

    cached.fetch(&doi_identifier()).unwrap();

    assert_eq!(memory.len(), 1);
}

#[test]
fn cached_fetch_does_not_cache_unavailable_errors() {
    let memory = MemoryCache::new();
    let calls = Rc::new(Cell::new(0));
    let fake = FakeSource {
        name: SourceName::Crossref,
        supports: true,
        response: Err(SourceError::Unavailable {
            message: "503".to_string(),
        }),
        calls: calls.clone(),
    };
    let cached = Cached::new(fake, SharedCache::new(&memory));
    let id = doi_identifier();

    assert!(cached.fetch(&id).is_err());
    assert!(cached.fetch(&id).is_err());

    assert_eq!(calls.get(), 2);
    assert!(memory.is_empty());
}

#[test]
fn cached_fetch_does_not_cache_not_found_errors() {
    let memory = MemoryCache::new();
    let calls = Rc::new(Cell::new(0));
    let fake = FakeSource {
        name: SourceName::Crossref,
        supports: true,
        response: Err(SourceError::NotFound),
        calls: calls.clone(),
    };
    let cached = Cached::new(fake, SharedCache::new(&memory));
    let id = doi_identifier();

    assert!(cached.fetch(&id).is_err());
    assert!(cached.fetch(&id).is_err());

    assert_eq!(calls.get(), 2);
    assert!(memory.is_empty());
}

#[test]
fn cached_fetch_different_identifiers_do_not_share_an_entry() {
    let memory = MemoryCache::new();
    let calls = Rc::new(Cell::new(0));
    let fake = FakeSource {
        name: SourceName::Crossref,
        supports: true,
        response: Ok(article_with_doi("10.1038/171737a0")),
        calls: calls.clone(),
    };
    let cached = Cached::new(fake, SharedCache::new(&memory));

    cached.fetch(&doi_identifier()).unwrap();
    cached.fetch(&other_doi_identifier()).unwrap();

    assert_eq!(calls.get(), 2);
    assert_eq!(memory.len(), 2);
}

#[test]
fn cached_name_delegates_to_wrapped_source() {
    let memory = MemoryCache::new();
    let fake = FakeSource {
        name: SourceName::OpenAlex,
        supports: true,
        response: Ok(article_with_doi("10.1038/171737a0")),
        calls: Rc::new(Cell::new(0)),
    };
    let cached = Cached::new(fake, SharedCache::new(&memory));

    assert_eq!(cached.name(), SourceName::OpenAlex);
}

#[test]
fn cached_supports_delegates_to_wrapped_source_when_true() {
    let memory = MemoryCache::new();
    let fake = FakeSource {
        name: SourceName::Crossref,
        supports: true,
        response: Ok(article_with_doi("10.1038/171737a0")),
        calls: Rc::new(Cell::new(0)),
    };
    let cached = Cached::new(fake, SharedCache::new(&memory));

    assert!(cached.supports(&doi_identifier()));
}

#[test]
fn cached_supports_delegates_to_wrapped_source_when_false() {
    let memory = MemoryCache::new();
    let fake = FakeSource {
        name: SourceName::Crossref,
        supports: false,
        response: Ok(article_with_doi("10.1038/171737a0")),
        calls: Rc::new(Cell::new(0)),
    };
    let cached = Cached::new(fake, SharedCache::new(&memory));

    assert!(!cached.supports(&doi_identifier()));
}

#[test]
fn cached_source_is_object_safe_and_resolves_through_dispatch() {
    let memory = MemoryCache::new();
    let fake = FakeSource {
        name: SourceName::Crossref,
        supports: true,
        response: Ok(article_with_doi("10.1038/171737a0")),
        calls: Rc::new(Cell::new(0)),
    };
    let cached = Cached::new(fake, SharedCache::new(&memory));
    let sources: Vec<&dyn Source> = vec![&cached];

    let resolved = resolve(&sources, &doi_identifier()).unwrap();

    assert_eq!(resolved.source, SourceName::Crossref);
}
