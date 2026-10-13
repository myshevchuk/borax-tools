#![allow(clippy::unwrap_used)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use borax::event::{
    AcceptedBy, AcceptedItemCause, Admission, Adoption, Condition, Event, Extraction, Finding,
    Level, LibraryAnswer, Repair,
};
use borax::evidence::{
    Consultation, Evidence, ExtractionEvidence, ExtractionStep, IdentifierInput, IndexEvidence,
    IndexRead, IndexWrite, LookupEvidence, MatchCheck, RecordRetrieval, Titles, Unattempted,
};
use borax::library::{
    ARTIFACT_STORE, Account, Admitted, Admitting, Adoptions, ArtifactStore, Consulted, ITEM_STORE,
    Identity, ItemStore, Reconciliation, Reconciling, RecordFaults, STATE_DIR, StoreFault,
    StoreWriter, Stores, Unrecorded, WorkDuplicate, accepted_by, admission_event, admit, adopt,
    artifacts, carries, contains, ensure_identity, excludes, extracted_identifier,
    extraction_event, identity, identity_fault_warning, identity_file, item_file_name,
    library_relative, missing, orphan_events, orphans, reconcile, reconciliation_events,
    recorded_at, relative_to, status_event, store_create, store_write, strands, survey, validate,
    validation_events,
};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{ArxivId, Doi, Identifier, Isbn, Pmid};
use borax_core::library::{
    ArtifactId, ArtifactRecord, HashEntry, Item, ItemId, LibraryId, LibraryIdentity, RunId,
};
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
        item: None,
    }
}

/// `entry`, naming `item` as the item its bytes were accepted under.
fn entry_naming(mut entry: HashEntry, item: &ItemId) -> HashEntry {
    entry.item = Some(item.clone());
    entry
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
// library_relative() and excludes() under any spelling
// ---------------------------------------------------------------------

// A run given `paper.pdf` discovers its library root as an absolute
// directory. The file is inside that library by lexical containment,
// however it was spelled, so an applying run records it there.
#[test]
fn a_relative_path_is_library_relative_to_an_absolute_root() {
    let working = std::env::current_dir().unwrap();

    assert_eq!(
        library_relative(&working, Path::new("paper.pdf")).as_deref(),
        Some("paper.pdf")
    );
    assert_eq!(
        library_relative(&working, Path::new("./sub/x/../paper.pdf")).as_deref(),
        Some("sub/paper.pdf")
    );
}

#[test]
fn an_absolute_path_is_library_relative_to_a_relative_root() {
    let working = std::env::current_dir().unwrap();

    assert_eq!(
        library_relative(Path::new("."), &working.join("sub/paper.pdf")).as_deref(),
        Some("sub/paper.pdf")
    );
}

#[test]
fn a_relative_path_climbing_out_of_the_root_is_not_library_relative() {
    let working = std::env::current_dir().unwrap();

    assert_eq!(library_relative(&working, Path::new("../paper.pdf")), None);
}

#[test]
fn a_relative_path_inside_the_root_is_not_excluded() {
    let working = std::env::current_dir().unwrap();

    assert!(!excludes(&working, Path::new("paper.pdf")));
    assert!(!excludes(Path::new("."), &working.join("paper.pdf")));
    assert!(excludes(&working, Path::new("../paper.pdf")));
}

/// design D1a / reviewer finding: `library_relative` compares its
/// stripped prefix with `strip_prefix`, which is case-sensitive, while
/// `contains` and `Account::is_incoming` compare with `same_name`,
/// which is case-insensitive on Windows. A root and a path differing
/// only in case must still be found relative on that platform, or the
/// two comparisons disagree about where the library's boundary runs.
#[cfg(windows)]
#[test]
fn library_relative_is_case_insensitive_on_windows() {
    assert_eq!(
        library_relative(
            Path::new("C:/Library"),
            Path::new("c:/library/sub/paper.pdf")
        )
        .as_deref(),
        Some("sub/paper.pdf")
    );
}

/// The same case-insensitivity for `excludes`: a path spelled in a
/// different case from the root it lies under must not be excluded on
/// that account alone.
#[cfg(windows)]
#[test]
fn excludes_is_case_insensitive_on_windows() {
    assert!(!excludes(
        Path::new("C:/Library"),
        Path::new("c:/library/sub/paper.pdf")
    ));
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
// 3.5: orphan_events() — design D3, D5, D10
// ---------------------------------------------------------------------

/// `orphan_events(&survey)` gives one `Event::LibraryCondition` of kind
/// `Orphan` per entry of `survey.orphans`, in that order, named
/// library-relative and `/`-separated as [`extraction_event`] names an
/// artifact — covering an orphan at the root and one two directories
/// down.
#[test]
fn orphan_events_names_each_orphan_of_the_survey_in_survey_order() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("a.pdf"), b"").unwrap();
    fs::create_dir_all(root.join("sub/deeper")).unwrap();
    fs::write(root.join("sub/deeper/b.pdf"), b"").unwrap();
    fs::write(root.join("recorded.pdf"), b"").unwrap();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "recorded.pdf",
        vec![hash_entry("r", "run-1")],
    );
    write_artifact_record(root, &record);

    let found = survey(root);
    assert_eq!(
        found.orphans,
        vec![root.join("a.pdf"), root.join("sub/deeper/b.pdf")],
        "fixture assumption: the recorded artifact must be excluded, got {:?}",
        found.orphans
    );

    let events = orphan_events(&found);

    assert_eq!(
        events,
        vec![
            Event::LibraryCondition {
                path: "a.pdf".to_string(),
                condition: Condition::Orphan,
            },
            Event::LibraryCondition {
                path: "sub/deeper/b.pdf".to_string(),
                condition: Condition::Orphan,
            },
        ],
        "got {events:?}"
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

// ---------------------------------------------------------------------
// 9: Validation — pre-change and malformed entries (design D7, D11)
// ---------------------------------------------------------------------

#[test]
fn validate_reports_no_finding_for_an_entry_naming_no_item() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "a.pdf",
        vec![hash_entry("a", "run-1")],
    );
    write_artifact_record(root, &record);

    let result = validate(root);

    assert!(result.findings.is_empty(), "got {:?}", result.findings);
}

#[test]
fn validate_reports_no_finding_for_an_entry_naming_an_item_the_library_does_not_hold() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let mut entry = hash_entry("a", "run-1");
    entry.item = Some(item_id(UUID_B));
    let record = artifact_record(artifact_id(UUID_A), None, "a.pdf", vec![entry]);
    write_artifact_record(root, &record);

    let result = validate(root);

    assert!(
        result.findings.is_empty(),
        "an entry naming an item the library no longer holds is the history of a \
         deleted item, not a finding: got {:?}",
        result.findings
    );
}

#[test]
fn validate_reports_a_record_whose_entry_item_is_not_a_canonical_uuid_as_unreadable() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let text = format!(
        "id = \"{UUID_A}\"\n\
         path = \"a.pdf\"\n\
         size = 1\n\
         modified_millis = 0\n\n\
         [[history]]\n\
         hash = \"{}\"\n\
         run = \"run-1\"\n\
         timestamp = \"t1\"\n\
         tool_version = \"v1\"\n\
         item = \"not-a-uuid\"\n",
        hash("a"),
    );
    let records_dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records_dir).unwrap();
    fs::write(records_dir.join(format!("{UUID_A}.toml")), &text).unwrap();

    let result = validate(root);

    assert_eq!(result.findings.len(), 1, "got {:?}", result.findings);
    assert!(
        matches!(result.findings[0].1, Finding::Unreadable { .. }),
        "a record whose entry item is not a canonical uuid does not parse, and is \
         reported as any unparseable record is: got {:?}",
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
    assert_eq!(result.orphans(), 3, "got {}", result.orphans());
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
    assert_eq!(result.missing(), 1, "got {}", result.missing());
}

/// A record whose last-known path points into a nested library is an
/// artifact this library cannot find, and is counted with the rest of
/// them: the subtree is opaque, so a file standing there is no more
/// visible to `borax validate` than it is to `borax reconcile`. It is
/// a count rather than a finding, since a record naming a path the
/// library cannot see is not malformed.
#[test]
fn validate_counts_a_record_inside_a_nested_library_as_missing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("nested/.borax.toml"), b"").unwrap();
    fs::write(root.join("nested/kept.pdf"), b"kept bytes").unwrap();
    let record = fresh_record(
        root,
        artifact_id(UUID_A),
        None,
        "nested/kept.pdf",
        vec![hash_entry("kept bytes", "run-1")],
    );
    write_artifact_record(root, &record);

    let result = validate(root);

    assert!(result.findings.is_empty(), "got {:?}", result.findings);
    assert_eq!(result.missing(), 1, "got {}", result.missing());
    assert_eq!(result.orphans(), 0, "got {}", result.orphans());
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
    assert_eq!(result.unlinked(), 1, "got {}", result.unlinked());
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
    assert_eq!(result.orphans(), 1, "got {}", result.orphans());
    assert_eq!(result.missing(), 0, "got {}", result.missing());
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
// 5.4: validate() — Validation::conditions, task 4.2
// ---------------------------------------------------------------------

/// What `Validation::orphans()` counted before this change
/// (design D5's "Context"), kept here so a test can show the migrated
/// method's total is unchanged without restating its own definition
/// back at it.
fn pre_change_orphans(root: &Path) -> usize {
    survey(root).orphans.len()
}

/// What `Validation::missing()` counted before this change.
fn pre_change_missing(root: &Path) -> usize {
    let records = ArtifactStore::read(root);
    missing(root, &records, &|path: &Path| {
        !excludes(root, path) && path.is_file()
    })
    .len()
}

/// What `Validation::unlinked()` counted before this change.
fn pre_change_unlinked(root: &Path) -> usize {
    let records = ArtifactStore::read(root);
    let items = ItemStore::read(root);
    items
        .iter()
        .filter(|item| records.by_item(&item.id).is_empty())
        .count()
}

/// Scenario "Validation names each condition it counts": an orphan, a
/// missing record and an unlinked item each give exactly one
/// condition, in kind order (orphan, missing, unlinked), and no
/// finding.
#[test]
fn validate_conditions_name_an_orphan_a_missing_record_and_an_unlinked_item() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("new.pdf"), b"").unwrap();
    let missing_record = artifact_record(
        artifact_id(UUID_A),
        None,
        "gone.pdf",
        vec![hash_entry("g", "run-1")],
    );
    write_artifact_record(root, &missing_record);
    let item = Item {
        id: item_id(UUID_B),
        record: minimal_record(EntryType::Article),
    };
    let item_name = item_file_name(Some("milner1978"), &item.id);
    write_item(root, &item_name, &item);

    let result = validate(root);

    assert!(result.findings.is_empty(), "got {:?}", result.findings);
    assert_eq!(
        result.conditions,
        vec![
            ("new.pdf".to_string(), Condition::Orphan),
            (
                "gone.pdf".to_string(),
                Condition::Missing {
                    id: UUID_A.to_string(),
                    record: format!("{STATE_DIR}/{ARTIFACT_STORE}/{UUID_A}.toml"),
                },
            ),
            (
                format!("{ITEM_STORE}/{item_name}"),
                Condition::Unlinked {
                    id: UUID_B.to_string(),
                },
            ),
        ],
        "got {:?}",
        result.conditions
    );
    assert_eq!(result.orphans(), 1, "got {}", result.orphans());
    assert_eq!(result.missing(), 1, "got {}", result.missing());
    assert_eq!(result.unlinked(), 1, "got {}", result.unlinked());
    assert_eq!(result.orphans(), pre_change_orphans(root));
    assert_eq!(result.missing(), pre_change_missing(root));
    assert_eq!(result.unlinked(), pre_change_unlinked(root));
}

/// Scenario "A record inside a nested library is missing, not an
/// orphan": a record naming a path under a nested library gives one
/// `Missing` condition and no `Orphan`.
#[test]
fn validate_conditions_name_a_record_inside_a_nested_library_as_missing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("nested/.borax.toml"), b"").unwrap();
    fs::write(root.join("nested/kept.pdf"), b"kept bytes").unwrap();
    let record = fresh_record(
        root,
        artifact_id(UUID_A),
        None,
        "nested/kept.pdf",
        vec![hash_entry("kept bytes", "run-1")],
    );
    write_artifact_record(root, &record);

    let result = validate(root);

    assert!(result.findings.is_empty(), "got {:?}", result.findings);
    assert_eq!(
        result.conditions,
        vec![(
            "nested/kept.pdf".to_string(),
            Condition::Missing {
                id: UUID_A.to_string(),
                record: format!("{STATE_DIR}/{ARTIFACT_STORE}/{UUID_A}.toml"),
            },
        )],
        "got {:?}",
        result.conditions
    );
    assert_eq!(result.orphans(), pre_change_orphans(root));
    assert_eq!(result.missing(), pre_change_missing(root));
    assert_eq!(result.unlinked(), pre_change_unlinked(root));
}

/// Scenario "Two item files carrying one identity, neither linked":
/// two `Unlinked` conditions and one `DuplicateIdentity` finding —
/// today's count of 2, unchanged.
#[test]
fn validate_conditions_give_two_unlinked_for_two_item_files_of_one_unlinked_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    let first = format!("a-first.{UUID_A}.toml");
    let second = format!("b-second.{UUID_A}.toml");
    write_item(root, &first, &item);
    write_item(root, &second, &item);

    let result = validate(root);

    assert_eq!(
        result.findings,
        vec![(
            root.join(ITEM_STORE).join(&second),
            Finding::DuplicateIdentity {
                id: UUID_A.to_string(),
                other: root.join(ITEM_STORE).join(&first),
            }
        )],
        "got {:?}",
        result.findings
    );
    assert_eq!(
        result.conditions,
        vec![
            (
                format!("{ITEM_STORE}/{first}"),
                Condition::Unlinked {
                    id: UUID_A.to_string()
                },
            ),
            (
                format!("{ITEM_STORE}/{second}"),
                Condition::Unlinked {
                    id: UUID_A.to_string()
                },
            ),
        ],
        "got {:?}",
        result.conditions
    );
    assert_eq!(result.unlinked(), 2, "today's count of 2, unchanged");
    assert_eq!(result.unlinked(), pre_change_unlinked(root));
}

/// Scenario "Two records of one identity are both named missing": the
/// shared identity is a `DuplicateIdentity` finding, and two `Missing`
/// conditions name `gone.pdf` and the identity, each with a different
/// `record`.
#[test]
fn validate_conditions_give_two_missing_for_two_records_of_one_missing_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let dir_path = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&dir_path).unwrap();
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "gone.pdf",
        vec![hash_entry("g1", "run-1")],
    );
    let first = format!("a-first.{UUID_A}.toml");
    fs::write(dir_path.join(&first), record.to_toml()).unwrap();
    let other_record = artifact_record(
        artifact_id(UUID_A),
        None,
        "gone.pdf",
        vec![hash_entry("g2", "run-1")],
    );
    let second = format!("b-second.{UUID_A}.toml");
    fs::write(dir_path.join(&second), other_record.to_toml()).unwrap();

    let result = validate(root);

    assert_eq!(
        result.findings,
        vec![(
            dir_path.join(&second),
            Finding::DuplicateIdentity {
                id: UUID_A.to_string(),
                other: dir_path.join(&first),
            }
        )],
        "got {:?}",
        result.findings
    );
    assert_eq!(
        result.conditions,
        vec![
            (
                "gone.pdf".to_string(),
                Condition::Missing {
                    id: UUID_A.to_string(),
                    record: format!("{STATE_DIR}/{ARTIFACT_STORE}/{first}"),
                },
            ),
            (
                "gone.pdf".to_string(),
                Condition::Missing {
                    id: UUID_A.to_string(),
                    record: format!("{STATE_DIR}/{ARTIFACT_STORE}/{second}"),
                },
            ),
        ],
        "got {:?}",
        result.conditions
    );
    assert_eq!(result.missing(), 2, "today's count of 2, unchanged");
    assert_eq!(result.missing(), pre_change_missing(root));
}

/// Scenario "Every check still reports" (D8's gate): a library holding
/// one instance of every finding kind, beside an orphan, a clean
/// missing record and a clean unlinked item. Every finding fixture here
/// is otherwise clean — its own record or item file is linked, present
/// and well formed apart from the one thing that triggers its finding —
/// so each of the twelve contributes no condition of its own, and the
/// three extra, deliberately clean fixtures are the only entries in
/// `conditions`. The findings equal, kind for kind and file for file,
/// what the per-finding tests above already pin; none of the three
/// conditions is reported as a finding.
#[test]
fn validate_every_check_still_reports_beside_all_three_conditions() {
    fn gate_uuid(tag: u8) -> String {
        format!("0198bbbb-0000-7000-8000-0000000000{tag:02x}")
    }

    let dir = tempdir().unwrap();
    let root = dir.path();

    // --- item-store findings ---

    // 1. An item file that cannot be read at all.
    let items_dir = root.join(ITEM_STORE);
    fs::create_dir_all(&items_dir).unwrap();
    fs::write(items_dir.join("bad.toml"), b"this is not [ valid toml").unwrap();

    // 2. An item file whose name disagrees with the identity inside it,
    //    linked by a control record so it is not also unlinked.
    let name_disagrees_item = Item {
        id: item_id(&gate_uuid(2)),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, "notes-on-this.toml", &name_disagrees_item);

    // 3. An item whose verbatim source fields do not parse as JSON,
    //    linked the same way.
    let mut bad_source_record = minimal_record(EntryType::Article);
    bad_source_record
        .borax
        .source_fields
        .insert("crossref-note".to_string(), serde_json::json!(42));
    let bad_source_item = Item {
        id: item_id(&gate_uuid(3)),
        record: bad_source_record,
    };
    let mut bad_source_text = bad_source_item.to_toml();
    assert!(
        bad_source_text.contains("\"42\""),
        "fixture assumption: a source field round-trips as its JSON text"
    );
    bad_source_text = bad_source_text.replace("\"42\"", "\"not json {\"");
    let bad_source_path = items_dir.join(format!("bad-source.{}.toml", gate_uuid(3)));
    fs::write(&bad_source_path, bad_source_text).unwrap();

    // 4. Two item files carrying one identity, linked the same way so
    //    this fixture contributes no condition either.
    let dup_item = Item {
        id: item_id(&gate_uuid(4)),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("dup-a.{}.toml", gate_uuid(4)), &dup_item);
    write_item(root, &format!("dup-b.{}.toml", gate_uuid(4)), &dup_item);

    // Three control records, each existing and clean, that link the
    // three items above so none of them is also `unlinked`.
    for (tag, control_id, path) in [
        (2u8, gate_uuid(2), "linked-2.pdf"),
        (3u8, gate_uuid(3), "linked-3.pdf"),
        (4u8, gate_uuid(4), "linked-4.pdf"),
    ] {
        fs::write(root.join(path), b"control bytes").unwrap();
        let control = fresh_record(
            root,
            artifact_id(&gate_uuid(100 + tag)),
            Some(item_id(&control_id)),
            path,
            vec![hash_entry("control bytes", "run-1")],
        );
        write_artifact_record(root, &control);
    }

    // --- artifact-record findings ---

    // a. An artifact record that cannot be read at all.
    let records_dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records_dir).unwrap();
    fs::write(records_dir.join("bad-record.toml"), b"id = \"not-a-uuid").unwrap();

    // b. A record whose file name disagrees with the identity inside
    //    it. Given a real, present path so it is not also missing.
    fs::write(root.join("name-disagrees.pdf"), b"nd").unwrap();
    let name_disagrees_record = fresh_record(
        root,
        artifact_id(&gate_uuid(10)),
        None,
        "name-disagrees.pdf",
        vec![hash_entry("nd", "run-1")],
    );
    let name_disagrees_path = records_dir.join(format!("{}.toml", gate_uuid(11)));
    fs::write(&name_disagrees_path, name_disagrees_record.to_toml()).unwrap();

    // c. A record whose last-known path is not library-relative. Given
    //    a real file at that absolute-looking path (still under the
    //    tempdir, since `relative_to` resolves it against `root`), so
    //    it is not also missing.
    fs::create_dir_all(root.join("etc")).unwrap();
    fs::write(root.join("etc").join("passwd"), b"not a pdf").unwrap();
    let nonrelative = artifact_record(
        artifact_id(&gate_uuid(12)),
        None,
        "/etc/passwd",
        vec![hash_entry("not a pdf", "run-1")],
    );
    write_artifact_record(root, &nonrelative);

    // d. An artifact record with an empty hash history.
    fs::write(root.join("empty-history.pdf"), b"eh").unwrap();
    let empty_history = fresh_record(
        root,
        artifact_id(&gate_uuid(13)),
        None,
        "empty-history.pdf",
        Vec::new(),
    );
    write_artifact_record(root, &empty_history);

    // e. A record holding a malformed hash.
    fs::write(root.join("malformed-hash.pdf"), b"mh").unwrap();
    let mut malformed_entry = hash_entry("mh", "run-1");
    malformed_entry.hash = malformed_hash("sha256-deadbeef");
    let malformed = fresh_record(
        root,
        artifact_id(&gate_uuid(14)),
        None,
        "malformed-hash.pdf",
        vec![malformed_entry],
    );
    write_artifact_record(root, &malformed);

    // f. A history entry naming no run.
    fs::write(root.join("no-run.pdf"), b"nr").unwrap();
    let mut no_run_entry = hash_entry("nr", "run-1");
    no_run_entry.run = RunId::new("");
    let no_run = fresh_record(
        root,
        artifact_id(&gate_uuid(15)),
        None,
        "no-run.pdf",
        vec![no_run_entry],
    );
    write_artifact_record(root, &no_run);

    // g. A record naming an item the library does not hold.
    fs::write(root.join("dangling-link.pdf"), b"dl").unwrap();
    let dangling = fresh_record(
        root,
        artifact_id(&gate_uuid(16)),
        Some(item_id(&gate_uuid(99))),
        "dangling-link.pdf",
        vec![hash_entry("dl", "run-1")],
    );
    write_artifact_record(root, &dangling);

    // h. Two artifact records carrying one identity, each with its own
    //    present file, so neither is also missing.
    fs::write(root.join("dup-record-1.pdf"), b"dr1").unwrap();
    fs::write(root.join("dup-record-2.pdf"), b"dr2").unwrap();
    let dup_record_1 = fresh_record(
        root,
        artifact_id(&gate_uuid(17)),
        None,
        "dup-record-1.pdf",
        vec![hash_entry("dr1", "run-1")],
    );
    let dup_record_1_name = format!("dup-record-a.{}.toml", gate_uuid(17));
    fs::write(records_dir.join(&dup_record_1_name), dup_record_1.to_toml()).unwrap();
    let dup_record_2 = fresh_record(
        root,
        artifact_id(&gate_uuid(17)),
        None,
        "dup-record-2.pdf",
        vec![hash_entry("dr2", "run-1")],
    );
    let dup_record_2_name = format!("dup-record-b.{}.toml", gate_uuid(17));
    fs::write(records_dir.join(&dup_record_2_name), dup_record_2.to_toml()).unwrap();

    // --- the three clean, condition-only fixtures ---

    fs::write(root.join("orphan.pdf"), b"").unwrap();
    let clean_missing = artifact_record(
        artifact_id(&gate_uuid(18)),
        None,
        "clean-missing.pdf",
        vec![hash_entry("cm", "run-1")],
    );
    write_artifact_record(root, &clean_missing);
    let clean_unlinked_item = Item {
        id: item_id(&gate_uuid(19)),
        record: minimal_record(EntryType::Article),
    };
    write_item(
        root,
        &format!("{}.toml", gate_uuid(19)),
        &clean_unlinked_item,
    );

    let result = validate(root);

    // The findings: exactly what the per-finding tests above pin, one
    // of each kind.
    assert_eq!(result.findings.len(), 12, "got {:?}", result.findings);
    let has = |predicate: &dyn Fn(&Finding) -> bool| {
        result
            .findings
            .iter()
            .any(|(_, finding)| predicate(finding))
    };
    assert!(
        result
            .findings
            .iter()
            .any(|(path, finding)| path == &items_dir.join("bad.toml")
                && matches!(finding, Finding::Unreadable { .. })),
        "got {:?}",
        result.findings
    );
    assert!(
        result.findings.contains(&(
            root.join(ITEM_STORE).join("notes-on-this.toml"),
            Finding::NameDisagrees { id: gate_uuid(2) }
        )),
        "got {:?}",
        result.findings
    );
    assert!(
        result.findings.contains(&(
            bad_source_path.clone(),
            Finding::Unreadable {
                message: "dropped unreadable source field \"crossref-note\"".to_string(),
            }
        )),
        "got {:?}",
        result.findings
    );
    assert!(
        has(&|finding: &Finding| matches!(
            finding,
            Finding::DuplicateIdentity { id, .. } if id == &gate_uuid(4)
        )),
        "got {:?}",
        result.findings
    );
    assert!(
        result.findings.iter().any(
            |(path, finding)| path == &records_dir.join("bad-record.toml")
                && matches!(finding, Finding::Unreadable { .. })
        ),
        "got {:?}",
        result.findings
    );
    assert!(
        result.findings.contains(&(
            name_disagrees_path.clone(),
            Finding::NameDisagrees { id: gate_uuid(10) }
        )),
        "got {:?}",
        result.findings
    );
    assert!(
        result.findings.contains(&(
            records_dir.join(format!("{}.toml", gate_uuid(12))),
            Finding::PathNotRelative {
                path: "/etc/passwd".to_string(),
            }
        )),
        "got {:?}",
        result.findings
    );
    assert!(
        result.findings.contains(&(
            records_dir.join(format!("{}.toml", gate_uuid(13))),
            Finding::EmptyHistory,
        )),
        "got {:?}",
        result.findings
    );
    assert!(
        result.findings.contains(&(
            records_dir.join(format!("{}.toml", gate_uuid(14))),
            Finding::MalformedHash {
                hash: "sha256-deadbeef".to_string(),
            }
        )),
        "got {:?}",
        result.findings
    );
    assert!(
        has(&|finding: &Finding| matches!(
            finding,
            Finding::HistoryEntryWithoutRun { hash: found } if found == &hash("nr").to_string()
        )),
        "got {:?}",
        result.findings
    );
    assert!(
        result.findings.contains(&(
            records_dir.join(format!("{}.toml", gate_uuid(16))),
            Finding::DanglingItem {
                item: gate_uuid(99),
            }
        )),
        "got {:?}",
        result.findings
    );
    assert!(
        has(&|finding: &Finding| matches!(
            finding,
            Finding::DuplicateIdentity { id, .. } if id == &gate_uuid(17)
        )),
        "got {:?}",
        result.findings
    );

    // No condition is a finding: every finding's path is one of the
    // files above, never `orphan.pdf`, `clean-missing.pdf` or the clean
    // unlinked item's file.
    let condition_paths: Vec<PathBuf> = vec![
        root.join("orphan.pdf"),
        records_dir.join(format!("{}.toml", gate_uuid(18))),
        items_dir.join(format!("{}.toml", gate_uuid(19))),
    ];
    for (path, finding) in &result.findings {
        assert!(
            !condition_paths.contains(path),
            "finding {finding:?} names a condition's own file {path:?}"
        );
    }

    // The three conditions: the orphan, the clean missing record and
    // the clean unlinked item — in kind order, each the only entry of
    // its kind.
    assert_eq!(
        result.conditions,
        vec![
            ("orphan.pdf".to_string(), Condition::Orphan),
            (
                "clean-missing.pdf".to_string(),
                Condition::Missing {
                    id: gate_uuid(18),
                    record: format!("{STATE_DIR}/{ARTIFACT_STORE}/{}.toml", gate_uuid(18)),
                },
            ),
            (
                format!("{ITEM_STORE}/{}.toml", gate_uuid(19)),
                Condition::Unlinked { id: gate_uuid(19) },
            ),
        ],
        "got {:?}",
        result.conditions
    );
    assert_eq!(result.orphans(), 1);
    assert_eq!(result.missing(), 1);
    assert_eq!(result.unlinked(), 1);
    assert_eq!(result.orphans(), pre_change_orphans(root));
    assert_eq!(result.missing(), pre_change_missing(root));
    assert_eq!(result.unlinked(), pre_change_unlinked(root));
}

