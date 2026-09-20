#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use borax::library::{
    ARTIFACT_STORE, ArtifactStore, ITEM_STORE, ItemStore, STATE_DIR, artifacts, contains, missing,
    orphans,
};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{Doi, Identifier};
use borax_core::library::{ArtifactId, ArtifactRecord, HashEntry, Item, ItemId, RunId};
use borax_core::record::{EntryType, Record};
use tempfile::tempdir;

// ---------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------

const UUID_A: &str = "018f2b36-7f21-7abc-8def-0123456789ab";
const UUID_B: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef0";

fn item_id(text: &str) -> ItemId {
    ItemId::parse(text).unwrap()
}

fn artifact_id(text: &str) -> ArtifactId {
    ArtifactId::parse(text).unwrap()
}

fn hash(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// A minimal but valid record of `entry_type`, enough to round-trip.
fn minimal_record(entry_type: EntryType) -> Record {
    let mut record = Record::new(entry_type);
    record.title = Some("A Title".to_string());
    record
}

fn hash_entry(seed: &str, run: &str) -> HashEntry {
    HashEntry {
        hash: hash(seed),
        run: RunId::new(run),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        tool_version: "0.6.0-test".to_string(),
    }
}

fn artifact_record(
    id: ArtifactId,
    item: Option<ItemId>,
    path: &str,
    history: Vec<HashEntry>,
) -> ArtifactRecord {
    ArtifactRecord {
        id,
        item,
        path: path.to_string(),
        size: 100,
        modified_millis: 0,
        history,
    }
}

/// Writes `item` under `root`'s item store, at `file_name`. The store
/// finds items by the `id` inside them, so `file_name` need not agree
/// with `item.id` — several tests below rely on exactly that.
fn write_item(root: &Path, file_name: &str, item: &Item) {
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    fs::write(items.join(file_name), item.to_toml()).unwrap();
}

/// Writes `record` under `root`'s artifact-record store, named for its
/// own identity as an applying run would name it.
fn write_artifact_record(root: &Path, record: &ArtifactRecord) {
    let dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{}.toml", record.id)), record.to_toml()).unwrap();
}

// ---------------------------------------------------------------------
// 2.2: contains() — lexical containment, symlinks not resolved
// ---------------------------------------------------------------------

#[test]
fn contains_a_path_under_the_root() {
    let root = Path::new("/lib");
    assert!(contains(root, Path::new("/lib/sub/paper.pdf")));
}

#[test]
fn contains_is_false_for_a_path_outside_the_root() {
    let root = Path::new("/lib");
    assert!(!contains(root, Path::new("/elsewhere/paper.pdf")));
}

/// Containment is by path component, not by string prefix: a sibling
/// directory whose name merely starts with the root's is still outside
/// it.
#[test]
fn contains_is_false_for_a_directory_sharing_only_a_string_prefix() {
    let root = Path::new("/lib");
    assert!(!contains(root, Path::new("/library-secondary/paper.pdf")));
}

#[test]
fn contains_the_root_itself() {
    let root = Path::new("/lib");
    assert!(contains(root, root));
}

/// A symlink inside the tree is inside it by its own path, whatever it
/// points at — containment does not resolve it. Built with a real
/// symlink on disk so the assertion cannot be satisfied by an
/// implementation that never touches the filesystem for the wrong
/// reason.
#[cfg(unix)]
#[test]
fn contains_a_symlink_by_its_own_path_inside_the_tree() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("lib");
    fs::create_dir_all(&root).unwrap();
    let outside = dir.path().join("outside.pdf");
    fs::write(&outside, b"bytes").unwrap();
    let link = root.join("link.pdf");
    std::os::unix::fs::symlink(&outside, &link).unwrap();

    assert!(contains(&root, &link));
}

/// The other half of the same rule: the symlink's target keeps its own
/// path, which lies outside the root and stays outside it — the link
/// inside the tree does not bring it in.
#[cfg(unix)]
#[test]
fn contains_does_not_bring_a_symlinks_target_into_the_library() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("lib");
    fs::create_dir_all(&root).unwrap();
    let outside = dir.path().join("outside.pdf");
    fs::write(&outside, b"bytes").unwrap();
    let link = root.join("link.pdf");
    std::os::unix::fs::symlink(&outside, &link).unwrap();

    assert!(!contains(&root, &outside));
}

// ---------------------------------------------------------------------
// 2.4: artifacts() — the walk
// ---------------------------------------------------------------------

