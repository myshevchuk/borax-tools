#![allow(clippy::unwrap_used)]

//! `borax::references`: reaching a library reference (design D4),
//! settling several (D5), and keeping one in the index (D3).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;

use borax::event::AcceptedItemCause;
use borax::library::{ARTIFACT_STORE, ITEM_STORE, STATE_DIR, identity_file};
use borax::references::{
    Libraries, Reach, ReferenceRead, ReferenceStatus, Unavailable, keep, settle,
};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{ArxivId, Doi, Identifier};
use borax_core::index::{Fact, LibraryReference};
use borax_core::library::{
    ArtifactId, ArtifactRecord, HashEntry, Item, ItemId, LibraryId, LibraryIdentity, RunId,
};
use borax_core::record::{EntryType, Record};
use borax_sources::cache::{CacheWrite, MemoryCache};
use borax_sources::store::{
    ContentIndex, FileCache, IndexStore, entry_path, reference_key, references_prefix,
};
use tempfile::{TempDir, tempdir};

// ---------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------

/// A canonical UUID whose last group is `n`.
fn uuid_text(n: u32) -> String {
    format!("018f2b36-7f21-7abc-8def-{n:012x}")
}

fn library_id(n: u32) -> LibraryId {
    LibraryId::parse(&uuid_text(n)).unwrap()
}

fn artifact_id(n: u32) -> ArtifactId {
    ArtifactId::parse(&uuid_text(n)).unwrap()
}

fn item_id(n: u32) -> ItemId {
    ItemId::parse(&uuid_text(n)).unwrap()
}

const LIBRARY: u32 = 1;
const OTHER_LIBRARY: u32 = 2;
const ARTIFACT: u32 = 10;
const OTHER_ARTIFACT: u32 = 11;
const ITEM: u32 = 20;
const OTHER_ITEM: u32 = 21;

fn reference(library: u32, root: &Path, artifact: u32, item: u32) -> LibraryReference {
    LibraryReference {
        library: library_id(library),
        root: root.to_path_buf(),
        artifact: artifact_id(artifact),
        item: item_id(item),
    }
}

fn hash(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

fn record_with_doi(doi: &str) -> Record {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("A Title".to_string());
    record.doi = Some(Doi::parse(doi).unwrap());
    record
}

fn record_with_arxiv(arxiv: &str) -> Record {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("A Title".to_string());
    record.borax.arxiv = Some(ArxivId::parse(arxiv).unwrap());
    record
}

fn record_with_nothing() -> Record {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("A Title".to_string());
    record
}

fn item(n: u32, record: Record) -> Item {
    Item {
        id: item_id(n),
        record,
    }
}

/// Writes the identity file of a library with identity `n` at `root`.
fn write_identity(root: &Path, n: u32) {
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(
        identity_file(root),
        LibraryIdentity { id: library_id(n) }.to_toml(),
    )
    .unwrap();
}

fn write_item(root: &Path, file_name: &str, item: &Item) {
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    fs::write(items.join(file_name), item.to_toml()).unwrap();
}

/// A library with identity `LIBRARY` holding `item` under its own name.
fn library_holding(item: &Item) -> TempDir {
    let dir = tempdir().unwrap();
    write_identity(dir.path(), LIBRARY);
    write_item(dir.path(), &format!("{}.toml", item.id), item);
    dir
}

/// A library with identity `n` and nothing else.
fn library_with_identity(n: u32) -> TempDir {
    let dir = tempdir().unwrap();
    write_identity(dir.path(), n);
    dir
}

fn write_artifact_record(root: &Path, record: &ArtifactRecord) {
    let dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{}.toml", record.id)), record.to_toml()).unwrap();
}

/// A path under a fresh temporary directory that is never created.
fn missing_root() -> (TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let root = dir.path().join("never-created");
    (dir, root)
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.is_file() {
                out.insert(path.clone(), fs::read(&path).unwrap());
            }
        }
    }

    let mut found = BTreeMap::new();
    walk(root, &mut found);
    found
}

