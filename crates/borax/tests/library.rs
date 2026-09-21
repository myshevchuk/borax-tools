#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use borax::event::{Event, Finding, Repair};
use borax::library::{
    ARTIFACT_STORE, ArtifactStore, ITEM_STORE, ItemStore, STATE_DIR, artifacts, contains, missing,
    orphans, reconcile, reconciliation_events, relative_to, survey, validate,
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
const UUID_C: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef1";
const UUID_D: &str = "0198c4de-1a2b-7c3d-9e4f-56789abcdef2";

/// The run stamp every reconcile fixture below uses, following
/// `hash_entry`'s own hard-coded timestamp and tool version.
const TIMESTAMP: &str = "2026-01-01T00:00:00Z";
const TOOL_VERSION: &str = "0.6.0-test";

fn item_id(text: &str) -> ItemId {
    ItemId::parse(text).unwrap()
}

fn artifact_id(text: &str) -> ArtifactId {
    ArtifactId::parse(text).unwrap()
}

fn hash(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// A [`ContentHash`] built from arbitrary text rather than from real
/// bytes, bypassing `hash_bytes` so a malformed shape can be written to
/// an artifact record — following the shape of the one in
/// `borax-core/tests/library.rs`.
fn malformed_hash(text: &str) -> ContentHash {
    serde_json::from_value(serde_json::json!(text)).unwrap()
}

/// Every file under `root`, with its bytes, keyed by path — including
/// whatever `.borax/` holds — so a tree can be compared byte for byte
/// before and after an operation rather than merely by whether it still
/// exists.
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

/// `path`'s size and modification time, in the form an artifact record
/// stores them.
fn stat(path: &Path) -> (u64, i64) {
    let metadata = fs::metadata(path).unwrap();
    let modified = metadata.modified().unwrap();
    let modified_millis = match modified.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as i64,
        Err(err) => -(err.duration().as_millis() as i64),
    };
    (metadata.len(), modified_millis)
}

/// An [`ArtifactRecord`] whose `size` and `modified_millis` are the
/// real, current stat of the file at `relative` under `root` — unlike
/// [`artifact_record`]'s hard-coded `size: 100, modified_millis: 0`,
/// this is what makes the fast path actually match.
fn fresh_record(
    root: &Path,
    id: ArtifactId,
    item: Option<ItemId>,
    relative: &str,
    history: Vec<HashEntry>,
) -> ArtifactRecord {
    let (size, modified_millis) = stat(&relative_to(root, relative));
    ArtifactRecord {
        id,
        item,
        path: relative.to_string(),
        size,
        modified_millis,
        history,
    }
}

/// Every file under `root`'s item store and artifact-record store, with
/// its bytes and its modification time, keyed by path.
///
/// [`snapshot`] compares bytes alone; task 6.5 and 6.2a also care about
/// the modification time reconciliation was run to protect, so this is
/// a second helper rather than a change to that one. Scoped to
/// `items/` and `.borax/artifacts/`, which is what those tasks name.
fn snapshot_with_mtime(root: &Path) -> BTreeMap<PathBuf, (Vec<u8>, SystemTime)> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, (Vec<u8>, SystemTime)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.is_file() {
                let bytes = fs::read(&path).unwrap();
                let modified = fs::metadata(&path).unwrap().modified().unwrap();
                out.insert(path.clone(), (bytes, modified));
            }
        }
    }

    let mut found = BTreeMap::new();
    walk(&root.join(ITEM_STORE), &mut found);
    walk(&root.join(STATE_DIR).join(ARTIFACT_STORE), &mut found);
    found
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

// ---------------------------------------------------------------------
// 2.5 (re-scoped half): survey() keeps a nested library's subtree out
// of the orphan count too, and names the nested root
// ---------------------------------------------------------------------

/// The other half of design D15, at the level `survey()` reports: a
/// nested `.borax.toml` takes its subtree out of the orphan count the
/// same way `artifacts_excludes_everything_beneath_a_nested_marker`
/// already pins for the walk, and the nested root shows up in
/// `Survey::nested` so a reader can tell a small library from a
/// subdivided one.
#[test]
fn survey_excludes_a_nested_librarys_subtree_from_orphans_and_names_it_nested() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("nested/.borax.toml"), b"").unwrap();
    fs::write(root.join("nested/inner.pdf"), b"").unwrap();
    fs::write(root.join("outer.pdf"), b"").unwrap();

    let found = survey(root);

    assert_eq!(
        found.artifacts,
        vec![root.join("outer.pdf")],
        "got {:?}",
        found.artifacts
    );
    assert_eq!(
        found.orphans,
        vec![root.join("outer.pdf")],
        "got {:?}",
        found.orphans
    );
    assert_eq!(
        found.nested,
        vec!["nested".to_string()],
        "got {:?}",
        found.nested
    );
}

// ---------------------------------------------------------------------
// 5.1: validate() — one test per finding
// ---------------------------------------------------------------------