/// An extractor double for a [`Reconciling`]: answers whatever
/// `answers` gives for a path, or `Extraction::Unreadable { message:
/// "not opened in this test" }` for a path it does not know, and counts
/// every call it received.
#[derive(Default)]
struct Extractor {
    answers: BTreeMap<PathBuf, Extraction>,
    calls: Cell<usize>,
}

impl Extractor {
    /// An extractor that answers from `answers`, keyed by full path.
    fn with(answers: BTreeMap<PathBuf, Extraction>) -> Extractor {
        Extractor {
            answers,
            calls: Cell::new(0),
        }
    }

    fn extract(&self, path: &Path) -> Extraction {
        self.calls.set(self.calls.get() + 1);
        self.answers
            .get(path)
            .cloned()
            .unwrap_or(Extraction::Unreadable {
                message: "not opened in this test".to_string(),
            })
    }

    fn calls(&self) -> usize {
        self.calls.get()
    }
}

/// A [`Reconciling`] built from `rehash`, the run name `run`, the
/// `--accept` paths given, and `extract` — the one helper task 4 adds.
fn reconciling<'a>(
    rehash: bool,
    run: &str,
    accept: &'a [PathBuf],
    extract: &'a dyn Fn(&Path) -> Extraction,
) -> Reconciling<'a> {
    Reconciling {
        rehash,
        accept,
        extract,
        run: RunId::new(run),
        timestamp: TIMESTAMP,
        tool_version: TOOL_VERSION,
    }
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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-2", &[], &|p| extractor.extract(p)),
    );

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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

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
    let item = item_id(UUID_B);
    write_item(root, "item.toml", &item_with_doi(item.clone(), "10.1000/x"));
    // A record whose size deliberately disagrees with the file, so the
    // fast path cannot possibly match it, over a history describing an
    // earlier version of the bytes.
    let mut record = fresh_record(
        root,
        id.clone(),
        Some(item.clone()),
        "paper.pdf",
        vec![entry_naming(
            hash_entry("version one content", "run-0"),
            &item,
        )],
    );
    record.size += 1;
    write_artifact_record(root, &record);

    let identifier = doi_identifier("10.1000/x");
    let mut answers = BTreeMap::new();
    answers.insert(
        root.join("paper.pdf"),
        Extraction::Found {
            identifier: identifier.to_string(),
            tier: "text-layer".to_string(),
        },
    );
    let extractor = Extractor::with(answers);
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    let new_hash = hash_bytes(b"version two content");
    assert_eq!(result.hashed, 1, "got {result:?}");
    assert_eq!(
        result.repairs,
        vec![(
            id.clone(),
            "paper.pdf".to_string(),
            Repair::Changed {
                hash: new_hash.to_string(),
                accepted: AcceptedBy::Extracted {
                    identifier: identifier.to_string(),
                },
            }
        )],
        "got {:?}",
        result.repairs
    );
    let after = ArtifactStore::read(root).by_id(&id).unwrap().clone();
    assert_eq!(
        after.history,
        vec![
            entry_naming(hash_entry("version one content", "run-0"), &item),
            HashEntry {
                hash: new_hash,
                run: RunId::new("run-1"),
                timestamp: TIMESTAMP.to_string(),
                tool_version: TOOL_VERSION.to_string(),
                item: Some(item),
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
    let item = item_id(UUID_B);
    write_item(root, "item.toml", &item_with_doi(item.clone(), "10.1000/x"));
    let record = fresh_record(
        root,
        id.clone(),
        Some(item.clone()),
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
    let extractor = Extractor::default();
    let plain = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

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

    let identifier = doi_identifier("10.1000/x");
    let mut answers = BTreeMap::new();
    answers.insert(
        path.clone(),
        Extraction::Found {
            identifier: identifier.to_string(),
            tier: "text-layer".to_string(),
        },
    );
    let extractor = Extractor::with(answers);
    let rehashed = reconcile(
        root,
        &reconciling(true, "run-2", &[], &|p| extractor.extract(p)),
    );

    let new_hash = hash_bytes(&edited);
    assert_eq!(rehashed.hashed, 1, "got {rehashed:?}");
    assert_eq!(
        rehashed.repairs,
        vec![(
            id.clone(),
            "paper.pdf".to_string(),
            Repair::Changed {
                hash: new_hash.to_string(),
                accepted: AcceptedBy::Extracted {
                    identifier: identifier.to_string(),
                },
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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

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
    let extractor = Extractor::default();
    let second = reconcile(
        root,
        &reconciling(false, "run-2", &[], &|p| extractor.extract(p)),
    );
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

    let extractor = Extractor::default();
    let first = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );
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
    let extractor = Extractor::default();
    let second = reconcile(
        root,
        &reconciling(false, "run-2", &[], &|p| extractor.extract(p)),
    );
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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-2", &[], &|p| extractor.extract(p)),
    );

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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

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

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(
        result.repairs,
        vec![(id, "gone.pdf".to_string(), Repair::Missing)],
        "neither items/ nor .borax/ may supply a candidate, got {:?}",
        result.repairs
    );
}

/// A record whose last-known path points *into* a nested library is not
/// confirmed by the file standing there: the subtree is opaque to the
/// enclosing library, so the record's artifact is one this library
/// cannot find. Asserted with `--rehash` as well, since the fast path
/// and the hashing path are two separate ways to confirm a record, and
/// with `hashed` at zero either way, which is what says the nested
/// file was never opened.
#[test]
fn reconcile_does_not_confirm_a_record_whose_path_is_inside_a_nested_library() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("nested/.borax.toml"), b"").unwrap();
    fs::write(root.join("nested/kept.pdf"), b"kept bytes").unwrap();
    let id = artifact_id(UUID_A);
    let record = fresh_record(
        root,
        id.clone(),
        None,
        "nested/kept.pdf",
        vec![hash_entry("kept bytes", "run-0")],
    );
    write_artifact_record(root, &record);
    let before = snapshot(root);

    for rehash in [false, true] {
        let extractor = Extractor::default();
        let result = reconcile(
            root,
            &reconciling(rehash, "run-1", &[], &|p| extractor.extract(p)),
        );

        assert_eq!(
            result.repairs,
            vec![(id.clone(), "nested/kept.pdf".to_string(), Repair::Missing)],
            "a record inside a nested library must be missing, not confirmed \
             (rehash: {rehash}), got {:?}",
            result.repairs
        );
        assert_eq!(result.confirmed, 0, "(rehash: {rehash}) got {result:?}");
        assert_eq!(
            result.hashed, 0,
            "a nested library's file must not be opened (rehash: {rehash}), got {result:?}"
        );
        assert_eq!(
            snapshot(root),
            before,
            "nothing may be written (rehash: {rehash})"
        );
    }
}

/// The same exclusion, by the same one rule: a record whose path points
/// under `items/` or under `.borax/` names something that is not this
/// library's artifact, so no file standing there confirms it.
#[test]
fn reconcile_does_not_confirm_a_record_whose_path_is_under_items_or_the_state_dir() {
    for excluded in [
        format!("{ITEM_STORE}/smuggled.pdf"),
        format!("{STATE_DIR}/smuggled.pdf"),
    ] {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let path = relative_to(root, &excluded);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"smuggled bytes").unwrap();
        let id = artifact_id(UUID_A);
        let record = fresh_record(
            root,
            id.clone(),
            None,
            &excluded,
            vec![hash_entry("smuggled bytes", "run-0")],
        );
        write_artifact_record(root, &record);

        let extractor = Extractor::default();
        let result = reconcile(
            root,
            &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
        );

        assert_eq!(
            result.repairs,
            vec![(id, excluded.clone(), Repair::Missing)],
            "a record naming {excluded} must be missing, not confirmed, got {:?}",
            result.repairs
        );
        assert_eq!(result.confirmed, 0, "got {result:?}");
    }
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

    let extractor = Extractor::default();
    let reconciliation = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );
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
            pending: 0,
            ambiguous: 0,
            missing: 0,
            hashed: 1,
        },
        "got {:?}",
        events[1]
    );
}

// ---------------------------------------------------------------------
// 4: reconciliation — the gate, pending, and --accept (design D1-D4)
// ---------------------------------------------------------------------

#[test]
fn an_unrelated_replacement_is_left_pending_and_nothing_is_written() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"replacement bytes").unwrap();
    let id = artifact_id(UUID_A);
    let item = item_id(UUID_B);
    write_item(
        root,
        "item.toml",
        &item_with_doi(item.clone(), "10.1000/orig"),
    );
    let record = artifact_record(
        id.clone(),
        Some(item.clone()),
        "paper.pdf",
        vec![hash_entry("original bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let before = snapshot(root);
    let identifier = doi_identifier("10.1000/other");
    let extraction = Extraction::Found {
        identifier: identifier.to_string(),
        tier: "text-layer".to_string(),
    };
    let mut answers = BTreeMap::new();
    answers.insert(root.join("paper.pdf"), extraction.clone());
    let extractor = Extractor::with(answers);
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(snapshot(root), before, "a pending change writes nothing");
    assert_eq!(
        result.repairs,
        vec![(
            id,
            "paper.pdf".to_string(),
            Repair::Pending {
                hash: hash_bytes(b"replacement bytes").to_string(),
                item: Some(item.to_string()),
                extraction,
            }
        )],
        "got {:?}",
        result.repairs
    );
    assert_eq!(result.pending, 1, "got {result:?}");
    assert_eq!(result.confirmed, 0, "got {result:?}");
}

#[test]
fn a_pending_change_is_reported_again_by_the_next_reconcile() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"replacement bytes").unwrap();
    let id = artifact_id(UUID_A);
    let item = item_id(UUID_B);
    write_item(
        root,
        "item.toml",
        &item_with_doi(item.clone(), "10.1000/orig"),
    );
    let record = artifact_record(
        id.clone(),
        Some(item.clone()),
        "paper.pdf",
        vec![hash_entry("original bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let identifier = doi_identifier("10.1000/other");
    let extraction = Extraction::Found {
        identifier: identifier.to_string(),
        tier: "text-layer".to_string(),
    };
    let mut answers = BTreeMap::new();
    answers.insert(root.join("paper.pdf"), extraction.clone());

    let extractor = Extractor::with(answers.clone());
    let first = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );
    let extractor = Extractor::with(answers);
    let second = reconcile(
        root,
        &reconciling(false, "run-2", &[], &|p| extractor.extract(p)),
    );

    for result in [&first, &second] {
        assert_eq!(result.hashed, 1, "got {result:?}");
        assert_eq!(
            result.repairs,
            vec![(
                id.clone(),
                "paper.pdf".to_string(),
                Repair::Pending {
                    hash: hash_bytes(b"replacement bytes").to_string(),
                    item: Some(item.to_string()),
                    extraction: extraction.clone(),
                }
            )],
            "got {:?}",
            result.repairs
        );
    }
}

#[test]
fn a_replacement_with_no_text_layer_is_left_pending() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"replacement bytes").unwrap();
    let id = artifact_id(UUID_A);
    let item = item_id(UUID_B);
    write_item(
        root,
        "item.toml",
        &item_with_doi(item.clone(), "10.1000/orig"),
    );
    let record = artifact_record(
        id.clone(),
        Some(item.clone()),
        "paper.pdf",
        vec![hash_entry("original bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let mut answers = BTreeMap::new();
    answers.insert(root.join("paper.pdf"), Extraction::NoTextLayer);
    let extractor = Extractor::with(answers);
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(
        result.repairs,
        vec![(
            id,
            "paper.pdf".to_string(),
            Repair::Pending {
                hash: hash_bytes(b"replacement bytes").to_string(),
                item: Some(item.to_string()),
                extraction: Extraction::NoTextLayer,
            }
        )],
        "got {:?}",
        result.repairs
    );
}

#[test]
fn an_unreadable_replacement_is_left_pending() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"replacement bytes").unwrap();
    let id = artifact_id(UUID_A);
    let item = item_id(UUID_B);
    write_item(
        root,
        "item.toml",
        &item_with_doi(item.clone(), "10.1000/orig"),
    );
    let record = artifact_record(
        id.clone(),
        Some(item.clone()),
        "paper.pdf",
        vec![hash_entry("original bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let mut answers = BTreeMap::new();
    answers.insert(
        root.join("paper.pdf"),
        Extraction::Unreadable {
            message: "truncated stream".to_string(),
        },
    );
    let extractor = Extractor::with(answers);
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(
        result.repairs,
        vec![(
            id,
            "paper.pdf".to_string(),
            Repair::Pending {
                hash: hash_bytes(b"replacement bytes").to_string(),
                item: Some(item.to_string()),
                extraction: Extraction::Unreadable {
                    message: "truncated stream".to_string(),
                },
            }
        )],
        "got {:?}",
        result.repairs
    );
}

#[test]
fn a_change_at_a_record_linking_no_item_is_pending_naming_no_item() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"replacement bytes").unwrap();
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id.clone(),
        None,
        "paper.pdf",
        vec![hash_entry("original bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let identifier = doi_identifier("10.1000/other");
    let extraction = Extraction::Found {
        identifier: identifier.to_string(),
        tier: "text-layer".to_string(),
    };
    let mut answers = BTreeMap::new();
    answers.insert(root.join("paper.pdf"), extraction.clone());
    let extractor = Extractor::with(answers);
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(
        result.repairs,
        vec![(
            id,
            "paper.pdf".to_string(),
            Repair::Pending {
                hash: hash_bytes(b"replacement bytes").to_string(),
                item: None,
                extraction,
            }
        )],
        "got {:?}",
        result.repairs
    );
}

#[test]
fn a_change_whose_linked_item_no_file_carries_is_pending() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"replacement bytes").unwrap();
    let id = artifact_id(UUID_A);
    // A dangling link: no item file names `item` at all.
    let item = item_id(UUID_B);
    let record = artifact_record(
        id.clone(),
        Some(item.clone()),
        "paper.pdf",
        vec![hash_entry("original bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let identifier = doi_identifier("10.1000/x");
    let extraction = Extraction::Found {
        identifier: identifier.to_string(),
        tier: "text-layer".to_string(),
    };
    let mut answers = BTreeMap::new();
    answers.insert(root.join("paper.pdf"), extraction.clone());
    let extractor = Extractor::with(answers);
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(
        result.repairs,
        vec![(
            id,
            "paper.pdf".to_string(),
            Repair::Pending {
                hash: hash_bytes(b"replacement bytes").to_string(),
                item: Some(item.to_string()),
                extraction,
            }
        )],
        "got {:?}",
        result.repairs
    );
}

#[test]
fn a_later_arxiv_version_is_accepted() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"replacement bytes").unwrap();
    let id = artifact_id(UUID_A);
    let item = item_id(UUID_B);
    let mut item_record = minimal_record(EntryType::Article);
    item_record.borax.arxiv = Some(ArxivId::parse("2401.12345v2").unwrap());
    write_item(
        root,
        "item.toml",
        &Item {
            id: item.clone(),
            record: item_record,
        },
    );
    let record = artifact_record(
        id.clone(),
        Some(item),
        "paper.pdf",
        vec![hash_entry("original bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let identifier = Identifier::Arxiv(ArxivId::parse("2401.12345v1").unwrap());
    let extraction = Extraction::Found {
        identifier: identifier.to_string(),
        tier: "text-layer".to_string(),
    };
    let mut answers = BTreeMap::new();
    answers.insert(root.join("paper.pdf"), extraction);
    let extractor = Extractor::with(answers);
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(
        result.repairs,
        vec![(
            id,
            "paper.pdf".to_string(),
            Repair::Changed {
                hash: hash_bytes(b"replacement bytes").to_string(),
                accepted: AcceptedBy::Extracted {
                    identifier: identifier.to_string(),
                },
            }
        )],
        "got {:?}",
        result.repairs
    );
}

#[test]
fn a_change_the_fast_path_hides_is_pending_under_rehash_when_its_identifier_is_not_carried() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    let original: &[u8] = b"original content, unedited";
    fs::write(&path, original).unwrap();
    let id = artifact_id(UUID_A);
    let item = item_id(UUID_B);
    write_item(
        root,
        "item.toml",
        &item_with_doi(item.clone(), "10.1000/orig"),
    );
    let record = fresh_record(
        root,
        id.clone(),
        Some(item.clone()),
        "paper.pdf",
        vec![hash_entry("original content, unedited", "run-0")],
    );
    write_artifact_record(root, &record);
    let modified = fs::metadata(&path).unwrap().modified().unwrap();

    let mut edited = original.to_vec();
    edited[0] = b'X';
    fs::write(&path, &edited).unwrap();
    fs::File::open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();

    let identifier = doi_identifier("10.1000/other");
    let extraction = Extraction::Found {
        identifier: identifier.to_string(),
        tier: "text-layer".to_string(),
    };
    let mut answers = BTreeMap::new();
    answers.insert(path.clone(), extraction.clone());
    let extractor = Extractor::with(answers);
    let rehashed = reconcile(
        root,
        &reconciling(true, "run-2", &[], &|p| extractor.extract(p)),
    );

    let new_hash = hash_bytes(&edited);
    assert_eq!(
        rehashed.repairs,
        vec![(
            id,
            "paper.pdf".to_string(),
            Repair::Pending {
                hash: new_hash.to_string(),
                item: Some(item.to_string()),
                extraction,
            }
        )],
        "got {:?}",
        rehashed.repairs
    );
}

#[test]
fn swapped_files_both_repair_and_no_file_is_opened() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("a.pdf"), b"content-b").unwrap();
    fs::write(root.join("b.pdf"), b"content-a").unwrap();
    let id_a = artifact_id(UUID_A);
    let id_b = artifact_id(UUID_B);
    let item_a = item_id(UUID_C);
    let item_b = item_id(UUID_D);
    let record_a = artifact_record(
        id_a.clone(),
        Some(item_a),
        "a.pdf",
        vec![hash_entry("content-a", "run-0")],
    );
    let record_b = artifact_record(
        id_b.clone(),
        Some(item_b),
        "b.pdf",
        vec![hash_entry("content-b", "run-0")],
    );
    write_artifact_record(root, &record_a);
    write_artifact_record(root, &record_b);

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(result.repairs.len(), 2, "got {:?}", result.repairs);
    assert!(
        result
            .repairs
            .iter()
            .all(|(_, _, repair)| matches!(repair, Repair::Repaired { .. })),
        "both must repair in step 2, before extraction would ever run: {:?}",
        result.repairs
    );
    assert_eq!(
        extractor.calls(),
        0,
        "a swap is settled in step 2, so no file is opened"
    );
}

#[test]
fn an_untouched_library_opens_no_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_file(root, "paper.pdf", b"steady content");
    let id = artifact_id(UUID_A);
    let record = fresh_record(
        root,
        id,
        None,
        "paper.pdf",
        vec![hash_entry("steady content", "run-0")],
    );
    write_artifact_record(root, &record);

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert!(result.repairs.is_empty(), "got {:?}", result.repairs);
    assert_eq!(extractor.calls(), 0);
}

#[test]
fn accept_accepts_a_pending_change_under_the_records_link() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"replacement bytes");
    let id = artifact_id(UUID_A);
    let item = item_id(UUID_B);
    let record = artifact_record(
        id.clone(),
        Some(item.clone()),
        "paper.pdf",
        vec![hash_entry("original bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let extractor = Extractor::default();
    let accept = vec![path.clone()];
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &accept, &|p| extractor.extract(p)),
    );

    let new_hash = hash_bytes(b"replacement bytes");
    assert_eq!(
        result.repairs,
        vec![(
            id.clone(),
            "paper.pdf".to_string(),
            Repair::Changed {
                hash: new_hash.to_string(),
                accepted: AcceptedBy::Operator,
            }
        )],
        "got {:?}",
        result.repairs
    );
    assert!(result.not_pending.is_empty(), "got {result:?}");

    let after = ArtifactStore::read(root).by_id(&id).unwrap().clone();
    assert_eq!(
        after.history.last().unwrap().item,
        Some(item),
        "the appended entry must name the record's link"
    );
    let (size, modified_millis) = stat(&path);
    assert_eq!(after.size, size);
    assert_eq!(after.modified_millis, modified_millis);

    // The next run settles the record on the fast path, hashing 0.
    let extractor = Extractor::default();
    let next = reconcile(
        root,
        &reconciling(false, "run-2", &[], &|p| extractor.extract(p)),
    );
    assert_eq!(next.hashed, 0, "got {next:?}");
    assert_eq!(next.confirmed, 1, "got {next:?}");
}

#[test]
fn accept_reaches_a_change_the_fast_path_hides_without_rehash() {
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

    let mut edited = original.to_vec();
    edited[0] = b'X';
    fs::write(&path, &edited).unwrap();
    fs::File::open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();

    let extractor = Extractor::default();
    let accept = vec![path];
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &accept, &|p| extractor.extract(p)),
    );

    let new_hash = hash_bytes(&edited);
    assert_eq!(result.hashed, 1, "got {result:?}");
    assert_eq!(
        result.repairs,
        vec![(
            id,
            "paper.pdf".to_string(),
            Repair::Changed {
                hash: new_hash.to_string(),
                accepted: AcceptedBy::Operator,
            }
        )],
        "got {:?}",
        result.repairs
    );
}