/// Whether directory permissions are not enforced here (running as
/// root), checked by trying to list `directory`.
#[cfg(unix)]
fn permissions_not_enforced(directory: &Path) -> bool {
    fs::read_dir(directory).is_ok()
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn reach(reference: &LibraryReference) -> Reach {
    Libraries::new().reach(reference)
}

// ---------------------------------------------------------------------
// 3.1 Reaching (design D4)
// ---------------------------------------------------------------------

#[test]
fn a_reference_to_a_missing_root_is_unavailable_missing() {
    let (_guard, root) = missing_root();

    let reached = reach(&reference(LIBRARY, &root, ARTIFACT, ITEM));

    assert!(
        matches!(reached, Reach::Unavailable(Unavailable::Missing)),
        "got {reached:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_reference_to_a_dangling_link_is_unavailable_missing() {
    let dir = tempdir().unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(dir.path().join("target-that-is-not-there"), &link).unwrap();

    let reached = reach(&reference(LIBRARY, &link, ARTIFACT, ITEM));

    assert!(
        matches!(reached, Reach::Unavailable(Unavailable::Missing)),
        "got {reached:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_reference_to_an_unreadable_root_is_unavailable_unreadable_naming_the_root() {
    let dir = tempdir().unwrap();
    let parent = dir.path().join("parent");
    let root = parent.join("library");
    fs::create_dir_all(&root).unwrap();
    write_identity(&root, LIBRARY);
    set_mode(&parent, 0o000);

    let unenforced = permissions_not_enforced(&parent);
    let reached = reach(&reference(LIBRARY, &root, ARTIFACT, ITEM));

    // Restored before the temp directory's own cleanup.
    set_mode(&parent, 0o755);

    if unenforced {
        eprintln!(
            "skipping a_reference_to_an_unreadable_root_is_unavailable_unreadable_naming_the_root: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }
    match reached {
        Reach::Unavailable(Unavailable::Unreadable { path, message }) => {
            assert_eq!(path, root);
            assert!(!message.is_empty());
        }
        other => panic!("expected unavailable/unreadable, got {other:?}"),
    }
}

#[test]
fn a_reference_to_a_root_that_is_a_file_is_other_library_with_no_found() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("a-file");
    fs::write(&file, b"not a directory").unwrap();

    let reached = reach(&reference(LIBRARY, &file, ARTIFACT, ITEM));

    assert!(
        matches!(
            reached,
            Reach::Unavailable(Unavailable::OtherLibrary { found: None })
        ),
        "got {reached:?}"
    );
}

#[test]
fn a_reference_to_a_root_with_no_identity_is_other_library_with_no_found() {
    let dir = tempdir().unwrap();

    let reached = reach(&reference(LIBRARY, dir.path(), ARTIFACT, ITEM));

    assert!(
        matches!(
            reached,
            Reach::Unavailable(Unavailable::OtherLibrary { found: None })
        ),
        "got {reached:?}"
    );
}

#[test]
fn a_reference_to_a_root_whose_identity_is_faulty_is_unreadable_naming_the_identity_file() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(STATE_DIR)).unwrap();
    fs::write(identity_file(dir.path()), "id = \"not-a-uuid\"\n").unwrap();

    let reached = reach(&reference(LIBRARY, dir.path(), ARTIFACT, ITEM));

    match reached {
        Reach::Unavailable(Unavailable::Unreadable { path, .. }) => {
            assert_eq!(path, identity_file(dir.path()));
        }
        other => panic!("expected unavailable/unreadable, got {other:?}"),
    }
}

#[test]
fn a_reference_to_another_library_is_other_library_naming_it() {
    let dir = library_with_identity(OTHER_LIBRARY);

    let reached = reach(&reference(LIBRARY, dir.path(), ARTIFACT, ITEM));

    match reached {
        Reach::Unavailable(Unavailable::OtherLibrary { found }) => {
            assert_eq!(found, Some(library_id(OTHER_LIBRARY)));
        }
        other => panic!("expected unavailable/other-library, got {other:?}"),
    }
}

#[test]
fn a_reference_whose_item_no_file_carries_is_item_unavailable_dangling() {
    let held = item(OTHER_ITEM, record_with_doi("10.1000/other"));
    let dir = library_holding(&held);

    let reached = reach(&reference(LIBRARY, dir.path(), ARTIFACT, ITEM));

    assert!(
        matches!(reached, Reach::ItemUnavailable(AcceptedItemCause::Dangling)),
        "got {reached:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_reference_whose_item_store_cannot_be_listed_is_item_unavailable_unreadable() {
    let held = item(ITEM, record_with_doi("10.1000/unlistable"));
    let dir = library_holding(&held);
    let items = dir.path().join(ITEM_STORE);
    set_mode(&items, 0o000);

    let unenforced = permissions_not_enforced(&items);
    let reached = reach(&reference(LIBRARY, dir.path(), ARTIFACT, ITEM));

    // Restored before the temp directory's own cleanup.
    set_mode(&items, 0o755);

    if unenforced {
        eprintln!(
            "skipping a_reference_whose_item_store_cannot_be_listed_is_item_unavailable_unreadable: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }
    match reached {
        Reach::ItemUnavailable(AcceptedItemCause::Unreadable { path, .. }) => {
            assert_eq!(path, items);
        }
        other => panic!("expected item-unavailable/unreadable, got {other:?}"),
    }
}

#[test]
fn a_reference_whose_item_two_files_carry_is_item_unavailable_ambiguous() {
    let held = item(ITEM, record_with_doi("10.1000/twice"));
    let dir = library_with_identity(LIBRARY);
    write_item(dir.path(), "one.toml", &held);
    write_item(dir.path(), "two.toml", &held);

    let reached = reach(&reference(LIBRARY, dir.path(), ARTIFACT, ITEM));

    match reached {
        Reach::ItemUnavailable(AcceptedItemCause::Ambiguous { mut files }) => {
            files.sort();
            assert_eq!(
                files,
                vec![
                    dir.path().join(ITEM_STORE).join("one.toml"),
                    dir.path().join(ITEM_STORE).join("two.toml"),
                ]
            );
        }
        other => panic!("expected item-unavailable/ambiguous, got {other:?}"),
    }
}

/// The reference stores the item, and the referenced artifact record is
/// not read: a record re-linked by hand after the reference was written
/// does not change the answer.
#[test]
fn a_reachable_reference_answers_with_its_stored_item_not_the_records_link() {
    let stored = item(ITEM, record_with_doi("10.1000/stored"));
    let relinked = item(OTHER_ITEM, record_with_doi("10.1000/relinked"));
    let dir = library_holding(&stored);
    write_item(dir.path(), &format!("{}.toml", relinked.id), &relinked);
    let mut entry = HashEntry {
        hash: hash("relinked bytes"),
        run: RunId::new("run-0"),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        tool_version: "0.0.0-test".to_string(),
        item: Some(stored.id.clone()),
        evidence: None,
    };
    let mut record = ArtifactRecord {
        id: artifact_id(ARTIFACT),
        item: Some(stored.id.clone()),
        path: "paper.pdf".to_string(),
        size: 1,
        modified_millis: 0,
        history: vec![entry.clone()],
    };
    write_artifact_record(dir.path(), &record);
    let written = reference(LIBRARY, dir.path(), ARTIFACT, ITEM);
    // The record is re-linked by hand after the reference was written.
    record.item = Some(relinked.id.clone());
    entry.item = Some(relinked.id.clone());
    record.history = vec![entry];
    write_artifact_record(dir.path(), &record);

    let reached = reach(&written);

    match reached {
        Reach::Reachable(found) => assert_eq!(found, stored),
        other => panic!("expected reachable, got {other:?}"),
    }
}

#[test]
fn a_reference_is_reachable_with_no_marker_at_its_root() {
    let held = item(ITEM, record_with_doi("10.1000/no-marker"));
    let dir = library_holding(&held);
    assert!(
        !dir.path().join(".borax.toml").exists(),
        "the fixture must hold no marker"
    );

    let reached = reach(&reference(LIBRARY, dir.path(), ARTIFACT, ITEM));

    match reached {
        Reach::Reachable(found) => assert_eq!(found, held),
        other => panic!("expected reachable, got {other:?}"),
    }
}

#[test]
fn libraries_answer_a_second_reference_to_a_root_from_its_first_read() {
    let original = item(ITEM, record_with_doi("10.1000/first-read"));
    let dir = library_holding(&original);
    let first = reference(LIBRARY, dir.path(), ARTIFACT, ITEM);
    let second = reference(LIBRARY, dir.path(), OTHER_ARTIFACT, ITEM);
    let libraries = Libraries::new();

    let before = libraries.reach(&first);
    let edited = item(ITEM, record_with_doi("10.1000/edited-between"));
    write_item(dir.path(), &format!("{}.toml", edited.id), &edited);
    let after = libraries.reach(&second);

    match (before, after) {
        (Reach::Reachable(first_read), Reach::Reachable(second_read)) => {
            assert_eq!(first_read, original);
            assert_eq!(
                second_read, original,
                "the second reach must answer with the first read's record"
            );
        }
        other => panic!("expected two reachable answers, got {other:?}"),
    }
}

#[test]
fn reaching_a_reference_writes_nothing() {
    let held = item(ITEM, record_with_doi("10.1000/writes-nothing"));
    let live = library_holding(&held);
    let bare = tempdir().unwrap();
    let other = library_with_identity(OTHER_LIBRARY);
    let (_guard, gone) = missing_root();
    let index_dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(index_dir.path()));
    let reference_to_live = reference(LIBRARY, live.path(), ARTIFACT, ITEM);
    let _ = index.put_reference(&hash("reach writes nothing"), &reference_to_live);
    let roots = [live.path(), bare.path(), other.path(), index_dir.path()];
    let before: Vec<_> = roots.iter().map(|root| snapshot(root)).collect();
    let libraries = Libraries::new();

    for target in [
        reference_to_live,
        reference(LIBRARY, bare.path(), ARTIFACT, ITEM),
        reference(LIBRARY, other.path(), ARTIFACT, ITEM),
        reference(LIBRARY, &gone, ARTIFACT, ITEM),
    ] {
        let _ = libraries.reach(&target);
    }

    let after: Vec<_> = roots.iter().map(|root| snapshot(root)).collect();
    assert_eq!(after, before);
    assert!(!bare.path().join(STATE_DIR).exists());
    assert!(!gone.exists());
}

// ---------------------------------------------------------------------
// 3.1 Settling (design D5, pure)
// ---------------------------------------------------------------------

fn statuses(reads: &[ReferenceRead]) -> Vec<ReferenceStatus> {
    reads.iter().map(|read| read.status.clone()).collect()
}

fn doi_identifier(doi: &str) -> Identifier {
    Identifier::Doi(Doi::parse(doi).unwrap())
}

fn arxiv_identifier(arxiv: &str) -> Identifier {
    Identifier::Arxiv(ArxivId::parse(arxiv).unwrap())
}

#[test]
fn settle_one_reachable_reference_answers() {
    let root = Path::new("/a");
    let held = item(ITEM, record_with_doi("10.1000/one"));
    let only = reference(LIBRARY, root, ARTIFACT, ITEM);

    let (answer, reads) = settle(vec![(only.clone(), Reach::Reachable(held.clone()))]);

    assert_eq!(answer, Some((only.clone(), held)));
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].reference, only);
    assert_eq!(reads[0].status, ReferenceStatus::Answered);
}

#[test]
fn settle_references_whose_items_carry_one_preferred_identifier_agree_and_the_first_answers() {
    let first = reference(LIBRARY, Path::new("/a"), ARTIFACT, ITEM);
    let second = reference(LIBRARY, Path::new("/b"), ARTIFACT, OTHER_ITEM);
    let first_item = item(ITEM, record_with_doi("10.1000/same"));
    let second_item = item(OTHER_ITEM, record_with_doi("10.1000/same"));

    let (answer, reads) = settle(vec![
        (first.clone(), Reach::Reachable(first_item.clone())),
        (second.clone(), Reach::Reachable(second_item)),
    ]);

    assert_eq!(answer, Some((first.clone(), first_item)));
    assert_eq!(reads[0].reference, first);
    assert_eq!(reads[1].reference, second);
    assert_eq!(
        statuses(&reads),
        vec![ReferenceStatus::Answered, ReferenceStatus::Agreed]
    );
}

#[test]
fn settle_references_whose_items_carry_different_identifiers_disagree_naming_each() {
    let first = reference(LIBRARY, Path::new("/a"), ARTIFACT, ITEM);
    let second = reference(LIBRARY, Path::new("/b"), ARTIFACT, OTHER_ITEM);

    let (answer, reads) = settle(vec![
        (
            first,
            Reach::Reachable(item(ITEM, record_with_doi("10.1000/x"))),
        ),
        (
            second,
            Reach::Reachable(item(OTHER_ITEM, record_with_doi("10.1000/y"))),
        ),
    ]);

    assert_eq!(answer, None);
    assert_eq!(
        statuses(&reads),
        vec![
            ReferenceStatus::Disagreeing {
                identifier: Some(doi_identifier("10.1000/x"))
            },
            ReferenceStatus::Disagreeing {
                identifier: Some(doi_identifier("10.1000/y"))
            },
        ]
    );
}

#[test]
fn settle_compares_the_preferred_identifier_so_a_doi_outranks_an_arxiv_id() {
    let first = reference(LIBRARY, Path::new("/a"), ARTIFACT, ITEM);
    let second = reference(LIBRARY, Path::new("/b"), ARTIFACT, OTHER_ITEM);
    let mut with_both = record_with_doi("10.1000/x");
    with_both.borax.arxiv = Some(ArxivId::parse("2401.12345").unwrap());

    let (answer, reads) = settle(vec![
        (first, Reach::Reachable(item(ITEM, with_both))),
        (
            second,
            Reach::Reachable(item(OTHER_ITEM, record_with_arxiv("2401.12345"))),
        ),
    ]);

    assert_eq!(answer, None);
    assert_eq!(
        statuses(&reads),
        vec![
            ReferenceStatus::Disagreeing {
                identifier: Some(doi_identifier("10.1000/x"))
            },
            ReferenceStatus::Disagreeing {
                identifier: Some(arxiv_identifier("2401.12345"))
            },
        ]
    );
}

#[test]
fn settle_items_carrying_no_identifier_agree_only_as_one_item_of_one_library() {
    // One item of one library, at two roots: agree.
    let same_item = item(ITEM, record_with_nothing());
    let (answer, reads) = settle(vec![
        (
            reference(LIBRARY, Path::new("/a"), ARTIFACT, ITEM),
            Reach::Reachable(same_item.clone()),
        ),
        (
            reference(LIBRARY, Path::new("/b"), ARTIFACT, ITEM),
            Reach::Reachable(same_item),
        ),
    ]);
    assert!(answer.is_some());
    assert_eq!(
        statuses(&reads),
        vec![ReferenceStatus::Answered, ReferenceStatus::Agreed]
    );

    // Two items of one library: disagree.
    let (answer, reads) = settle(vec![
        (
            reference(LIBRARY, Path::new("/a"), ARTIFACT, ITEM),
            Reach::Reachable(item(ITEM, record_with_nothing())),
        ),
        (
            reference(LIBRARY, Path::new("/b"), ARTIFACT, OTHER_ITEM),
            Reach::Reachable(item(OTHER_ITEM, record_with_nothing())),
        ),
    ]);
    assert_eq!(answer, None);
    assert_eq!(
        statuses(&reads),
        vec![
            ReferenceStatus::Disagreeing { identifier: None },
            ReferenceStatus::Disagreeing { identifier: None },
        ]
    );

    // One item identity in two libraries: disagree.
    let (answer, reads) = settle(vec![
        (
            reference(LIBRARY, Path::new("/a"), ARTIFACT, ITEM),
            Reach::Reachable(item(ITEM, record_with_nothing())),
        ),
        (
            reference(OTHER_LIBRARY, Path::new("/b"), ARTIFACT, ITEM),
            Reach::Reachable(item(ITEM, record_with_nothing())),
        ),
    ]);
    assert_eq!(answer, None);
    assert_eq!(
        statuses(&reads),
        vec![
            ReferenceStatus::Disagreeing { identifier: None },
            ReferenceStatus::Disagreeing { identifier: None },
        ]
    );
}

#[test]
fn settle_an_item_carrying_an_identifier_and_one_carrying_none_disagree() {
    let (answer, reads) = settle(vec![
        (
            reference(LIBRARY, Path::new("/a"), ARTIFACT, ITEM),
            Reach::Reachable(item(ITEM, record_with_doi("10.1000/x"))),
        ),
        (
            reference(LIBRARY, Path::new("/b"), ARTIFACT, OTHER_ITEM),
            Reach::Reachable(item(OTHER_ITEM, record_with_nothing())),
        ),
    ]);

    assert_eq!(answer, None);
    assert_eq!(
        statuses(&reads),
        vec![
            ReferenceStatus::Disagreeing {
                identifier: Some(doi_identifier("10.1000/x"))
            },
            ReferenceStatus::Disagreeing { identifier: None },
        ]
    );
}

#[test]
fn settle_keeps_each_unavailable_status_beside_an_answer() {
    let answering = reference(LIBRARY, Path::new("/a"), ARTIFACT, ITEM);
    let held = item(ITEM, record_with_doi("10.1000/x"));

    let (answer, reads) = settle(vec![
        (
            reference(LIBRARY, Path::new("/0"), ARTIFACT, ITEM),
            Reach::Unavailable(Unavailable::Missing),
        ),
        (answering.clone(), Reach::Reachable(held.clone())),
        (
            reference(LIBRARY, Path::new("/b"), ARTIFACT, ITEM),
            Reach::Unavailable(Unavailable::OtherLibrary {
                found: Some(library_id(OTHER_LIBRARY)),
            }),
        ),
        (
            reference(LIBRARY, Path::new("/c"), ARTIFACT, ITEM),
            Reach::ItemUnavailable(AcceptedItemCause::Dangling),
        ),
    ]);

    assert_eq!(answer, Some((answering, held)));
    assert_eq!(
        statuses(&reads),
        vec![
            ReferenceStatus::Unavailable(Unavailable::Missing),
            ReferenceStatus::Answered,
            ReferenceStatus::Unavailable(Unavailable::OtherLibrary {
                found: Some(library_id(OTHER_LIBRARY))
            }),
            ReferenceStatus::ItemUnavailable(AcceptedItemCause::Dangling),
        ]
    );
}

#[test]
fn settle_with_nothing_reachable_answers_nothing() {
    let (answer, reads) = settle(vec![
        (
            reference(LIBRARY, Path::new("/a"), ARTIFACT, ITEM),
            Reach::Unavailable(Unavailable::Missing),
        ),
        (
            reference(LIBRARY, Path::new("/b"), ARTIFACT, ITEM),
            Reach::ItemUnavailable(AcceptedItemCause::Dangling),
        ),
    ]);

    assert_eq!(answer, None);
    assert_eq!(
        statuses(&reads),
        vec![
            ReferenceStatus::Unavailable(Unavailable::Missing),
            ReferenceStatus::ItemUnavailable(AcceptedItemCause::Dangling),
        ]
    );
    assert_eq!(settle(Vec::new()).0, None);
}

#[test]
fn settle_two_locations_of_one_library_are_two_references() {
    let held = item(ITEM, record_with_doi("10.1000/two-places"));
    let first = reference(LIBRARY, Path::new("/one"), ARTIFACT, ITEM);
    let second = reference(LIBRARY, Path::new("/two"), ARTIFACT, ITEM);

    let (answer, reads) = settle(vec![
        (first.clone(), Reach::Reachable(held.clone())),
        (second.clone(), Reach::Reachable(held.clone())),
    ]);

    assert_eq!(answer, Some((first.clone(), held)));
    assert_eq!(reads.len(), 2);
    assert_eq!(reads[0].reference, first);
    assert_eq!(reads[1].reference, second);
    assert_eq!(
        statuses(&reads),
        vec![ReferenceStatus::Answered, ReferenceStatus::Agreed]
    );
}

// ---------------------------------------------------------------------
// 3.1 Keeping (design D3)
// ---------------------------------------------------------------------

fn file_index() -> (TempDir, ContentIndex<FileCache>) {
    let dir = tempdir().unwrap();
    let index = ContentIndex::new(FileCache::new(dir.path()));
    (dir, index)
}

fn reference_file(index_root: &Path, hash: &ContentHash, reference: &LibraryReference) -> PathBuf {
    entry_path(index_root, &reference_key(hash, &reference.name().unwrap())).unwrap()
}

#[test]
fn keep_writes_a_reference_the_entry_lacks() {
    let (_dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let hash = hash("keep lacks");
    let wanted = reference(LIBRARY, live.path(), ARTIFACT, ITEM);

    let kept = keep(&index, &hash, &wanted, &Libraries::new()).expect("a write is reported");

    assert_eq!(kept.reference, wanted);
    assert!(kept.repaired_from.is_empty());
    assert!(matches!(kept.write, CacheWrite::Written));
    assert_eq!(index.references(&hash), vec![wanted]);
}

#[test]
fn keep_writes_nothing_for_a_reference_held_with_the_same_item() {
    let (dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let hash = hash("keep same item");
    let wanted = reference(LIBRARY, live.path(), ARTIFACT, ITEM);
    let _ = index.put_reference(&hash, &wanted);
    let file = reference_file(dir.path(), &hash, &wanted);
    let before = fs::read(&file).unwrap();

    let kept = keep(&index, &hash, &wanted, &Libraries::new());

    assert!(kept.is_none(), "a reference already held reports nothing");
    assert_eq!(fs::read(&file).unwrap(), before);
}

#[test]
fn keep_rewrites_a_reference_whose_item_differs() {
    let (_dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let hash = hash("keep other item");
    let _ = index.put_reference(&hash, &reference(LIBRARY, live.path(), ARTIFACT, ITEM));
    let relinked = reference(LIBRARY, live.path(), ARTIFACT, OTHER_ITEM);

    let kept = keep(&index, &hash, &relinked, &Libraries::new()).expect("a rewrite is reported");

    assert!(matches!(kept.write, CacheWrite::Written));
    assert!(kept.repaired_from.is_empty());
    assert_eq!(index.references(&hash), vec![relinked]);
}

#[test]
fn keep_rewrites_an_unreadable_reference_file() {
    let (dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let hash = hash("keep unreadable");
    let wanted = reference(LIBRARY, live.path(), ARTIFACT, ITEM);
    let _ = index.put_reference(&hash, &wanted);
    let file = reference_file(dir.path(), &hash, &wanted);
    fs::write(&file, b"not json").unwrap();
    assert!(matches!(
        index.reference(&hash, &wanted),
        Fact::Unreadable(_)
    ));

    let kept = keep(&index, &hash, &wanted, &Libraries::new()).expect("a rewrite is reported");

    assert!(matches!(kept.write, CacheWrite::Written));
    assert_eq!(index.reference(&hash, &wanted), Fact::Held(wanted));
}

#[test]
fn keep_repairs_a_reference_to_the_same_artifact_at_a_missing_root() {
    let (dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let (_guard, gone) = missing_root();
    let hash = hash("keep repairs missing");
    let stale = reference(LIBRARY, &gone, ARTIFACT, ITEM);
    let _ = index.put_reference(&hash, &stale);
    let wanted = reference(LIBRARY, live.path(), ARTIFACT, ITEM);

    let kept = keep(&index, &hash, &wanted, &Libraries::new()).expect("a write is reported");

    assert!(matches!(kept.write, CacheWrite::Written));
    assert_eq!(kept.repaired_from, vec![gone]);
    assert!(!reference_file(dir.path(), &hash, &stale).exists());
    assert_eq!(index.references(&hash), vec![wanted]);
}

#[test]
fn keep_repairs_a_reference_whose_root_holds_no_library() {
    let (_dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let empty = tempdir().unwrap();
    let hash = hash("keep repairs no library");
    let _ = index.put_reference(&hash, &reference(LIBRARY, empty.path(), ARTIFACT, ITEM));
    let wanted = reference(LIBRARY, live.path(), ARTIFACT, ITEM);

    let kept = keep(&index, &hash, &wanted, &Libraries::new()).expect("a write is reported");

    assert_eq!(kept.repaired_from, vec![empty.path().to_path_buf()]);
    assert_eq!(index.references(&hash), vec![wanted]);
}

#[test]
fn keep_repairs_a_reference_whose_root_holds_another_library() {
    let (_dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let other = library_with_identity(OTHER_LIBRARY);
    let hash = hash("keep repairs other library");
    let _ = index.put_reference(&hash, &reference(LIBRARY, other.path(), ARTIFACT, ITEM));
    let wanted = reference(LIBRARY, live.path(), ARTIFACT, ITEM);

    let kept = keep(&index, &hash, &wanted, &Libraries::new()).expect("a write is reported");

    assert_eq!(kept.repaired_from, vec![other.path().to_path_buf()]);
    assert_eq!(index.references(&hash), vec![wanted]);
}

#[test]
fn keep_repairs_every_stale_root_of_the_same_artifact() {
    let (_dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let (_guard, gone) = missing_root();
    let empty = tempdir().unwrap();
    let hash = hash("keep repairs every");
    let _ = index.put_reference(&hash, &reference(LIBRARY, &gone, ARTIFACT, ITEM));
    let _ = index.put_reference(&hash, &reference(LIBRARY, empty.path(), ARTIFACT, ITEM));
    let wanted = reference(LIBRARY, live.path(), ARTIFACT, ITEM);

    let kept = keep(&index, &hash, &wanted, &Libraries::new()).expect("a write is reported");

    let mut repaired = kept.repaired_from;
    repaired.sort();
    let mut expected = vec![gone, empty.path().to_path_buf()];
    expected.sort();
    assert_eq!(repaired, expected);
    assert_eq!(index.references(&hash), vec![wanted]);
}

#[test]
fn keep_leaves_a_reference_at_a_root_holding_the_same_library() {
    let (_dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let copy = library_with_identity(LIBRARY);
    let hash = hash("keep leaves live copy");
    let beside = reference(LIBRARY, copy.path(), ARTIFACT, ITEM);
    let _ = index.put_reference(&hash, &beside);
    let wanted = reference(LIBRARY, live.path(), ARTIFACT, ITEM);

    let kept = keep(&index, &hash, &wanted, &Libraries::new()).expect("a write is reported");

    assert!(kept.repaired_from.is_empty());
    let mut found = index.references(&hash);
    found.sort_by(|left, right| left.root.cmp(&right.root));
    let mut expected = vec![beside, wanted];
    expected.sort_by(|left, right| left.root.cmp(&right.root));
    assert_eq!(found, expected);
}

#[cfg(unix)]
#[test]
fn keep_leaves_a_reference_at_an_unreadable_root() {
    let (_dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let parent = tempdir().unwrap();
    let hidden = parent.path().join("library");
    fs::create_dir_all(&hidden).unwrap();
    write_identity(&hidden, LIBRARY);
    let hash = hash("keep leaves unreadable");
    let beside = reference(LIBRARY, &hidden, ARTIFACT, ITEM);
    let _ = index.put_reference(&hash, &beside);
    let wanted = reference(LIBRARY, live.path(), ARTIFACT, ITEM);
    set_mode(parent.path(), 0o000);

    let unenforced = permissions_not_enforced(parent.path());
    let kept = keep(&index, &hash, &wanted, &Libraries::new());

    // Restored before the temp directory's own cleanup.
    set_mode(parent.path(), 0o755);

    if unenforced {
        eprintln!(
            "skipping keep_leaves_a_reference_at_an_unreadable_root: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }
    let kept = kept.expect("a write is reported");
    assert!(kept.repaired_from.is_empty());
    assert!(index.references(&hash).contains(&beside));
}

#[test]
fn keep_leaves_a_stale_reference_when_its_own_key_was_already_held() {
    let (_dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let (_guard, gone) = missing_root();
    let hash = hash("keep leaves stale when held");
    let stale = reference(LIBRARY, &gone, ARTIFACT, ITEM);
    let _ = index.put_reference(&hash, &stale);
    let _ = index.put_reference(&hash, &reference(LIBRARY, live.path(), ARTIFACT, ITEM));
    let relinked = reference(LIBRARY, live.path(), ARTIFACT, OTHER_ITEM);

    let kept = keep(&index, &hash, &relinked, &Libraries::new()).expect("a rewrite is reported");

    assert!(matches!(kept.write, CacheWrite::Written));
    assert!(kept.repaired_from.is_empty());
    assert_eq!(index.reference(&hash, &stale), Fact::Held(stale));
    assert_eq!(index.reference(&hash, &relinked), Fact::Held(relinked));
}

#[test]
fn keep_leaves_references_to_other_artifacts_and_other_libraries() {
    let (_dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let (_guard, gone) = missing_root();
    let hash = hash("keep leaves others");
    let other_artifact = reference(LIBRARY, &gone, OTHER_ARTIFACT, ITEM);
    let other_library = reference(OTHER_LIBRARY, &gone, ARTIFACT, ITEM);
    let _ = index.put_reference(&hash, &other_artifact);
    let _ = index.put_reference(&hash, &other_library);
    let wanted = reference(LIBRARY, live.path(), ARTIFACT, ITEM);

    let kept = keep(&index, &hash, &wanted, &Libraries::new()).expect("a write is reported");

    assert!(kept.repaired_from.is_empty());
    assert_eq!(
        index.reference(&hash, &other_artifact),
        Fact::Held(other_artifact)
    );
    assert_eq!(
        index.reference(&hash, &other_library),
        Fact::Held(other_library)
    );
    assert_eq!(index.references(&hash).len(), 3);
}

#[test]
fn keep_leaves_an_unreadable_reference_of_the_same_artifact() {
    let (dir, index) = file_index();
    let live = library_with_identity(LIBRARY);
    let hash = hash("keep leaves unreadable file");
    let _ = index.put_reference(
        &hash,
        &reference(LIBRARY, live.path(), OTHER_ARTIFACT, ITEM),
    );
    let strayed = reference_file(
        dir.path(),
        &hash,
        &reference(LIBRARY, live.path(), OTHER_ARTIFACT, ITEM),
    )
    .with_file_name(format!("{}.json", hash_bytes(b"some other key")));
    fs::write(&strayed, b"not json").unwrap();
    let wanted = reference(LIBRARY, live.path(), ARTIFACT, ITEM);

    let kept = keep(&index, &hash, &wanted, &Libraries::new()).expect("a write is reported");

    assert!(kept.repaired_from.is_empty());
    assert_eq!(fs::read(&strayed).unwrap(), b"not json");
}

/// A store over a [`MemoryCache`] that logs every call by operation and
/// key, and can be told to refuse writes or removals of a reference.
struct LoggingStore {
    inner: MemoryCache,
    log: Arc<Mutex<Vec<String>>>,
    fail_writes: Arc<AtomicBool>,
    fail_removes: Arc<AtomicBool>,
}

/// The test's handles on a [`LoggingStore`] once it is moved into its
/// index.
struct Handles {
    log: Arc<Mutex<Vec<String>>>,
    fail_writes: Arc<AtomicBool>,
    fail_removes: Arc<AtomicBool>,
}

impl LoggingStore {
    fn new() -> (LoggingStore, Handles) {
        let handles = Handles {
            log: Arc::new(Mutex::new(Vec::new())),
            fail_writes: Arc::new(AtomicBool::new(false)),
            fail_removes: Arc::new(AtomicBool::new(false)),
        };
        let store = LoggingStore {
            inner: MemoryCache::new(),
            log: Arc::clone(&handles.log),
            fail_writes: Arc::clone(&handles.fail_writes),
            fail_removes: Arc::clone(&handles.fail_removes),
        };
        (store, handles)
    }

    fn record(&self, operation: &str, key: &str) {
        self.log.lock().unwrap().push(format!("{operation} {key}"));
    }
}

impl IndexStore for LoggingStore {
    fn read_fact(&self, key: &str) -> Option<Result<String, String>> {
        self.record("read", key);
        self.inner.read_fact(key)
    }

    fn write_fact(&self, key: &str, text: &str) -> CacheWrite {
        self.record("write", key);
        if self.fail_writes.load(Ordering::SeqCst) {
            return CacheWrite::Failed {
                message: "write refused".to_string(),
            };
        }
        self.inner.write_fact(key, text)
    }

    fn list_facts(&self, prefix: &str) -> Result<Vec<String>, String> {
        self.record("list", prefix);
        self.inner.list_facts(prefix)
    }

    fn remove_fact(&self, key: &str) -> CacheWrite {
        self.record("remove", key);
        if self.fail_removes.load(Ordering::SeqCst) {
            return CacheWrite::Failed {
                message: "remove refused".to_string(),
            };
        }
        self.inner.remove_fact(key)
    }
}

/// An index over a [`LoggingStore`] holding one stale reference (a
/// missing root), with the log cleared after the seeding.
struct Staled {
    index: ContentIndex<LoggingStore>,
    log: Arc<Mutex<Vec<String>>>,
    fail_writes: Arc<AtomicBool>,
    fail_removes: Arc<AtomicBool>,
    hash: ContentHash,
    stale: LibraryReference,
    wanted: LibraryReference,
    _live: TempDir,
    _gone: TempDir,
}

fn staled(seed: &str) -> Staled {
    let (store, handles) = LoggingStore::new();
    let index = ContentIndex::new(store);
    let live = library_with_identity(LIBRARY);
    let (gone_guard, gone) = missing_root();
    let hash = hash(seed);
    let stale = reference(LIBRARY, &gone, ARTIFACT, ITEM);
    let _ = index.put_reference(&hash, &stale);
    handles.log.lock().unwrap().clear();
    let wanted = reference(LIBRARY, live.path(), ARTIFACT, ITEM);
    Staled {
        index,
        log: handles.log,
        fail_writes: handles.fail_writes,
        fail_removes: handles.fail_removes,
        hash,
        stale,
        wanted,
        _live: live,
        _gone: gone_guard,
    }
}

fn key_of(hash: &ContentHash, reference: &LibraryReference) -> String {
    reference_key(hash, &reference.name().unwrap())
}

#[test]
fn keep_writes_the_new_reference_before_removing_a_stale_one() {
    let setup = staled("keep order");

    let kept = keep(&setup.index, &setup.hash, &setup.wanted, &Libraries::new())
        .expect("a write is reported");

    assert_eq!(kept.repaired_from, vec![setup.stale.root.clone()]);
    let log = setup.log.lock().unwrap();
    let changes: Vec<&String> = log
        .iter()
        .filter(|call| call.starts_with("write ") || call.starts_with("remove "))
        .collect();
    assert_eq!(
        changes,
        vec![
            &format!("write {}", key_of(&setup.hash, &setup.wanted)),
            &format!("remove {}", key_of(&setup.hash, &setup.stale)),
        ]
    );
}

#[test]
fn keep_whose_write_fails_removes_nothing() {
    let setup = staled("keep write fails");
    setup.fail_writes.store(true, Ordering::SeqCst);

    let kept = keep(&setup.index, &setup.hash, &setup.wanted, &Libraries::new())
        .expect("a failed write is reported");

    assert!(matches!(kept.write, CacheWrite::Failed { .. }));
    assert!(kept.repaired_from.is_empty());
    let log = setup.log.lock().unwrap();
    assert!(
        !log.iter().any(|call| call.starts_with("remove ")),
        "nothing may be removed after a failed write: {log:?}"
    );
    drop(log);
    assert_eq!(
        setup.index.reference(&setup.hash, &setup.stale),
        Fact::Held(setup.stale.clone())
    );
}

#[test]
fn keep_whose_removal_fails_reports_failed_naming_the_file_and_leaves_it() {
    let setup = staled("keep remove fails");
    setup.fail_removes.store(true, Ordering::SeqCst);

    let kept = keep(&setup.index, &setup.hash, &setup.wanted, &Libraries::new())
        .expect("a failed removal is reported");

    match kept.write {
        CacheWrite::Failed { message } => assert!(
            message.contains(&setup.stale.name().unwrap()),
            "the message must name the file it could not remove: {message}"
        ),
        other => panic!("expected failed, got {other:?}"),
    }
    assert_eq!(
        setup.index.reference(&setup.hash, &setup.stale),
        Fact::Held(setup.stale.clone())
    );
    assert_eq!(
        setup.index.reference(&setup.hash, &setup.wanted),
        Fact::Held(setup.wanted.clone()),
        "the new reference stays written"
    );
}

#[test]
fn keep_reads_and_writes_no_evidence_fact() {
    let setup = staled("keep no evidence");
    let libraries = Libraries::new();

    let _ = keep(&setup.index, &setup.hash, &setup.wanted, &libraries);
    let _ = keep(&setup.index, &setup.hash, &setup.wanted, &libraries);

    let log = setup.log.lock().unwrap();
    assert!(!log.is_empty());
    for call in log.iter() {
        assert!(
            !call.contains("extracted") && !call.contains("operator"),
            "a reference write touched an evidence fact: {call}"
        );
        assert!(
            call.contains(&references_prefix(&setup.hash)),
            "every call must stay under the refs directory: {call}"
        );
    }
}

/// A store that makes two removals meet: `remove_fact` waits on a
/// barrier of two before removing.
struct BarrierStore {
    inner: MemoryCache,
    barrier: Barrier,
    removals: Mutex<Vec<String>>,
}

struct SharedStore(Arc<BarrierStore>);

impl IndexStore for SharedStore {
    fn read_fact(&self, key: &str) -> Option<Result<String, String>> {
        self.0.inner.read_fact(key)
    }

    fn write_fact(&self, key: &str, text: &str) -> CacheWrite {
        self.0.inner.write_fact(key, text)
    }

    fn list_facts(&self, prefix: &str) -> Result<Vec<String>, String> {
        self.0.inner.list_facts(prefix)
    }

    fn remove_fact(&self, key: &str) -> CacheWrite {
        self.0.barrier.wait();
        self.0.removals.lock().unwrap().push(key.to_string());
        self.0.inner.remove_fact(key)
    }
}

/// design D3's concurrency seam. Both writers have listed `refs/`,
/// found S and written their own file before either removes S, because
/// `remove_fact` waits for both. No sleep is involved.
#[test]
fn two_keeps_at_live_roots_racing_over_one_stale_reference_keep_both_and_remove_it() {
    let shared = Arc::new(BarrierStore {
        inner: MemoryCache::new(),
        barrier: Barrier::new(2),
        removals: Mutex::new(Vec::new()),
    });
    let live_a = library_with_identity(LIBRARY);
    let live_b = library_with_identity(LIBRARY);
    let (_guard, gone) = missing_root();
    let hash = hash("keep race");
    let stale = reference(LIBRARY, &gone, ARTIFACT, ITEM);
    let first = reference(LIBRARY, live_a.path(), ARTIFACT, ITEM);
    let second = reference(LIBRARY, live_b.path(), ARTIFACT, ITEM);
    let _ = ContentIndex::new(SharedStore(Arc::clone(&shared))).put_reference(&hash, &stale);

    let (sender, receiver) = mpsc::channel();
    for wanted in [first.clone(), second.clone()] {
        let sender = sender.clone();
        let shared = Arc::clone(&shared);
        let hash = hash.clone();
        std::thread::spawn(move || {
            let index = ContentIndex::new(SharedStore(shared));
            let kept = keep(&index, &hash, &wanted, &Libraries::new());
            let _ = sender.send(kept.map(|kept| (kept.repaired_from, kept.write)));
        });
    }
    drop(sender);
    let mut outcomes = Vec::new();
    for _ in 0..2 {
        outcomes.push(
            receiver
                .recv_timeout(Duration::from_secs(30))
                .expect("both keeps must finish: each must remove S through the barrier")
                .expect("each keep writes its own reference"),
        );
    }

    for (repaired_from, write) in &outcomes {
        assert!(matches!(write, CacheWrite::Written), "got {write:?}");
        assert_eq!(repaired_from, &vec![gone.clone()]);
    }
    let index = ContentIndex::new(SharedStore(Arc::clone(&shared)));
    let mut remaining = index.references(&hash);
    remaining.sort_by(|left, right| left.root.cmp(&right.root));
    let mut expected = vec![first, second];
    expected.sort_by(|left, right| left.root.cmp(&right.root));
    assert_eq!(remaining, expected);
    assert_eq!(index.reference(&hash, &stale), Fact::Absent);
    assert_eq!(
        *shared.removals.lock().unwrap(),
        vec![key_of(&hash, &stale); 2],
        "both writers removed S, the second finding it already absent"
    );
}