#[test]
fn artifacts_finds_every_pdf_at_any_depth_case_insensitively_sorted() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("sub/deeper")).unwrap();
    fs::write(root.join("b.pdf"), b"").unwrap();
    fs::write(root.join("sub/A.PDF"), b"").unwrap();
    fs::write(root.join("sub/deeper/c.Pdf"), b"").unwrap();

    let found = artifacts(root);

    assert_eq!(
        found,
        vec![
            root.join("b.pdf"),
            root.join("sub/A.PDF"),
            root.join("sub/deeper/c.Pdf"),
        ],
        "got {found:?}"
    );
}

#[test]
fn artifacts_excludes_everything_under_the_state_directory() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".borax/artifacts")).unwrap();
    fs::write(root.join(".borax/artifacts/x.pdf"), b"").unwrap();
    fs::write(root.join("real.pdf"), b"").unwrap();

    assert_eq!(artifacts(root), vec![root.join("real.pdf")]);
}

/// A PDF under `items/` is not an artifact whatever its extension: the
/// item store's own files are not the library's documents.
#[test]
fn artifacts_excludes_everything_under_the_item_store() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("items")).unwrap();
    fs::write(root.join("items/smith2024.pdf"), b"").unwrap();
    fs::write(root.join("real.pdf"), b"").unwrap();

    assert_eq!(artifacts(root), vec![root.join("real.pdf")]);
}

#[test]
fn artifacts_excludes_a_bib_sidecar() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"").unwrap();
    fs::write(root.join("paper.pdf.bib"), b"").unwrap();

    assert_eq!(artifacts(root), vec![root.join("paper.pdf")]);
}

/// Built with a real symlink, following the same reasoning as
/// `contains_a_symlink_by_its_own_path_inside_the_tree`: a link is
/// never counted, whether or not its target is a PDF.
#[cfg(unix)]
#[test]
fn artifacts_excludes_a_symlink() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("real.pdf"), b"").unwrap();
    std::os::unix::fs::symlink(root.join("real.pdf"), root.join("link.pdf")).unwrap();

    assert_eq!(artifacts(root), vec![root.join("real.pdf")]);
}

#[test]
fn artifacts_excludes_a_file_of_another_extension() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"").unwrap();
    fs::write(root.join("notes.txt"), b"").unwrap();

    assert_eq!(artifacts(root), vec![root.join("paper.pdf")]);
}

// ---------------------------------------------------------------------
// 2.5: artifacts() stops at a nested marker
// ---------------------------------------------------------------------

/// The walk's own half of design D15: files beneath a subdirectory
/// holding its own `.borax.toml` are not this library's artifacts. The
/// same rule for the orphan count, for an applying run's admissions and
/// for reconcile belongs with those operations once they exist.
#[test]
fn artifacts_excludes_everything_beneath_a_nested_marker() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("nested/.borax.toml"), b"").unwrap();
    fs::write(root.join("nested/inner.pdf"), b"").unwrap();
    fs::write(root.join("outer.pdf"), b"").unwrap();

    assert_eq!(artifacts(root), vec![root.join("outer.pdf")]);
}

// ---------------------------------------------------------------------
// 3.1: ItemStore
// ---------------------------------------------------------------------

#[test]
fn item_store_reads_every_items_toml_file_that_parses() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let a = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    let b = Item {
        id: item_id(UUID_B),
        record: minimal_record(EntryType::Book),
    };
    write_item(root, &format!("a.{UUID_A}.toml"), &a);
    write_item(root, &format!("b.{UUID_B}.toml"), &b);

    let store = ItemStore::read(root);

    assert_eq!(store.len(), 2);
    assert!(store.faults.is_empty(), "got {:?}", store.faults);
}

/// The store finds an item by the `id` field inside it, never by
/// parsing the file name — this file's name carries a UUID that names
/// no item at all.
#[test]
fn item_store_finds_an_item_by_the_id_inside_it_not_by_its_file_name() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("mislabeled.{UUID_B}.toml"), &item);

    let store = ItemStore::read(root);

    assert_eq!(store.by_id(&item.id), Some(&item));
}

/// An item file renamed by hand — to a name with no UUID in it at all
/// — is still the item it was.
#[test]
fn item_store_finds_an_item_renamed_by_hand() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, "notes-on-this.toml", &item);

    let store = ItemStore::read(root);

    assert_eq!(store.by_id(&item.id), Some(&item));
}