#[test]
fn accept_on_a_record_linking_no_item_appends_an_entry_naming_none() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"replacement bytes");
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id.clone(),
        None,
        "paper.pdf",
        vec![hash_entry("original bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let extractor = Extractor::default();
    let accept = vec![path];
    let _ = reconcile(
        root,
        &reconciling(false, "run-1", &accept, &|p| extractor.extract(p)),
    );

    let after = ArtifactStore::read(root).by_id(&id).unwrap().clone();
    assert_eq!(after.history.last().unwrap().item, None);
}

#[test]
fn accept_with_two_paths_accepts_both() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path_a = write_file(root, "a.pdf", b"replacement a");
    let path_b = write_file(root, "b.pdf", b"replacement b");
    let id_a = artifact_id(UUID_A);
    let id_b = artifact_id(UUID_B);
    write_artifact_record(
        root,
        &artifact_record(id_a, None, "a.pdf", vec![hash_entry("original a", "run-0")]),
    );
    write_artifact_record(
        root,
        &artifact_record(id_b, None, "b.pdf", vec![hash_entry("original b", "run-0")]),
    );

    let extractor = Extractor::default();
    let accept = vec![path_a, path_b];
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &accept, &|p| extractor.extract(p)),
    );

    assert_eq!(result.repairs.len(), 2, "got {:?}", result.repairs);
    assert!(
        result.repairs.iter().all(|(_, _, repair)| matches!(
            repair,
            Repair::Changed {
                accepted: AcceptedBy::Operator,
                ..
            }
        )),
        "got {:?}",
        result.repairs
    );
}

#[test]
fn accept_naming_one_path_twice_accepts_it_once() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"replacement bytes");
    let id = artifact_id(UUID_A);
    write_artifact_record(
        root,
        &artifact_record(
            id.clone(),
            None,
            "paper.pdf",
            vec![hash_entry("original bytes", "run-0")],
        ),
    );

    let extractor = Extractor::default();
    let accept = vec![path.clone(), path];
    let _ = reconcile(
        root,
        &reconciling(false, "run-1", &accept, &|p| extractor.extract(p)),
    );

    let after = ArtifactStore::read(root).by_id(&id).unwrap().clone();
    assert_eq!(after.history.len(), 2, "one appended entry, not two");
}

#[test]
fn accept_normalises_dot_dot_in_a_named_path() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("sub")).unwrap();
    write_file(root, "a.pdf", b"replacement bytes");
    let id = artifact_id(UUID_A);
    write_artifact_record(
        root,
        &artifact_record(
            id.clone(),
            None,
            "a.pdf",
            vec![hash_entry("original bytes", "run-0")],
        ),
    );

    let extractor = Extractor::default();
    let accept = vec![root.join("sub").join("..").join("a.pdf")];
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &accept, &|p| extractor.extract(p)),
    );

    assert!(result.not_pending.is_empty(), "got {result:?}");
    let after = ArtifactStore::read(root).by_id(&id).unwrap().clone();
    assert_eq!(
        after.history.len(),
        2,
        "the dot-dot path must still reach a.pdf's record"
    );
}

#[test]
fn accept_of_a_change_the_identifier_accepted_reports_it_extracted() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"replacement bytes");
    let id = artifact_id(UUID_A);
    let item = item_id(UUID_B);
    write_item(root, "item.toml", &item_with_doi(item.clone(), "10.1000/x"));
    write_artifact_record(
        root,
        &artifact_record(
            id.clone(),
            Some(item),
            "paper.pdf",
            vec![hash_entry("original bytes", "run-0")],
        ),
    );

    let identifier = doi_identifier("10.1000/x");
    let mut answers = BTreeMap::new();
    answers.insert(
        path.clone(),
        Extraction::Found {
            identifier: identifier.to_string(),
            tier: "text-layer".to_string(),
        },
    );
    let extractor = Extractor::with(answers);
    let accept = vec![path];
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &accept, &|p| extractor.extract(p)),
    );

    assert!(result.not_pending.is_empty(), "got {result:?}");
    assert_eq!(
        result.repairs,
        vec![(
            id,
            "paper.pdf".to_string(),
            Repair::Changed {
                hash: hash_bytes(b"replacement bytes").to_string(),
                accepted: AcceptedBy::Extracted {
                    identifier: identifier.to_string(),
                },
            }
        )],
        "got {:?}",
        result.repairs
    );
}

#[test]
fn accept_of_a_confirmed_path_reports_nothing_and_writes_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"steady content");
    let id = artifact_id(UUID_A);
    write_artifact_record(
        root,
        &fresh_record(
            root,
            id,
            None,
            "paper.pdf",
            vec![hash_entry("steady content", "run-0")],
        ),
    );
    let before = snapshot(root);

    let extractor = Extractor::default();
    let accept = vec![path];
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &accept, &|p| extractor.extract(p)),
    );

    assert!(result.repairs.is_empty(), "got {:?}", result.repairs);
    assert!(result.not_pending.is_empty(), "got {result:?}");
    assert_eq!(snapshot(root), before);
}

#[test]
fn accept_of_a_path_no_record_names_is_not_pending() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "typo.pdf", b"anything");

    let extractor = Extractor::default();
    let accept = vec![path.clone()];
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &accept, &|p| extractor.extract(p)),
    );

    assert_eq!(result.not_pending, vec![path]);
}

#[test]
fn accept_of_a_path_whose_record_was_repaired_elsewhere_is_not_pending() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("new")).unwrap();
    fs::write(root.join("new/paper.pdf"), b"moved bytes").unwrap();
    let id = artifact_id(UUID_A);
    write_artifact_record(
        root,
        &artifact_record(
            id,
            None,
            "old/paper.pdf",
            vec![hash_entry("moved bytes", "run-0")],
        ),
    );

    let extractor = Extractor::default();
    let accept_path = root.join("old/paper.pdf");
    let accept = vec![accept_path.clone()];
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &accept, &|p| extractor.extract(p)),
    );

    assert_eq!(result.not_pending, vec![accept_path]);
}

#[test]
fn accept_of_a_path_outside_the_library_is_not_pending() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let outside = tempdir().unwrap();
    let path = outside.path().join("elsewhere.pdf");
    fs::write(&path, b"anything").unwrap();

    let extractor = Extractor::default();
    let accept = vec![path.clone()];
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &accept, &|p| extractor.extract(p)),
    );

    assert_eq!(result.not_pending, vec![path]);
}

#[test]
fn an_accepted_entry_is_appended_as_a_table_naming_its_item_and_earlier_entries_are_byte_identical()
{
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"replacement bytes");
    let id = artifact_id(UUID_A);
    let item = item_id(UUID_B);

    let before_record = artifact_record(
        id.clone(),
        Some(item.clone()),
        "paper.pdf",
        vec![entry_naming(hash_entry("original bytes", "run-0"), &item)],
    );
    let mut before_text = before_record.to_toml();
    // A comment the fixture places in the record, which an edit to one
    // field must leave alone.
    before_text = before_text.replacen(
        "\npath = ",
        "\n# a comment the fixture places in the record\npath = ",
        1,
    );
    let record_dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&record_dir).unwrap();
    let record_path = record_dir.join(format!("{id}.toml"));
    fs::write(&record_path, &before_text).unwrap();

    let extractor = Extractor::default();
    let accept = vec![path.clone()];
    let _ = reconcile(
        root,
        &reconciling(false, "run-1", &accept, &|p| extractor.extract(p)),
    );

    let after_text = fs::read_to_string(&record_path).unwrap();

    assert!(
        after_text.contains("# a comment the fixture places in the record"),
        "got {after_text:?}"
    );
    assert!(
        after_text.contains(&format!("id = \"{id}\"")),
        "got {after_text:?}"
    );
    assert!(
        after_text.contains(&format!("item = \"{item}\"")),
        "got {after_text:?}"
    );
    assert!(
        after_text.contains("path = \"paper.pdf\""),
        "got {after_text:?}"
    );

    let (size, modified_millis) = stat(&path);
    assert!(
        after_text.contains(&format!("size = {size}")),
        "got {after_text:?}"
    );
    assert!(
        after_text.contains(&format!("modified_millis = {modified_millis}")),
        "got {after_text:?}"
    );

    let marker = "[[history]]";
    let before_first = before_text.find(marker).expect("one history table");
    let before_table = &before_text[before_first..];
    let after_first = after_text.find(marker).expect("a history table");
    let after_rest = &after_text[after_first..];
    let second_at = after_rest[marker.len()..]
        .find(marker)
        .map(|at| at + marker.len())
        .expect("exactly one new table must follow the old one");
    assert_eq!(
        after_rest[..second_at].trim_end(),
        before_table.trim_end(),
        "the table that existed before must be byte-identical"
    );
    assert!(
        !after_rest[second_at + marker.len()..].contains(marker),
        "exactly one new table, got {after_text:?}"
    );
    let new_table = &after_rest[second_at..];
    assert!(
        new_table
            .trim_end()
            .ends_with(&format!("item = \"{item}\"")),
        "the new table must end naming its item, got {new_table:?}"
    );
}

#[test]
fn an_acceptance_after_a_hand_correction_restates_the_newest_bytes() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_file(root, "paper.pdf", b"content-b");
    let id = artifact_id(UUID_A);
    let x = item_id(UUID_C);
    let y = item_id(UUID_D);
    write_item(root, "item.toml", &item_with_doi(y.clone(), "10.1000/y"));

    let mut entry_a = hash_entry("content-a", "run-0");
    entry_a.item = Some(x.clone());
    let record = artifact_record(
        id.clone(),
        Some(y.clone()),
        "paper.pdf",
        vec![entry_a.clone()],
    );
    write_artifact_record(root, &record);

    let identifier = doi_identifier("10.1000/y");
    let mut answers = BTreeMap::new();
    answers.insert(
        root.join("paper.pdf"),
        Extraction::Found {
            identifier: identifier.to_string(),
            tier: "text-layer".to_string(),
        },
    );
    let extractor = Extractor::with(answers);
    let _ = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    let after = ArtifactStore::read(root).by_id(&id).unwrap().clone();
    assert_eq!(after.history.len(), 3, "got {:?}", after.history);
    assert_eq!(after.history[0].hash, entry_a.hash);
    assert_eq!(after.history[0].item, Some(x));
    assert_eq!(
        after.history[1].hash, entry_a.hash,
        "the restatement names a's bytes"
    );
    assert_eq!(after.history[1].item, Some(y.clone()));
    assert_eq!(after.history[2].hash, hash_bytes(b"content-b"));
    assert_eq!(after.history[2].item, Some(y));
}

#[test]
fn a_record_written_before_entries_named_items_is_confirmed_without_being_rewritten() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let bytes = b"steady content";
    let path = root.join("paper.pdf");
    fs::write(&path, bytes).unwrap();
    let (size, modified_millis) = stat(&path);
    let hash = hash_bytes(bytes);

    let text = format!(
        "id = \"{UUID_A}\"\n\
         path = \"paper.pdf\"\n\
         size = {size}\n\
         modified_millis = {modified_millis}\n\n\
         [[history]]\n\
         hash = \"{hash}\"\n\
         run = \"run-0\"\n\
         timestamp = \"2026-01-01T00:00:00Z\"\n\
         tool_version = \"0.9.0\"\n",
    );
    let record_dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&record_dir).unwrap();
    let record_path = record_dir.join(format!("{UUID_A}.toml"));
    fs::write(&record_path, &text).unwrap();

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(result.confirmed, 1, "got {result:?}");
    assert_eq!(
        extractor.calls(),
        0,
        "a fast-path match reads nothing, let alone extracts"
    );
    let after_text = fs::read_to_string(&record_path).unwrap();
    assert_eq!(
        after_text, text,
        "a record confirmed by its 0.9.0-shaped entries must not be rewritten"
    );
}

// ---------------------------------------------------------------------
// group 7 fixtures — writing real files for `admit`'s stat, and a
// record carrying a chosen identifier
// ---------------------------------------------------------------------

/// Writes `bytes` at `relative` under `root`, creating whatever
/// directories it needs, and hands back the full path — the file
/// `admit` and `recorded_at` need present on disk, since both stat or
/// hash it.
fn write_file(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, bytes).unwrap();
    path
}

/// A minimal record carrying `doi_value` as its DOI, the one identifier
/// these fixtures need.
fn record_with_doi(doi_value: &str) -> Record {
    let mut record = minimal_record(EntryType::Article);
    record.doi = Some(Doi::parse(doi_value).unwrap());
    record
}

// ---------------------------------------------------------------------
// 7.1/7.3a: recorded_at
// ---------------------------------------------------------------------

/// [`recorded_at`] finds the record whose last-known path is the file's
/// current path — the lookup an applying run makes before it moves a
/// file, and the one `--no-record`'s pre-move check is built on.
#[test]
fn recorded_at_finds_the_record_naming_the_current_path() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let record = fresh_record(
        root,
        artifact_id(UUID_A),
        None,
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let found = recorded_at(root, &path);

    assert_eq!(
        found,
        Some(record),
        "the record naming the file's current path must be found"
    );
}

/// A file no record names yields `None`: a move nothing has recorded
/// strands nothing.
#[test]
fn recorded_at_is_none_when_no_record_names_the_path() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "untouched.pdf", b"untouched bytes");

    assert_eq!(
        recorded_at(root, &path),
        None,
        "a file with no record must not be found by an unrelated one"
    );
}

// ---------------------------------------------------------------------
// 7.3a: strands
// ---------------------------------------------------------------------

/// No record names the file's path: moving it strands nothing.
#[test]
fn strands_is_false_when_no_record_names_the_path() {
    assert!(
        !strands(None, &hash("anything")),
        "a move with no record in the way must never be refused"
    );
}

/// A record whose history holds the file's hash survives the move: the
/// bytes are evidence a move does not touch.
#[test]
fn strands_is_false_when_the_history_holds_the_hash() {
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );

    assert!(
        !strands(Some(&record), &hash("paper bytes")),
        "a hash already in the history must not strand the record"
    );
}

/// A record whose history does not hold the file's hash cannot survive
/// the move: after it, the record's path holds nothing and none of its
/// hashes match anything.
#[test]
fn strands_is_true_when_the_history_does_not_hold_the_hash() {
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "paper.pdf",
        vec![hash_entry("stale bytes", "run-0")],
    );

    assert!(
        strands(Some(&record), &hash("fresh bytes")),
        "a move that would leave no matching hash behind must be refused"
    );
}

// ---------------------------------------------------------------------
// 7.1: item_file_name
// ---------------------------------------------------------------------

/// A key that folds to something names the file alongside the item's
/// full UUID.
#[test]
fn item_file_name_uses_the_folded_key_and_full_uuid() {
    let id = item_id(UUID_A);

    assert_eq!(
        item_file_name(Some("Milner 1978"), &id),
        format!("milner-1978.{UUID_A}.toml")
    );
}

/// No key at all: the UUID alone has to be unique on its own.
#[test]
fn item_file_name_falls_back_to_the_uuid_alone_when_key_is_none() {
    let id = item_id(UUID_A);

    assert_eq!(item_file_name(None, &id), format!("{UUID_A}.toml"));
}

/// A key that folds to nothing — punctuation alone — is the same as no
/// key at all.
#[test]
fn item_file_name_falls_back_to_the_uuid_alone_when_the_key_folds_to_nothing() {
    let id = item_id(UUID_A);

    assert_eq!(item_file_name(Some("???"), &id), format!("{UUID_A}.toml"));
}

// ---------------------------------------------------------------------
// 7.1/7.2/7.2a/7.2b: admit — minting, reuse, and re-linking
// ---------------------------------------------------------------------

/// task 7.1: an applying run's admission of a new file mints an item and
/// an artifact record naming the file's new path, its hash, and its
/// current size and modification time.
#[test]
fn admit_mints_an_item_and_an_artifact_record_for_a_new_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let record = record_with_doi("10.1000/admit-mint");
    let hash = hash("paper bytes");

    let admitted = admit(
        root,
        &Admitting {
            path: &path,
            record: &record,
            hash: &hash,
            held: None,
            reidentified: false,
            key: Some("smith2024"),
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .expect("admission of a plain new file must succeed");

    assert_eq!(
        admitted.relinked_from, None,
        "minting is not a relink: got {admitted:?}"
    );

    let items = ItemStore::read(root);
    assert_eq!(items.len(), 1, "exactly one item must be minted");
    let item = items
        .by_id(&admitted.item)
        .expect("the minted item must be found by the id admit reported");
    assert_eq!(item.record.doi, record.doi);

    let records = ArtifactStore::read(root);
    let saved = records
        .by_id(&admitted.artifact)
        .expect("the minted artifact record must be found by the id admit reported");
    assert_eq!(saved.path, "paper.pdf");
    assert_eq!(saved.item, Some(admitted.item.clone()));
    assert_eq!(saved.history.len(), 1, "got {:?}", saved.history);
    assert_eq!(saved.history[0].hash, hash);
    assert_eq!(saved.history[0].run, RunId::new("run-1"));
    let (size, modified_millis) = stat(&path);
    assert_eq!(saved.size, size);
    assert_eq!(saved.modified_millis, modified_millis);
}

/// task 7.2: a second PDF of one work reuses the item the library
/// already holds for one of the record's identifiers, so it becomes a
/// second record naming that one item rather than a second item.
#[test]
fn admit_reuses_the_item_the_library_already_holds_for_the_records_identifier() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "second.pdf", b"second bytes");
    let record = record_with_doi("10.1000/admit-reuse");
    let hash = hash("second bytes");
    let existing = Item {
        id: item_id(UUID_A),
        record: record.clone(),
    };
    write_item(root, &format!("first.{UUID_A}.toml"), &existing);

    let admitted = admit(
        root,
        &Admitting {
            path: &path,
            record: &record,
            hash: &hash,
            held: None,
            reidentified: false,
            key: Some("second-key"),
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    assert_eq!(
        admitted.item, existing.id,
        "the second PDF of one work must name the item already held for its identifier"
    );
    assert_eq!(
        ItemStore::read(root).len(),
        1,
        "no second item may be minted for a work the library already has"
    );
}

/// task 7.2: a record with no identifier at all still gets an item, and
/// two different such files become two different items — an item's
/// identity is its minted UUID, and identifiers are optional.
#[test]
fn admit_gives_two_files_with_no_identifier_two_different_items() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let record = minimal_record(EntryType::Article);

    let path_a = write_file(root, "a.pdf", b"a bytes");
    let hash_a = hash("a bytes");
    let admitted_a = admit(
        root,
        &Admitting {
            path: &path_a,
            record: &record,
            hash: &hash_a,
            held: None,
            reidentified: false,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let path_b = write_file(root, "b.pdf", b"b bytes");
    let hash_b = hash("b bytes");
    let admitted_b = admit(
        root,
        &Admitting {
            path: &path_b,
            record: &record,
            hash: &hash_b,
            held: None,
            reidentified: false,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    assert_ne!(
        admitted_a.item, admitted_b.item,
        "two identifier-less files must not collapse onto one item"
    );
    assert_eq!(ItemStore::read(root).len(), 2, "each must get its own item");
}

/// task 7.2a: an artifact that already has a record keeps its item link
/// across three ordinary re-admissions of a file resolving to no
/// identifier — one repetition would not show the accumulation design
/// D4 rules out.
#[test]
fn admit_keeps_the_held_records_item_link_over_three_ordinary_reruns() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "steady.pdf", b"steady bytes");
    let hash = hash("steady bytes");
    let record = minimal_record(EntryType::Article);
    let item = Item {
        id: item_id(UUID_A),
        record: record.clone(),
    };
    write_item(root, &format!("steady.{UUID_A}.toml"), &item);
    let held = fresh_record(
        root,
        artifact_id(UUID_B),
        Some(item.id.clone()),
        "steady.pdf",
        vec![hash_entry("steady bytes", "run-0")],
    );
    write_artifact_record(root, &held);

    for run in 1..=3 {
        let held_now = ArtifactStore::read(root)
            .by_id(&artifact_id(UUID_B))
            .cloned();
        let admitted = admit(
            root,
            &Admitting {
                path: &path,
                record: &record,
                hash: &hash,
                held: held_now.as_ref(),
                reidentified: false,
                key: None,
                run: RunId::new(format!("run-{run}")),
                timestamp: TIMESTAMP,
                tool_version: TOOL_VERSION,
            },
            &store_write,
        )
        .unwrap();

        assert_eq!(admitted.item, item.id, "run {run}: the link must not move");
        assert_eq!(
            admitted.relinked_from, None,
            "run {run}: an ordinary rerun must never report a relink"
        );
    }

    assert_eq!(
        ItemStore::read(root).len(),
        1,
        "no item may accumulate behind a file nobody re-identified"
    );
    assert_eq!(
        ArtifactStore::read(root).len(),
        1,
        "no second artifact record may be minted for the same file"
    );
}

/// task 7.2b: an operator's re-identification, and only an operator's,
/// re-links a recorded artifact to the item for the record they
/// settled on, keeping the artifact's own identity and leaving the item
/// it came from nameless but intact.
#[test]
fn admit_relinks_to_the_supplied_records_item_when_reidentified() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "wrong.pdf", b"wrong bytes");
    let hash = hash("wrong bytes");
    let wrong_item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("wrong.{UUID_A}.toml"), &wrong_item);
    let right_record = record_with_doi("10.1000/admit-relink");
    let right_item = Item {
        id: item_id(UUID_B),
        record: right_record.clone(),
    };
    write_item(root, &format!("right.{UUID_B}.toml"), &right_item);
    let held = fresh_record(
        root,
        artifact_id(UUID_C),
        Some(wrong_item.id.clone()),
        "wrong.pdf",
        vec![hash_entry("wrong bytes", "run-0")],
    );
    write_artifact_record(root, &held);

    let admitted = admit(
        root,
        &Admitting {
            path: &path,
            record: &right_record,
            hash: &hash,
            held: Some(&held),
            reidentified: true,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    assert_eq!(admitted.item, right_item.id, "got {admitted:?}");
    assert_eq!(
        admitted.relinked_from,
        Some(wrong_item.id.clone()),
        "the run must report which item the record was moved from"
    );

    let records = ArtifactStore::read(root);
    assert_eq!(
        records.by_id(&artifact_id(UUID_C)).unwrap().id,
        artifact_id(UUID_C),
        "the artifact keeps its own identity across the relink"
    );
    assert_eq!(
        records.by_id(&artifact_id(UUID_C)).unwrap().item,
        Some(right_item.id.clone())
    );
    assert!(
        records.by_item(&wrong_item.id).is_empty(),
        "the artifact must no longer name the item it left"
    );
}

// ---------------------------------------------------------------------
// 6: Admission — each entry names its item (design D6, D11)
// ---------------------------------------------------------------------

#[test]
fn admit_mints_a_record_whose_entry_names_its_item() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let record = record_with_doi("10.1000/admit-names-item");
    let hash = hash("paper bytes");

    let admitted = admit(
        root,
        &Admitting {
            path: &path,
            record: &record,
            hash: &hash,
            held: None,
            reidentified: false,
            key: Some("smith2024"),
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let saved = ArtifactStore::read(root)
        .by_id(&admitted.artifact)
        .unwrap()
        .clone();
    assert_eq!(saved.history.len(), 1, "got {:?}", saved.history);
    assert_eq!(saved.history[0].item, Some(admitted.item));
}

#[test]
fn admit_of_bytes_already_held_appends_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("item.{UUID_A}.toml"), &item);
    let entry_a = hash_entry("bytes a", "run-0");
    let entry_b = hash_entry("bytes b", "run-1");
    let held = artifact_record(
        artifact_id(UUID_B),
        Some(item.id.clone()),
        "paper.pdf",
        vec![entry_a.clone(), entry_b.clone()],
    );
    write_artifact_record(root, &held);

    let admitted = admit(
        root,
        &Admitting {
            path: &path,
            record: &item.record,
            hash: &hash_a,
            held: Some(&held),
            reidentified: false,
            key: None,
            run: RunId::new("run-2"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    assert_eq!(admitted.item, item.id);
    assert_eq!(admitted.relinked_from, None);

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_B))
        .unwrap()
        .clone();
    assert_eq!(
        after.history,
        vec![entry_a, entry_b],
        "an admission of bytes already held must append nothing"
    );
    let (size, modified_millis) = stat(&path);
    assert_eq!(after.size, size);
    assert_eq!(after.modified_millis, modified_millis);
    assert_eq!(after.item, Some(item.id));
}

#[test]
fn admit_of_the_newest_bytes_on_a_rerun_appends_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes b");
    let hash_b = hash("bytes b");
    let item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("item.{UUID_A}.toml"), &item);
    let held = artifact_record(
        artifact_id(UUID_B),
        Some(item.id.clone()),
        "paper.pdf",
        vec![hash_entry("bytes b", "run-0")],
    );
    write_artifact_record(root, &held);

    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &item.record,
            hash: &hash_b,
            held: Some(&held),
            reidentified: false,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_B))
        .unwrap()
        .clone();
    assert_eq!(after.history.len(), 1, "got {:?}", after.history);
}