/// Scenario "A dangling item link": an artifact record naming an item
/// the library does not hold.
#[test]
fn validate_reports_a_dangling_item_link() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_B)),
        "orphaned-link.pdf",
        vec![hash_entry("bytes", "run-1")],
    );
    write_artifact_record(root, &record);

    let result = validate(root);

    assert_eq!(
        result.findings,
        vec![(
            root.join(STATE_DIR)
                .join(ARTIFACT_STORE)
                .join(format!("{UUID_A}.toml")),
            Finding::DanglingItem {
                item: UUID_B.to_string()
            }
        )],
        "got {:?}",
        result.findings
    );
}

/// Two item files carrying one item identity: the second is reported,
/// naming the first as `other`.
///
/// Both file names carry `UUID_A` — matching the identity inside them —
/// so this isolates the duplicate-identity finding from
/// [`Finding::NameDisagrees`], which a name carrying no UUID at all
/// would also trigger.
#[test]
fn validate_reports_two_item_files_carrying_one_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("a-first.{UUID_A}.toml"), &item);
    write_item(root, &format!("b-second.{UUID_A}.toml"), &item);

    let result = validate(root);

    assert_eq!(
        result.findings,
        vec![(
            root.join(ITEM_STORE)
                .join(format!("b-second.{UUID_A}.toml")),
            Finding::DuplicateIdentity {
                id: UUID_A.to_string(),
                other: root.join(ITEM_STORE).join(format!("a-first.{UUID_A}.toml")),
            }
        )],
        "got {:?}",
        result.findings
    );
}

/// The same finding on the artifact-record side: two records under one
/// artifact identity. Both file names carry `UUID_A`, for the same
/// reason as the item-store version above.
#[test]
fn validate_reports_two_artifact_records_carrying_one_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "a.pdf",
        vec![hash_entry("a", "run-1")],
    );
    let dir_path = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&dir_path).unwrap();
    fs::write(
        dir_path.join(format!("a-first.{UUID_A}.toml")),
        record.to_toml(),
    )
    .unwrap();
    let other_record = artifact_record(
        artifact_id(UUID_A),
        None,
        "b.pdf",
        vec![hash_entry("b", "run-1")],
    );
    fs::write(
        dir_path.join(format!("b-second.{UUID_A}.toml")),
        other_record.to_toml(),
    )
    .unwrap();

    let result = validate(root);

    assert_eq!(
        result.findings,
        vec![(
            dir_path.join(format!("b-second.{UUID_A}.toml")),
            Finding::DuplicateIdentity {
                id: UUID_A.to_string(),
                other: dir_path.join(format!("a-first.{UUID_A}.toml")),
            }
        )],
        "got {:?}",
        result.findings
    );
}

/// Scenario "An item file renamed by hand": the store still finds the
/// item by the `id` inside it (group 3's guarantee), and `validate`
/// separately reports the name — carrying no UUID at all — as
/// disagreeing with that field.
#[test]
fn validate_reports_an_item_renamed_by_hand_as_disagreeing_with_its_id() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, "notes-on-this.toml", &item);

    let result = validate(root);

    assert_eq!(
        result.findings,
        vec![(
            root.join(ITEM_STORE).join("notes-on-this.toml"),
            Finding::NameDisagrees {
                id: UUID_A.to_string()
            }
        )],
        "got {:?}",
        result.findings
    );
}

/// The artifact-record side of the same finding: the file is named for
/// a UUID the record does not carry.
#[test]
fn validate_reports_an_artifact_record_whose_file_name_disagrees_with_the_id_inside_it() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "a.pdf",
        vec![hash_entry("a", "run-1")],
    );
    let dir_path = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&dir_path).unwrap();
    // Named for UUID_B, but the record inside carries UUID_A.
    let path = dir_path.join(format!("{UUID_B}.toml"));
    fs::write(&path, record.to_toml()).unwrap();

    let result = validate(root);

    assert_eq!(
        result.findings,
        vec![(
            path,
            Finding::NameDisagrees {
                id: UUID_A.to_string()
            }
        )],
        "got {:?}",
        result.findings
    );
}

/// An artifact record whose last-known path is not library-relative.
#[test]
fn validate_reports_an_artifact_records_path_that_is_not_library_relative() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "/etc/passwd",
        vec![hash_entry("a", "run-1")],
    );
    write_artifact_record(root, &record);

    let result = validate(root);

    assert_eq!(
        result.findings,
        vec![(
            root.join(STATE_DIR)
                .join(ARTIFACT_STORE)
                .join(format!("{UUID_A}.toml")),
            Finding::PathNotRelative {
                path: "/etc/passwd".to_string()
            }
        )],
        "got {:?}",
        result.findings
    );
}

/// An artifact record with an empty hash history: evidence about
/// nothing.
#[test]
fn validate_reports_an_artifact_record_with_an_empty_hash_history() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let record = artifact_record(artifact_id(UUID_A), None, "a.pdf", Vec::new());
    write_artifact_record(root, &record);

    let result = validate(root);

    assert_eq!(
        result.findings,
        vec![(
            root.join(STATE_DIR)
                .join(ARTIFACT_STORE)
                .join(format!("{UUID_A}.toml")),
            Finding::EmptyHistory
        )],
        "got {:?}",
        result.findings
    );
}