/// A `.toml` that is not an item record is reported as a fault rather
/// than skipped in silence, and costs the store nothing else: the
/// well-formed item beside it is still read.
#[test]
fn item_store_reports_a_toml_file_that_is_not_an_item_record_as_a_fault() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    fs::write(items.join("bad.toml"), b"this is not [ valid toml").unwrap();
    let good = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("good.{UUID_A}.toml"), &good);

    let store = ItemStore::read(root);

    assert_eq!(store.len(), 1);
    assert_eq!(store.by_id(&good.id), Some(&good));
    assert_eq!(store.faults.len(), 1, "got {:?}", store.faults);
    assert_eq!(store.faults[0].path, items.join("bad.toml"));
}

#[test]
fn item_store_finds_an_item_by_an_identifier_its_record_carries() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let mut record = minimal_record(EntryType::Article);
    record.doi = Some(Doi::parse("10.1000/found").unwrap());
    let item = Item {
        id: item_id(UUID_A),
        record,
    };
    write_item(root, &format!("a.{UUID_A}.toml"), &item);

    let store = ItemStore::read(root);

    let identifier = Identifier::Doi(Doi::parse("10.1000/found").unwrap());
    assert_eq!(store.by_identifier(&identifier), Some(&item));
}

#[test]
fn item_store_by_identifier_is_none_when_no_record_carries_it() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let store = ItemStore::read(root);

    let identifier = Identifier::Doi(Doi::parse("10.1000/absent").unwrap());
    assert_eq!(store.by_identifier(&identifier), None);
}

#[test]
fn item_store_of_an_absent_item_store_directory_is_empty() {
    let dir = tempdir().unwrap();

    let store = ItemStore::read(dir.path());

    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
}

#[test]
fn item_store_iter_yields_every_item_read() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let a = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("a.{UUID_A}.toml"), &a);

    let store = ItemStore::read(root);

    let ids: Vec<&ItemId> = store.iter().map(|item| &item.id).collect();
    assert_eq!(ids, vec![&a.id]);
}

// ---------------------------------------------------------------------
// 3.2: ArtifactStore
// ---------------------------------------------------------------------

#[test]
fn artifact_store_reads_every_borax_artifacts_toml_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let a = artifact_record(
        artifact_id(UUID_A),
        None,
        "a.pdf",
        vec![hash_entry("a", "run-1")],
    );
    let b = artifact_record(
        artifact_id(UUID_B),
        None,
        "b.pdf",
        vec![hash_entry("b", "run-1")],
    );
    write_artifact_record(root, &a);
    write_artifact_record(root, &b);

    let store = ArtifactStore::read(root);

    assert_eq!(store.len(), 2);
    assert!(store.faults.is_empty(), "got {:?}", store.faults);
}

#[test]
fn artifact_store_answers_by_artifact_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let a = artifact_record(
        artifact_id(UUID_A),
        None,
        "a.pdf",
        vec![hash_entry("a", "run-1")],
    );
    write_artifact_record(root, &a);

    let store = ArtifactStore::read(root);

    assert_eq!(store.by_id(&a.id), Some(&a));
}

#[test]
fn artifact_store_answers_by_any_hash_in_a_records_history() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "a.pdf",
        vec![
            hash_entry("old-bytes", "run-1"),
            hash_entry("new-bytes", "run-2"),
        ],
    );
    write_artifact_record(root, &record);

    let store = ArtifactStore::read(root);

    assert_eq!(store.by_hash(&hash("old-bytes")), vec![&record]);
    assert_eq!(store.by_hash(&hash("new-bytes")), vec![&record]);
    assert_eq!(
        store.by_hash(&hash("never-recorded")),
        Vec::<&ArtifactRecord>::new()
    );
}

#[test]
fn artifact_store_answers_by_item_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let item = item_id(UUID_A);
    let a = artifact_record(
        artifact_id(UUID_B),
        Some(item.clone()),
        "a.pdf",
        vec![hash_entry("a", "run-1")],
    );
    write_artifact_record(root, &a);

    let store = ArtifactStore::read(root);

    assert_eq!(store.by_item(&item), vec![&a]);
}

#[test]
fn artifact_store_answers_by_last_known_path() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let a = artifact_record(
        artifact_id(UUID_A),
        None,
        "sub/a.pdf",
        vec![hash_entry("a", "run-1")],
    );
    write_artifact_record(root, &a);

    let store = ArtifactStore::read(root);

    assert_eq!(store.by_path("sub/a.pdf"), Some(&a));
    assert_eq!(store.by_path("sub/other.pdf"), None);
}

