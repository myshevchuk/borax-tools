#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use borax_core::content::hash_bytes;
use borax_core::identifier::{ArxivId, Doi, Isbn, Pmid};
use borax_core::record::{BoraxExt, DateParts, EntryType, Name, Record, Source};
use borax_sources::cache::{Cache, MemoryCache};
use borax_sources::store::{
    ContentIndex, FORMAT_VERSION, FileCache, cache_root, content_key, entry_path, hash_file,
};
use tempfile::tempdir;

fn doi(value: &str) -> Doi {
    Doi::parse(value).unwrap()
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

#[cfg(unix)]
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
fn env(entries: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
    move |name| {
        entries
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(*value))
    }
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

    cache.put("full", &record);

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

    cache.put("widget", &article_with_doi("10.1038/171737a0"));

    assert!(root.exists());
}

#[test]
fn file_cache_overwriting_a_key_yields_the_new_record() {
    let dir = tempdir().unwrap();
    let cache = FileCache::new(dir.path());
    let first = article_with_doi("10.1038/171737a0");
    let second = article_with_doi("10.1021/jacs.4c01234");

    cache.put("widget", &first);
    cache.put("widget", &second);

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

    cache.put("Invalid Key", &article_with_doi("10.1038/171737a0"));

    assert!(!root.exists());
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
    cache.put("widget", &article_with_doi("10.1038/171737a0"));

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
    cache.put("widget", &article_with_doi("10.1038/171737a0"));
    cache.clear().unwrap();

    let record = article_with_doi("10.1021/jacs.4c01234");
    cache.put("widget", &record);

    assert_eq!(cache.get("widget"), Some(record));
}

// --- ContentIndex, over both MemoryCache and FileCache ---

fn content_index_round_trips_a_record<C: Cache>(cache: C) {
    let index = ContentIndex::new(cache);
    let hash = hash_bytes(b"borax content");
    let record = article_with_doi("10.1038/171737a0");

    index.put(&hash, &record);

    assert_eq!(index.get(&hash), Some(record));
}

#[test]
fn content_index_round_trips_a_record_over_memory_cache() {
    content_index_round_trips_a_record(MemoryCache::new());
}

#[test]
fn content_index_round_trips_a_record_over_file_cache() {
    let dir = tempdir().unwrap();
    content_index_round_trips_a_record(FileCache::new(dir.path()));
}

fn content_index_misses_an_unknown_hash<C: Cache>(cache: C) {
    let index = ContentIndex::new(cache);
    assert_eq!(index.get(&hash_bytes(b"never indexed")), None);
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

/// Spec scenario: "Renamed file, same content" — a record indexed under a
/// file's content hash is found again when the same bytes appear under a
/// different path.
#[test]
fn renamed_file_with_identical_content_is_served_from_the_content_index() {
    let dir = tempdir().unwrap();
    let bytes = b"same bytes, different name";
    let original = dir.path().join("original.pdf");
    let renamed = dir.path().join("renamed.pdf");
    std::fs::write(&original, bytes).unwrap();
    std::fs::write(&renamed, bytes).unwrap();

    let index = ContentIndex::new(MemoryCache::new());
    let record = article_with_doi("10.1038/171737a0");
    index.put(&hash_file(&original).unwrap(), &record);

    assert_eq!(index.get(&hash_file(&renamed).unwrap()), Some(record));
}

/// Spec scenario: "Re-run over the same directory is offline" — entries
/// survive a fresh [`FileCache`] opened on the same root.
#[test]
fn entries_survive_a_fresh_file_cache_opened_on_the_same_root() {
    let dir = tempdir().unwrap();
    let hash = hash_bytes(b"offline re-run");
    let record = article_with_doi("10.1038/171737a0");

    let first_run = ContentIndex::new(FileCache::new(dir.path()));
    first_run.put(&hash, &record);
    drop(first_run);

    let second_run = ContentIndex::new(FileCache::new(dir.path()));
    assert_eq!(second_run.get(&hash), Some(record));
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