/// A history entry holding a hash that is not one borax writes.
#[test]
fn validate_reports_an_artifact_record_with_a_malformed_hash() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let mut entry = hash_entry("a", "run-1");
    entry.hash = malformed_hash("sha256-deadbeef");
    let record = artifact_record(artifact_id(UUID_A), None, "a.pdf", vec![entry]);
    write_artifact_record(root, &record);

    let result = validate(root);

    assert_eq!(
        result.findings,
        vec![(
            root.join(STATE_DIR)
                .join(ARTIFACT_STORE)
                .join(format!("{UUID_A}.toml")),
            Finding::MalformedHash {
                hash: "sha256-deadbeef".to_string()
            }
        )],
        "got {:?}",
        result.findings
    );
}

/// A history entry naming no run: what it recorded cannot be
/// attributed to anything the library did.
#[test]
fn validate_reports_a_history_entry_naming_no_run() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let mut entry = hash_entry("a", "run-1");
    entry.run = RunId::new("");
    let record = artifact_record(artifact_id(UUID_A), None, "a.pdf", vec![entry]);
    write_artifact_record(root, &record);

    let result = validate(root);

    assert_eq!(
        result.findings,
        vec![(
            root.join(STATE_DIR)
                .join(ARTIFACT_STORE)
                .join(format!("{UUID_A}.toml")),
            Finding::HistoryEntryWithoutRun {
                hash: hash("a").to_string()
            }
        )],
        "got {:?}",
        result.findings
    );
}

/// A `.toml` in the item store that does not parse as an item record.
/// The well-formed item beside it costs nothing, following
/// `item_store_reports_a_toml_file_that_is_not_an_item_record_as_a_fault`.
#[test]
fn validate_reports_an_item_store_file_that_does_not_parse_as_an_item_record() {
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

    let result = validate(root);

    assert_eq!(result.findings.len(), 1, "got {:?}", result.findings);
    let (path, finding) = &result.findings[0];
    assert_eq!(path, &items.join("bad.toml"));
    assert!(
        matches!(finding, Finding::Unreadable { .. }),
        "got {finding:?}"
    );
}

/// An item whose verbatim source fields do not parse as JSON: the field
/// is dropped by `ItemStore::read` and reported as a fault, which
/// `validate` reports as a finding about that file.
#[test]
fn validate_reports_an_item_whose_source_fields_do_not_parse_as_json() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let mut record = minimal_record(EntryType::Article);
    record
        .borax
        .source_fields
        .insert("crossref-note".to_string(), serde_json::json!(42));
    let item = Item {
        id: item_id(UUID_A),
        record,
    };
    let mut text = item.to_toml();
    assert!(
        text.contains("\"42\""),
        "fixture assumption: a source field round-trips as its JSON text, got {text:?}"
    );
    text = text.replace("\"42\"", "\"not json {\"");
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    let path = items.join(format!("a.{UUID_A}.toml"));
    fs::write(&path, text).unwrap();

    let result = validate(root);

    assert_eq!(
        result.findings,
        vec![(
            path,
            Finding::Unreadable {
                message: "dropped unreadable source field \"crossref-note\"".to_string()
            }
        )],
        "got {:?}",
        result.findings
    );
}

// ---------------------------------------------------------------------
// 5.2: validate() — what is not a finding
// ---------------------------------------------------------------------

/// Scenario "A library of orphans validates": a library holding
/// artifacts and no records reports no finding.
#[test]
fn validate_over_a_library_of_only_orphans_is_clean() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    for name in ["a.pdf", "b.pdf", "c.pdf"] {
        fs::write(root.join(name), b"").unwrap();
    }

    let result = validate(root);

    assert!(result.findings.is_empty(), "got {:?}", result.findings);
    assert_eq!(result.orphans, 3, "got {}", result.orphans);
}

/// A record whose artifact cannot be found is the `missing` count, not
/// a finding.
#[test]
fn validate_reports_a_records_missing_artifact_as_a_count_not_a_finding() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "gone.pdf",
        vec![hash_entry("g", "run-1")],
    );
    write_artifact_record(root, &record);

    let result = validate(root);

    assert!(result.findings.is_empty(), "got {:?}", result.findings);
    assert_eq!(result.missing, 1, "got {}", result.missing);
}

/// An item nothing links to is the `unlinked` count, not a finding.
#[test]
fn validate_reports_an_unlinked_item_as_a_count_not_a_finding() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("a.{UUID_A}.toml"), &item);

    let result = validate(root);

    assert!(result.findings.is_empty(), "got {:?}", result.findings);
    assert_eq!(result.unlinked, 1, "got {}", result.unlinked);
}