/// One file that does not parse costs its own record and no other: the
/// good record beside it is still readable and answers by every one of
/// its keys.
#[test]
fn artifact_store_a_file_that_does_not_parse_costs_its_own_record_and_no_other() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let store_dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&store_dir).unwrap();
    fs::write(store_dir.join("bad.toml"), b"this is not [ valid toml").unwrap();
    let good = artifact_record(
        artifact_id(UUID_A),
        None,
        "a.pdf",
        vec![hash_entry("a", "run-1")],
    );
    write_artifact_record(root, &good);

    let store = ArtifactStore::read(root);

    assert_eq!(store.len(), 1);
    assert_eq!(store.by_id(&good.id), Some(&good));
    assert_eq!(store.faults.len(), 1, "got {:?}", store.faults);
    assert_eq!(store.faults[0].path, store_dir.join("bad.toml"));
}

#[test]
fn artifact_store_of_an_absent_state_directory_is_empty() {
    let dir = tempdir().unwrap();

    let store = ArtifactStore::read(dir.path());

    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
}

#[test]
fn artifact_store_iter_yields_every_record_read() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let a = artifact_record(
        artifact_id(UUID_A),
        None,
        "a.pdf",
        vec![hash_entry("a", "run-1")],
    );
    write_artifact_record(root, &a);

    let store = ArtifactStore::read(root);

    let ids: Vec<&ArtifactId> = store.iter().map(|record| &record.id).collect();
    assert_eq!(ids, vec![&a.id]);
}

// ---------------------------------------------------------------------
// 3.4: orphans() and missing()
// ---------------------------------------------------------------------

#[test]
fn orphans_are_artifacts_no_record_names() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("orphan.pdf"), b"").unwrap();
    fs::write(root.join("recorded.pdf"), b"").unwrap();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "recorded.pdf",
        vec![hash_entry("r", "run-1")],
    );
    write_artifact_record(root, &record);
    let store = ArtifactStore::read(root);
    let found = artifacts(root);

    let result = orphans(root, &found, &store);

    assert_eq!(result, vec![root.join("orphan.pdf")]);
}

/// The asymmetry that matters: a record whose last-known path holds no
/// file is not an orphan. The artifact walk never finds `gone.pdf` — it
/// is not there — so `orphans` must not manufacture an entry for it out
/// of the record alone.
#[test]
fn a_record_whose_path_holds_no_file_is_not_an_orphan() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "gone.pdf",
        vec![hash_entry("g", "run-1")],
    );
    write_artifact_record(root, &record);
    let store = ArtifactStore::read(root);
    let found = artifacts(root);

    let result = orphans(root, &found, &store);

    assert_eq!(result, Vec::<PathBuf>::new(), "got {result:?}");
}

/// The other direction of the same asymmetry: an artifact with a record
/// is not an orphan, whether or not the record's file is where it says.
#[test]
fn an_artifact_with_a_record_is_not_an_orphan() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("recorded.pdf"), b"").unwrap();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "recorded.pdf",
        vec![hash_entry("r", "run-1")],
    );
    write_artifact_record(root, &record);
    let store = ArtifactStore::read(root);
    let found = artifacts(root);

    let result = orphans(root, &found, &store);

    assert_eq!(result, Vec::<PathBuf>::new(), "got {result:?}");
}

#[test]
fn missing_is_a_record_whose_last_known_path_holds_no_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "gone.pdf",
        vec![hash_entry("g", "run-1")],
    );
    write_artifact_record(root, &record);
    let store = ArtifactStore::read(root);
    let exists: &dyn Fn(&Path) -> bool = &|_: &Path| false;

    let result = missing(root, &store, exists);

    assert_eq!(result, vec![&record]);
}

#[test]
fn missing_excludes_a_record_whose_path_holds_a_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("present.pdf"), b"").unwrap();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "present.pdf",
        vec![hash_entry("p", "run-1")],
    );
    write_artifact_record(root, &record);
    let store = ArtifactStore::read(root);
    let exists: &dyn Fn(&Path) -> bool = &|path: &Path| path.exists();

    let result = missing(root, &store, exists);

    assert_eq!(result, Vec::<&ArtifactRecord>::new(), "got {result:?}");
}