#[test]
fn admit_reidentifying_the_newest_bytes_moves_the_link_and_appends_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let x = item_id(UUID_C);
    let y_record = record_with_doi("10.1000/admit-transition-y");
    let y = Item {
        id: item_id(UUID_D),
        record: y_record.clone(),
    };
    write_item(root, &format!("y.{UUID_D}.toml"), &y);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x.clone());
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(x.clone()),
        "paper.pdf",
        vec![entry_a.clone()],
    );
    write_artifact_record(root, &held);

    let admitted = admit(
        root,
        &Admitting {
            path: &path,
            record: &y_record,
            hash: &hash_a,
            held: Some(&held),
            reidentified: true,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    assert_eq!(admitted.item, y.id);
    assert_eq!(admitted.relinked_from, Some(x));

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(
        after.history,
        vec![entry_a],
        "no entry must be appended, got {:?}",
        after.history
    );
    assert_eq!(after.item, Some(y.id));
}

#[test]
fn admit_reidentifying_restored_bytes_appends_an_entry_under_the_new_item() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let x = item_id(UUID_B);
    let y = item_id(UUID_C);
    let z_record = record_with_doi("10.1000/admit-transition-z");
    let z = Item {
        id: item_id(UUID_D),
        record: z_record.clone(),
    };
    write_item(root, &format!("z.{UUID_D}.toml"), &z);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x);
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.clone());
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(y),
        "paper.pdf",
        vec![entry_a.clone(), entry_b.clone()],
    );
    write_artifact_record(root, &held);

    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &z_record,
            hash: &hash_a,
            held: Some(&held),
            reidentified: true,
            key: None,
            run: RunId::new("run-2"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(after.history.len(), 3, "got {:?}", after.history);
    assert_eq!(after.history[0], entry_a);
    assert_eq!(after.history[1], entry_b);
    assert_eq!(after.history[2].hash, entry_a.hash);
    assert_eq!(after.history[2].item, Some(z.id));
}

#[test]
fn admit_reidentifying_the_newest_bytes_as_the_item_they_already_answer_with_appends_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let x_record = record_with_doi("10.1000/admit-already-answers-x");
    let x = Item {
        id: item_id(UUID_B),
        record: x_record.clone(),
    };
    write_item(root, &format!("x.{UUID_B}.toml"), &x);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x.id.clone());
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(x.id.clone()),
        "paper.pdf",
        vec![entry_a.clone()],
    );
    write_artifact_record(root, &held);

    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &x_record,
            hash: &hash_a,
            held: Some(&held),
            reidentified: true,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(
        after.history,
        vec![entry_a],
        "re-identifying as the item already answered with must append nothing"
    );
}

#[test]
fn admit_reidentifying_restored_bytes_as_the_item_they_already_answer_with_appends_an_entry() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let x_record = record_with_doi("10.1000/admit-restored-already-answers-x");
    let x = Item {
        id: item_id(UUID_B),
        record: x_record.clone(),
    };
    write_item(root, &format!("x.{UUID_B}.toml"), &x);
    let y = item_id(UUID_C);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x.id.clone());
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.clone());
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(y),
        "paper.pdf",
        vec![entry_a.clone(), entry_b.clone()],
    );
    write_artifact_record(root, &held);

    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &x_record,
            hash: &hash_a,
            held: Some(&held),
            reidentified: true,
            key: None,
            run: RunId::new("run-2"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(after.history.len(), 3, "got {:?}", after.history);
    assert_eq!(after.history[2].hash, entry_a.hash);
    assert_eq!(after.history[2].item, Some(x.id));
}

#[test]
fn admit_captures_the_link_as_read_before_assigning_the_new_one() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let x_record = record_with_doi("10.1000/admit-link-as-read-x");
    let x = Item {
        id: item_id(UUID_B),
        record: x_record.clone(),
    };
    write_item(root, &format!("x.{UUID_B}.toml"), &x);
    let y = item_id(UUID_C);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x.id.clone());
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.clone());
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(y),
        "paper.pdf",
        vec![entry_a.clone(), entry_b.clone()],
    );
    write_artifact_record(root, &held);

    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &x_record,
            hash: &hash_a,
            held: Some(&held),
            reidentified: true,
            key: None,
            run: RunId::new("run-2"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let mut expected_new = entry_a.clone();
    expected_new.run = RunId::new("run-2");
    expected_new.item = Some(x.id.clone());

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(
        after.history,
        vec![entry_a, entry_b, expected_new],
        "no restatement of b: its own entry already names y, the link as read"
    );
}

#[test]
fn admit_of_accepted_changed_bytes_whose_record_write_fails_leaves_the_record_unchanged() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes b");
    let hash_b = hash("bytes b");
    let item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("item.{UUID_A}.toml"), &item);
    let held = artifact_record(
        artifact_id(UUID_B),
        Some(item.id.clone()),
        "paper.pdf",
        vec![hash_entry("bytes a", "run-0")],
    );
    write_artifact_record(root, &held);
    let record_path = root
        .join(STATE_DIR)
        .join(ARTIFACT_STORE)
        .join(format!("{}.toml", held.id));
    let before = fs::read(&record_path).unwrap();

    let records_dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    let write = |target: &Path, bytes: &[u8]| -> std::io::Result<()> {
        if target.starts_with(&records_dir) {
            Err(std::io::Error::other("simulated artifact write failure"))
        } else {
            store_write(target, bytes)
        }
    };

    let result = admit(
        root,
        &Admitting {
            path: &path,
            record: &item.record,
            hash: &hash_b,
            held: Some(&held),
            reidentified: false,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &write,
    );

    assert!(matches!(result, Err(Unrecorded { .. })), "got {result:?}");
    let after = fs::read(&record_path).unwrap();
    assert_eq!(
        after, before,
        "a failed write must leave the record byte-identical"
    );
}

#[test]
fn admit_of_accepted_changed_bytes_appends_an_entry_naming_the_linked_item() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes b");
    let hash_b = hash("bytes b");
    let item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("item.{UUID_A}.toml"), &item);
    let held = artifact_record(
        artifact_id(UUID_B),
        Some(item.id.clone()),
        "paper.pdf",
        vec![entry_naming(hash_entry("bytes a", "run-0"), &item.id)],
    );
    write_artifact_record(root, &held);

    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &item.record,
            hash: &hash_b,
            held: Some(&held),
            reidentified: false,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_B))
        .unwrap()
        .clone();
    assert_eq!(after.history.len(), 2, "got {:?}", after.history);
    assert_eq!(after.history[1].item, Some(item.id));
}

#[test]
fn learn_takes_in_no_entry_for_bytes_already_held() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"steady bytes");
    let hash = hash("steady bytes");
    let item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("steady.{UUID_A}.toml"), &item);
    let held = fresh_record(
        root,
        artifact_id(UUID_B),
        Some(item.id.clone()),
        "paper.pdf",
        vec![hash_entry("steady bytes", "run-0")],
    );
    write_artifact_record(root, &held);

    let mut stores = Stores::read(root);
    let admitting = Admitting {
        path: &path,
        record: &item.record,
        hash: &hash,
        held: Some(&held),
        reidentified: false,
        key: None,
        run: RunId::new("run-1"),
        timestamp: TIMESTAMP,
        tool_version: TOOL_VERSION,
    };
    let admitted = Admitted {
        artifact: held.id.clone(),
        item: item.id.clone(),
        relinked_from: None,
    };
    stores.learn(&admitting, &admitted);

    let exists = |_: &Path| true;
    let account = stores.account(&exists);
    let learned = account.records.by_id(&held.id).unwrap();
    assert_eq!(
        learned.history, held.history,
        "an admission of bytes already held must append nothing"
    );
}

#[test]
fn foresee_takes_in_no_entry_for_bytes_already_held() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"steady bytes");
    let hash = hash("steady bytes");
    let item = Item {
        id: item_id(UUID_A),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("steady.{UUID_A}.toml"), &item);
    let held = fresh_record(
        root,
        artifact_id(UUID_B),
        Some(item.id.clone()),
        "paper.pdf",
        vec![hash_entry("steady bytes", "run-0")],
    );
    write_artifact_record(root, &held);

    let mut stores = Stores::read(root);
    let admitting = Admitting {
        path: &path,
        record: &item.record,
        hash: &hash,
        held: Some(&held),
        reidentified: false,
        key: None,
        run: RunId::new("run-1"),
        timestamp: TIMESTAMP,
        tool_version: TOOL_VERSION,
    };
    let admitted = stores
        .foresee(&admitting)
        .expect("a path inside the library must foresee something");
    assert_eq!(admitted.item, item.id);

    let exists = |_: &Path| true;
    let account = stores.account(&exists);
    let foreseen = account.records.by_id(&held.id).unwrap();
    assert_eq!(
        foreseen.history, held.history,
        "a foreseen admission of bytes already held must append nothing"
    );
}

#[test]
fn admit_giving_a_record_with_no_link_an_item_appends_an_entry_under_it() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let record = record_with_doi("10.1000/admit-no-link-gets-item");
    let held = artifact_record(
        artifact_id(UUID_A),
        None,
        "paper.pdf",
        vec![
            hash_entry("bytes a", "run-0"),
            hash_entry("bytes b", "run-0"),
        ],
    );
    write_artifact_record(root, &held);

    let admitted = admit(
        root,
        &Admitting {
            path: &path,
            record: &record,
            hash: &hash_a,
            held: Some(&held),
            reidentified: false,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(after.history.len(), 3, "got {:?}", after.history);
    assert_eq!(after.history[2].hash, hash_a);
    assert_eq!(after.history[2].item, Some(admitted.item));
}

// --- The D6 transitions, each driven through admit, reconcile (with an
// extractor) and Stores::consult over a real directory ---

#[test]
fn transition_a_reidentifying_restored_bytes_leaves_newer_bytes_with_their_item() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let hash_b = hash("bytes b");
    let x_record = record_with_doi("10.1000/transition-a-x");
    let x = Item {
        id: item_id(UUID_C),
        record: x_record.clone(),
    };
    write_item(root, &format!("x.{UUID_C}.toml"), &x);
    let y = item_id(UUID_D);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x.id.clone());
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.clone());
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(y.clone()),
        "paper.pdf",
        vec![entry_a.clone(), entry_b.clone()],
    );
    write_artifact_record(root, &held);

    // Restore a; re-identify it as x.
    fs::write(&path, b"bytes a").unwrap();
    let admitted = admit(
        root,
        &Admitting {
            path: &path,
            record: &x_record,
            hash: &hash_a,
            held: Some(&held),
            reidentified: true,
            key: None,
            run: RunId::new("run-2"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();
    assert_eq!(admitted.item, x.id);

    // Restore b: an ordinary rerun.
    fs::write(&path, b"bytes b").unwrap();
    let held_now = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &x_record,
            hash: &hash_b,
            held: Some(&held_now),
            reidentified: false,
            key: None,
            run: RunId::new("run-3"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(after.history.len(), 3, "got {:?}", after.history);
    assert_eq!(after.history[0], entry_a);
    assert_eq!(after.history[1], entry_b);
    assert_eq!(after.history[2].hash, hash_a);
    assert_eq!(after.history[2].item, Some(x.id.clone()));
    assert_eq!(after.item, Some(x.id.clone()));
    assert_eq!(after.item_for(&hash_a), Some(&x.id), "a: x");
    assert_eq!(after.item_for(&hash_b), Some(&y), "b: y");
}

#[test]
fn transition_b_a_hand_correction_survives_an_edit_and_a_restore() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let hash_b = hash("bytes b");
    let x = item_id(UUID_C);
    let y_record = record_with_doi("10.1000/transition-b-y");
    let y = Item {
        id: item_id(UUID_D),
        record: y_record.clone(),
    };
    write_item(root, &format!("y.{UUID_D}.toml"), &y);

    let entry_a = hash_entry("bytes a", "run-0");
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(x),
        "paper.pdf",
        vec![entry_a.clone()],
    );
    write_artifact_record(root, &held);

    // A person corrects the link by hand, in the text.
    let hand_corrected = ArtifactRecord {
        item: Some(y.id.clone()),
        ..held.clone()
    };
    write_artifact_record(root, &hand_corrected);

    // An edited b is accepted under y.
    fs::write(&path, b"bytes b").unwrap();
    let held_now = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &y_record,
            hash: &hash_b,
            held: Some(&held_now),
            reidentified: false,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    // Restore a.
    fs::write(&path, b"bytes a").unwrap();

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(after.history.len(), 3, "got {:?}", after.history);
    assert_eq!(after.history[0], entry_a);
    assert_eq!(
        after.history[1].hash, hash_a,
        "the restatement names a's bytes"
    );
    assert_eq!(after.history[1].item, Some(y.id.clone()));
    assert_eq!(after.history[2].hash, hash_b);
    assert_eq!(after.history[2].item, Some(y.id.clone()));
    assert_eq!(after.item_for(&hash_a), Some(&y.id), "a: y");
    assert_eq!(after.item_for(&hash_b), Some(&y.id), "b: y");
}

#[test]
fn transition_c_relink_then_restore_then_re_edit() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let hash_b = hash("bytes b");
    let x_record = record_with_doi("10.1000/transition-c-x");
    let x = Item {
        id: item_id(UUID_C),
        record: x_record.clone(),
    };
    write_item(root, &format!("x.{UUID_C}.toml"), &x);
    let y = Item {
        id: item_id(UUID_D),
        record: record_with_doi("10.1000/transition-c-y"),
    };
    write_item(root, &format!("y.{UUID_D}.toml"), &y);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x.id.clone());
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(x.id.clone()),
        "paper.pdf",
        vec![entry_a.clone()],
    );
    write_artifact_record(root, &held);

    // An edited b is accepted under x (an ordinary rerun).
    fs::write(&path, b"bytes b").unwrap();
    let held_now = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &x_record,
            hash: &hash_b,
            held: Some(&held_now),
            reidentified: false,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    // b is re-identified as y.
    let held_now = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &y.record,
            hash: &hash_b,
            held: Some(&held_now),
            reidentified: true,
            key: None,
            run: RunId::new("run-2"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let after_relink = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(
        after_relink.history.len(),
        2,
        "re-identifying the newest bytes appends nothing, got {:?}",
        after_relink.history
    );
    assert_eq!(after_relink.item, Some(y.id.clone()));
    assert_eq!(after_relink.item_for(&hash_a), Some(&x.id), "a: x");
    assert_eq!(after_relink.item_for(&hash_b), Some(&y.id), "b: y");

    // Restore a, then edit it to a', carrying x's doi: y does not
    // carry it, so the change is pending.
    fs::write(&path, b"a-prime bytes").unwrap();
    let identifier = doi_identifier("10.1000/transition-c-x");
    let extraction = Extraction::Found {
        identifier: identifier.to_string(),
        tier: "text-layer".to_string(),
    };
    let mut answers = BTreeMap::new();
    answers.insert(path.clone(), extraction.clone());
    let extractor = Extractor::with(answers);
    let result = reconcile(
        root,
        &reconciling(false, "run-3", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(
        result.repairs,
        vec![(
            artifact_id(UUID_A),
            "paper.pdf".to_string(),
            Repair::Pending {
                hash: hash_bytes(b"a-prime bytes").to_string(),
                item: Some(y.id.to_string()),
                extraction,
            }
        )],
        "got {:?}",
        result.repairs
    );
}

#[test]
fn transition_d_a_correction_made_while_an_older_version_is_restored_is_credited_to_the_newest_bytes()
 {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes b");
    let hash_a = hash("bytes a");
    let hash_c = hash("bytes c");
    let x = item_id(UUID_B);
    let y = item_id(UUID_C);
    let z_record = record_with_doi("10.1000/transition-d-z");
    let z = Item {
        id: item_id(UUID_D),
        record: z_record.clone(),
    };
    write_item(root, &format!("z.{UUID_D}.toml"), &z);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x.clone());
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.clone());
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(y),
        "paper.pdf",
        vec![entry_a.clone(), entry_b.clone()],
    );
    write_artifact_record(root, &held);

    // A person sets the link to z by hand.
    let hand_corrected = ArtifactRecord {
        item: Some(z.id.clone()),
        ..held.clone()
    };
    write_artifact_record(root, &hand_corrected);

    // Restore a (no admission: the record is unaffected by the file
    // alone).
    fs::write(&path, b"bytes a").unwrap();

    // An edited c is accepted under z.
    fs::write(&path, b"bytes c").unwrap();
    let held_now = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &z_record,
            hash: &hash_c,
            held: Some(&held_now),
            reidentified: false,
            key: None,
            run: RunId::new("run-2"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(after.history.len(), 4, "got {:?}", after.history);
    assert_eq!(after.history[0], entry_a);
    assert_eq!(after.history[1], entry_b);
    assert_eq!(
        after.history[2].hash, entry_b.hash,
        "the restatement names b's bytes"
    );
    assert_eq!(after.history[2].item, Some(z.id.clone()));
    assert_eq!(after.history[3].hash, hash_c);
    assert_eq!(after.history[3].item, Some(z.id.clone()));
    assert_eq!(after.item_for(&hash_a), Some(&x), "a: x");
    assert_eq!(after.item_for(&entry_b.hash), Some(&z.id), "b: z");
    assert_eq!(after.item_for(&hash_c), Some(&z.id), "c: z");
}

#[test]
fn transition_e_a_pre_change_record_pins_its_newest_bytes_at_its_first_acceptance() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let hash_b = hash("bytes b");
    let x_record = record_with_doi("10.1000/transition-e-x");
    let x = Item {
        id: item_id(UUID_C),
        record: x_record.clone(),
    };
    write_item(root, &format!("x.{UUID_C}.toml"), &x);
    let y = Item {
        id: item_id(UUID_D),
        record: record_with_doi("10.1000/transition-e-y"),
    };
    write_item(root, &format!("y.{UUID_D}.toml"), &y);

    // A pre-change entry: written before entries named items.
    let entry_a = hash_entry("bytes a", "run-0");
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(x.id.clone()),
        "paper.pdf",
        vec![entry_a.clone()],
    );
    write_artifact_record(root, &held);

    // An edited b is accepted.
    fs::write(&path, b"bytes b").unwrap();
    let held_now = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &x_record,
            hash: &hash_b,
            held: Some(&held_now),
            reidentified: false,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    // b is re-identified as y.
    let held_now = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &y.record,
            hash: &hash_b,
            held: Some(&held_now),
            reidentified: true,
            key: None,
            run: RunId::new("run-2"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    // Restore a.
    fs::write(&path, b"bytes a").unwrap();

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(after.history.len(), 3, "got {:?}", after.history);
    assert_eq!(after.history[0], entry_a);
    assert_eq!(
        after.history[1].hash, hash_a,
        "the restatement names a's bytes"
    );
    assert_eq!(after.history[1].item, Some(x.id.clone()));
    assert_eq!(after.history[2].hash, hash_b);
    assert_eq!(after.history[2].item, Some(x.id.clone()));
    assert_eq!(after.item, Some(y.id.clone()));
    assert_eq!(
        after.item_for(&hash_a),
        Some(&x.id),
        "a answers x, not the interim y"
    );
    assert_eq!(after.item_for(&hash_b), Some(&y.id), "b: y");
}

#[test]
fn transition_f_relinking_the_newest_bytes_appends_nothing_until_later_bytes_restate_it() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let hash_b = hash("bytes b");
    let x = item_id(UUID_C);
    let y_record = record_with_doi("10.1000/transition-f-y");
    let y = Item {
        id: item_id(UUID_D),
        record: y_record.clone(),
    };
    write_item(root, &format!("y.{UUID_D}.toml"), &y);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x);
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(entry_a.item.clone().unwrap()),
        "paper.pdf",
        vec![entry_a.clone()],
    );
    write_artifact_record(root, &held);

    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &y_record,
            hash: &hash_a,
            held: Some(&held),
            reidentified: true,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let after_relink = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(
        after_relink.history,
        vec![entry_a.clone()],
        "re-identifying the newest bytes appends nothing"
    );
    assert_eq!(after_relink.item, Some(y.id.clone()));

    fs::write(&path, b"bytes b").unwrap();
    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &y_record,
            hash: &hash_b,
            held: Some(&after_relink),
            reidentified: false,
            key: None,
            run: RunId::new("run-2"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(after.history.len(), 3, "got {:?}", after.history);
    assert_eq!(after.history[0], entry_a);
    assert_eq!(after.history[1].hash, hash_a);
    assert_eq!(after.history[1].item, Some(y.id.clone()));
    assert_eq!(after.history[2].hash, hash_b);
    assert_eq!(after.history[2].item, Some(y.id.clone()));
    assert_eq!(after.item_for(&hash_a), Some(&y.id), "a: y throughout");
    assert_eq!(after.item_for(&hash_b), Some(&y.id), "b: y");
}

#[test]
fn transition_g_ten_ordinary_reruns_over_a_restored_file_append_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let x = item_id(UUID_B);
    let y = Item {
        id: item_id(UUID_C),
        record: minimal_record(EntryType::Article),
    };
    write_item(root, &format!("y.{UUID_C}.toml"), &y);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x);
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.id.clone());
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(y.id.clone()),
        "paper.pdf",
        vec![entry_a.clone(), entry_b.clone()],
    );
    write_artifact_record(root, &held);

    for run in 1..=10 {
        let held_now = ArtifactStore::read(root)
            .by_id(&artifact_id(UUID_A))
            .unwrap()
            .clone();
        let _ = admit(
            root,
            &Admitting {
                path: &path,
                record: &y.record,
                hash: &hash_a,
                held: Some(&held_now),
                reidentified: false,
                key: None,
                run: RunId::new(format!("run-{run}")),
                timestamp: TIMESTAMP,
                tool_version: TOOL_VERSION,
            },
            &store_write,
        )
        .unwrap();
    }

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(
        after.history,
        vec![entry_a, entry_b],
        "ten ordinary reruns over a restored file must append nothing"
    );
    assert_eq!(after.item, Some(y.id));
}

#[test]
fn transition_h_restored_bytes_reidentified_as_the_links_own_item_are_appended() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let hash_a = hash("bytes a");
    let x = item_id(UUID_B);
    let y_record = record_with_doi("10.1000/transition-h-y");
    let y = Item {
        id: item_id(UUID_C),
        record: y_record.clone(),
    };
    write_item(root, &format!("y.{UUID_C}.toml"), &y);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x);
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.id.clone());
    let held = artifact_record(
        artifact_id(UUID_A),
        Some(y.id.clone()),
        "paper.pdf",
        vec![entry_a.clone(), entry_b.clone()],
    );
    write_artifact_record(root, &held);

    let _ = admit(
        root,
        &Admitting {
            path: &path,
            record: &y_record,
            hash: &hash_a,
            held: Some(&held),
            reidentified: true,
            key: None,
            run: RunId::new("run-2"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &store_write,
    )
    .unwrap();

    let after = ArtifactStore::read(root)
        .by_id(&artifact_id(UUID_A))
        .unwrap()
        .clone();
    assert_eq!(after.history.len(), 3, "got {:?}", after.history);
    assert_eq!(after.history[0], entry_a);
    assert_eq!(after.history[1], entry_b);
    assert_eq!(after.history[2].hash, hash_a);
    assert_eq!(after.history[2].item, Some(y.id.clone()));
    assert_eq!(after.item, Some(y.id));
}

// ---------------------------------------------------------------------
// 7.4: durability
// ---------------------------------------------------------------------

/// task 7.4: an item write that fails leaves no artifact record behind
/// — the reverse order would leave a record naming an item that does
/// not exist.
#[test]
fn admit_writes_no_artifact_record_when_the_item_write_fails() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "durable.pdf", b"durable bytes");
    let hash = hash("durable bytes");
    let record = record_with_doi("10.1000/admit-durability-item");
    let item_store = root.join(ITEM_STORE);

    let write = |target: &Path, bytes: &[u8]| -> std::io::Result<()> {
        if target.starts_with(&item_store) {
            Err(std::io::Error::other("simulated item write failure"))
        } else {
            store_write(target, bytes)
        }
    };

    let result = admit(
        root,
        &Admitting {
            path: &path,
            record: &record,
            hash: &hash,
            held: None,
            reidentified: false,
            key: Some("durable"),
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &write,
    );

    assert!(result.is_err(), "a failed item write must fail admission");
    assert!(
        ItemStore::read(root).is_empty(),
        "no item may be left by a failed write"
    );
    assert!(
        ArtifactStore::read(root).is_empty(),
        "an item write that fails must never be followed by an artifact write"
    );
}