/// Scenario "A library observed mid-edit": a half-written artifact
/// record is a finding about that file alone, and the rest of the
/// library — a good record and an orphan — is reported as it is.
#[test]
fn a_half_written_record_is_a_finding_about_itself_and_the_rest_of_the_library_is_unaffected() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("orphan.pdf"), b"").unwrap();
    let good = artifact_record(
        artifact_id(UUID_A),
        None,
        "recorded.pdf",
        vec![hash_entry("r", "run-1")],
    );
    fs::write(root.join("recorded.pdf"), b"").unwrap();
    write_artifact_record(root, &good);
    let store_dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&store_dir).unwrap();
    let half_written = store_dir.join("half-written.toml");
    fs::write(&half_written, b"id = \"not-even-a-uu").unwrap();

    let result = validate(root);

    assert_eq!(result.findings.len(), 1, "got {:?}", result.findings);
    assert_eq!(result.findings[0].0, half_written);
    assert!(
        matches!(result.findings[0].1, Finding::Unreadable { .. }),
        "got {:?}",
        result.findings[0].1
    );
    assert_eq!(result.orphans, 1, "got {}", result.orphans);
    assert_eq!(result.missing, 0, "got {}", result.missing);
}

// ---------------------------------------------------------------------
// 5.3 (first half): validate() repairs nothing
// ---------------------------------------------------------------------

/// Scenario "Validation refuses nothing", the read-only half of it: a
/// `validate` that reports findings leaves the store byte-identical —
/// compared by content, not merely by whether the files still exist.
#[test]
fn validate_leaves_the_store_byte_identical_even_when_it_reports_findings() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_B)),
        "orphaned-link.pdf",
        vec![hash_entry("bytes", "run-1")],
    );
    write_artifact_record(root, &record);
    let before = snapshot(root);

    let result = validate(root);

    assert!(
        !result.findings.is_empty(),
        "the fixture must carry a finding to be worth this test"
    );
    assert_eq!(
        snapshot(root),
        before,
        "validate must write nothing to the store"
    );
}

// ---------------------------------------------------------------------
// 6.1: reconcile() repairs a moved artifact by matching its history
// ---------------------------------------------------------------------

/// Scenario "A file manager move is repaired", matched on the record's
/// current (newest) hash: the artifact's identity and item link survive
/// the move, and the repair is reported naming where it was.
#[test]
fn a_moved_artifact_is_repaired_by_matching_its_current_hash() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("new")).unwrap();
    fs::write(root.join("new/paper.pdf"), b"steady content").unwrap();
    let item = item_id(UUID_C);
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id.clone(),
        Some(item.clone()),
        "old/paper.pdf",
        vec![hash_entry("steady content", "run-0")],
    );
    write_artifact_record(root, &record);

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(
        result.repairs,
        vec![(
            id.clone(),
            "new/paper.pdf".to_string(),
            Repair::Repaired {
                from: "old/paper.pdf".to_string()
            }
        )],
        "got {:?}",
        result.repairs
    );
    let after = ArtifactStore::read(root).by_id(&id).unwrap().clone();
    assert_eq!(after.path, "new/paper.pdf");
    assert_eq!(after.id, id);
    assert_eq!(after.item, Some(item));
    assert_eq!(
        after.history,
        vec![hash_entry("steady content", "run-0")],
        "matching the current hash appends nothing"
    );
}

/// The same repair, matched on a hash the record only holds as history:
/// the file's bytes are the *older* version the record once recorded,
/// not its newest one.
#[test]
fn a_moved_artifact_is_repaired_by_matching_a_historical_hash() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("new")).unwrap();
    fs::write(root.join("new/paper.pdf"), b"older content").unwrap();
    let item = item_id(UUID_C);
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id.clone(),
        Some(item.clone()),
        "old/paper.pdf",
        vec![
            hash_entry("older content", "run-0"),
            hash_entry("newer content", "run-1"),
        ],
    );
    write_artifact_record(root, &record);

    let result = reconcile(root, false, RunId::new("run-2"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(
        result.repairs,
        vec![(
            id.clone(),
            "new/paper.pdf".to_string(),
            Repair::Repaired {
                from: "old/paper.pdf".to_string()
            }
        )],
        "got {:?}",
        result.repairs
    );
    let after = ArtifactStore::read(root).by_id(&id).unwrap().clone();
    assert_eq!(after.path, "new/paper.pdf");
    assert_eq!(after.item, Some(item));
    assert_eq!(
        after.history,
        vec![
            hash_entry("older content", "run-0"),
            hash_entry("newer content", "run-1"),
        ],
        "matching a historical hash appends nothing"
    );
}

// ---------------------------------------------------------------------
// 6.2: reconcile() — the fast path
// ---------------------------------------------------------------------

/// A record whose recorded size and modification time are the file's is
/// confirmed without being hashed.
#[test]
fn fast_path_record_matching_size_and_mtime_is_not_hashed() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"steady content").unwrap();
    let id = artifact_id(UUID_A);
    let record = fresh_record(
        root,
        id.clone(),
        None,
        "paper.pdf",
        vec![hash_entry("steady content", "run-0")],
    );
    write_artifact_record(root, &record);
    let before = snapshot(root);

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(result.hashed, 0, "got {result:?}");
    assert!(result.repairs.is_empty(), "got {:?}", result.repairs);
    assert_eq!(result.confirmed, 1, "got {result:?}");
    assert_eq!(
        snapshot(root),
        before,
        "a record the fast path settles is not written"
    );
}

