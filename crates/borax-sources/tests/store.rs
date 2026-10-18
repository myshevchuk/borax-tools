#![allow(clippy::unwrap_used)]

use std::cell::Cell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Barrier};

use borax_core::content::hash_bytes;
use borax_core::identifier::{ArxivId, Doi, Identifier, Isbn, Pmid};
use borax_core::index::{Fact, LibraryReference, reference_to_json};
use borax_core::library::{ArtifactId, EvidenceOrigin, IdentificationEvidence, ItemId, LibraryId};
use borax_core::record::{BoraxExt, DateParts, EntryType, Name, Record, Source};
use borax_sources::cache::{Cache, CacheWrite, MemoryCache};
use borax_sources::store::{
    ContentIndex, FORMAT_VERSION, FileCache, INDEX_FORMAT_VERSION, IndexStore, cache_root,
    content_key, create_atomically, create_atomically_with, data_root, entry_path, extracted_key,
    hash_file, operator_key, reference_key, references_prefix, write_atomically,
};

/// A [`Cache`] whose `put` always reports [`CacheWrite::Failed`]
/// without storing anything, and an [`IndexStore`] that fails every
/// write of a fact on the same terms.
struct WriteFailingCache;

impl Cache for WriteFailingCache {
    fn get(&self, _key: &str) -> Option<Record> {
        None
    }

    fn put(&self, _key: &str, _record: &Record) -> CacheWrite {
        CacheWrite::Failed {
            message: "write-failing cache".to_string(),
        }
    }
}

impl IndexStore for WriteFailingCache {
    fn read_fact(&self, _key: &str) -> Option<Result<String, String>> {
        None
    }

    fn write_fact(&self, _key: &str, _text: &str) -> CacheWrite {
        CacheWrite::Failed {
            message: "write-failing cache".to_string(),
        }
    }

    fn list_facts(&self, _prefix: &str) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    fn remove_fact(&self, _key: &str) -> CacheWrite {
        CacheWrite::Failed {
            message: "write-failing cache".to_string(),
        }
    }
}
use tempfile::tempdir;

fn doi(value: &str) -> Doi {
    Doi::parse(value).unwrap()
}

fn doi_identifier(value: &str) -> Identifier {
    Identifier::Doi(doi(value))
}

fn article_with_doi(value: &str) -> Record {
    Record {
        title: Some("On the Structure of Borax".to_string()),
        doi: Some(doi(value)),
        ..Record::new(EntryType::Article)
    }
}

/// A record with every field populated, including the `borax` extension,
/// used to check that storage round-trips lose nothing.
fn full_record() -> Record {
    let mut provenance = BTreeMap::new();
    provenance.insert("title".to_string(), Source::Crossref);
    provenance.insert("DOI".to_string(), Source::Extraction);

    let mut source_fields = BTreeMap::new();
    source_fields.insert(
        "crossref-extra".to_string(),
        serde_json::json!({"funder": ["NSF"], "is-referenced-by-count": 3}),
    );

    Record {
        entry_type: EntryType::Article,
        title: Some("Bor\u{e1}x \u{2014} a study".to_string()),
        authors: vec![
            Name {
                family: "Smith".to_string(),
                given: Some("Jane".to_string()),
            },
            Name {
                family: "Doe".to_string(),
                given: None,
            },
        ],
        issued: Some(DateParts {
            year: 2024,
            month: Some(5),
            day: Some(17),
        }),
        container_title: Some("Journal of Chemical Education".to_string()),
        volume: Some("12".to_string()),
        issue: Some("3".to_string()),
        pages: Some("100-110".to_string()),
        publisher: Some("ACS".to_string()),
        doi: Some(doi("10.1021/jacs.4c01234")),
        pmid: Some(Pmid::parse("12345678").unwrap()),
        isbn: Some(Isbn::parse("978-1-59327-828-1").unwrap()),
        borax: BoraxExt {
            arxiv: Some(ArxivId::parse("2401.12345v2").unwrap()),
            confidence: Some(0.97),
            provenance,
            source_fields,
        },
    }
}

// --- cache_root() ---

/// A `lookup` that answers from `entries` and knows nothing else, so a
/// test states the whole environment it depends on.
#[cfg(any(unix, windows))]
fn env(entries: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
    move |name| {
        entries
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(*value))
    }
}