/// task 7.4: an artifact write that fails leaves the item — the
/// residue the write order is chosen for, and never a dangling link the
/// other order would leave.
#[test]
fn admit_leaves_the_item_when_the_artifact_write_fails() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "durable2.pdf", b"durable2 bytes");
    let hash = hash("durable2 bytes");
    let record = record_with_doi("10.1000/admit-durability-artifact");
    let artifact_store_dir = root.join(STATE_DIR).join(ARTIFACT_STORE);

    let write = |target: &Path, bytes: &[u8]| -> std::io::Result<()> {
        if target.starts_with(&artifact_store_dir) {
            Err(std::io::Error::other("simulated artifact write failure"))
        } else {
            store_write(target, bytes)
        }
    };

    let result = admit(
        root,
        &Admitting {
            path: &path,
            record: &record,
            hash: &hash,
            held: None,
            reidentified: false,
            key: Some("durable2"),
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &write,
    );

    assert!(
        result.is_err(),
        "a failed artifact write must fail admission"
    );
    assert_eq!(
        ItemStore::read(root).len(),
        1,
        "the item write must stand even though the artifact write after it failed"
    );
    assert!(
        ArtifactStore::read(root).is_empty(),
        "no artifact record may exist when its own write failed"
    );
}

/// task 7.4: a write interrupted part-way leaves the previous record
/// byte-identical, with no temporary a reader would take for a record
/// left behind under `.borax/artifacts/`.
#[test]
fn admit_leaves_the_previous_record_untouched_when_the_write_is_interrupted() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "edited.pdf", b"edited bytes v2");
    let new_hash = hash("edited bytes v2");
    let record = minimal_record(EntryType::Article);
    let item = Item {
        id: item_id(UUID_A),
        record: record.clone(),
    };
    write_item(root, &format!("edited.{UUID_A}.toml"), &item);
    let held = fresh_record(
        root,
        artifact_id(UUID_B),
        Some(item.id.clone()),
        "edited.pdf",
        vec![hash_entry("edited bytes v1", "run-0")],
    );
    write_artifact_record(root, &held);
    let store_dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    let record_file = store_dir.join(format!("{UUID_B}.toml"));
    let before = fs::read(&record_file).unwrap();

    let write = |_target: &Path, _bytes: &[u8]| -> std::io::Result<()> {
        Err(std::io::Error::other("simulated interruption"))
    };

    let result = admit(
        root,
        &Admitting {
            path: &path,
            record: &record,
            hash: &new_hash,
            held: Some(&held),
            reidentified: false,
            key: None,
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &write,
    );

    assert!(result.is_err());
    let after = fs::read(&record_file).unwrap();
    assert_eq!(
        before, after,
        "the previous record must be byte-identical after an interrupted write"
    );

    let strays: Vec<PathBuf> = fs::read_dir(&store_dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path != &record_file)
        .filter(|path| {
            let text = fs::read_to_string(path).unwrap_or_default();
            ArtifactRecord::from_toml(&text).is_err()
        })
        .collect();
    assert!(
        strays.is_empty(),
        "no temporary a reader would mistake for a record may remain: got {strays:?}"
    );
}

// ---------------------------------------------------------------------
// admission_event
// ---------------------------------------------------------------------

/// An admission that minted or updated a record and left its item link
/// where it was reports nothing beyond the file's own outcome event.
#[test]
fn admission_event_is_none_for_an_ordinary_admission() {
    let admitted = Admitted {
        artifact: artifact_id(UUID_A),
        item: item_id(UUID_B),
        relinked_from: None,
    };

    let event = admission_event(Path::new("/lib/paper.pdf"), &Ok(admitted));

    assert_eq!(
        event, None,
        "an ordinary admission has nothing to add to the file's own outcome event"
    );
}

/// A relinked admission is reported naming both items.
#[test]
fn admission_event_reports_a_relink() {
    let admitted = Admitted {
        artifact: artifact_id(UUID_A),
        item: item_id(UUID_C),
        relinked_from: Some(item_id(UUID_B)),
    };

    let event = admission_event(Path::new("/lib/paper.pdf"), &Ok(admitted));

    assert_eq!(
        event,
        Some(Event::LibraryAdmission {
            path: PathBuf::from("/lib/paper.pdf"),
            admission: Admission::Relinked {
                id: artifact_id(UUID_A).to_string(),
                from: item_id(UUID_B).to_string(),
                to: item_id(UUID_C).to_string(),
            },
        }),
        "got {event:?}"
    );
}

/// A store write that failed is reported naming the file, with the
/// filesystem's own message.
#[test]
fn admission_event_reports_an_unwritten_record() {
    let event = admission_event(
        Path::new("/lib/paper.pdf"),
        &Err(Unrecorded {
            message: "disk full".to_string(),
        }),
    );

    assert_eq!(
        event,
        Some(Event::LibraryAdmission {
            path: PathBuf::from("/lib/paper.pdf"),
            admission: Admission::Unwritten {
                message: "disk full".to_string(),
            },
        }),
        "got {event:?}"
    );
}

// ---------------------------------------------------------------------
// 7.6/7.6a/7.6b: Account — content and work duplicate detection
// ---------------------------------------------------------------------

/// An `exists` double for [`Account`]: answers `true` for exactly the
/// paths it is told are live, and records every path it is asked
/// about — so a check's staleness rule ("asked about a record that
/// matched and about nothing else") can be asserted directly.
struct RecordingExists {
    live: Vec<PathBuf>,
    asked: RefCell<Vec<PathBuf>>,
}

impl RecordingExists {
    fn new(live: Vec<PathBuf>) -> RecordingExists {
        RecordingExists {
            live,
            asked: RefCell::new(Vec::new()),
        }
    }

    fn check(&self, path: &Path) -> bool {
        self.asked.borrow_mut().push(path.to_path_buf());
        self.live.contains(&path.to_path_buf())
    }

    fn asked(&self) -> Vec<PathBuf> {
        self.asked.borrow().clone()
    }
}

/// An `identifier` built from a DOI, the one kind these fixtures need.
fn doi_identifier(value: &str) -> Identifier {
    Identifier::Doi(Doi::parse(value).unwrap())
}

/// An item carrying `doi_value` as its DOI, at identity `id`.
fn item_with_doi(id: ItemId, doi_value: &str) -> Item {
    Item {
        id,
        record: record_with_doi(doi_value),
    }
}

// ---------------------------------------------------------------------
// 2: Identifier comparison and the gate (design D2, D11)
// ---------------------------------------------------------------------

#[test]
fn carries_a_doi_the_item_record_holds() {
    let record = record_with_doi("10.1000/x");
    assert!(carries(&record, &doi_identifier("10.1000/x")));
}

#[test]
fn carries_compares_only_the_field_of_the_identifiers_own_kind() {
    let doi_record = record_with_doi("10.1000/x");
    let arxiv = Identifier::Arxiv(ArxivId::parse("2401.12345").unwrap());
    assert!(
        !carries(&doi_record, &arxiv),
        "a record whose DOI field holds a value does not carry an arXiv identifier"
    );

    let mut arxiv_record = minimal_record(EntryType::Article);
    arxiv_record.borax.arxiv = Some(ArxivId::parse("2401.12345").unwrap());
    assert!(
        !carries(&arxiv_record, &doi_identifier("10.1000/x")),
        "nor the reverse"
    );
}

#[test]
fn carries_an_arxiv_identifier_whatever_its_version() {
    let mut record = minimal_record(EntryType::Article);
    record.borax.arxiv = Some(ArxivId::parse("2401.12345v2").unwrap());

    assert!(carries(
        &record,
        &Identifier::Arxiv(ArxivId::parse("arXiv:2401.12345v1").unwrap())
    ));
    assert!(carries(
        &record,
        &Identifier::Arxiv(ArxivId::parse("2401.12345").unwrap())
    ));
}

#[test]
fn does_not_carry_an_isbn_10_for_an_item_holding_the_isbn_13() {
    let mut record = minimal_record(EntryType::Article);
    record.isbn = Some(Isbn::parse("9780306406157").unwrap());

    let isbn_10 = Identifier::Isbn(Isbn::parse("0306406152").unwrap());
    assert!(!carries(&record, &isbn_10));
}

#[test]
fn carries_a_pmid_and_an_isbn_of_the_same_length() {
    let mut record = minimal_record(EntryType::Article);
    record.pmid = Some(Pmid::parse("1234567890").unwrap());
    record.isbn = Some(Isbn::parse("0306406152").unwrap());

    assert!(carries(
        &record,
        &Identifier::Pmid(Pmid::parse("1234567890").unwrap())
    ));
    assert!(carries(
        &record,
        &Identifier::Isbn(Isbn::parse("0306406152").unwrap())
    ));
}

#[test]
fn extracted_identifier_reads_back_every_form_display_writes() {
    let doi = doi_identifier("10.1000/x");
    let arxiv = Identifier::Arxiv(ArxivId::parse("2401.12345v2").unwrap());
    let pmid = Identifier::Pmid(Pmid::parse("123456").unwrap());
    let isbn = Identifier::Isbn(Isbn::parse("9780306406157").unwrap());

    for identifier in [doi, arxiv, pmid, isbn] {
        let extraction = Extraction::Found {
            identifier: identifier.to_string(),
            tier: "text-layer".to_string(),
        };
        assert_eq!(
            extracted_identifier(&extraction),
            Some(identifier.clone()),
            "{identifier} did not round-trip"
        );
    }
}

#[test]
fn extracted_identifier_is_none_for_every_failure() {
    let failures = [
        Extraction::NoTextLayer,
        Extraction::TextWithoutIdentifier,
        Extraction::Encrypted,
        Extraction::Unreadable {
            message: "broken".to_string(),
        },
    ];
    for extraction in failures {
        assert_eq!(extracted_identifier(&extraction), None, "{extraction:?}");
    }
}

#[test]
fn accepted_by_is_extracted_when_the_linked_item_carries_the_extracted_identifier() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let link = item_id(UUID_B);
    write_item(root, "item.toml", &item_with_doi(link.clone(), "10.1000/x"));
    let items = ItemStore::read(root);
    let held = artifact_record(artifact_id(UUID_A), Some(link), "paper.pdf", vec![]);
    let extracted = doi_identifier("10.1000/x");

    assert_eq!(
        accepted_by(&items, &held, Some(&extracted), None),
        Some(AcceptedBy::Extracted {
            identifier: extracted.to_string()
        })
    );
}

#[test]
fn accepted_by_prefers_extracted_over_supplied() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let link = item_id(UUID_B);
    let mut record = record_with_doi("10.1000/x");
    record.borax.arxiv = Some(ArxivId::parse("2401.12345").unwrap());
    write_item(
        root,
        "item.toml",
        &Item {
            id: link.clone(),
            record,
        },
    );
    let items = ItemStore::read(root);
    let held = artifact_record(artifact_id(UUID_A), Some(link), "paper.pdf", vec![]);

    let extracted = doi_identifier("10.1000/x");
    let supplied = Identifier::Arxiv(ArxivId::parse("2401.12345").unwrap());

    assert_eq!(
        accepted_by(&items, &held, Some(&extracted), Some(&supplied)),
        Some(AcceptedBy::Extracted {
            identifier: extracted.to_string()
        }),
        "extracted is tried first"
    );
}

#[test]
fn accepted_by_is_supplied_when_only_the_supplied_identifier_is_carried() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let link = item_id(UUID_B);
    write_item(root, "item.toml", &item_with_doi(link.clone(), "10.1000/x"));
    let items = ItemStore::read(root);
    let held = artifact_record(artifact_id(UUID_A), Some(link), "paper.pdf", vec![]);

    let extracted = Identifier::Arxiv(ArxivId::parse("2401.12345").unwrap());
    let supplied = doi_identifier("10.1000/x");

    assert_eq!(
        accepted_by(&items, &held, Some(&extracted), Some(&supplied)),
        Some(AcceptedBy::Supplied {
            identifier: supplied.to_string()
        })
    );
}

#[test]
fn accepted_by_is_none_when_neither_is_carried() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let link = item_id(UUID_B);
    write_item(root, "item.toml", &item_with_doi(link.clone(), "10.1000/x"));
    let items = ItemStore::read(root);
    let held = artifact_record(artifact_id(UUID_A), Some(link), "paper.pdf", vec![]);

    let extracted = doi_identifier("10.1000/other");
    let supplied = Identifier::Arxiv(ArxivId::parse("2401.12345").unwrap());

    assert_eq!(
        accepted_by(&items, &held, Some(&extracted), Some(&supplied)),
        None
    );
}

#[test]
fn accepted_by_is_none_for_a_record_linking_no_item() {
    let items = ItemStore::read(tempdir().unwrap().path());
    let held = artifact_record(artifact_id(UUID_A), None, "paper.pdf", vec![]);
    let extracted = doi_identifier("10.1000/x");

    assert_eq!(accepted_by(&items, &held, Some(&extracted), None), None);
}

#[test]
fn accepted_by_is_none_when_no_item_file_carries_the_linked_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let link = item_id(UUID_B);
    // No item file names `link` at all: a dangling link.
    write_item(
        root,
        "other.toml",
        &item_with_doi(item_id(UUID_C), "10.1000/x"),
    );
    let items = ItemStore::read(root);
    let held = artifact_record(artifact_id(UUID_A), Some(link), "paper.pdf", vec![]);
    let extracted = doi_identifier("10.1000/x");

    assert_eq!(accepted_by(&items, &held, Some(&extracted), None), None);
}

#[test]
fn accepted_by_is_none_when_two_item_files_carry_the_linked_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let link = item_id(UUID_B);
    write_item(root, "one.toml", &item_with_doi(link.clone(), "10.1000/x"));
    write_item(root, "two.toml", &item_with_doi(link.clone(), "10.1000/x"));
    let items = ItemStore::read(root);
    let held = artifact_record(artifact_id(UUID_A), Some(link), "paper.pdf", vec![]);
    let extracted = doi_identifier("10.1000/x");

    assert_eq!(accepted_by(&items, &held, Some(&extracted), None), None);
}

// ---- design D2: Account::is_incoming ----

/// An [`Account`] rooted at `root` over empty stores. The stores and
/// `exists` play no part in `is_incoming`, so they are fixtures rather
/// than anything a test varies.
fn account_at<'a>(
    root: &'a Path,
    items: &'a ItemStore,
    records: &'a ArtifactStore,
    exists: &'a dyn Fn(&Path) -> bool,
) -> Account<'a> {
    Account {
        root,
        items,
        records,
        exists,
    }
}

/// A relative incoming path is made absolute against the working
/// directory before it is compared, so it recognises the record that
/// names the same file relative to the library root.
#[test]
fn is_incoming_recognises_a_relative_input_path_as_the_file_itself() {
    let cwd = std::env::current_dir().unwrap();
    let root = cwd.join("library");
    let (items, records) = (ItemStore::default(), ArtifactStore::default());
    let exists = |_: &Path| true;
    let account = account_at(&root, &items, &records, &exists);

    assert!(account.is_incoming(
        "archived/Smith2024.pdf",
        Path::new("library/archived/Smith2024.pdf"),
    ));
}

/// `..` in the incoming path is resolved lexically before comparison,
/// not left to defeat it.
#[test]
fn is_incoming_recognises_an_input_path_containing_dot_dot_as_the_file_itself() {
    let cwd = std::env::current_dir().unwrap();
    let root = cwd.join("library");
    let (items, records) = (ItemStore::default(), ArtifactStore::default());
    let exists = |_: &Path| true;
    let account = account_at(&root, &items, &records, &exists);

    assert!(account.is_incoming(
        "archived/Smith2024.pdf",
        Path::new("library/other/../archived/Smith2024.pdf"),
    ));
}

#[test]
fn is_incoming_is_false_for_a_different_file() {
    let root = Path::new("/library");
    let (items, records) = (ItemStore::default(), ArtifactStore::default());
    let exists = |_: &Path| true;
    let account = account_at(root, &items, &records, &exists);

    assert!(!account.is_incoming(
        "archived/Smith2024.pdf",
        Path::new("/library/archived/Other2024.pdf"),
    ));
}

/// Symlinks are never resolved: a link and the file it points at are
/// two names, and `is_incoming` compares names, not inodes. Built with
/// a real symlink on disk so the assertion cannot be satisfied by an
/// implementation that happens not to touch the filesystem for the
/// wrong reason.
#[test]
#[cfg(unix)]
fn is_incoming_does_not_resolve_a_symlink_to_the_recorded_file() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("library");
    fs::create_dir_all(root.join("archived")).unwrap();
    fs::write(root.join("archived/Smith2024.pdf"), b"bytes").unwrap();
    std::os::unix::fs::symlink(
        root.join("archived/Smith2024.pdf"),
        root.join("link-to-smith.pdf"),
    )
    .unwrap();
    let (items, records) = (ItemStore::default(), ArtifactStore::default());
    let exists = |_: &Path| true;
    let account = account_at(&root, &items, &records, &exists);

    assert!(!account.is_incoming("archived/Smith2024.pdf", &root.join("link-to-smith.pdf")));
}

// ---- 7.6: content_duplicate ----

/// task 7.6: a match is any record whose history holds the incoming
/// hash, and the incoming file is a content duplicate of the file its
/// recorded path still holds.
#[test]
fn content_duplicate_finds_the_record_holding_the_hash() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let existing = relative_to(root, "existing.pdf");
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_A),
            None,
            "existing.pdf",
            vec![hash_entry("shared bytes", "run-0")],
        ),
    );
    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(vec![existing.clone()]);
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.content_duplicate(&root.join("incoming.pdf"), &hash("shared bytes"));

    assert_eq!(
        found,
        Some(existing),
        "the file holding the same bytes must be found"
    );
}

/// task 7.6: matching a hash anywhere in a record's history, and not
/// only its current hash, is what the content check means.
#[test]
fn content_duplicate_matches_a_historical_hash_not_only_the_current_one() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let existing = relative_to(root, "existing.pdf");
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_A),
            None,
            "existing.pdf",
            vec![
                hash_entry("old bytes", "run-0"),
                hash_entry("new bytes", "run-1"),
            ],
        ),
    );
    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(vec![existing.clone()]);
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.content_duplicate(&root.join("incoming.pdf"), &hash("old bytes"));

    assert_eq!(
        found,
        Some(existing),
        "a hash from earlier in the history must still be found"
    );
}

/// task 7.6: the incoming file's own record is passed over and the
/// search goes on, so a second copy recorded elsewhere in the library
/// is not hidden behind it.
#[test]
fn content_duplicate_passes_over_the_incoming_files_own_record_and_finds_a_second_copy() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let incoming = root.join("incoming.pdf");
    let elsewhere = relative_to(root, "elsewhere.pdf");
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_A),
            None,
            "incoming.pdf",
            vec![hash_entry("shared bytes", "run-0")],
        ),
    );
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_B),
            None,
            "elsewhere.pdf",
            vec![hash_entry("shared bytes", "run-0")],
        ),
    );
    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(vec![incoming.clone(), elsewhere.clone()]);
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.content_duplicate(&incoming, &hash("shared bytes"));

    assert_eq!(
        found,
        Some(elsewhere),
        "the search must not stop at the incoming file's own record: got {found:?}"
    );
}

/// task 7.6: an artifact record whose recorded path is the incoming
/// file's own path never makes it a duplicate at either level, and
/// with no other copy recorded, the incoming file is a content
/// duplicate of nothing.
#[test]
fn content_duplicate_is_none_when_the_only_match_is_the_incoming_files_own_record() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let incoming = root.join("incoming.pdf");
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_A),
            None,
            "incoming.pdf",
            vec![hash_entry("shared bytes", "run-0")],
        ),
    );
    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(vec![incoming.clone()]);
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.content_duplicate(&incoming, &hash("shared bytes"));

    assert_eq!(
        found, None,
        "a file's own record must never make it a duplicate of itself"
    );
}

/// task 7.6: a duplicate report first verifies that the recorded path
/// still holds a file; a stale one is passed over, never vetoing the
/// admission, and `exists` is asked about exactly that path.
#[test]
fn content_duplicate_is_none_when_the_recorded_path_holds_no_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let stale = relative_to(root, "gone.pdf");
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_A),
            None,
            "gone.pdf",
            vec![hash_entry("shared bytes", "run-0")],
        ),
    );
    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(Vec::new());
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.content_duplicate(&root.join("incoming.pdf"), &hash("shared bytes"));

    assert_eq!(
        found, None,
        "a stale last-known path must never veto an admission"
    );
    assert!(
        checker.asked().contains(&stale),
        "the stale path must be asked about, which is what surfaces it to reconcile: got {:?}",
        checker.asked()
    );
}

/// task 7.6: no record holding the incoming hash at all is no content
/// duplicate.
#[test]
fn content_duplicate_is_none_when_no_record_holds_the_hash() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let existing = relative_to(root, "existing.pdf");
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_A),
            None,
            "existing.pdf",
            vec![hash_entry("other bytes", "run-0")],
        ),
    );
    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(vec![existing]);
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.content_duplicate(&root.join("incoming.pdf"), &hash("shared bytes"));

    assert_eq!(found, None, "no matching hash means no content duplicate");
}

// ---- 7.6a/7.6b: work_duplicate ----

/// task 7.6a: an identifier carried by an item that has an artifact
/// whose recorded path still holds a file makes the incoming file a
/// work duplicate, naming that item's file and that artifact's path.
#[test]
fn work_duplicate_finds_the_item_holding_the_identifier_with_a_live_artifact() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let existing_item = item_with_doi(item_id(UUID_A), "10.1000/work-dup");
    write_item(root, &format!("existing.{UUID_A}.toml"), &existing_item);
    let existing_path = relative_to(root, "existing.pdf");
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_B),
            Some(existing_item.id.clone()),
            "existing.pdf",
            vec![hash_entry("existing bytes", "run-0")],
        ),
    );
    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(vec![existing_path.clone()]);
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.work_duplicate(
        &root.join("incoming.pdf"),
        None,
        &[doi_identifier("10.1000/work-dup")],
    );

    assert_eq!(
        found,
        Some(WorkDuplicate {
            item: existing_item.id.clone(),
            item_file: items.file_of(&existing_item.id).map(Path::to_path_buf),
            existing: existing_path,
        }),
        "got {found:?}"
    );
}

/// task 7.6a: identifiers are tried in the order given, and the first
/// that answers decides.
#[test]
fn work_duplicate_tries_identifiers_in_order_and_the_first_that_answers_decides() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let existing_item = item_with_doi(item_id(UUID_A), "10.1000/work-dup-second");
    write_item(root, &format!("existing.{UUID_A}.toml"), &existing_item);
    let existing_path = relative_to(root, "existing.pdf");
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_B),
            Some(existing_item.id.clone()),
            "existing.pdf",
            vec![hash_entry("existing bytes", "run-0")],
        ),
    );
    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(vec![existing_path.clone()]);
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.work_duplicate(
        &root.join("incoming.pdf"),
        None,
        &[
            doi_identifier("10.1000/no-item-holds-this"),
            doi_identifier("10.1000/work-dup-second"),
        ],
    );

    assert_eq!(
        found.map(|duplicate| duplicate.item),
        Some(existing_item.id),
        "the second identifier, the one an item actually carries, must decide"
    );
}

/// task 7.6a/normative "An item with no artifact recorded against it
/// ... SHALL NOT make an incoming file a work duplicate": the incoming
/// file is that item's first artifact.
#[test]
fn work_duplicate_is_none_when_the_item_has_no_artifact() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let existing_item = item_with_doi(item_id(UUID_A), "10.1000/no-artifact-yet");
    write_item(root, &format!("existing.{UUID_A}.toml"), &existing_item);
    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(Vec::new());
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.work_duplicate(
        &root.join("incoming.pdf"),
        None,
        &[doi_identifier("10.1000/no-artifact-yet")],
    );

    assert_eq!(
        found, None,
        "an item with no artifact at all must not make the incoming file a work duplicate"
    );
}