/// A file at its recorded path whose bytes, size and modification time
/// all changed has its new hash appended after the ones already
/// recorded, in order, and is reported as changed.
#[test]
fn fast_path_miss_appends_the_new_hash_after_the_ones_already_recorded() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"version two content").unwrap();
    let id = artifact_id(UUID_A);
    // A record whose size deliberately disagrees with the file, so the
    // fast path cannot possibly match it, over a history describing an
    // earlier version of the bytes.
    let mut record = fresh_record(
        root,
        id.clone(),
        None,
        "paper.pdf",
        vec![hash_entry("version one content", "run-0")],
    );
    record.size += 1;
    write_artifact_record(root, &record);

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    let new_hash = hash_bytes(b"version two content");
    assert_eq!(result.hashed, 1, "got {result:?}");
    assert_eq!(
        result.repairs,
        vec![(
            id.clone(),
            "paper.pdf".to_string(),
            Repair::Changed {
                hash: new_hash.to_string()
            }
        )],
        "got {:?}",
        result.repairs
    );
    let after = ArtifactStore::read(root).by_id(&id).unwrap().clone();
    assert_eq!(
        after.history,
        vec![
            hash_entry("version one content", "run-0"),
            HashEntry {
                hash: new_hash,
                run: RunId::new("run-1"),
                timestamp: TIMESTAMP.to_string(),
                tool_version: TOOL_VERSION.to_string(),
            },
        ],
        "the new hash must be appended after the ones already recorded, got {:?}",
        after.history
    );
}

/// The exact counterexample the fast path is deliberately blind to:
/// bytes changed while size and modification time did not. A plain
/// reconcile reports nothing and writes no library state; `--rehash`
/// reports the change and appends.
#[test]
fn a_change_the_fast_path_cannot_see_is_missed_without_rehash_and_found_with_it() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    let original: &[u8] = b"original content, unedited";
    fs::write(&path, original).unwrap();
    let id = artifact_id(UUID_A);
    let record = fresh_record(
        root,
        id.clone(),
        None,
        "paper.pdf",
        vec![hash_entry("original content, unedited", "run-0")],
    );
    write_artifact_record(root, &record);
    let modified = fs::metadata(&path).unwrap().modified().unwrap();

    // Edit the bytes to the same length and put the modification time
    // back exactly as it was, so only the hash comparison can catch it.
    let mut edited = original.to_vec();
    edited[0] = b'X';
    assert_eq!(edited.len(), original.len(), "fixture must keep the length");
    fs::write(&path, &edited).unwrap();
    fs::File::open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();

    let before = snapshot(root);
    let plain = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(
        plain.hashed, 0,
        "the fast path must still match on size and mtime"
    );
    assert!(plain.repairs.is_empty(), "got {:?}", plain.repairs);
    assert_eq!(
        snapshot(root),
        before,
        "a plain reconcile must write no library state here"
    );

    let rehashed = reconcile(root, true, RunId::new("run-2"), TIMESTAMP, TOOL_VERSION);

    let new_hash = hash_bytes(&edited);
    assert_eq!(rehashed.hashed, 1, "got {rehashed:?}");
    assert_eq!(
        rehashed.repairs,
        vec![(
            id.clone(),
            "paper.pdf".to_string(),
            Repair::Changed {
                hash: new_hash.to_string()
            }
        )],
        "got {:?}",
        rehashed.repairs
    );
}

// ---------------------------------------------------------------------
// 6.2a: the touch counterexample
// ---------------------------------------------------------------------

/// `touch paper.pdf`: the fast path misses because the modification
/// time changed, the hash confirms the record, and the recorded size
/// and modification time are refreshed to the file's current ones — so
/// a second reconcile settles the record on the fast path.
#[test]
fn a_touch_is_confirmed_by_hash_and_refreshes_the_recorded_fields() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    fs::write(&path, b"untouched bytes").unwrap();
    let id = artifact_id(UUID_A);
    let record = fresh_record(
        root,
        id.clone(),
        None,
        "paper.pdf",
        vec![hash_entry("untouched bytes", "run-0")],
    );
    let recorded_millis = record.modified_millis;
    write_artifact_record(root, &record);

    // Touch: change the modification time, leave the bytes exactly as
    // they are.
    let touched = SystemTime::now() + std::time::Duration::from_secs(3600);
    fs::File::open(&path)
        .unwrap()
        .set_modified(touched)
        .unwrap();

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(result.hashed, 1, "the fast path must miss on the touch");
    assert_eq!(result.confirmed, 1, "got {result:?}");
    assert!(
        result.repairs.is_empty(),
        "a confirmation is not a repair, got {:?}",
        result.repairs
    );

    let after = ArtifactStore::read(root).by_id(&id).unwrap().clone();
    let (current_size, current_millis) = stat(&path);
    assert_ne!(
        current_millis, recorded_millis,
        "fixture assumption: the touch must actually change the mtime"
    );
    assert_eq!(
        after.size, current_size,
        "the confirmed record must be refreshed to the file's current size"
    );
    assert_eq!(
        after.modified_millis, current_millis,
        "the confirmed record must be refreshed to the file's current mtime"
    );
    assert_eq!(
        after.history,
        vec![hash_entry("untouched bytes", "run-0")],
        "a confirmation gains no history entry"
    );

    // A second reconcile now settles the record on the fast path.
    let before = snapshot_with_mtime(root);
    let second = reconcile(root, false, RunId::new("run-2"), TIMESTAMP, TOOL_VERSION);
    assert_eq!(second.hashed, 0, "got {second:?}");
    assert_eq!(
        snapshot_with_mtime(root),
        before,
        "a reconcile settled on the fast path writes nothing, mtimes included"
    );
}