#[cfg(unix)]
#[test]
fn cache_root_honors_xdg_cache_home_on_unix() {
    let root = cache_root(env(&[
        ("XDG_CACHE_HOME", "/base/home/.cache"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home/.cache")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn cache_root_falls_back_to_home_dot_cache_on_unix() {
    let root = cache_root(env(&[("HOME", "/base/home")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home")
                .join(".cache")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn cache_root_prefers_xdg_cache_home_over_home_on_unix() {
    let root = cache_root(env(&[
        ("XDG_CACHE_HOME", "/xdg/cache"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/xdg/cache")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn cache_root_skips_an_empty_xdg_cache_home_on_unix() {
    let root = cache_root(env(&[("XDG_CACHE_HOME", ""), ("HOME", "/base/home")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home")
                .join(".cache")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn cache_root_skips_a_relative_xdg_cache_home_on_unix() {
    let root = cache_root(env(&[
        ("XDG_CACHE_HOME", "relative/cache"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home")
                .join(".cache")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn cache_root_is_none_when_nothing_qualifies_on_unix() {
    assert_eq!(cache_root(env(&[])), None);
}

#[cfg(unix)]
#[test]
fn cache_root_is_none_for_an_empty_home_with_no_xdg_cache_home_on_unix() {
    assert_eq!(cache_root(env(&[("HOME", "")])), None);
}

#[cfg(unix)]
#[test]
fn cache_root_is_none_for_a_relative_home_with_no_xdg_cache_home_on_unix() {
    assert_eq!(cache_root(env(&[("HOME", "relative")])), None);
}

#[cfg(windows)]
#[test]
fn cache_root_honors_localappdata_on_windows() {
    let root = cache_root(env(&[("LOCALAPPDATA", r"C:\Users\test\AppData\Local")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\AppData\Local")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn cache_root_falls_back_to_xdg_cache_home_on_windows() {
    let root = cache_root(env(&[("XDG_CACHE_HOME", r"C:\Users\test\.cache")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\.cache")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn cache_root_prefers_localappdata_over_xdg_cache_home_on_windows() {
    let root = cache_root(env(&[
        ("LOCALAPPDATA", r"C:\Users\test\AppData\Local"),
        ("XDG_CACHE_HOME", r"C:\Users\test\.cache"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\AppData\Local")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn cache_root_skips_an_empty_localappdata_on_windows() {
    let root = cache_root(env(&[
        ("LOCALAPPDATA", ""),
        ("XDG_CACHE_HOME", r"C:\Users\test\.cache"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\.cache")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn cache_root_skips_a_relative_localappdata_on_windows() {
    let root = cache_root(env(&[
        ("LOCALAPPDATA", r"AppData\Local"),
        ("XDG_CACHE_HOME", r"C:\Users\test\.cache"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\.cache")
                .join("borax")
                .join(FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn cache_root_is_none_when_nothing_qualifies_on_windows() {
    assert_eq!(cache_root(env(&[])), None);
}

#[cfg(windows)]
#[test]
fn cache_root_is_none_for_an_empty_xdg_cache_home_with_no_localappdata_on_windows() {
    assert_eq!(cache_root(env(&[("XDG_CACHE_HOME", "")])), None);
}

#[cfg(windows)]
#[test]
fn cache_root_is_none_for_a_relative_xdg_cache_home_with_no_localappdata_on_windows() {
    assert_eq!(cache_root(env(&[("XDG_CACHE_HOME", "relative")])), None);
}

// --- data_root() (design D1) ---

#[cfg(unix)]
#[test]
fn data_root_honors_xdg_data_home_on_unix() {
    let root = data_root(env(&[
        ("XDG_DATA_HOME", "/base/home/.local/share"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home/.local/share")
                .join("borax")
                .join(INDEX_FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn data_root_falls_back_to_home_dot_local_share_on_unix() {
    let root = data_root(env(&[("HOME", "/base/home")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home")
                .join(".local/share")
                .join("borax")
                .join(INDEX_FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn data_root_prefers_xdg_data_home_over_home_on_unix() {
    let root = data_root(env(&[
        ("XDG_DATA_HOME", "/xdg/data"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/xdg/data")
                .join("borax")
                .join(INDEX_FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn data_root_skips_an_empty_xdg_data_home_on_unix() {
    let root = data_root(env(&[("XDG_DATA_HOME", ""), ("HOME", "/base/home")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home")
                .join(".local/share")
                .join("borax")
                .join(INDEX_FORMAT_VERSION)
        )
    );
}

#[cfg(unix)]
#[test]
fn data_root_skips_a_relative_xdg_data_home_on_unix() {
    let root = data_root(env(&[
        ("XDG_DATA_HOME", "relative/data"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home")
                .join(".local/share")
                .join("borax")
                .join(INDEX_FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn data_root_honors_appdata_on_windows() {
    let root = data_root(env(&[("APPDATA", r"C:\Users\test\AppData\Roaming")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\AppData\Roaming")
                .join("borax")
                .join(INDEX_FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn data_root_falls_back_to_xdg_data_home_on_windows() {
    let root = data_root(env(&[("XDG_DATA_HOME", r"C:\Users\test\.data")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\.data")
                .join("borax")
                .join(INDEX_FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn data_root_prefers_appdata_over_xdg_data_home_on_windows() {
    let root = data_root(env(&[
        ("APPDATA", r"C:\Users\test\AppData\Roaming"),
        ("XDG_DATA_HOME", r"C:\Users\test\.data"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\AppData\Roaming")
                .join("borax")
                .join(INDEX_FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn data_root_skips_an_empty_appdata_on_windows() {
    let root = data_root(env(&[
        ("APPDATA", ""),
        ("XDG_DATA_HOME", r"C:\Users\test\.data"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\.data")
                .join("borax")
                .join(INDEX_FORMAT_VERSION)
        )
    );
}

#[cfg(windows)]
#[test]
fn data_root_skips_a_relative_appdata_on_windows() {
    let root = data_root(env(&[
        ("APPDATA", r"AppData\Roaming"),
        ("XDG_DATA_HOME", r"C:\Users\test\.data"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\.data")
                .join("borax")
                .join(INDEX_FORMAT_VERSION)
        )
    );
}

#[cfg(any(unix, windows))]
#[test]
fn data_root_ends_in_borax_v2() {
    #[cfg(unix)]
    let root = data_root(env(&[("HOME", "/base/home")])).unwrap();
    #[cfg(windows)]
    let root = data_root(env(&[("APPDATA", r"C:\Users\test\AppData\Roaming")])).unwrap();

    assert!(
        root.ends_with(Path::new("borax").join(INDEX_FORMAT_VERSION)),
        "got {root:?}"
    );
    assert_eq!(INDEX_FORMAT_VERSION, "v2");
}

#[cfg(unix)]
#[test]
fn data_root_is_none_when_no_candidate_qualifies() {
    assert_eq!(data_root(env(&[])), None);
}

#[cfg(windows)]
#[test]
fn data_root_is_none_when_no_candidate_qualifies() {
    assert_eq!(data_root(env(&[])), None);
}

#[cfg(unix)]
#[test]
fn data_root_and_cache_root_differ_for_one_environment() {
    let environment = env(&[
        ("XDG_DATA_HOME", "/base/home/.local/share"),
        ("XDG_CACHE_HOME", "/base/home/.cache"),
        ("HOME", "/base/home"),
    ]);

    assert_ne!(data_root(&environment), cache_root(&environment));
}

#[cfg(windows)]
#[test]
fn data_root_and_cache_root_differ_for_one_environment() {
    let environment = env(&[
        ("APPDATA", r"C:\Users\test\AppData\Roaming"),
        ("LOCALAPPDATA", r"C:\Users\test\AppData\Local"),
    ]);

    assert_ne!(data_root(&environment), cache_root(&environment));
}

// --- entry_path() ---

#[test]
fn entry_path_appends_json_and_maps_slashes_to_nested_dirs() {
    let root = Path::new("/cache/root");

    assert_eq!(
        entry_path(root, "crossref/doi/10.1038-171737a0"),
        Some(
            root.join("crossref")
                .join("doi")
                .join("10.1038-171737a0.json")
        )
    );
}

#[test]
fn entry_path_accepts_a_flat_key() {
    let root = Path::new("/cache/root");
    assert_eq!(entry_path(root, "widget"), Some(root.join("widget.json")));
}

#[test]
fn entry_path_rejects_an_empty_key() {
    assert_eq!(entry_path(Path::new("/root"), ""), None);
}

#[test]
fn entry_path_rejects_a_bare_dot_key() {
    assert_eq!(entry_path(Path::new("/root"), "."), None);
}

#[test]
fn entry_path_rejects_a_bare_dot_dot_key() {
    assert_eq!(entry_path(Path::new("/root"), ".."), None);
}

#[test]
fn entry_path_rejects_a_dot_segment() {
    assert_eq!(entry_path(Path::new("/root"), "a/./b"), None);
}

#[test]
fn entry_path_rejects_a_dot_dot_segment() {
    assert_eq!(entry_path(Path::new("/root"), "a/../b"), None);
}

#[test]
fn entry_path_rejects_an_empty_segment() {
    assert_eq!(entry_path(Path::new("/root"), "a//b"), None);
}

#[test]
fn entry_path_rejects_a_leading_slash() {
    assert_eq!(entry_path(Path::new("/root"), "/a"), None);
}

#[test]
fn entry_path_rejects_a_trailing_slash() {
    assert_eq!(entry_path(Path::new("/root"), "a/"), None);
}

#[test]
fn entry_path_rejects_a_backslash() {
    assert_eq!(entry_path(Path::new("/root"), "a\\b"), None);
}

#[test]
fn entry_path_rejects_uppercase_characters() {
    assert_eq!(entry_path(Path::new("/root"), "Crossref/doi"), None);
}

#[test]
fn entry_path_rejects_spaces() {
    assert_eq!(entry_path(Path::new("/root"), "a b"), None);
}

#[test]
fn entry_path_rejects_other_punctuation() {
    assert_eq!(entry_path(Path::new("/root"), "a:b"), None);
}

#[test]
fn entry_path_rejects_a_traversal_attempt() {
    assert_eq!(entry_path(Path::new("/root"), "../../etc/passwd"), None);
}

// --- content_key() ---

#[test]
fn content_key_is_content_slash_hash() {
    let hash = hash_bytes(b"borax");
    assert_eq!(content_key(&hash), format!("content/{hash}"));
}

#[test]
fn content_key_round_trips_through_entry_path() {
    let hash = hash_bytes(b"borax");
    let key = content_key(&hash);

    assert!(entry_path(Path::new("/root"), &key).is_some());
}

// --- FileCache ---

#[test]
fn file_cache_round_trips_a_record_with_every_field_set() {
    let dir = tempdir().unwrap();
    let cache = FileCache::new(dir.path());
    let record = full_record();

    assert_eq!(cache.put("full", &record), CacheWrite::Written);

    assert_eq!(cache.get("full"), Some(record));
}

#[test]
fn file_cache_get_for_an_unknown_key_is_a_miss() {
    let dir = tempdir().unwrap();
    let cache = FileCache::new(dir.path());

    assert_eq!(cache.get("absent"), None);
}

#[test]
fn file_cache_new_does_not_create_the_root_directory() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("cache");

    FileCache::new(&root);

    assert!(!root.exists());
}

#[test]
fn file_cache_put_creates_the_root_directory() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("cache");
    let cache = FileCache::new(&root);

    let _ = cache.put("widget", &article_with_doi("10.1038/171737a0"));

    assert!(root.exists());
}

#[test]
fn file_cache_overwriting_a_key_yields_the_new_record() {
    let dir = tempdir().unwrap();
    let cache = FileCache::new(dir.path());
    let first = article_with_doi("10.1038/171737a0");
    let second = article_with_doi("10.1021/jacs.4c01234");

    let _ = cache.put("widget", &first);
    let _ = cache.put("widget", &second);

    assert_eq!(cache.get("widget"), Some(second));
}

#[test]
fn file_cache_corrupt_entry_reads_as_a_miss() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = entry_path(root, "widget").unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"not json").unwrap();

    let cache = FileCache::new(root);

    assert_eq!(cache.get("widget"), None);
}

#[test]
fn file_cache_put_with_an_invalid_key_is_a_silent_no_op() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("cache");
    let cache = FileCache::new(&root);

    let _ = cache.put("Invalid Key", &article_with_doi("10.1038/171737a0"));

    assert!(!root.exists());
}

#[test]
fn file_cache_put_with_an_invalid_key_returns_failed_with_a_message() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("cache");
    let cache = FileCache::new(&root);

    let result = cache.put("Invalid Key", &article_with_doi("10.1038/171737a0"));

    match result {
        CacheWrite::Failed { message } => assert!(!message.is_empty()),
        CacheWrite::Written => panic!("expected Failed, got Written"),
    }
    assert_eq!(cache.get("Invalid Key"), None);
}

#[test]
fn file_cache_put_over_a_root_blocked_by_a_regular_file_returns_failed() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("cache");
    std::fs::write(&root, b"not a directory").unwrap();
    let cache = FileCache::new(&root);

    let result = cache.put("widget", &article_with_doi("10.1038/171737a0"));

    match result {
        CacheWrite::Failed { message } => assert!(!message.is_empty()),
        CacheWrite::Written => panic!("expected Failed, got Written"),
    }
    assert_eq!(cache.get("widget"), None);
}

#[test]
fn file_cache_get_with_an_invalid_key_is_a_miss() {
    let dir = tempdir().unwrap();
    let cache = FileCache::new(dir.path());

    assert_eq!(cache.get("Invalid Key"), None);
}

#[test]
fn file_cache_clear_removes_the_entries() {
    let dir = tempdir().unwrap();
    let cache = FileCache::new(dir.path());
    let _ = cache.put("widget", &article_with_doi("10.1038/171737a0"));

    cache.clear().unwrap();

    assert_eq!(cache.get("widget"), None);
}

#[test]
fn file_cache_clear_succeeds_on_an_already_absent_root() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("cache");
    let cache = FileCache::new(&root);

    assert!(cache.clear().is_ok());
}

#[test]
fn file_cache_works_again_after_clear() {
    let dir = tempdir().unwrap();
    let cache = FileCache::new(dir.path());
    let _ = cache.put("widget", &article_with_doi("10.1038/171737a0"));
    cache.clear().unwrap();

    let record = article_with_doi("10.1021/jacs.4c01234");
    let _ = cache.put("widget", &record);

    assert_eq!(cache.get("widget"), Some(record));
}

// --- ContentIndex, over both MemoryCache and FileCache ---

fn content_index_round_trips_both_facts<S: IndexStore>(store: S) {
    let index = ContentIndex::new(store);
    let hash = hash_bytes(b"borax content");
    let identifier = doi_identifier("10.1038/171737a0");
    let evidence = IdentificationEvidence {
        identifier: identifier.clone(),
        origin: EvidenceOrigin::Supplied,
    };

    let _ = index.put_extracted(&hash, &identifier);
    let _ = index.put_operator(&hash, &evidence);

    let entry = index.entry(&hash);
    assert_eq!(entry.extracted, Fact::Held(identifier));
    assert_eq!(entry.operator, Fact::Held(evidence));
}

#[test]
fn content_index_put_returns_written_over_memory_cache() {
    let index = ContentIndex::new(MemoryCache::new());
    let hash = hash_bytes(b"borax content");
    let identifier = doi_identifier("10.1038/171737a0");

    assert_eq!(index.put_extracted(&hash, &identifier), CacheWrite::Written);
}

#[test]
fn content_index_put_returns_what_its_cache_returned() {
    let index = ContentIndex::new(WriteFailingCache);
    let hash = hash_bytes(b"borax content");
    let identifier = doi_identifier("10.1038/171737a0");

    match index.put_extracted(&hash, &identifier) {
        CacheWrite::Failed { message } => assert_eq!(message, "write-failing cache"),
        CacheWrite::Written => panic!("expected Failed, got Written"),
    }
}

#[test]
fn content_index_round_trips_both_facts_over_memory_cache() {
    content_index_round_trips_both_facts(MemoryCache::new());
}

#[test]
fn content_index_round_trips_both_facts_over_file_cache() {
    let dir = tempdir().unwrap();
    content_index_round_trips_both_facts(FileCache::new(dir.path()));
}

fn content_index_misses_an_unknown_hash<S: IndexStore>(store: S) {
    let index = ContentIndex::new(store);
    assert_eq!(index.entry(&hash_bytes(b"never indexed")).answer(), None);
}

#[test]
fn content_index_misses_an_unknown_hash_over_memory_cache() {
    content_index_misses_an_unknown_hash(MemoryCache::new());
}

#[test]
fn content_index_misses_an_unknown_hash_over_file_cache() {
    let dir = tempdir().unwrap();
    content_index_misses_an_unknown_hash(FileCache::new(dir.path()));
}

/// Spec scenario: "Renamed file, same content" — an identifier indexed
/// under a file's content hash is found again when the same bytes
/// appear under a different path.
#[test]
fn renamed_file_with_identical_content_is_answered_by_the_content_index() {
    let dir = tempdir().unwrap();
    let bytes = b"same bytes, different name";
    let original = dir.path().join("original.pdf");
    let renamed = dir.path().join("renamed.pdf");
    std::fs::write(&original, bytes).unwrap();
    std::fs::write(&renamed, bytes).unwrap();

    let index = ContentIndex::new(MemoryCache::new());
    let identifier = doi_identifier("10.1038/171737a0");
    let _ = index.put_extracted(&hash_file(&original).unwrap(), &identifier);

    assert_eq!(
        index.entry(&hash_file(&renamed).unwrap()).answer(),
        Some(IdentificationEvidence {
            identifier,
            origin: EvidenceOrigin::Extracted,
        })
    );
}

/// Spec scenario: "Re-run over the same directory is offline" — entries
/// survive a fresh [`FileCache`] opened on the same root.
#[test]
fn entries_survive_a_fresh_file_cache_opened_on_the_same_root() {
    let dir = tempdir().unwrap();
    let hash = hash_bytes(b"offline re-run");
    let identifier = doi_identifier("10.1038/171737a0");

    let first_run = ContentIndex::new(FileCache::new(dir.path()));
    let _ = first_run.put_extracted(&hash, &identifier);
    drop(first_run);

    let second_run = ContentIndex::new(FileCache::new(dir.path()));
    assert_eq!(
        second_run.entry(&hash).answer(),
        Some(IdentificationEvidence {
            identifier,
            origin: EvidenceOrigin::Extracted,
        })
    );
}

// --- the content-index evidence API: IndexStore, entry/put_extracted/
// put_operator (design D1, D2, D10) ---

#[test]
fn extracted_and_operator_keys_name_files_in_the_hashs_directory() {
    let hash = hash_bytes(b"evidence keys");
    let root = Path::new("/data/root");

    assert_eq!(
        entry_path(root, &extracted_key(&hash)),
        Some(
            root.join("content")
                .join(hash.to_string())
                .join("extracted.json")
        )
    );
    assert_eq!(
        entry_path(root, &operator_key(&hash)),
        Some(
            root.join("content")
                .join(hash.to_string())
                .join("operator.json")
        )
    );
}

#[test]
fn put_extracted_writes_only_extracted_json() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"put extracted only");

    let _ = index.put_extracted(&hash, &doi_identifier("10.1000/x"));

    let hash_dir = dir.path().join("content").join(hash.to_string());
    let mut names: Vec<String> = std::fs::read_dir(&hash_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["extracted.json".to_string()]);
}

#[test]
fn put_operator_writes_only_operator_json() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"put operator only");
    let evidence = IdentificationEvidence {
        identifier: doi_identifier("10.1000/x"),
        origin: EvidenceOrigin::Supplied,
    };

    let _ = index.put_operator(&hash, &evidence);

    let hash_dir = dir.path().join("content").join(hash.to_string());
    let mut names: Vec<String> = std::fs::read_dir(&hash_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["operator.json".to_string()]);
}

#[test]
fn put_operator_refuses_evidence_of_a_non_operator_origin_without_writing() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"put operator non-operator origin");
    let evidence = IdentificationEvidence {
        identifier: doi_identifier("10.1000/x"),
        origin: EvidenceOrigin::Extracted,
    };

    let result = index.put_operator(&hash, &evidence);

    assert!(
        matches!(result, CacheWrite::Failed { .. }),
        "got {result:?}"
    );
    assert!(
        !dir.path()
            .join("content")
            .join(hash.to_string())
            .join("operator.json")
            .exists(),
        "a refused write must leave no file"
    );
}

#[test]
fn put_extracted_leaves_an_existing_operator_json_byte_identical() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"leave operator byte identical");
    let _ = index.put_operator(
        &hash,
        &IdentificationEvidence {
            identifier: doi_identifier("10.1000/op"),
            origin: EvidenceOrigin::Supplied,
        },
    );
    let operator_path = dir
        .path()
        .join("content")
        .join(hash.to_string())
        .join("operator.json");
    let before = std::fs::read(&operator_path).unwrap();

    let _ = index.put_extracted(&hash, &doi_identifier("10.1000/ext"));

    assert_eq!(std::fs::read(&operator_path).unwrap(), before);
}

#[test]
fn put_operator_leaves_an_existing_extracted_json_byte_identical() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"leave extracted byte identical");
    let _ = index.put_extracted(&hash, &doi_identifier("10.1000/ext"));
    let extracted_path = dir
        .path()
        .join("content")
        .join(hash.to_string())
        .join("extracted.json");
    let before = std::fs::read(&extracted_path).unwrap();

    let _ = index.put_operator(
        &hash,
        &IdentificationEvidence {
            identifier: doi_identifier("10.1000/op"),
            origin: EvidenceOrigin::Overridden,
        },
    );

    assert_eq!(std::fs::read(&extracted_path).unwrap(), before);
}

#[test]
fn a_later_operator_write_replaces_the_earlier_one() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"later operator write replaces");
    let _ = index.put_operator(
        &hash,
        &IdentificationEvidence {
            identifier: doi_identifier("10.1000/first"),
            origin: EvidenceOrigin::Supplied,
        },
    );
    let second = IdentificationEvidence {
        identifier: doi_identifier("10.1000/second"),
        origin: EvidenceOrigin::Overridden,
    };

    let _ = index.put_operator(&hash, &second);

    assert_eq!(index.entry(&hash).operator, Fact::Held(second));
}

#[test]
fn entry_of_an_absent_directory_has_both_facts_absent() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"absent directory");

    let entry = index.entry(&hash);

    assert_eq!(entry.extracted, Fact::Absent);
    assert_eq!(entry.operator, Fact::Absent);
}

#[test]
fn entry_ignores_a_temporary_file_in_the_directory() {
    let dir = tempdir().unwrap();
    let hash = hash_bytes(b"ignores temp file");
    let hash_dir = dir.path().join("content").join(hash.to_string());
    std::fs::create_dir_all(&hash_dir).unwrap();
    std::fs::write(hash_dir.join(".tmp-12345-0"), b"garbage").unwrap();

    let index = ContentIndex::new(FileCache::new(dir.path()));
    let entry = index.entry(&hash);

    assert_eq!(entry.extracted, Fact::Absent);
    assert_eq!(entry.operator, Fact::Absent);
}

#[test]
fn entry_reports_an_operator_json_that_does_not_parse_as_unreadable() {
    let dir = tempdir().unwrap();
    let hash = hash_bytes(b"operator does not parse");
    let hash_dir = dir.path().join("content").join(hash.to_string());
    std::fs::create_dir_all(&hash_dir).unwrap();
    std::fs::write(hash_dir.join("operator.json"), b"not json").unwrap();

    let index = ContentIndex::new(FileCache::new(dir.path()));
    let entry = index.entry(&hash);

    assert!(
        matches!(entry.operator, Fact::Unreadable(_)),
        "got {:?}",
        entry.operator
    );
}

/// The record API is gone (group 4), so this is read directly against
/// [`MemoryCache`]'s two traits rather than through [`ContentIndex`]:
/// its [`Cache`] records and its [`IndexStore`] facts are two separate
/// maps, and writing one leaves the other exactly as it was.
#[test]
fn memory_cache_holds_facts_apart_from_records() {
    let cache = MemoryCache::new();
    let hash = hash_bytes(b"facts apart from records");
    let record = article_with_doi("10.1038/171737a0");

    let _ = cache.put("some-service-cache-key", &record);
    let _ = cache.write_fact(
        &extracted_key(&hash),
        "{\"identifier\":\"doi:10.1000/fact\"}",
    );

    assert_eq!(cache.get("some-service-cache-key"), Some(record));
    assert_eq!(
        cache.read_fact(&extracted_key(&hash)),
        Some(Ok("{\"identifier\":\"doi:10.1000/fact\"}".to_string()))
    );
}

#[cfg(unix)]
#[test]
fn content_index_fact_write_reports_failed_over_a_read_only_root() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path().join("locked");
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o555)).unwrap();

    let index = ContentIndex::new(FileCache::new(&root));
    let hash = hash_bytes(b"read only root");

    let result = index.put_extracted(&hash, &doi_identifier("10.1000/x"));

    // Restored before the temp directory's own cleanup.
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();

    if matches!(result, CacheWrite::Written) {
        eprintln!(
            "skipping content_index_fact_write_reports_failed_over_a_read_only_root: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    assert!(
        matches!(result, CacheWrite::Failed { .. }),
        "got {result:?}"
    );
}

/// The record API is gone (group 4), so the `v1` record this test puts
/// beside the entry is written directly, as the old record API would
/// have: a whole record, serialized under `content/<hash>.json`, with
/// no `v2` entry directory for it.
#[test]
fn a_v1_record_file_beside_the_entry_is_not_read_as_a_fact() {
    let dir = tempdir().unwrap();
    let hash = hash_bytes(b"v1 record beside entry");
    let record = article_with_doi("10.1038/171737a0");
    let path = entry_path(dir.path(), &content_key(&hash)).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();

    let index = ContentIndex::new(FileCache::new(dir.path()));
    let entry = index.entry(&hash);

    assert_eq!(entry.extracted, Fact::Absent);
    assert_eq!(entry.operator, Fact::Absent);
}

// --- hash_file() ---

#[test]
fn hash_file_of_an_empty_file_matches_hash_bytes_of_an_empty_slice() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("empty");
    std::fs::write(&path, b"").unwrap();

    assert_eq!(hash_file(&path).unwrap(), hash_bytes(b""));
}

#[test]
fn hash_file_of_a_multi_megabyte_file_matches_hash_bytes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("large");
    let bytes: Vec<u8> = (0..5_000_000u32).map(|n| (n % 251) as u8).collect();
    std::fs::write(&path, &bytes).unwrap();

    assert_eq!(hash_file(&path).unwrap(), hash_bytes(&bytes));
}

#[test]
fn hash_file_of_a_missing_path_is_an_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("does-not-exist");

    assert!(hash_file(&path).is_err());
}

// ---------------------------------------------------------------------
// write_atomically
// ---------------------------------------------------------------------

#[test]
fn write_atomically_creates_the_file_and_its_parent_directories() {
    let root = tempdir().unwrap();
    let path = root.path().join("deep").join("nested").join("refs.bib");

    write_atomically(&path, b"@article{a,}\n").unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "@article{a,}\n");
}

#[test]
fn write_atomically_replaces_the_content_that_was_there() {
    let root = tempdir().unwrap();
    let path = root.path().join("refs.bib");
    std::fs::write(&path, "old content").unwrap();

    write_atomically(&path, b"new content").unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content");
}

// The temporary the write goes through is never left behind, so a
// directory holds the file and nothing else.
#[test]
fn write_atomically_leaves_no_temporary_behind() {
    let root = tempdir().unwrap();
    let path = root.path().join("refs.bib");

    write_atomically(&path, b"content").unwrap();

    let entries: Vec<String> = std::fs::read_dir(root.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, ["refs.bib"]);
}

// A destination that cannot be written reports the failure rather than
// destroying what was there: the original file survives intact.
#[test]
fn write_atomically_leaves_the_original_intact_when_it_cannot_finish() {
    let root = tempdir().unwrap();
    // A directory where the file should be: the rename cannot replace
    // it, so the write fails after the temporary is written.
    let path = root.path().join("refs.bib");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("sentinel"), "still here").unwrap();

    assert!(write_atomically(&path, b"new content").is_err());

    assert_eq!(
        std::fs::read_to_string(path.join("sentinel")).unwrap(),
        "still here"
    );
}

// ---------------------------------------------------------------------
// create_atomically / create_atomically_with (design D4, D8)
// ---------------------------------------------------------------------

/// A `name` seam that yields `.tmp-test-<n>` from a counter it owns,
/// for a test that does not care which temporary name is used.
fn counting_name() -> impl Fn() -> String {
    let counter = Cell::new(0u32);
    move || {
        let n = counter.get();
        counter.set(n + 1);
        format!(".tmp-test-{n}")
    }
}

/// A `name` seam that yields each of `names` in turn, for a test that
/// pins which temporary name is tried.
fn sequence_name(names: Vec<String>) -> impl Fn() -> String {
    let index = Cell::new(0usize);
    move || {
        let i = index.get();
        index.set(i + 1);
        names
            .get(i)
            .cloned()
            .unwrap_or_else(|| panic!("the name sequence was exhausted after {i} calls"))
    }
}

#[test]
fn create_atomically_creates_the_file_and_its_parent_directories() {
    let root = tempdir().unwrap();
    let path = root.path().join("deep").join("nested").join("library.toml");

    create_atomically(&path, b"id = \"x\"\n").unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"id = \"x\"\n");
}

#[test]
fn create_atomically_refuses_an_existing_file_and_leaves_it_byte_identical() {
    let root = tempdir().unwrap();
    let path = root.path().join("library.toml");
    std::fs::write(&path, b"original").unwrap();

    let error = create_atomically(&path, b"new").unwrap_err();

    assert_eq!(error.kind(), ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
}

#[test]
fn create_atomically_leaves_no_temporary_behind_when_it_creates() {
    let root = tempdir().unwrap();
    let path = root.path().join("library.toml");

    create_atomically(&path, b"content").unwrap();

    let entries: Vec<String> = std::fs::read_dir(root.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, ["library.toml"]);
}

#[test]
fn create_atomically_leaves_no_temporary_behind_when_the_file_exists() {
    let root = tempdir().unwrap();
    let path = root.path().join("library.toml");
    std::fs::write(&path, b"original").unwrap();

    let _ = create_atomically(&path, b"new");

    let entries: Vec<String> = std::fs::read_dir(root.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, ["library.toml"]);
}

#[test]
fn create_atomically_with_a_link_that_is_unsupported_falls_back_to_an_exclusive_create() {
    let root = tempdir().unwrap();
    let path = root.path().join("library.toml");
    let name = counting_name();

    create_atomically_with(
        &path,
        b"content",
        &|_from, _to| Err(io::Error::from(ErrorKind::Unsupported)),
        &name,
    )
    .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"content");
    let entries: Vec<String> = std::fs::read_dir(root.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, ["library.toml"]);
}

#[test]
fn create_atomically_with_a_link_that_is_unsupported_still_refuses_an_existing_file() {
    let root = tempdir().unwrap();
    let path = root.path().join("library.toml");
    std::fs::write(&path, b"original").unwrap();
    let name = counting_name();

    let error = create_atomically_with(
        &path,
        b"new",
        &|_from, _to| Err(io::Error::from(ErrorKind::Unsupported)),
        &name,
    )
    .unwrap_err();

    assert_eq!(error.kind(), ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    let entries: Vec<String> = std::fs::read_dir(root.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, ["library.toml"]);
}

#[test]
fn create_atomically_with_a_link_reporting_already_exists_does_not_fall_back() {
    let root = tempdir().unwrap();
    let path = root.path().join("library.toml");
    let name = counting_name();

    let error = create_atomically_with(
        &path,
        b"content",
        &|_from, _to| Err(io::Error::from(ErrorKind::AlreadyExists)),
        &name,
    )
    .unwrap_err();

    assert_eq!(error.kind(), ErrorKind::AlreadyExists);
    assert!(
        !path.exists(),
        "a link reporting AlreadyExists must not fall back to an exclusive create"
    );
}

#[test]
fn create_atomically_leaves_a_file_at_a_taken_temporary_name_untouched() {
    let root = tempdir().unwrap();
    let path = root.path().join("library.toml");
    std::fs::write(root.path().join("taken.tmp"), b"taken bytes").unwrap();
    let name = sequence_name(vec!["taken.tmp".to_string(), "fresh.tmp".to_string()]);

    create_atomically_with(
        &path,
        b"content",
        &|from: &Path, to: &Path| std::fs::hard_link(from, to),
        &name,
    )
    .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"content");
    assert_eq!(
        std::fs::read(root.path().join("taken.tmp")).unwrap(),
        b"taken bytes",
        "a file already at a taken temporary name must be untouched"
    );
    assert!(
        !root.path().join("fresh.tmp").exists(),
        "the temporary that succeeded must not be left behind"
    );
}

#[test]
fn create_atomically_never_writes_through_a_stale_temporary_linked_to_the_destination() {
    let root = tempdir().unwrap();
    let path = root.path().join("library.toml");
    std::fs::write(&path, b"existing identity").unwrap();
    // A crash after the link step, before the temporary was removed,
    // leaves exactly this: a second name for the destination.
    std::fs::hard_link(&path, root.path().join("taken.tmp")).unwrap();
    let name = sequence_name(vec!["taken.tmp".to_string(), "fresh.tmp".to_string()]);

    let error = create_atomically_with(
        &path,
        b"new content",
        &|from: &Path, to: &Path| std::fs::hard_link(from, to),
        &name,
    )
    .unwrap_err();

    assert_eq!(error.kind(), ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&path).unwrap(), b"existing identity");
    assert_eq!(
        std::fs::read(root.path().join("taken.tmp")).unwrap(),
        b"existing identity",
        "the stale temporary must never be opened and written through"
    );
}

#[test]
fn create_atomically_gives_up_after_sixteen_taken_temporary_names() {
    let root = tempdir().unwrap();
    let path = root.path().join("library.toml");
    let names: Vec<String> = (0..16).map(|n| format!("taken-{n}.tmp")).collect();
    for name in &names {
        std::fs::write(root.path().join(name), format!("bytes-{name}")).unwrap();
    }
    let call_count = Rc::new(Cell::new(0usize));
    let counter = Rc::clone(&call_count);
    let name_list = names.clone();
    let name = move || {
        let i = counter.get();
        counter.set(i + 1);
        name_list[i].clone()
    };

    let error = create_atomically_with(
        &path,
        b"content",
        &|from: &Path, to: &Path| std::fs::hard_link(from, to),
        &name,
    )
    .unwrap_err();

    assert_ne!(
        error.kind(),
        ErrorKind::AlreadyExists,
        "giving up must not read as a lost race"
    );
    assert!(!path.exists());
    for name in &names {
        assert_eq!(
            std::fs::read(root.path().join(name)).unwrap(),
            format!("bytes-{name}").into_bytes(),
            "every taken file must stay byte-identical"
        );
    }
    assert_eq!(call_count.get(), 16);
}

#[test]
fn concurrent_creates_of_one_path_leave_exactly_one_winner() {
    let root = tempdir().unwrap();
    let path = Arc::new(root.path().join("library.toml"));
    let barrier = Arc::new(Barrier::new(8));

    let threads: Vec<_> = (0..8)
        .map(|n| {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                create_atomically(&path, format!("bytes-{n}").as_bytes())
            })
        })
        .collect();

    let results: Vec<io::Result<()>> = threads.into_iter().map(|t| t.join().unwrap()).collect();

    let ok_count = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok_count, 1, "exactly one creator must win the race");
    assert!(
        results
            .iter()
            .filter(|r| r.is_err())
            .all(|r| r.as_ref().unwrap_err().kind() == ErrorKind::AlreadyExists),
        "every loser must see AlreadyExists"
    );

    let winner = results
        .iter()
        .position(Result::is_ok)
        .expect("one winner exists");
    assert_eq!(
        std::fs::read_to_string(path.as_path()).unwrap(),
        format!("bytes-{winner}")
    );

    let entries: Vec<String> = std::fs::read_dir(root.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, ["library.toml"]);
}

// --- references: one file each under `content/<hash>/refs/` (design D1,
// D9) ---

/// A canonical UUID whose last group is `n`.
fn uuid_text(n: u32) -> String {
    format!("018f2b36-7f21-7abc-8def-{n:012x}")
}

/// A reference to library `library` at `root`, artifact `artifact`,
/// item `item`; the numbers sort as they are written.
fn reference_of(root: &str, library: u32, artifact: u32, item: u32) -> LibraryReference {
    LibraryReference {
        library: LibraryId::parse(&uuid_text(library)).unwrap(),
        root: PathBuf::from(root),
        artifact: ArtifactId::parse(&uuid_text(artifact)).unwrap(),
        item: ItemId::parse(&uuid_text(item)).unwrap(),
    }
}

fn refs_dir(root: &Path, hash: &borax_core::content::ContentHash) -> PathBuf {
    root.join("content").join(hash.to_string()).join("refs")
}

/// The file names in `directory`, sorted.
fn file_names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn reference_file(
    root: &Path,
    hash: &borax_core::content::ContentHash,
    r: &LibraryReference,
) -> PathBuf {
    refs_dir(root, hash).join(format!("{}.json", r.name().unwrap()))
}

#[test]
fn reference_key_names_a_file_in_the_hashs_refs_directory() {
    let hash = hash_bytes(b"reference keys");
    let root = Path::new("/data/root");

    assert_eq!(references_prefix(&hash), format!("content/{hash}/refs"));
    assert_eq!(
        reference_key(&hash, "sha256-abc"),
        format!("content/{hash}/refs/sha256-abc")
    );
    assert_eq!(
        entry_path(root, &reference_key(&hash, "sha256-abc")),
        Some(
            root.join("content")
                .join(hash.to_string())
                .join("refs")
                .join("sha256-abc.json")
        )
    );
}

#[test]
fn put_reference_writes_one_file_named_for_its_key() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"put reference one file");
    let reference = reference_of("/lib", 1, 2, 3);

    let result = index.put_reference(&hash, &reference);

    assert!(matches!(result, CacheWrite::Written), "got {result:?}");
    assert_eq!(
        file_names(&refs_dir(dir.path(), &hash)),
        vec![format!("{}.json", reference.name().unwrap())]
    );
    assert_eq!(
        std::fs::read_to_string(reference_file(dir.path(), &hash, &reference)).unwrap(),
        reference_to_json(&reference).unwrap()
    );
    assert_eq!(
        file_names(&dir.path().join("content").join(hash.to_string())),
        vec!["refs"]
    );
}

#[test]
fn put_reference_replaces_the_file_of_the_same_key_whole() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"put reference replaces");
    let first = reference_of("/lib", 1, 2, 3);
    let second = reference_of("/lib", 1, 2, 4);
    let _ = index.put_reference(&hash, &first);

    let result = index.put_reference(&hash, &second);

    assert!(matches!(result, CacheWrite::Written), "got {result:?}");
    assert_eq!(file_names(&refs_dir(dir.path(), &hash)).len(), 1);
    assert_eq!(
        std::fs::read_to_string(reference_file(dir.path(), &hash, &first)).unwrap(),
        reference_to_json(&second).unwrap()
    );
    assert_eq!(index.references(&hash), vec![second]);
}

#[test]
fn put_reference_for_another_root_writes_a_second_file() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"put reference second root");

    let _ = index.put_reference(&hash, &reference_of("/one", 1, 2, 3));
    let _ = index.put_reference(&hash, &reference_of("/two", 1, 2, 3));

    assert_eq!(file_names(&refs_dir(dir.path(), &hash)).len(), 2);
}

#[test]
fn put_reference_leaves_extracted_and_operator_json_byte_identical() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"reference leaves evidence");
    let _ = index.put_extracted(&hash, &doi_identifier("10.1000/ext"));
    let _ = index.put_operator(
        &hash,
        &IdentificationEvidence {
            identifier: doi_identifier("10.1000/op"),
            origin: EvidenceOrigin::Supplied,
        },
    );
    let hash_dir = dir.path().join("content").join(hash.to_string());
    let extracted = std::fs::read(hash_dir.join("extracted.json")).unwrap();
    let operator = std::fs::read(hash_dir.join("operator.json")).unwrap();

    let result = index.put_reference(&hash, &reference_of("/lib", 1, 2, 3));

    assert!(matches!(result, CacheWrite::Written), "got {result:?}");
    assert_eq!(
        std::fs::read(hash_dir.join("extracted.json")).unwrap(),
        extracted
    );
    assert_eq!(
        std::fs::read(hash_dir.join("operator.json")).unwrap(),
        operator
    );
}

#[test]
fn put_extracted_and_put_operator_leave_every_reference_byte_identical() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"evidence leaves references");
    let references = [reference_of("/one", 1, 2, 3), reference_of("/two", 1, 2, 3)];
    for reference in &references {
        let _ = index.put_reference(&hash, reference);
    }
    let before: Vec<Vec<u8>> = references
        .iter()
        .map(|r| std::fs::read(reference_file(dir.path(), &hash, r)).unwrap())
        .collect();

    let _ = index.put_extracted(&hash, &doi_identifier("10.1000/ext"));
    let _ = index.put_operator(
        &hash,
        &IdentificationEvidence {
            identifier: doi_identifier("10.1000/op"),
            origin: EvidenceOrigin::Overridden,
        },
    );

    let after: Vec<Vec<u8>> = references
        .iter()
        .map(|r| std::fs::read(reference_file(dir.path(), &hash, r)).unwrap())
        .collect();
    assert_eq!(after, before);
    assert_eq!(file_names(&refs_dir(dir.path(), &hash)).len(), 2);
}

#[test]
fn references_of_an_entry_with_no_refs_directory_are_none() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"no refs directory");
    let _ = index.put_extracted(&hash, &doi_identifier("10.1000/ext"));

    assert_eq!(index.references(&hash), Vec::new());
    assert_eq!(
        index.references(&hash_bytes(b"no entry at all")),
        Vec::new()
    );
}

#[test]
fn references_are_in_root_then_library_then_artifact_order() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"reference order");
    let in_order = vec![
        reference_of("/a", 1, 1, 9),
        reference_of("/a", 1, 2, 9),
        reference_of("/a", 2, 1, 9),
        reference_of("/b", 1, 1, 9),
    ];
    for position in [3, 1, 0, 2] {
        let _ = index.put_reference(&hash, &in_order[position]);
    }

    assert_eq!(index.references(&hash), in_order);
}

#[test]
fn references_skip_a_file_that_does_not_parse() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"references skip unparsable");
    let good = reference_of("/good", 1, 2, 3);
    let _ = index.put_reference(&hash, &good);
    let broken = reference_of("/broken", 1, 2, 3);
    std::fs::write(reference_file(dir.path(), &hash, &broken), b"not json").unwrap();

    assert_eq!(index.references(&hash), vec![good]);
}

#[test]
fn references_ignore_a_temporary_and_a_file_not_ending_json() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"references ignore strays");
    let kept = reference_of("/kept", 1, 2, 3);
    let _ = index.put_reference(&hash, &kept);
    let stray = reference_to_json(&reference_of("/stray", 1, 2, 3)).unwrap();
    let directory = refs_dir(dir.path(), &hash);
    std::fs::write(directory.join(".tmp-12345-0"), &stray).unwrap();
    std::fs::write(directory.join("sha256-0123.txt"), &stray).unwrap();
    std::fs::write(directory.join("sha256-4567"), &stray).unwrap();

    assert_eq!(index.references(&hash), vec![kept]);
}

#[test]
fn reference_reads_the_file_of_its_key_as_absent_held_or_unreadable() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"reference three states");
    let reference = reference_of("/lib", 1, 2, 3);

    assert_eq!(index.reference(&hash, &reference), Fact::Absent);

    let _ = index.put_reference(&hash, &reference);
    assert_eq!(
        index.reference(&hash, &reference),
        Fact::Held(reference.clone())
    );

    std::fs::write(reference_file(dir.path(), &hash, &reference), b"not json").unwrap();
    assert!(
        matches!(index.reference(&hash, &reference), Fact::Unreadable(_)),
        "got {:?}",
        index.reference(&hash, &reference)
    );
}

#[test]
fn entry_is_unchanged_by_references() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"entry unchanged by references");
    let _ = index.put_extracted(&hash, &doi_identifier("10.1000/ext"));
    let without = index.entry(&hash);

    let _ = index.put_reference(&hash, &reference_of("/lib", 1, 2, 3));

    assert_eq!(index.entry(&hash), without);
}

#[test]
fn remove_reference_removes_only_its_own_file() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"remove reference own file");
    let gone = reference_of("/gone", 1, 2, 3);
    let stays = reference_of("/stays", 1, 2, 3);
    let _ = index.put_extracted(&hash, &doi_identifier("10.1000/ext"));
    let _ = index.put_reference(&hash, &gone);
    let _ = index.put_reference(&hash, &stays);

    let result = index.remove_reference(&hash, &gone);

    assert!(matches!(result, CacheWrite::Written), "got {result:?}");
    assert!(!reference_file(dir.path(), &hash, &gone).exists());
    assert_eq!(index.references(&hash), vec![stays]);
    assert_eq!(
        index.entry(&hash).extracted,
        Fact::Held(doi_identifier("10.1000/ext"))
    );
}

#[test]
fn remove_reference_of_an_absent_file_is_written() {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"remove absent reference");

    let result = index.remove_reference(&hash, &reference_of("/never", 1, 2, 3));

    assert!(matches!(result, CacheWrite::Written), "got {result:?}");
}

#[test]
fn memory_cache_lists_only_the_facts_directly_under_a_prefix() {
    let cache = MemoryCache::new();
    for key in [
        "content/h/refs/b",
        "content/h/refs/a",
        "content/h/refs/nested/c",
        "content/h/refs2/d",
        "content/h/extracted",
        "content/other/refs/e",
    ] {
        let _ = cache.write_fact(key, "{}");
    }

    assert_eq!(
        cache.list_facts("content/h/refs"),
        Ok(vec![
            "content/h/refs/a".to_string(),
            "content/h/refs/b".to_string()
        ])
    );
    assert_eq!(cache.list_facts("content/none/refs"), Ok(Vec::new()));
}

#[test]
fn memory_cache_removes_a_fact() {
    let cache = MemoryCache::new();
    let _ = cache.write_fact("content/h/refs/a", "{}");
    let _ = cache.write_fact("content/h/refs/b", "{}");

    let removed = cache.remove_fact("content/h/refs/a");
    let again = cache.remove_fact("content/h/refs/a");

    assert!(matches!(removed, CacheWrite::Written), "got {removed:?}");
    assert!(matches!(again, CacheWrite::Written), "got {again:?}");
    assert_eq!(cache.read_fact("content/h/refs/a"), None);
    assert_eq!(
        cache.read_fact("content/h/refs/b"),
        Some(Ok("{}".to_string()))
    );
}

#[cfg(unix)]
#[test]
fn file_cache_list_facts_of_an_unreadable_directory_is_an_error() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let store = FileCache::new(dir.path());
    let hash = hash_bytes(b"unreadable refs directory");
    let _ = ContentIndex::new(FileCache::new(dir.path()))
        .put_reference(&hash, &reference_of("/lib", 1, 2, 3));
    let directory = refs_dir(dir.path(), &hash);
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o000)).unwrap();

    let listed = store.list_facts(&references_prefix(&hash));

    // Restored before the temp directory's own cleanup.
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();

    if listed.is_ok() {
        eprintln!(
            "skipping file_cache_list_facts_of_an_unreadable_directory_is_an_error: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }
    assert!(listed.is_err(), "got {listed:?}");
}

#[cfg(unix)]
#[test]
fn put_reference_with_a_root_that_is_not_utf8_fails_without_writing() {
    use std::os::unix::ffi::OsStrExt;

    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    let hash = hash_bytes(b"non utf8 reference root");
    let reference = LibraryReference {
        root: PathBuf::from(OsString::from(std::ffi::OsStr::from_bytes(
            b"/home/\xff/papers",
        ))),
        ..reference_of("/r", 1, 2, 3)
    };

    let result = index.put_reference(&hash, &reference);

    assert!(
        matches!(result, CacheWrite::Failed { .. }),
        "got {result:?}"
    );
    assert!(
        !dir.path().join("content").join(hash.to_string()).exists(),
        "a refused write must leave nothing behind"
    );
    assert_eq!(index.references(&hash), Vec::new());
}