/// task 7.6a/normative: an item whose every recorded artifact names a
/// path holding no file is the same as one with none — the incoming
/// file is admitted as its first artifact, not refused as a duplicate
/// of a file that is no longer there.
#[test]
fn work_duplicate_is_none_when_every_recorded_artifact_of_the_item_is_stale() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let existing_item = item_with_doi(item_id(UUID_A), "10.1000/all-stale");
    write_item(root, &format!("existing.{UUID_A}.toml"), &existing_item);
    let stale_path = relative_to(root, "gone.pdf");
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_B),
            Some(existing_item.id.clone()),
            "gone.pdf",
            vec![hash_entry("gone bytes", "run-0")],
        ),
    );
    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(Vec::new());
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.work_duplicate(
        &root.join("incoming.pdf"),
        None,
        &[doi_identifier("10.1000/all-stale")],
    );

    assert_eq!(
        found, None,
        "an item whose only recorded artifact is gone must not block re-admission"
    );
    assert!(
        checker.asked().contains(&stale_path),
        "the stale path must still be asked about: got {:?}",
        checker.asked()
    );
}

/// task 7.6b: the item the incoming file's own record already links to
/// is passed over, and so is that record itself — the rule that makes
/// a multi-artifact item usable. Without it, every artifact of one
/// item would meet its siblings as a duplicate of the item it belongs
/// to.
#[test]
fn work_duplicate_passes_over_the_item_the_incoming_files_own_record_already_links_to() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let incoming = root.join("incoming.pdf");
    let own_item = item_with_doi(item_id(UUID_A), "10.1000/own-work");
    write_item(root, &format!("own.{UUID_A}.toml"), &own_item);
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_B),
            Some(own_item.id.clone()),
            "incoming.pdf",
            vec![hash_entry("incoming bytes", "run-0")],
        ),
    );
    let sibling_path = relative_to(root, "sibling.pdf");
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_C),
            Some(own_item.id.clone()),
            "sibling.pdf",
            vec![hash_entry("sibling bytes", "run-0")],
        ),
    );
    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(vec![incoming.clone(), sibling_path]);
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.work_duplicate(&incoming, None, &[doi_identifier("10.1000/own-work")]);

    assert_eq!(
        found, None,
        "an item the incoming file's own record already links to must never be reported \
         as a work duplicate: got {found:?}"
    );
}

/// design D6: a restored version answers with x, while the own record
/// still links y. The item the own record *answers with* for the
/// incoming bytes is excluded too, not only its link.
#[test]
fn work_duplicate_passes_over_the_item_the_own_record_answers_with_for_the_incoming_bytes() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let incoming = root.join("incoming.pdf");
    let incoming_hash = hash("incoming bytes");
    let x_item = item_with_doi(item_id(UUID_A), "10.1000/answered-with-x");
    write_item(root, &format!("x.{UUID_A}.toml"), &x_item);
    let y_item = item_with_doi(item_id(UUID_C), "10.1000/link-y");
    write_item(root, &format!("y.{UUID_C}.toml"), &y_item);

    let mut entry_incoming = hash_entry("incoming bytes", "run-0");
    entry_incoming.item = Some(x_item.id.clone());
    let mut entry_newer = hash_entry("newer bytes", "run-1");
    entry_newer.item = Some(y_item.id.clone());
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_B),
            Some(y_item.id.clone()),
            "incoming.pdf",
            vec![entry_incoming, entry_newer],
        ),
    );

    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(vec![incoming.clone()]);
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.work_duplicate(
        &incoming,
        Some(&incoming_hash),
        &[doi_identifier("10.1000/answered-with-x")],
    );

    assert_eq!(
        found, None,
        "the item the own record answers with for the incoming bytes must be passed over, \
         even though it is not the record's link: got {found:?}"
    );
}

/// design D6: with no hash given, only the record's link is excluded,
/// as today — so x, the item the restored bytes answer with, is still
/// reported when it has another recorded artifact.
#[test]
fn work_duplicate_with_no_hash_passes_over_only_the_link() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let incoming = root.join("incoming.pdf");
    let x_item = item_with_doi(item_id(UUID_A), "10.1000/no-hash-x");
    write_item(root, &format!("x.{UUID_A}.toml"), &x_item);
    let y_item = item_with_doi(item_id(UUID_C), "10.1000/no-hash-y");
    write_item(root, &format!("y.{UUID_C}.toml"), &y_item);

    let mut entry_incoming = hash_entry("incoming bytes", "run-0");
    entry_incoming.item = Some(x_item.id.clone());
    let mut entry_newer = hash_entry("newer bytes", "run-1");
    entry_newer.item = Some(y_item.id.clone());
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_B),
            Some(y_item.id.clone()),
            "incoming.pdf",
            vec![entry_incoming, entry_newer],
        ),
    );
    let other_path = relative_to(root, "other.pdf");
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_D),
            Some(x_item.id.clone()),
            "other.pdf",
            vec![hash_entry("other bytes", "run-0")],
        ),
    );

    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(vec![incoming.clone(), other_path.clone()]);
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.work_duplicate(&incoming, None, &[doi_identifier("10.1000/no-hash-x")]);

    assert_eq!(
        found,
        Some(WorkDuplicate {
            item: x_item.id.clone(),
            item_file: items.file_of(&x_item.id).map(Path::to_path_buf),
            existing: other_path,
        }),
        "with no hash, only the link (y) is excluded, so x's other artifact is found"
    );
}

/// design D6: the other side stays by link. An incoming file of y is
/// still reported a duplicate of a recorded path linking y, even
/// though that path currently holds a restored version answering with
/// x.
#[test]
fn work_duplicate_still_finds_a_recorded_path_of_the_link_item_holding_a_restored_version() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let incoming = root.join("incoming.pdf");
    let y_item = item_with_doi(item_id(UUID_A), "10.1000/restored-version-y");
    write_item(root, &format!("y.{UUID_A}.toml"), &y_item);
    let x_item = item_id(UUID_C);

    let other_path = relative_to(root, "other.pdf");
    let mut other_entry_x = hash_entry("other bytes x", "run-0");
    other_entry_x.item = Some(x_item);
    let mut other_entry_y = hash_entry("other bytes y", "run-1");
    other_entry_y.item = Some(y_item.id.clone());
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_D),
            Some(y_item.id.clone()),
            "other.pdf",
            vec![other_entry_x, other_entry_y],
        ),
    );

    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(vec![incoming.clone(), other_path.clone()]);
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.work_duplicate(
        &incoming,
        None,
        &[doi_identifier("10.1000/restored-version-y")],
    );

    assert_eq!(
        found,
        Some(WorkDuplicate {
            item: y_item.id.clone(),
            item_file: items.file_of(&y_item.id).map(Path::to_path_buf),
            existing: other_path,
        }),
        "the other side stays by link: a recorded path of y is reported even though it \
         currently holds a restored version answering with x"
    );
}

/// task 7.6b: passing over the incoming file's own item must not hide
/// a genuine duplicate of a different work among the same identifiers.
#[test]
fn work_duplicate_still_finds_a_duplicate_of_a_different_item() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let incoming = root.join("incoming.pdf");
    let own_item = item_with_doi(item_id(UUID_A), "10.1000/own-work-2");
    write_item(root, &format!("own.{UUID_A}.toml"), &own_item);
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_B),
            Some(own_item.id.clone()),
            "incoming.pdf",
            vec![hash_entry("incoming bytes", "run-0")],
        ),
    );
    let other_item = item_with_doi(item_id(UUID_C), "10.1000/other-work-2");
    write_item(root, &format!("other.{UUID_C}.toml"), &other_item);
    let other_path = relative_to(root, "other.pdf");
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_D),
            Some(other_item.id.clone()),
            "other.pdf",
            vec![hash_entry("other bytes", "run-0")],
        ),
    );
    let items = ItemStore::read(root);
    let records = ArtifactStore::read(root);
    let checker = RecordingExists::new(vec![incoming.clone(), other_path.clone()]);
    let exists = |path: &Path| checker.check(path);
    let account = Account {
        root,
        items: &items,
        records: &records,
        exists: &exists,
    };

    let found = account.work_duplicate(
        &incoming,
        None,
        &[
            doi_identifier("10.1000/own-work-2"),
            doi_identifier("10.1000/other-work-2"),
        ],
    );

    assert_eq!(
        found,
        Some(WorkDuplicate {
            item: other_item.id.clone(),
            item_file: items.file_of(&other_item.id).map(Path::to_path_buf),
            existing: other_path,
        }),
        "the own item's identifier must be passed over without hiding a real duplicate: \
         got {found:?}"
    );
}

// ---------------------------------------------------------------------
// consult-library-first, task 1.1: Stores::consult / Stores::record_faults
//
// design D1's table, in order, plus D1a (normalised paths), D2/D2a (the
// eight problem kinds and the store-fault rules) and the interim
// rollback policy. Fixtures follow this file's own convention: a
// `tempdir` holding `items/` and `.borax/artifacts/`, written as
// `Item::to_toml` and `ArtifactRecord::to_toml` write them.
// ---------------------------------------------------------------------

/// An [`Item`] carrying `id` and a minimal article record, for fixtures
/// that only need an identity to link an artifact record to.
fn item_at(id: ItemId) -> Item {
    Item {
        id,
        record: minimal_record(EntryType::Article),
    }
}

/// Reads the two stores at `root` and consults them about `path`, over
/// `hash` — the one call [`Stores::consult`] exists for.
fn consult(root: &Path, path: &Path, hash: Option<&ContentHash>) -> Option<Consulted> {
    Stores::read(root).consult(path, hash)
}

// --- Tracked (design D1's table, and the interim rollback policy) ---

/// A record whose current hash is the file's own: the ordinary case,
/// answering with the linked item.
#[test]
fn consult_reports_tracked_when_the_hash_is_the_records_newest_entry() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let item = item_at(item_id(UUID_B));
    write_item(root, &item_file_name(None, &item.id), &item);
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item.id.clone()),
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("paper bytes"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::Tracked {
            artifact: UUID_A.to_string(),
            item: UUID_B.to_string(),
            accepted_under: None,
        }
    );
    assert_eq!(found.item, Some(item), "the linked item must come back too");
}

/// The pre-change-entry rule (design D7): both entries here were
/// written before an entry named an item, so a rollback to the earlier
/// one still answers `Tracked` with no `accepted_under`, the record's
/// current link — the only thing an entry naming no item can answer
/// with.
#[test]
fn consult_reports_tracked_for_a_rollback_to_an_older_history_entry_interim_policy() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"work a bytes");
    let item_b = item_at(item_id(UUID_B));
    write_item(root, &item_file_name(None, &item_b.id), &item_b);
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_b.id.clone()),
        "paper.pdf",
        vec![
            hash_entry("work a bytes", "run-0"),
            hash_entry("work b bytes", "run-1"),
        ],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("work a bytes"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::Tracked {
            artifact: UUID_A.to_string(),
            item: UUID_B.to_string(),
            accepted_under: None,
        },
        "work A's bytes answer as work B: neither entry names an item, so the record's \
         current link is the only answer a pre-change entry can give (design D7)"
    );
}

// --- Restored bytes answer from the item they were accepted under
// (design D6, D9) ---

#[test]
fn consult_answers_restored_bytes_with_the_item_their_entry_names() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let x = item_at(item_id(UUID_C));
    let y = item_at(item_id(UUID_D));
    write_item(root, &item_file_name(None, &x.id), &x);
    write_item(root, &item_file_name(None, &y.id), &y);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x.id.clone());
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.id.clone());
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(y.id.clone()),
        "paper.pdf",
        vec![entry_a, entry_b],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("bytes a"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::Tracked {
            artifact: UUID_A.to_string(),
            item: UUID_D.to_string(),
            accepted_under: Some(UUID_C.to_string()),
        }
    );
    assert_eq!(found.item, Some(x), "the item answered with is x's record");
}

#[test]
fn consult_answers_the_newest_bytes_with_no_accepted_under() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes b");
    let y = item_at(item_id(UUID_D));
    write_item(root, &item_file_name(None, &y.id), &y);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(item_id(UUID_C));
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.id.clone());
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(y.id.clone()),
        "paper.pdf",
        vec![entry_a, entry_b],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("bytes b"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::Tracked {
            artifact: UUID_A.to_string(),
            item: UUID_D.to_string(),
            accepted_under: None,
        }
    );
}

#[test]
fn consult_reports_dangling_item_about_the_link_even_for_restored_bytes() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let x = item_at(item_id(UUID_C));
    write_item(root, &item_file_name(None, &x.id), &x);
    // The link, y, is dangling: no file names it.
    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x.id.clone());
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(item_id(UUID_D));
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_D)),
        "paper.pdf",
        vec![entry_a, entry_b],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("bytes a"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::DanglingItem {
            artifact: UUID_A.to_string(),
            item: UUID_D.to_string(),
        },
        "the link, not the entry's own readable item, is what is wrong"
    );
}

#[test]
fn consult_reports_no_item_for_restored_bytes_when_the_record_links_none() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(item_id(UUID_C));
    let entry_b = hash_entry("bytes b", "run-1");
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "paper.pdf",
        vec![entry_a, entry_b],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("bytes a"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::NoItem {
            artifact: UUID_A.to_string(),
        }
    );
}

#[test]
fn consult_answers_the_newest_bytes_with_the_link_corrected_by_hand() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let y = item_at(item_id(UUID_D));
    write_item(root, &item_file_name(None, &y.id), &y);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(item_id(UUID_C));
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(y.id.clone()),
        "paper.pdf",
        vec![entry_a],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("bytes a"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::Tracked {
            artifact: UUID_A.to_string(),
            item: UUID_D.to_string(),
            accepted_under: None,
        },
        "a's bytes are the newest entry, so the record's current link answers, even \
         though that entry names a different item"
    );
}

#[test]
fn consult_reports_accepted_item_unavailable_for_restored_bytes_whose_item_is_gone() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let y = item_at(item_id(UUID_D));
    write_item(root, &item_file_name(None, &y.id), &y);
    // x, the entry's item, is deleted: no file names it.

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(item_id(UUID_C));
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.id.clone());
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(y.id.clone()),
        "paper.pdf",
        vec![entry_a, entry_b],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("bytes a"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::AcceptedItemUnavailable {
            artifact: UUID_A.to_string(),
            item: UUID_D.to_string(),
            accepted_under: UUID_C.to_string(),
            cause: AcceptedItemCause::Dangling,
        }
    );
}

#[test]
fn consult_reports_accepted_item_unavailable_when_two_files_carry_the_entrys_item() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let y = item_at(item_id(UUID_D));
    write_item(root, &item_file_name(None, &y.id), &y);
    let x = item_at(item_id(UUID_C));
    write_item(root, "one.toml", &x);
    write_item(root, "two.toml", &x);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x.id.clone());
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.id.clone());
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(y.id.clone()),
        "paper.pdf",
        vec![entry_a, entry_b],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("bytes a"))).unwrap();

    match found.answer {
        LibraryAnswer::AcceptedItemUnavailable {
            artifact,
            item,
            accepted_under,
            cause: AcceptedItemCause::Ambiguous { files },
        } => {
            assert_eq!(artifact, UUID_A.to_string());
            assert_eq!(item, UUID_D.to_string());
            assert_eq!(accepted_under, UUID_C.to_string());
            assert_eq!(files.len(), 2, "got {files:?}");
        }
        other => panic!("expected AcceptedItemUnavailable/ambiguous, got {other:?}"),
    }
}

#[test]
fn consult_reports_accepted_item_unavailable_when_the_entrys_item_file_cannot_be_read() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let y = item_at(item_id(UUID_D));
    write_item(root, &item_file_name(None, &y.id), &y);
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    let item_path = items.join(format!("badkey.{UUID_C}.toml"));
    fs::write(&item_path, "not valid toml {{{").unwrap();

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(item_id(UUID_C));
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.id.clone());
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(y.id.clone()),
        "paper.pdf",
        vec![entry_a, entry_b],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("bytes a"))).unwrap();

    match found.answer {
        LibraryAnswer::AcceptedItemUnavailable {
            artifact,
            item,
            accepted_under,
            cause:
                AcceptedItemCause::Unreadable {
                    path: reported_path,
                    ..
                },
        } => {
            assert_eq!(artifact, UUID_A.to_string());
            assert_eq!(item, UUID_D.to_string());
            assert_eq!(accepted_under, UUID_C.to_string());
            assert_eq!(reported_path, item_path);
        }
        other => panic!("expected AcceptedItemUnavailable/unreadable, got {other:?}"),
    }
}

#[test]
fn consult_reports_unreadable_item_about_the_link_over_a_valid_entry_item() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let x = item_at(item_id(UUID_C));
    write_item(root, &item_file_name(None, &x.id), &x);
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    fs::write(
        items.join(format!("badkey.{UUID_D}.toml")),
        "not valid toml {{{",
    )
    .unwrap();

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x.id.clone());
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(item_id(UUID_D));
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_D)),
        "paper.pdf",
        vec![entry_a, entry_b],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("bytes a"))).unwrap();

    match found.answer {
        LibraryAnswer::UnreadableItem { artifact, item, .. } => {
            assert_eq!(artifact, UUID_A.to_string());
            assert_eq!(
                item,
                UUID_D.to_string(),
                "names the link, not the entry's own (readable) item"
            );
        }
        other => panic!("expected UnreadableItem, got {other:?}"),
    }
}

#[test]
fn consult_reports_ambiguous_item_about_the_link_over_a_valid_entry_item() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"bytes a");
    let x = item_at(item_id(UUID_C));
    write_item(root, &item_file_name(None, &x.id), &x);
    let y = item_at(item_id(UUID_D));
    write_item(root, "one.toml", &y);
    write_item(root, "two.toml", &y);

    let mut entry_a = hash_entry("bytes a", "run-0");
    entry_a.item = Some(x.id.clone());
    let mut entry_b = hash_entry("bytes b", "run-1");
    entry_b.item = Some(y.id.clone());
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(y.id.clone()),
        "paper.pdf",
        vec![entry_a, entry_b],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("bytes a"))).unwrap();

    match found.answer {
        LibraryAnswer::AmbiguousItem {
            artifact,
            item,
            files,
        } => {
            assert_eq!(artifact, UUID_A.to_string());
            assert_eq!(
                item,
                UUID_D.to_string(),
                "names the link, not the entry's own (readable) item"
            );
            assert_eq!(files.len(), 2, "got {files:?}");
        }
        other => panic!("expected AmbiguousItem, got {other:?}"),
    }
}

#[test]
fn evidence_retrieval_names_the_item_answered_with_for_restored_bytes() {
    let evidence = Evidence {
        library: Consultation::Consulted(LibraryAnswer::Tracked {
            artifact: UUID_A.to_string(),
            item: UUID_D.to_string(),
            accepted_under: Some(UUID_C.to_string()),
        }),
        content_index: IndexEvidence {
            read: IndexRead::NotAttempted(Unattempted::LibraryAnswered),
            write: IndexWrite::NotAttempted(Unattempted::LibraryAnswered),
        },
        extraction: ExtractionEvidence {
            result: ExtractionStep::NotAttempted(Unattempted::LibraryAnswered),
            titles: Titles::NotAttempted(Unattempted::LibraryAnswered),
        },
        identifier_input: IdentifierInput::NotAttempted(Unattempted::LibraryAnswered),
        lookup: LookupEvidence::NotAttempted(Unattempted::LibraryAnswered),
        match_check: MatchCheck::NotAttempted(Unattempted::LibraryAnswered),
    };

    assert_eq!(
        evidence.retrieval(),
        Some(RecordRetrieval::Library {
            artifact: UUID_A.to_string(),
            item: UUID_C.to_string(),
        }),
        "the record retrieved is the one whose item the bytes were accepted under"
    );
}

#[test]
fn consult_reports_unrecognised_content_for_bytes_reconcile_left_pending() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"replacement bytes");
    let id = artifact_id(UUID_A);
    let item = item_id(UUID_B);
    write_item(
        root,
        "item.toml",
        &item_with_doi(item.clone(), "10.1000/orig"),
    );
    let record = artifact_record(
        id,
        Some(item),
        "paper.pdf",
        vec![hash_entry("original bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let identifier = doi_identifier("10.1000/other");
    let mut answers = BTreeMap::new();
    answers.insert(
        path.clone(),
        Extraction::Found {
            identifier: identifier.to_string(),
            tier: "text-layer".to_string(),
        },
    );
    let extractor = Extractor::with(answers);
    let _ = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    let found = consult(root, &path, Some(&hash_bytes(b"replacement bytes"))).unwrap();

    assert!(
        matches!(found.answer, LibraryAnswer::UnrecognisedContent { .. }),
        "a pending change writes nothing, so the new bytes are still unrecognised: got {:?}",
        found.answer
    );
}

// --- Untracked ---

/// A PDF no record names at all.
#[test]
fn consult_is_untracked_for_a_pdf_no_record_names() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "orphan.pdf", b"orphan bytes");

    let found = consult(root, &path, Some(&hash("orphan bytes"))).unwrap();

    assert_eq!(found.answer, LibraryAnswer::Untracked);
    assert_eq!(found.item, None);
}

/// A byte-identical copy of a recorded artifact, at a path no record
/// names: only the records naming *this* path are examined (design D1),
/// so a copy elsewhere is untracked even though its bytes are in the
/// library's history.
#[test]
fn consult_is_untracked_for_a_byte_identical_copy_at_another_path() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_file(root, "original.pdf", b"shared bytes");
    let copy = write_file(root, "copy.pdf", b"shared bytes");
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_B)),
        "original.pdf",
        vec![hash_entry("shared bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &copy, Some(&hash("shared bytes"))).unwrap();

    assert_eq!(found.answer, LibraryAnswer::Untracked);
}

/// A recorded artifact moved to a path no record names: reconciling is
/// the remedy the library already names for this (design D1).
#[test]
fn consult_is_untracked_for_an_artifact_moved_to_a_path_no_record_names() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let moved = write_file(root, "new-home/paper.pdf", b"paper bytes");
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_B)),
        "old-home/paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &moved, Some(&hash("paper bytes"))).unwrap();

    assert_eq!(found.answer, LibraryAnswer::Untracked);
}

/// A file that cannot be hashed (`hash: None`) and whose path no record
/// names is plainly untracked (design D2), not `unhashable`.
#[test]
fn consult_is_untracked_for_an_unhashable_file_no_record_names() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "unreadable.pdf", b"whatever");

    let found = consult(root, &path, None).unwrap();

    assert_eq!(found.answer, LibraryAnswer::Untracked);
}

// --- None: outside the library, or in a nested one ---

/// A path outside the root is not consulted at all.
#[test]
fn consult_is_none_for_a_path_outside_the_root() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("library");
    fs::create_dir_all(&root).unwrap();
    let outside = dir.path().join("elsewhere").join("paper.pdf");

    assert_eq!(consult(&root, &outside, Some(&hash("x"))), None);
}

/// A path inside a nested library (a `.borax.toml` below the root) is
/// not consulted either: the run's library stops at the nested marker,
/// as [`excludes`] already does for every other check (design D8).
#[test]
fn consult_is_none_for_a_path_inside_a_nested_library() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let nested = root.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join(".borax.toml"), "").unwrap();
    let path = write_file(root, "nested/paper.pdf", b"paper bytes");
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_B)),
        "nested/paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    assert_eq!(consult(root, &path, Some(&hash("paper bytes"))), None);
}

// --- D1a: paths compared normalised ---

/// A record naming `sub/paper.pdf` answers for the file reached with a
/// `.` and a `..` in its spelling, as `Account::is_incoming` already
/// treats them (design D1a).
#[test]
fn consult_normalises_dot_and_dotdot_in_the_input_path() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_file(root, "sub/paper.pdf", b"paper bytes");
    let item = item_at(item_id(UUID_B));
    write_item(root, &item_file_name(None, &item.id), &item);
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item.id.clone()),
        "sub/paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);
    let tracked = LibraryAnswer::Tracked {
        artifact: UUID_A.to_string(),
        item: UUID_B.to_string(),
        accepted_under: None,
    };

    let via_dot = root.join("sub").join(".").join("paper.pdf");
    assert_eq!(
        consult(root, &via_dot, Some(&hash("paper bytes")))
            .unwrap()
            .answer,
        tracked,
        "a `.` component must not change the answer"
    );

    let via_dotdot = root.join("x").join("..").join("sub").join("paper.pdf");
    assert_eq!(
        consult(root, &via_dotdot, Some(&hash("paper bytes")))
            .unwrap()
            .answer,
        tracked,
        "a `..` climbing back into the tree must not change the answer"
    );
}