// ---------------------------------------------------------------------
// 6.2b: a repair settles the record on the fast path afterwards
// ---------------------------------------------------------------------

/// A repair writes the file's current size and modification time along
/// with its new path, so the reconcile right after settles that record
/// on the fast path.
#[test]
fn a_repair_settles_the_record_on_the_fast_path_for_the_next_reconcile() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("new")).unwrap();
    fs::write(root.join("new/paper.pdf"), b"moved bytes").unwrap();
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id.clone(),
        None,
        "old/paper.pdf",
        vec![hash_entry("moved bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let first = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);
    assert_eq!(
        first.repairs,
        vec![(
            id.clone(),
            "new/paper.pdf".to_string(),
            Repair::Repaired {
                from: "old/paper.pdf".to_string()
            }
        )],
        "got {:?}",
        first.repairs
    );

    let before = snapshot(root);
    let second = reconcile(root, false, RunId::new("run-2"), TIMESTAMP, TOOL_VERSION);
    assert_eq!(
        second.hashed, 0,
        "the repair must have written the file's current size and mtime"
    );
    assert_eq!(
        snapshot(root),
        before,
        "a reconcile settled on the fast path writes nothing"
    );
}

// ---------------------------------------------------------------------
// 6.3: the precedence of design D6
// ---------------------------------------------------------------------

/// Two recorded artifacts that swapped paths outside borax: each
/// record's path is repaired to the file whose hash it records, and
/// neither hash history gains an entry nor item link changes — the
/// order that stops step 3 from reading each as the other's artifact
/// edited in place.
#[test]
fn two_records_whose_files_swapped_paths_both_repair_without_gaining_history_or_changing_links() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    // a.pdf now holds what record B used to be at, and vice versa.
    fs::write(root.join("a.pdf"), b"content-b").unwrap();
    fs::write(root.join("b.pdf"), b"content-a").unwrap();
    let id_a = artifact_id(UUID_A);
    let id_b = artifact_id(UUID_B);
    let item_a = item_id(UUID_C);
    let item_b = item_id(UUID_D);
    let record_a = artifact_record(
        id_a.clone(),
        Some(item_a.clone()),
        "a.pdf",
        vec![hash_entry("content-a", "run-0")],
    );
    let record_b = artifact_record(
        id_b.clone(),
        Some(item_b.clone()),
        "b.pdf",
        vec![hash_entry("content-b", "run-0")],
    );
    write_artifact_record(root, &record_a);
    write_artifact_record(root, &record_b);

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(
        result.repairs,
        vec![
            (
                id_a.clone(),
                "b.pdf".to_string(),
                Repair::Repaired {
                    from: "a.pdf".to_string()
                }
            ),
            (
                id_b.clone(),
                "a.pdf".to_string(),
                Repair::Repaired {
                    from: "b.pdf".to_string()
                }
            ),
        ],
        "got {:?}",
        result.repairs
    );
    let after = ArtifactStore::read(root);
    let got_a = after.by_id(&id_a).unwrap();
    let got_b = after.by_id(&id_b).unwrap();
    assert_eq!(got_a.path, "b.pdf");
    assert_eq!(got_a.item, Some(item_a));
    assert_eq!(
        got_a.history,
        vec![hash_entry("content-a", "run-0")],
        "the swap must gain no history entry"
    );
    assert_eq!(got_b.path, "a.pdf");
    assert_eq!(got_b.item, Some(item_b));
    assert_eq!(
        got_b.history,
        vec![hash_entry("content-b", "run-0")],
        "the swap must gain no history entry"
    );
}

/// A settled record's file is not a candidate for another record: A is
/// fine at its own path, and B's path holds nothing while B's history
/// holds the same hash A's file has. B must not be repaired onto A's
/// already-claimed file.
#[test]
fn a_settled_records_file_is_not_a_candidate_for_another_record() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("a.pdf"), b"shared bytes").unwrap();
    let id_a = artifact_id(UUID_A);
    let id_b = artifact_id(UUID_B);
    let record_a = fresh_record(
        root,
        id_a.clone(),
        None,
        "a.pdf",
        vec![hash_entry("shared bytes", "run-0")],
    );
    let record_b = artifact_record(
        id_b.clone(),
        None,
        "b.pdf",
        vec![hash_entry("shared bytes", "run-0")],
    );
    write_artifact_record(root, &record_a);
    write_artifact_record(root, &record_b);

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(
        result.repairs,
        vec![(id_b.clone(), "b.pdf".to_string(), Repair::Missing)],
        "A's own file must not be reachable as B's candidate, got {:?}",
        result.repairs
    );
    let after = ArtifactStore::read(root);
    assert_eq!(after.by_id(&id_a).unwrap().path, "a.pdf");
    assert_eq!(after.by_id(&id_b).unwrap().path, "b.pdf");
}

/// A current-hash match wins over a historical one when both are
/// unclaimed candidates for the same record.
#[test]
fn a_current_hash_match_wins_over_a_historical_one() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("old")).unwrap();
    fs::create_dir_all(root.join("current")).unwrap();
    fs::write(root.join("old/v1.pdf"), b"version one").unwrap();
    fs::write(root.join("current/v2.pdf"), b"version two").unwrap();
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id.clone(),
        None,
        "gone.pdf",
        vec![
            hash_entry("version one", "run-0"),
            hash_entry("version two", "run-1"),
        ],
    );
    write_artifact_record(root, &record);

    let result = reconcile(root, false, RunId::new("run-2"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(
        result.repairs,
        vec![(
            id.clone(),
            "current/v2.pdf".to_string(),
            Repair::Repaired {
                from: "gone.pdf".to_string()
            }
        )],
        "the current hash's match must win, got {:?}",
        result.repairs
    );
}

// ---------------------------------------------------------------------
// 6.4: ambiguity preserves
// ---------------------------------------------------------------------

/// One record with two candidates: it keeps its path, its history and
/// its item link, and the run reports it ambiguous naming both
/// candidates in path order.
#[test]
fn one_record_with_two_candidates_is_left_exactly_as_it_was() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("copy1.pdf"), b"duplicated bytes").unwrap();
    fs::write(root.join("copy2.pdf"), b"duplicated bytes").unwrap();
    let id = artifact_id(UUID_A);
    let item = item_id(UUID_C);
    let record = artifact_record(
        id.clone(),
        Some(item.clone()),
        "gone.pdf",
        vec![hash_entry("duplicated bytes", "run-0")],
    );
    write_artifact_record(root, &record);
    let before = ArtifactStore::read(root).by_id(&id).unwrap().clone();

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(
        result.repairs,
        vec![(
            id.clone(),
            "gone.pdf".to_string(),
            Repair::Ambiguous {
                candidates: vec!["copy1.pdf".to_string(), "copy2.pdf".to_string()]
            }
        )],
        "got {:?}",
        result.repairs
    );
    let after = ArtifactStore::read(root).by_id(&id).unwrap().clone();
    assert_eq!(
        after, before,
        "an ambiguous record must be left byte-for-byte as it was"
    );
    assert_eq!(
        after.item,
        Some(item),
        "the item link specifically must not be reassigned"
    );
}

/// Two records both contending for one unclaimed file: neither is
/// repaired, neither claims the file, and both are reported ambiguous —
/// path, history and item link untouched for each.
#[test]
fn two_records_contending_for_one_file_are_both_left_exactly_as_they_were() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("shared.pdf"), b"contended bytes").unwrap();
    let id_a = artifact_id(UUID_A);
    let id_b = artifact_id(UUID_B);
    let item_a = item_id(UUID_C);
    let item_b = item_id(UUID_D);
    let record_a = artifact_record(
        id_a.clone(),
        Some(item_a.clone()),
        "gone-a.pdf",
        vec![hash_entry("contended bytes", "run-0")],
    );
    let record_b = artifact_record(
        id_b.clone(),
        Some(item_b.clone()),
        "gone-b.pdf",
        vec![hash_entry("contended bytes", "run-0")],
    );
    write_artifact_record(root, &record_a);
    write_artifact_record(root, &record_b);
    let before_a = ArtifactStore::read(root).by_id(&id_a).unwrap().clone();
    let before_b = ArtifactStore::read(root).by_id(&id_b).unwrap().clone();

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(
        result.repairs,
        vec![
            (
                id_a.clone(),
                "gone-a.pdf".to_string(),
                Repair::Ambiguous {
                    candidates: vec!["shared.pdf".to_string()]
                }
            ),
            (
                id_b.clone(),
                "gone-b.pdf".to_string(),
                Repair::Ambiguous {
                    candidates: vec!["shared.pdf".to_string()]
                }
            ),
        ],
        "got {:?}",
        result.repairs
    );
    let after_a = ArtifactStore::read(root).by_id(&id_a).unwrap().clone();
    let after_b = ArtifactStore::read(root).by_id(&id_b).unwrap().clone();
    assert_eq!(after_a, before_a);
    assert_eq!(after_b, before_b);
    assert_eq!(after_a.item, Some(item_a));
    assert_eq!(after_b.item, Some(item_b));
}

// ---------------------------------------------------------------------
// 6.5: an untouched library, an orphan, and a missing artifact
// ---------------------------------------------------------------------

/// A reconcile over a library nothing has touched writes no library
/// state — every file of the item store and of `.borax/artifacts/`
/// byte-identical, mtimes included.
#[test]
fn a_reconcile_over_an_untouched_library_writes_no_library_state() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("a.pdf"), b"content a").unwrap();
    fs::write(root.join("b.pdf"), b"content b").unwrap();
    let item = Item {
        id: item_id(UUID_C),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("item.{UUID_C}.toml"), &item);
    let record_a = fresh_record(
        root,
        artifact_id(UUID_A),
        Some(item.id.clone()),
        "a.pdf",
        vec![hash_entry("content a", "run-0")],
    );
    let record_b = fresh_record(
        root,
        artifact_id(UUID_B),
        None,
        "b.pdf",
        vec![hash_entry("content b", "run-0")],
    );
    write_artifact_record(root, &record_a);
    write_artifact_record(root, &record_b);
    let before = snapshot_with_mtime(root);

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(result.hashed, 0, "got {result:?}");
    assert!(result.repairs.is_empty(), "got {:?}", result.repairs);
    assert_eq!(result.records, 2, "got {result:?}");
    assert_eq!(result.confirmed, result.records, "got {result:?}");
    assert_eq!(
        snapshot_with_mtime(root),
        before,
        "an untouched library must be left byte-identical, mtimes included"
    );
}