/// A relative spelling of the file, from the test's own working
/// directory, is the same file to the consultation ([`paths::route`]
/// rather than a `chdir`, since tests run in one process).
#[test]
fn consult_normalises_a_relative_spelling_of_the_input_path() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_file(root, "paper.pdf", b"paper bytes");
    let item = item_at(item_id(UUID_B));
    write_item(root, &item_file_name(None, &item.id), &item);
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item.id.clone()),
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);
    let cwd = std::env::current_dir().unwrap();
    let relative = borax::paths::route(&root.join("paper.pdf"), &cwd);

    let found = consult(root, &relative, Some(&hash("paper bytes"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::Tracked {
            artifact: UUID_A.to_string(),
            item: UUID_B.to_string(),
            accepted_under: None,
        },
        "a relative spelling of the same file must answer the same way: {relative:?}"
    );
}

/// A relative spelling of the *root* answers the same way too.
#[test]
fn consult_normalises_a_relative_spelling_of_the_root() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_file(root, "paper.pdf", b"paper bytes");
    let item = item_at(item_id(UUID_B));
    write_item(root, &item_file_name(None, &item.id), &item);
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item.id.clone()),
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);
    let cwd = std::env::current_dir().unwrap();
    let relative_root = borax::paths::route(root, &cwd);

    let found = consult(
        &relative_root,
        &root.join("paper.pdf"),
        Some(&hash("paper bytes")),
    )
    .unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::Tracked {
            artifact: UUID_A.to_string(),
            item: UUID_B.to_string(),
            accepted_under: None,
        },
        "a relative root must answer the same way: {relative_root:?}"
    );
}

/// design D3: `UnreadableItem.path` and `AmbiguousItem.files` are full
/// paths, "as `duplicate.existing_path` is, so a reader can open them" —
/// even when the library was read through a relative spelling of the
/// root. The working directory is never changed; `paths::route` builds
/// the relative spelling from it instead.
#[test]
fn consult_reports_full_paths_even_when_the_root_was_read_relatively() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let cwd = std::env::current_dir().unwrap();
    let relative_root = borax::paths::route(root, &cwd);

    // An unreadable item file.
    let bad_path = write_file(root, "unreadable.pdf", b"unreadable item bytes");
    let bad_item = item_id(UUID_B);
    let bad_record = artifact_record(
        artifact_id(UUID_A),
        Some(bad_item.clone()),
        "unreadable.pdf",
        vec![hash_entry("unreadable item bytes", "run-0")],
    );
    write_artifact_record(root, &bad_record);
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    let bad_item_path = items.join(format!("badkey.{UUID_B}.toml"));
    fs::write(&bad_item_path, "not valid toml {{{").unwrap();

    let unreadable = consult(
        &relative_root,
        &bad_path,
        Some(&hash("unreadable item bytes")),
    )
    .unwrap();
    match unreadable.answer {
        LibraryAnswer::UnreadableItem { path, .. } => {
            assert!(
                path.is_absolute(),
                "UnreadableItem.path must be a full path even over a relative root: {path:?}"
            );
            assert_eq!(path, bad_item_path);
        }
        other => panic!("expected UnreadableItem, got {other:?}"),
    }

    // Two item files carrying one identity.
    let ambiguous_path = write_file(root, "ambiguous.pdf", b"ambiguous item bytes");
    let shared_item = item_id(UUID_C);
    let ambiguous_record = artifact_record(
        artifact_id(UUID_D),
        Some(shared_item.clone()),
        "ambiguous.pdf",
        vec![hash_entry("ambiguous item bytes", "run-0")],
    );
    write_artifact_record(root, &ambiguous_record);
    let item = item_at(shared_item);
    write_item(root, &format!("a-first.{UUID_C}.toml"), &item);
    write_item(root, &format!("b-second.{UUID_C}.toml"), &item);

    let ambiguous = consult(
        &relative_root,
        &ambiguous_path,
        Some(&hash("ambiguous item bytes")),
    )
    .unwrap();
    match ambiguous.answer {
        LibraryAnswer::AmbiguousItem { files, .. } => {
            assert!(
                files.iter().all(|file| file.is_absolute()),
                "AmbiguousItem.files must be full paths even over a relative root: {files:?}"
            );
            assert_eq!(
                files,
                vec![
                    items.join(format!("a-first.{UUID_C}.toml")),
                    items.join(format!("b-second.{UUID_C}.toml")),
                ]
            );
        }
        other => panic!("expected AmbiguousItem, got {other:?}"),
    }
}

// --- Ambiguity among records at one path ---

/// `UnrecognisedContent` lists every record naming the path when none
/// of them holds the hash.
#[test]
fn consult_reports_unrecognised_content_listing_every_record_at_the_path() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"edited bytes");
    let first = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_B)),
        "paper.pdf",
        vec![hash_entry("original bytes", "run-0")],
    );
    let second = artifact_record(
        artifact_id(UUID_C),
        Some(item_id(UUID_B)),
        "paper.pdf",
        vec![hash_entry("another original", "run-0")],
    );
    write_artifact_record(root, &first);
    write_artifact_record(root, &second);

    let found = consult(root, &path, Some(&hash("edited bytes"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::UnrecognisedContent {
            artifacts: vec![UUID_A.to_string(), UUID_C.to_string()],
        },
        "both records naming the path, in read order, must be listed"
    );
}

/// `Ambiguous` lists every record at the path that holds the hash, in
/// read order — the library declines to guess which is the file's
/// (design D1, "Rejected: resolving ambiguity by preferring one
/// record").
#[test]
fn consult_reports_ambiguous_listing_every_record_holding_the_hash() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"shared bytes");
    let first = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_B)),
        "paper.pdf",
        vec![hash_entry("shared bytes", "run-0")],
    );
    let second = artifact_record(
        artifact_id(UUID_C),
        Some(item_id(UUID_B)),
        "paper.pdf",
        vec![hash_entry("shared bytes", "run-0")],
    );
    write_artifact_record(root, &first);
    write_artifact_record(root, &second);

    let found = consult(root, &path, Some(&hash("shared bytes"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::Ambiguous {
            artifacts: vec![UUID_A.to_string(), UUID_C.to_string()],
        }
    );
    assert_eq!(found.item, None);
}

/// When two records name the path and only one holds the hash, that
/// one answers — the file is `Tracked`, not `Ambiguous`.
#[test]
fn consult_prefers_the_one_record_that_actually_holds_the_hash() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"current bytes");
    let item = item_at(item_id(UUID_B));
    write_item(root, &item_file_name(None, &item.id), &item);
    let stale = artifact_record(
        artifact_id(UUID_A),
        Some(item.id.clone()),
        "paper.pdf",
        vec![hash_entry("stale bytes", "run-0")],
    );
    let current = artifact_record(
        artifact_id(UUID_C),
        Some(item.id.clone()),
        "paper.pdf",
        vec![hash_entry("current bytes", "run-0")],
    );
    write_artifact_record(root, &stale);
    write_artifact_record(root, &current);

    let found = consult(root, &path, Some(&hash("current bytes"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::Tracked {
            artifact: UUID_C.to_string(),
            item: UUID_B.to_string(),
            accepted_under: None,
        }
    );
}

// --- Item-side problems ---

/// The linked item's file is named `<key>.<item-uuid>.toml` and does
/// not parse: `UnreadableItem` carries that path and the parser's
/// message.
#[test]
fn consult_reports_unreadable_item_when_the_linked_items_file_does_not_parse() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let item_id_b = item_id(UUID_B);
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    let item_path = items.join(format!("badkey.{UUID_B}.toml"));
    fs::write(&item_path, "not valid toml {{{").unwrap();
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_id_b),
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("paper bytes"))).unwrap();

    match found.answer {
        LibraryAnswer::UnreadableItem {
            artifact,
            item,
            path: reported_path,
            ..
        } => {
            assert_eq!(artifact, UUID_A.to_string());
            assert_eq!(item, UUID_B.to_string());
            assert_eq!(reported_path, item_path);
        }
        other => panic!("expected UnreadableItem, got {other:?}"),
    }
    assert_eq!(found.item, None);
}

/// No item file claims the linked UUID at all, and the item store has
/// no fault: `DanglingItem`, not `UnreadableItem`.
#[test]
fn consult_reports_dangling_item_when_no_item_file_claims_the_uuid() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_B)),
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("paper bytes"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::DanglingItem {
            artifact: UUID_A.to_string(),
            item: UUID_B.to_string(),
        }
    );
}

/// The one record at the path holds the hash but links no item at all:
/// `NoItem`, naming the artifact (design D1's table, row 5).
#[test]
fn consult_reports_no_item_when_the_holding_record_links_none() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let record = artifact_record(
        artifact_id(UUID_A),
        None,
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("paper bytes"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::NoItem {
            artifact: UUID_A.to_string(),
        }
    );
    assert_eq!(found.item, None);
}

/// A record names the path, but the file could not be hashed:
/// `Unhashable`, naming the artifact — no record can be confirmed
/// without a hash to check it against (design D1's table, row 3).
#[test]
fn consult_reports_unhashable_when_a_record_names_the_path_and_the_file_has_no_hash() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_B)),
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, None).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::Unhashable {
            artifacts: vec![UUID_A.to_string()],
        }
    );
}

/// `items/` cannot be listed at all: `UnreadableItem` carrying the
/// directory itself, never `DanglingItem` — a store that could not be
/// read cannot say an item is absent (design D2).
#[cfg(unix)]
#[test]
fn consult_reports_unreadable_item_naming_the_items_directory_when_it_cannot_be_listed() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_B)),
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    fs::set_permissions(&items, fs::Permissions::from_mode(0o000)).unwrap();

    if fs::read_dir(&items).is_ok() {
        fs::set_permissions(&items, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping consult_reports_unreadable_item_naming_the_items_directory_when_it_cannot_be_listed: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    let found = consult(root, &path, Some(&hash("paper bytes")));

    fs::set_permissions(&items, fs::Permissions::from_mode(0o755)).unwrap();

    match found.unwrap().answer {
        LibraryAnswer::UnreadableItem {
            artifact,
            item,
            path: reported_path,
            ..
        } => {
            assert_eq!(artifact, UUID_A.to_string());
            assert_eq!(item, UUID_B.to_string());
            assert_eq!(reported_path, items);
        }
        other => panic!("expected UnreadableItem naming items/, got {other:?}"),
    }
}

/// Two item files carry the linked identity: `AmbiguousItem`, naming
/// both files in read order, and `Consulted::item` is `None`.
#[test]
fn consult_reports_ambiguous_item_when_two_item_files_carry_the_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let item = item_at(item_id(UUID_B));
    write_item(root, &format!("a-first.{UUID_B}.toml"), &item);
    write_item(root, &format!("b-second.{UUID_B}.toml"), &item);
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item.id.clone()),
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    let found = consult(root, &path, Some(&hash("paper bytes"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::AmbiguousItem {
            artifact: UUID_A.to_string(),
            item: UUID_B.to_string(),
            files: vec![
                root.join(ITEM_STORE).join(format!("a-first.{UUID_B}.toml")),
                root.join(ITEM_STORE)
                    .join(format!("b-second.{UUID_B}.toml")),
            ],
        }
    );
    assert_eq!(found.item, None);
}

// --- Record-store faults (design D2a) ---

/// One artifact-record file does not parse, and no readable record
/// names the path: `UnreadableRecords { listed: true, unreadable: 1 }`.
#[test]
fn consult_reports_unreadable_records_when_one_record_file_does_not_parse() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "unrecorded.pdf", b"unrecorded bytes");
    let dir_path = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&dir_path).unwrap();
    fs::write(
        dir_path.join(format!("{UUID_A}.toml")),
        "not valid toml {{{",
    )
    .unwrap();

    let found = consult(root, &path, Some(&hash("unrecorded bytes"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::UnreadableRecords {
            listed: true,
            unreadable: 1,
        }
    );
    assert_eq!(
        Stores::read(root).record_faults(),
        Some(RecordFaults {
            listed: true,
            unreadable: 1,
        })
    );
}

/// `.borax/artifacts/` cannot be listed at all:
/// `UnreadableRecords { listed: false, unreadable: 0 }`.
#[cfg(unix)]
#[test]
fn consult_reports_unreadable_records_when_the_artifact_store_cannot_be_listed() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "unrecorded.pdf", b"unrecorded bytes");
    let records = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records).unwrap();
    fs::set_permissions(&records, fs::Permissions::from_mode(0o000)).unwrap();

    if fs::read_dir(&records).is_ok() {
        fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping consult_reports_unreadable_records_when_the_artifact_store_cannot_be_listed: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    let found = consult(root, &path, Some(&hash("unrecorded bytes")));
    let faults = Stores::read(root).record_faults();

    fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(
        found.unwrap().answer,
        LibraryAnswer::UnreadableRecords {
            listed: false,
            unreadable: 0,
        }
    );
    assert_eq!(
        faults,
        Some(RecordFaults {
            listed: false,
            unreadable: 0,
        })
    );
}

/// A tracked file whose own record parses cleanly is still `Tracked`,
/// even while an unrelated record file in the same store is unreadable:
/// one fault costs its own record and no other (design D2).
#[test]
fn consult_reports_tracked_even_when_an_unrelated_record_file_is_unreadable() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let item = item_at(item_id(UUID_B));
    write_item(root, &item_file_name(None, &item.id), &item);
    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item.id.clone()),
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);
    let dir_path = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::write(
        dir_path.join(format!("{UUID_C}.toml")),
        "not valid toml {{{",
    )
    .unwrap();

    let found = consult(root, &path, Some(&hash("paper bytes"))).unwrap();

    assert_eq!(
        found.answer,
        LibraryAnswer::Tracked {
            artifact: UUID_A.to_string(),
            item: UUID_B.to_string(),
            accepted_under: None,
        },
        "the file's own record is clean, so the unrelated fault must not affect it"
    );
}

/// [`Stores::record_faults`] is `None` when the artifact store was read
/// whole: no fault at all, and an absent store directory alike.
#[test]
fn record_faults_is_none_for_a_store_read_whole_or_absent() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    assert_eq!(
        Stores::read(root).record_faults(),
        None,
        "an absent store directory has nothing to fault"
    );

    let record = artifact_record(
        artifact_id(UUID_A),
        Some(item_id(UUID_B)),
        "paper.pdf",
        vec![hash_entry("paper bytes", "run-0")],
    );
    write_artifact_record(root, &record);

    assert_eq!(
        Stores::read(root).record_faults(),
        None,
        "a store read whole, with nothing unreadable, has no fault to report"
    );
}

// --- ItemStore::read / ArtifactStore::read over an unlistable directory ---

/// An absent store directory is an empty store with no fault, on
/// [`ItemStore::read`]'s documented terms.
#[test]
fn item_store_of_an_absent_directory_has_no_fault() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let store = ItemStore::read(root);

    assert!(store.is_empty());
    assert_eq!(store.faults, Vec::new());
}

/// An unlistable `items/` directory gives one fault whose path is the
/// directory itself (design D2a).
#[cfg(unix)]
#[test]
fn item_store_of_an_unlistable_directory_gives_a_fault_at_the_directory() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path();
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    fs::set_permissions(&items, fs::Permissions::from_mode(0o000)).unwrap();

    if fs::read_dir(&items).is_ok() {
        fs::set_permissions(&items, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping item_store_of_an_unlistable_directory_gives_a_fault_at_the_directory: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    let store = ItemStore::read(root);

    fs::set_permissions(&items, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(store.is_empty());
    assert_eq!(store.faults.len(), 1, "got {:?}", store.faults);
    assert_eq!(store.faults[0].path, items);
}

/// The same, on the artifact-record side: an unlistable
/// `.borax/artifacts/` gives one fault at that directory.
#[cfg(unix)]
#[test]
fn artifact_store_of_an_unlistable_directory_gives_a_fault_at_the_directory() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path();
    let records = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records).unwrap();
    fs::set_permissions(&records, fs::Permissions::from_mode(0o000)).unwrap();

    if fs::read_dir(&records).is_ok() {
        fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping artifact_store_of_an_unlistable_directory_gives_a_fault_at_the_directory: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    let store = ArtifactStore::read(root);

    fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(store.is_empty());
    assert_eq!(store.faults.len(), 1, "got {:?}", store.faults);
    assert_eq!(store.faults[0].path, records);
}

// --- validate() over an unlistable artifact store (design D2a) ---

/// `borax validate` over a library whose `.borax/artifacts/` cannot be
/// listed reports an `unreadable` finding at that directory, instead of
/// certifying an empty, clean library.
#[cfg(unix)]
#[test]
fn validate_reports_an_unreadable_finding_for_an_unlistable_artifact_store() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path();
    let records = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records).unwrap();
    fs::set_permissions(&records, fs::Permissions::from_mode(0o000)).unwrap();

    if fs::read_dir(&records).is_ok() {
        fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping validate_reports_an_unreadable_finding_for_an_unlistable_artifact_store: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    let result = validate(root);

    fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(
        result.findings.len(),
        1,
        "an unlistable artifact-record store must be a finding, not a clean, empty library: \
         got {:?}",
        result.findings
    );
    let (path, finding) = &result.findings[0];
    assert_eq!(path, &records);
    assert!(
        matches!(finding, Finding::Unreadable { .. }),
        "got {finding:?}"
    );
}

// ---------------------------------------------------------------------
// report-extraction-per-file, task 3.6: extraction_event names the
// artifact library-relative (design D6, D10)
// ---------------------------------------------------------------------

/// `extraction_event` reports `extraction` for `artifact`, named
/// relative to `survey.root` and `/`-separated, for an artifact at the
/// root and one two directories down.
#[test]
fn extraction_event_names_the_artifact_library_relative() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("root.pdf"), b"").unwrap();
    fs::create_dir_all(root.join("a/b")).unwrap();
    fs::write(root.join("a/b/nested.pdf"), b"").unwrap();

    let surveyed = survey(root);

    let root_event = extraction_event(&surveyed, &root.join("root.pdf"), Extraction::NoTextLayer);
    assert_eq!(
        root_event,
        Event::LibraryExtraction {
            path: "root.pdf".to_string(),
            extraction: Extraction::NoTextLayer,
        }
    );

    let nested_event = extraction_event(
        &surveyed,
        &root.join("a/b/nested.pdf"),
        Extraction::Encrypted,
    );
    assert_eq!(
        nested_event,
        Event::LibraryExtraction {
            path: "a/b/nested.pdf".to_string(),
            extraction: Extraction::Encrypted,
        }
    );
}

// ---------------------------------------------------------------------
// 3.1: identity, ensure_identity and StoreWriter (design D3, D4, D8)
// ---------------------------------------------------------------------

#[test]
fn identity_of_a_library_with_no_state_directory_is_absent() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    assert_eq!(identity(root), Identity::Absent);
}

#[test]
fn identity_of_a_library_with_records_and_no_identity_file_is_absent() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_A),
            None,
            "paper.pdf",
            vec![hash_entry("bytes", "run-0")],
        ),
    );

    assert_eq!(identity(root), Identity::Absent);
}

#[test]
fn identity_reads_the_id_the_file_holds() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), format!("id = \"{UUID_A}\"\n")).unwrap();

    assert_eq!(
        identity(root),
        Identity::Present(LibraryId::parse(UUID_A).unwrap())
    );
}

#[test]
fn identity_of_a_file_that_does_not_parse_is_faulty_naming_the_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), b"id = 7\n").unwrap();

    match identity(root) {
        Identity::Faulty(fault) => {
            assert_eq!(fault.path, identity_file(root));
            assert!(!fault.message.is_empty());
        }
        other => panic!("expected Faulty, got {other:?}"),
    }
}

#[test]
fn identity_of_an_empty_file_is_faulty() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), b"").unwrap();

    assert!(matches!(identity(root), Identity::Faulty(_)));
}

#[test]
fn identity_of_a_directory_at_the_identity_path_is_faulty() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(identity_file(root)).unwrap();

    assert!(matches!(identity(root), Identity::Faulty(_)));
}

#[cfg(unix)]
#[test]
fn identity_of_a_dangling_symlink_is_faulty() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    std::os::unix::fs::symlink(root.join("nowhere"), identity_file(root)).unwrap();

    assert!(matches!(identity(root), Identity::Faulty(_)));
}

#[test]
fn ensure_identity_mints_a_v7_identity_into_an_absent_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let result = ensure_identity(root, &store_create).unwrap();

    let id = match result {
        Identity::Present(id) => id,
        other => panic!("expected Present, got {other:?}"),
    };
    assert_eq!(id.uuid().get_version_num(), 7, "the minted uuid must be v7");
    assert_eq!(
        fs::read_to_string(identity_file(root)).unwrap(),
        format!("id = \"{id}\"\n")
    );
    assert_eq!(identity(root), Identity::Present(id));
}

#[test]
fn ensure_identity_creates_the_state_directory_when_there_is_none() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    assert!(!root.join(STATE_DIR).exists());

    ensure_identity(root, &store_create).unwrap();

    assert!(identity_file(root).exists());
}

#[test]
fn ensure_identity_returns_the_identity_already_there_and_calls_no_create() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), format!("id = \"{UUID_A}\"\n")).unwrap();
    let before = snapshot(root);
    let calls = Cell::new(0usize);
    let create = |path: &Path, bytes: &[u8]| -> std::io::Result<()> {
        calls.set(calls.get() + 1);
        store_create(path, bytes)
    };

    let result = ensure_identity(root, &create).unwrap();

    assert_eq!(result, Identity::Present(LibraryId::parse(UUID_A).unwrap()));
    assert_eq!(
        calls.get(),
        0,
        "an identity already there must not be minted over"
    );
    assert_eq!(snapshot(root), before);
}

#[test]
fn ensure_identity_does_not_mint_over_a_faulty_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), b"not-toml-at-all {{").unwrap();
    let before = fs::read(identity_file(root)).unwrap();
    let calls = Cell::new(0usize);
    let create = |path: &Path, bytes: &[u8]| -> std::io::Result<()> {
        calls.set(calls.get() + 1);
        store_create(path, bytes)
    };

    let result = ensure_identity(root, &create).unwrap();

    assert!(matches!(result, Identity::Faulty(_)));
    assert_eq!(fs::read(identity_file(root)).unwrap(), before);
    assert_eq!(calls.get(), 0);
}

#[test]
fn ensure_identity_reports_a_failed_mint_naming_the_identity_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let create = |_path: &Path, _bytes: &[u8]| -> std::io::Result<()> {
        Err(std::io::Error::other("simulated"))
    };

    let error = ensure_identity(root, &create).unwrap_err();

    let text = error.to_string();
    assert!(
        text.contains("cannot create the library's identity at"),
        "got {text:?}"
    );
    assert!(text.contains(&identity_file(root).display().to_string()));
    assert!(text.contains("simulated"));
    assert!(!identity_file(root).exists());
}

#[test]
fn ensure_identity_after_a_lost_race_returns_the_winners_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let create = |path: &Path, _bytes: &[u8]| -> std::io::Result<()> {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("id = \"{UUID_B}\"\n")).unwrap();
        Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
    };

    let result = ensure_identity(root, &create).unwrap();

    assert_eq!(result, Identity::Present(LibraryId::parse(UUID_B).unwrap()));
}

#[test]
fn ensure_identity_after_a_lost_race_to_a_faulty_file_returns_it_faulty() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let create = |path: &Path, _bytes: &[u8]| -> std::io::Result<()> {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"not-toml-at-all {{").unwrap();
        Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
    };

    let result = ensure_identity(root, &create).unwrap();

    assert!(matches!(result, Identity::Faulty(_)));
}

#[test]
fn concurrent_ensures_converge_on_one_identity() {
    let dir = tempdir().unwrap();
    let root = Arc::new(dir.path().to_path_buf());
    let barrier = Arc::new(std::sync::Barrier::new(8));

    let threads: Vec<_> = (0..8)
        .map(|_| {
            let root = Arc::clone(&root);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                ensure_identity(&root, &store_create)
            })
        })
        .collect();

    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    let id = match results[0].as_ref().unwrap() {
        Identity::Present(id) => id.clone(),
        other => panic!("expected Present, got {other:?}"),
    };
    for result in &results {
        assert_eq!(result.as_ref().unwrap(), &Identity::Present(id.clone()));
    }
    assert_eq!(identity(&root), Identity::Present(id));
}