/// A reconcile creates no record for an orphan.
#[test]
fn reconcile_creates_no_record_for_an_orphan() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("orphan.pdf"), b"orphan bytes").unwrap();

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(result.records, 0, "got {result:?}");
    assert!(
        !root.join(STATE_DIR).join(ARTIFACT_STORE).exists(),
        "no record must be created for an orphan"
    );
}

/// A reconcile deletes no record whose artifact is gone: it is reported
/// missing and left byte-identical.
#[test]
fn reconcile_deletes_no_record_whose_artifact_is_gone() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id.clone(),
        None,
        "gone.pdf",
        vec![hash_entry("gone", "run-0")],
    );
    write_artifact_record(root, &record);
    let before = snapshot(root);

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(
        result.repairs,
        vec![(id, "gone.pdf".to_string(), Repair::Missing)],
        "got {:?}",
        result.repairs
    );
    assert_eq!(
        snapshot(root),
        before,
        "a record whose artifact is gone must be left byte-identical"
    );
}

// ---------------------------------------------------------------------
// 2.5 (reconcile's half): the bounded walk stops at a nested marker
// ---------------------------------------------------------------------

/// An artifact moved into a nested library's subtree is not a
/// candidate: the outer record is reported missing rather than
/// repaired, and the nested library's files are untouched.
#[test]
fn reconcile_does_not_repair_into_a_nested_librarys_subtree() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("nested/.borax.toml"), b"").unwrap();
    fs::write(root.join("nested/moved.pdf"), b"escaped bytes").unwrap();
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id.clone(),
        None,
        "gone.pdf",
        vec![hash_entry("escaped bytes", "run-0")],
    );
    write_artifact_record(root, &record);
    let nested_before = snapshot(&root.join("nested"));

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(
        result.repairs,
        vec![(id, "gone.pdf".to_string(), Repair::Missing)],
        "the nested library's file must not be a candidate, got {:?}",
        result.repairs
    );
    assert_eq!(
        snapshot(&root.join("nested")),
        nested_before,
        "a nested library's files must be untouched by an outer reconcile"
    );
}

/// A PDF placed under `items/` or under `.borax/` is not a candidate
/// either, following the same exclusion the walk and the orphan count
/// already obey.
#[test]
fn reconcile_does_not_treat_a_pdf_under_items_or_the_state_dir_as_a_candidate() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(ITEM_STORE)).unwrap();
    fs::write(
        root.join(ITEM_STORE).join("smuggled.pdf"),
        b"smuggled bytes",
    )
    .unwrap();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(root.join(STATE_DIR).join("smuggled.pdf"), b"smuggled bytes").unwrap();
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id.clone(),
        None,
        "gone.pdf",
        vec![hash_entry("smuggled bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let result = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);

    assert_eq!(
        result.repairs,
        vec![(id, "gone.pdf".to_string(), Repair::Missing)],
        "neither items/ nor .borax/ may supply a candidate, got {:?}",
        result.repairs
    );
}

// ---------------------------------------------------------------------
// reconciliation_events: sanity over the two stores together
// ---------------------------------------------------------------------

/// `reconciliation_events` emits one `LibraryRepair` per record the run
/// had something to say about, then one `LibraryReconciled` carrying
/// the totals — exercised directly over [`reconcile`]'s own output so a
/// defect in the event mapping is distinguishable from one in
/// reconciliation itself.
#[test]
fn reconciliation_events_emits_one_repair_event_then_the_totals() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("new")).unwrap();
    fs::write(root.join("new/paper.pdf"), b"moved bytes").unwrap();
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id.clone(),
        None,
        "old/paper.pdf",
        vec![hash_entry("moved bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let reconciliation = reconcile(root, false, RunId::new("run-1"), TIMESTAMP, TOOL_VERSION);
    let events = reconciliation_events(&reconciliation);

    assert_eq!(events.len(), 2, "got {events:?}");
    assert_eq!(
        events[0],
        Event::LibraryRepair {
            id: id.to_string(),
            path: "new/paper.pdf".to_string(),
            repair: Repair::Repaired {
                from: "old/paper.pdf".to_string()
            },
        },
        "got {:?}",
        events[0]
    );
    assert_eq!(
        events[1],
        Event::LibraryReconciled {
            root: root.to_path_buf(),
            records: 1,
            confirmed: 0,
            repaired: 1,
            changed: 0,
            ambiguous: 0,
            missing: 0,
            hashed: 1,
        },
        "got {:?}",
        events[1]
    );
}