/// design D4, "What concurrent minting guarantees", made deterministic:
/// run A has exclusively created the identity file and written only its
/// first 10 bytes when run B meets it.
#[test]
fn a_run_meeting_the_identity_file_mid_write_writes_without_minting_and_remembers_the_fault() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    let whole = LibraryIdentity {
        id: LibraryId::parse(UUID_A).unwrap(),
    }
    .to_toml();
    let short = &whole.as_bytes()[..10];
    fs::write(identity_file(root), short).unwrap();

    let create_calls = Cell::new(0usize);
    let create = |path: &Path, bytes: &[u8]| -> std::io::Result<()> {
        create_calls.set(create_calls.get() + 1);
        store_create(path, bytes)
    };
    let write_log: RefCell<Vec<PathBuf>> = RefCell::new(Vec::new());
    let write = |path: &Path, bytes: &[u8]| -> std::io::Result<()> {
        write_log.borrow_mut().push(path.to_path_buf());
        store_write(path, bytes)
    };
    let writer = StoreWriter::new(root, &create, &write);
    let target = root.join("target.toml");

    writer.write(&target, b"target bytes").unwrap();

    assert_eq!(
        create_calls.get(),
        0,
        "B must not try to mint over a short file"
    );
    assert_eq!(write_log.borrow().as_slice(), std::slice::from_ref(&target));
    let fault = writer
        .identity_fault()
        .expect("the fault B met must be remembered");
    assert_eq!(fault.path, identity_file(root));
    assert_eq!(fs::read(identity_file(root)).unwrap(), short);

    // A completes.
    fs::write(identity_file(root), &whole).unwrap();
    assert_eq!(
        identity(root),
        Identity::Present(LibraryId::parse(UUID_A).unwrap())
    );
}

#[test]
fn a_run_whose_concurrent_minter_failed_and_removed_its_file_leaves_no_identity_and_keeps_its_write()
 {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    let whole = LibraryIdentity {
        id: LibraryId::parse(UUID_A).unwrap(),
    }
    .to_toml();
    fs::write(identity_file(root), &whole.as_bytes()[..10]).unwrap();

    let writer = StoreWriter::new(root, &store_create, &store_write);
    let target = root.join("target.toml");
    writer.write(&target, b"B's bytes").unwrap();

    // A fails and removes its file.
    fs::remove_file(identity_file(root)).unwrap();
    assert_eq!(identity(root), Identity::Absent);
    assert_eq!(fs::read(&target).unwrap(), b"B's bytes");

    let second_writer = StoreWriter::new(root, &store_create, &store_write);
    second_writer
        .write(&root.join("second.toml"), b"more bytes")
        .unwrap();
    assert!(matches!(identity(root), Identity::Present(_)));
}

#[test]
fn store_writer_ensures_the_identity_before_the_write_it_guards() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let log: RefCell<Vec<PathBuf>> = RefCell::new(Vec::new());
    let create = |path: &Path, bytes: &[u8]| -> std::io::Result<()> {
        log.borrow_mut().push(path.to_path_buf());
        store_create(path, bytes)
    };
    let write = |path: &Path, bytes: &[u8]| -> std::io::Result<()> {
        log.borrow_mut().push(path.to_path_buf());
        store_write(path, bytes)
    };
    let writer = StoreWriter::new(root, &create, &write);
    let target = root.join("target.toml");

    writer.write(&target, b"bytes").unwrap();

    assert_eq!(log.borrow().as_slice(), [identity_file(root), target]);
}

#[test]
fn store_writer_does_not_write_when_the_mint_fails() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let create = |_path: &Path, _bytes: &[u8]| -> std::io::Result<()> {
        Err(std::io::Error::other("simulated"))
    };
    let write_calls = Cell::new(0usize);
    let write = |_path: &Path, _bytes: &[u8]| -> std::io::Result<()> {
        write_calls.set(write_calls.get() + 1);
        Ok(())
    };
    let writer = StoreWriter::new(root, &create, &write);

    let error = writer
        .write(&root.join("target.toml"), b"bytes")
        .unwrap_err();

    assert_eq!(
        write_calls.get(),
        0,
        "a failed mint must not be followed by a write"
    );
    assert!(
        error
            .to_string()
            .contains(&identity_file(root).display().to_string())
    );
}

#[test]
fn store_writer_writes_into_a_library_whose_identity_file_is_faulty_and_remembers_the_fault() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), b"not-toml-at-all {{").unwrap();
    let before = fs::read(identity_file(root)).unwrap();
    let write_calls = Cell::new(0usize);
    let write = |path: &Path, bytes: &[u8]| -> std::io::Result<()> {
        write_calls.set(write_calls.get() + 1);
        store_write(path, bytes)
    };
    let writer = StoreWriter::new(root, &store_create, &write);

    writer.write(&root.join("target.toml"), b"bytes").unwrap();

    assert_eq!(
        write_calls.get(),
        1,
        "a faulty identity must not block the write"
    );
    let fault = writer.identity_fault().expect("a fault must be remembered");
    assert_eq!(fault.path, identity_file(root));
    assert_eq!(fs::read(identity_file(root)).unwrap(), before);
}

#[test]
fn store_writer_ensures_the_identity_before_each_write() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let writer = StoreWriter::new(root, &store_create, &store_write);

    writer.write(&root.join("first.toml"), b"one").unwrap();
    assert!(identity_file(root).exists());

    fs::remove_file(identity_file(root)).unwrap();

    writer.write(&root.join("second.toml"), b"two").unwrap();
    assert!(
        identity_file(root).exists(),
        "each write must ensure the identity for itself"
    );
}

#[test]
fn store_writer_that_writes_nothing_mints_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let _writer = StoreWriter::new(root, &store_create, &store_write);

    assert_eq!(identity(root), Identity::Absent);
}

#[test]
fn admit_through_a_store_writer_creates_the_identity_before_the_item_and_the_record() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let record = record_with_doi("10.1000/store-writer-admit");
    let hash = hash("paper bytes");
    let log: RefCell<Vec<PathBuf>> = RefCell::new(Vec::new());
    let create = |p: &Path, b: &[u8]| -> std::io::Result<()> {
        log.borrow_mut().push(p.to_path_buf());
        store_create(p, b)
    };
    let write = |p: &Path, b: &[u8]| -> std::io::Result<()> {
        log.borrow_mut().push(p.to_path_buf());
        store_write(p, b)
    };
    let writer = StoreWriter::new(root, &create, &write);

    let admitted = admit(
        root,
        &Admitting {
            path: &path,
            record: &record,
            hash: &hash,
            held: None,
            reidentified: false,
            key: Some("store-writer"),
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &|p, b| writer.write(p, b),
    )
    .unwrap();

    let item_file = root
        .join(ITEM_STORE)
        .join(item_file_name(Some("store-writer"), &admitted.item));
    let record_file = root
        .join(STATE_DIR)
        .join(ARTIFACT_STORE)
        .join(format!("{}.toml", admitted.artifact));
    assert_eq!(
        log.borrow().as_slice(),
        [identity_file(root), item_file, record_file]
    );
}

#[test]
fn admit_through_a_store_writer_whose_mint_fails_writes_nothing_and_names_the_identity_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"paper bytes");
    let record = record_with_doi("10.1000/store-writer-mint-fails");
    let hash = hash("paper bytes");
    let create =
        |_p: &Path, _b: &[u8]| -> std::io::Result<()> { Err(std::io::Error::other("simulated")) };
    let writer = StoreWriter::new(root, &create, &store_write);

    let error = admit(
        root,
        &Admitting {
            path: &path,
            record: &record,
            hash: &hash,
            held: None,
            reidentified: false,
            key: Some("store-writer"),
            run: RunId::new("run-1"),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        },
        &|p, b| writer.write(p, b),
    )
    .unwrap_err();

    assert!(
        error.message.contains(".borax/library.toml"),
        "got {error:?}"
    );
    assert!(!root.join(ITEM_STORE).exists());
    assert!(!root.join(STATE_DIR).join(ARTIFACT_STORE).exists());
}

#[test]
fn an_admission_that_rewrites_nothing_mints_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = write_file(root, "paper.pdf", b"steady bytes");
    let record = record_with_doi("10.1000/rewrite-nothing-mints-nothing");
    let hash = hash("steady bytes");

    fn admitting<'a>(
        path: &'a Path,
        record: &'a Record,
        hash: &'a ContentHash,
        held: Option<&'a ArtifactRecord>,
        run: &str,
    ) -> Admitting<'a> {
        Admitting {
            path,
            record,
            hash,
            held,
            reidentified: false,
            key: Some("rewrite-nothing"),
            run: RunId::new(run),
            timestamp: TIMESTAMP,
            tool_version: TOOL_VERSION,
        }
    }
    let admitting = |held, run| admitting(&path, &record, &hash, held, run);

    // Through real seams, as a run admits: a StoreWriter over
    // store_create and store_write, which mints the identity.
    let real = StoreWriter::new(root, &store_create, &store_write);
    admit(root, &admitting(None, "run-1"), &|p, b| real.write(p, b)).unwrap();
    let held = recorded_at(root, &path);
    admit(root, &admitting(held.as_ref(), "run-2"), &|p, b| {
        real.write(p, b)
    })
    .unwrap();

    fs::remove_file(identity_file(root)).unwrap();

    let held = recorded_at(root, &path);
    let write_calls = Cell::new(0usize);
    let write = |p: &Path, b: &[u8]| -> std::io::Result<()> {
        write_calls.set(write_calls.get() + 1);
        store_write(p, b)
    };
    let writer = StoreWriter::new(root, &store_create, &write);

    admit(root, &admitting(held.as_ref(), "run-3"), &|p, b| {
        writer.write(p, b)
    })
    .unwrap();

    assert_eq!(
        write_calls.get(),
        0,
        "an admission that rewrites nothing must never reach write"
    );
    assert_eq!(identity(root), Identity::Absent);
}

#[test]
fn identity_fault_warning_names_the_file_and_validate() {
    let fault = StoreFault {
        path: identity_file(Path::new("/lib")),
        message: "malformed toml: not a canonical library uuid: \"not-a-uuid\"".to_string(),
    };

    let diagnostic = identity_fault_warning(&fault);

    assert_eq!(diagnostic.level, Level::Warning);
    assert_eq!(
        diagnostic.message,
        format!(
            "{} could not be read as the library's identity, so it was left as it is and the \
             library has none another location can name; borax validate reports why",
            fault.path.display()
        )
    );
}

// ---------------------------------------------------------------------
// 4.1: reconciliation and adoption write through the guard (design D3, D5)
// ---------------------------------------------------------------------

#[test]
fn reconcile_that_repairs_a_record_mints_the_identity_first() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("new")).unwrap();
    fs::write(root.join("new/paper.pdf"), b"steady content").unwrap();
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id,
        None,
        "old/paper.pdf",
        vec![hash_entry("steady content", "run-0")],
    );
    write_artifact_record(root, &record);
    assert_eq!(identity(root), Identity::Absent);

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(result.repairs.len(), 1, "got {:?}", result.repairs);
    assert!(matches!(identity(root), Identity::Present(_)));
}

#[test]
fn reconcile_over_an_untouched_library_mints_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"steady content").unwrap();
    let id = artifact_id(UUID_A);
    let record = fresh_record(
        root,
        id,
        None,
        "paper.pdf",
        vec![hash_entry("steady content", "run-0")],
    );
    write_artifact_record(root, &record);

    let extractor = Extractor::default();
    reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(identity(root), Identity::Absent);
}

#[test]
fn reconcile_under_rehash_that_confirms_records_already_described_mints_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"steady content").unwrap();
    let id = artifact_id(UUID_A);
    let record = fresh_record(
        root,
        id,
        None,
        "paper.pdf",
        vec![hash_entry("steady content", "run-0")],
    );
    write_artifact_record(root, &record);

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(true, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(result.hashed, 1, "rehash forces hashing past the fast path");
    assert_eq!(result.confirmed, 1, "got {result:?}");
    assert_eq!(identity(root), Identity::Absent);
}

#[test]
fn reconcile_that_refreshes_a_touched_record_mints_the_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    fs::write(&path, b"untouched bytes").unwrap();
    let id = artifact_id(UUID_A);
    let record = fresh_record(
        root,
        id,
        None,
        "paper.pdf",
        vec![hash_entry("untouched bytes", "run-0")],
    );
    write_artifact_record(root, &record);
    let touched = SystemTime::now() + std::time::Duration::from_secs(3600);
    fs::File::open(&path)
        .unwrap()
        .set_modified(touched)
        .unwrap();

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(result.confirmed, 1, "got {result:?}");
    assert!(matches!(identity(root), Identity::Present(_)));
}

#[test]
fn reconcile_that_finds_only_a_pending_change_mints_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("paper.pdf"), b"replacement bytes").unwrap();
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id,
        None,
        "paper.pdf",
        vec![hash_entry("original bytes", "run-0")],
    );
    write_artifact_record(root, &record);
    let identifier = doi_identifier("10.1000/pending-mints-nothing");
    let extraction = Extraction::Found {
        identifier: identifier.to_string(),
        tier: "text-layer".to_string(),
    };
    let mut answers = BTreeMap::new();
    answers.insert(root.join("paper.pdf"), extraction);
    let extractor = Extractor::with(answers);

    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(result.pending, 1, "got {result:?}");
    assert_eq!(identity(root), Identity::Absent);
}

#[test]
fn reconcile_that_finds_only_missing_records_mints_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let id = artifact_id(UUID_A);
    let record = artifact_record(id, None, "gone.pdf", vec![hash_entry("gone", "run-0")]);
    write_artifact_record(root, &record);

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(result.repairs.len(), 1, "got {:?}", result.repairs);
    assert_eq!(identity(root), Identity::Absent);
}

#[test]
fn reconcile_into_a_library_with_an_identity_leaves_it_byte_identical() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    ensure_identity(root, &store_create).unwrap();
    let before = fs::read(identity_file(root)).unwrap();
    fs::create_dir_all(root.join("new")).unwrap();
    fs::write(root.join("new/paper.pdf"), b"steady content").unwrap();
    let id = artifact_id(UUID_B);
    let record = artifact_record(
        id,
        None,
        "old/paper.pdf",
        vec![hash_entry("steady content", "run-0")],
    );
    write_artifact_record(root, &record);

    let extractor = Extractor::default();
    reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(fs::read(identity_file(root)).unwrap(), before);
}

#[test]
fn reconcile_into_a_library_with_a_faulty_identity_file_writes_the_repair_and_reports_the_fault() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), b"not-toml-at-all {{").unwrap();
    let before = fs::read(identity_file(root)).unwrap();
    fs::create_dir_all(root.join("new")).unwrap();
    fs::write(root.join("new/paper.pdf"), b"steady content").unwrap();
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id,
        None,
        "old/paper.pdf",
        vec![hash_entry("steady content", "run-0")],
    );
    write_artifact_record(root, &record);

    let extractor = Extractor::default();
    let result: Reconciliation = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    assert_eq!(result.repairs.len(), 1, "got {:?}", result.repairs);
    assert_eq!(fs::read(identity_file(root)).unwrap(), before);
    let fault = result
        .identity_fault
        .expect("the fault must be reported on the reconciliation");
    assert_eq!(fault.path, identity_file(root));
}

#[cfg(unix)]
#[test]
fn reconcile_whose_mint_fails_writes_no_record_and_reports_unwritten_naming_the_identity_file() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR).join(ARTIFACT_STORE)).unwrap();
    fs::create_dir_all(root.join(ITEM_STORE)).unwrap();
    fs::create_dir_all(root.join("new")).unwrap();
    fs::write(root.join("new/paper.pdf"), b"steady content").unwrap();
    let id = artifact_id(UUID_A);
    let record = artifact_record(
        id.clone(),
        None,
        "old/paper.pdf",
        vec![hash_entry("steady content", "run-0")],
    );
    write_artifact_record(root, &record);
    let record_file = root
        .join(STATE_DIR)
        .join(ARTIFACT_STORE)
        .join(format!("{id}.toml"));
    let before = fs::read(&record_file).unwrap();

    let state_dir = root.join(STATE_DIR);
    fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o555)).unwrap();
    let probe = state_dir.join("probe.tmp");
    if fs::write(&probe, b"x").is_ok() {
        let _ = fs::remove_file(&probe);
        fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping reconcile_whose_mint_fails_writes_no_record_and_reports_unwritten_naming_the_identity_file: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    let extractor = Extractor::default();
    let result = reconcile(
        root,
        &reconciling(false, "run-1", &[], &|p| extractor.extract(p)),
    );

    fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(fs::read(&record_file).unwrap(), before);
    match &result.repairs[0] {
        (_, _, Repair::Unwritten { message }) => {
            assert!(message.contains("library.toml"), "got {message:?}");
        }
        other => panic!("expected Unwritten, got {other:?}"),
    }
}

#[test]
fn adopt_that_records_an_orphan_mints_the_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("orphan.pdf"), b"orphan bytes").unwrap();
    let orphan_hash = hash("orphan bytes");
    let lookup = |candidate: &ContentHash| -> Option<Record> {
        if *candidate == orphan_hash {
            Some(record_with_doi("10.1000/adopt-mints-identity"))
        } else {
            None
        }
    };
    let mut key = |_record: &Record, _hash: &ContentHash| -> Option<String> { None };

    let result = adopt(
        root,
        &lookup,
        &mut key,
        RunId::new("run-1"),
        TIMESTAMP,
        TOOL_VERSION,
    );

    assert_eq!(result.adopted(), 1, "got {result:?}");
    assert!(matches!(identity(root), Identity::Present(_)));
}

#[test]
fn adopt_that_records_nothing_mints_nothing() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("orphan.pdf"), b"unindexed bytes").unwrap();
    let lookup = |_hash: &ContentHash| -> Option<Record> { None };
    let mut key = |_record: &Record, _hash: &ContentHash| -> Option<String> { None };

    let result = adopt(
        root,
        &lookup,
        &mut key,
        RunId::new("run-1"),
        TIMESTAMP,
        TOOL_VERSION,
    );

    assert_eq!(result.adopted(), 0, "got {result:?}");
    assert_eq!(identity(root), Identity::Absent);
}

#[test]
fn adopt_into_a_library_with_a_faulty_identity_file_records_and_reports_the_fault() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), b"not-toml-at-all {{").unwrap();
    let before = fs::read(identity_file(root)).unwrap();
    fs::write(root.join("orphan.pdf"), b"orphan bytes faulty").unwrap();
    let orphan_hash = hash("orphan bytes faulty");
    let lookup = |candidate: &ContentHash| -> Option<Record> {
        if *candidate == orphan_hash {
            Some(record_with_doi("10.1000/adopt-faulty-identity"))
        } else {
            None
        }
    };
    let mut key = |_record: &Record, _hash: &ContentHash| -> Option<String> { None };

    let result: Adoptions = adopt(
        root,
        &lookup,
        &mut key,
        RunId::new("run-1"),
        TIMESTAMP,
        TOOL_VERSION,
    );

    assert_eq!(result.adopted(), 1, "got {result:?}");
    assert_eq!(fs::read(identity_file(root)).unwrap(), before);
    let fault = result
        .identity_fault
        .expect("the fault must be reported on the adoptions");
    assert_eq!(fault.path, identity_file(root));
}

#[cfg(unix)]
#[test]
fn adopt_whose_mint_fails_records_nothing_and_reports_unwritten_naming_the_identity_file() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR).join(ARTIFACT_STORE)).unwrap();
    fs::create_dir_all(root.join(ITEM_STORE)).unwrap();
    fs::write(root.join("orphan.pdf"), b"orphan bytes mint fails").unwrap();
    let orphan_hash = hash("orphan bytes mint fails");

    let state_dir = root.join(STATE_DIR);
    fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o555)).unwrap();
    let probe = state_dir.join("probe.tmp");
    if fs::write(&probe, b"x").is_ok() {
        let _ = fs::remove_file(&probe);
        fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping adopt_whose_mint_fails_records_nothing_and_reports_unwritten_naming_the_identity_file: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    let lookup = |candidate: &ContentHash| -> Option<Record> {
        if *candidate == orphan_hash {
            Some(record_with_doi("10.1000/adopt-mint-fails"))
        } else {
            None
        }
    };
    let mut key = |_record: &Record, _hash: &ContentHash| -> Option<String> { None };

    let result = adopt(
        root,
        &lookup,
        &mut key,
        RunId::new("run-1"),
        TIMESTAMP,
        TOOL_VERSION,
    );

    fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(result.adoptions.len(), 1, "got {:?}", result.adoptions);
    match &result.adoptions[0] {
        (_, Adoption::Unwritten { message }) => {
            assert!(message.contains("library.toml"), "got {message:?}");
        }
        other => panic!("expected Unwritten, got {other:?}"),
    }
    assert!(
        ItemStore::read(root).is_empty(),
        "a failed mint must leave no item"
    );
    assert!(
        ArtifactStore::read(root).is_empty(),
        "a failed mint must leave no artifact record"
    );
}

// ---------------------------------------------------------------------
// 6.1: status and validation carry the identity (design D6, D7)
// ---------------------------------------------------------------------

#[test]
fn survey_carries_the_identity_the_library_holds() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), format!("id = \"{UUID_A}\"\n")).unwrap();

    let surveyed = survey(root);

    assert_eq!(surveyed.identity, Some(LibraryId::parse(UUID_A).unwrap()));
}

#[test]
fn survey_of_a_library_with_no_identity_file_carries_none() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    assert_eq!(survey(root).identity, None);
}

#[test]
fn survey_of_a_library_with_a_faulty_identity_file_carries_none() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), b"not-toml-at-all {{").unwrap();

    assert_eq!(survey(root).identity, None);
}

#[test]
fn status_event_carries_the_surveys_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), format!("id = \"{UUID_A}\"\n")).unwrap();

    let surveyed = survey(root);
    let event = status_event(&surveyed, None);

    match event {
        Event::LibraryStatus { id, .. } => {
            assert_eq!(id, Some(UUID_A.to_string()));
        }
        other => panic!("expected LibraryStatus, got {other:?}"),
    }
}

#[test]
fn validate_reports_an_identity_file_that_does_not_parse_as_unreadable() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), b"id = \"not-a-uuid\"\n").unwrap();

    let result = validate(root);

    assert_eq!(result.findings.len(), 1, "got {:?}", result.findings);
    let (path, finding) = &result.findings[0];
    assert_eq!(path, &identity_file(root));
    match finding {
        Finding::Unreadable { message } => {
            assert!(
                message.contains("not a canonical library uuid"),
                "got {message:?}"
            );
        }
        other => panic!("expected Unreadable, got {other:?}"),
    }
}

#[test]
fn validate_reports_an_identity_path_holding_a_directory_as_unreadable() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(identity_file(root)).unwrap();

    let result = validate(root);

    assert_eq!(result.findings.len(), 1, "got {:?}", result.findings);
    let (path, finding) = &result.findings[0];
    assert_eq!(path, &identity_file(root));
    assert!(matches!(finding, Finding::Unreadable { .. }));
}

#[test]
fn validate_reports_the_identity_finding_before_the_item_and_record_findings() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), b"id = \"not-a-uuid\"\n").unwrap();
    let items = root.join(ITEM_STORE);
    fs::create_dir_all(&items).unwrap();
    fs::write(items.join("bad.toml"), b"this is not [ valid toml").unwrap();
    let records_dir = root.join(STATE_DIR).join(ARTIFACT_STORE);
    fs::create_dir_all(&records_dir).unwrap();
    fs::write(
        records_dir.join("bad.toml"),
        b"this is not [ valid toml either",
    )
    .unwrap();

    let result = validate(root);

    assert_eq!(result.findings.len(), 3, "got {:?}", result.findings);
    assert_eq!(result.findings[0].0, identity_file(root));
    assert_eq!(result.findings[1].0, items.join("bad.toml"));
    assert_eq!(result.findings[2].0, records_dir.join("bad.toml"));
}

#[test]
fn validate_reports_no_finding_and_no_condition_for_a_library_with_no_identity_file() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let result = validate(root);

    assert!(result.findings.is_empty(), "got {:?}", result.findings);
    assert_eq!(result.orphans(), 0);
    assert_eq!(result.missing(), 0);
    assert_eq!(result.unlinked(), 0);
}

#[test]
fn validate_reports_no_finding_for_an_identity_file_with_an_unknown_key() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(
        identity_file(root),
        format!("id = \"{UUID_A}\"\nunknown = 1\n"),
    )
    .unwrap();

    let result = validate(root);

    assert!(result.findings.is_empty(), "got {:?}", result.findings);
}

#[test]
fn validate_carries_the_identity_on_its_totals() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(STATE_DIR)).unwrap();
    fs::write(identity_file(root), format!("id = \"{UUID_A}\"\n")).unwrap();

    let result = validate(root);
    let events = validation_events(&result);

    match events.last() {
        Some(Event::LibraryValidated { id, .. }) => {
            assert_eq!(id, &Some(UUID_A.to_string()));
        }
        other => panic!("expected LibraryValidated last, got {other:?}"),
    }
}

#[test]
fn validate_mints_no_identity() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_artifact_record(
        root,
        &artifact_record(
            artifact_id(UUID_A),
            None,
            "paper.pdf",
            vec![hash_entry("bytes", "run-0")],
        ),
    );
    let before = snapshot(root);

    validate(root);

    assert_eq!(snapshot(root), before);
}
