#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::path::Path;

use borax::event::Level;
use borax::ledger::{
    ACCOUNTING_DIR, FileLedger, LEDGER_FILE, Ledger, LedgerWarning, Loaded, Scanned,
    admission_entry, duplicate_is_live, prepare, rebuild, relative_to, scan_collection,
};
use borax::pipeline::FileRecord;
use borax_core::bib_output::sidecar;
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{ArxivId, Doi, Isbn, Pmid};
use borax_core::ledger::{
    Duplicate, DuplicateReason, Entry, Index, RunId, Unparsable, serialize_jsonl,
};
use borax_core::record::{BoraxExt, EntryType, Record};
use borax_sources::store::hash_file;
use tempfile::tempdir;

// ---------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------

/// A [`Ledger`] fake returning a fixed [`Loaded`] from every `load`,
/// counting how many times `load` was called — the counter is what
/// proves [`prepare`] never touches a disabled or root-less ledger —
/// and recording every `append`.
struct FakeLedger {
    loaded: Loaded,
    load_calls: RefCell<usize>,
    appended: RefCell<Vec<Entry>>,
}

impl FakeLedger {
    fn new(loaded: Loaded) -> FakeLedger {
        FakeLedger {
            loaded,
            load_calls: RefCell::new(0),
            appended: RefCell::new(Vec::new()),
        }
    }

    fn load_calls(&self) -> usize {
        *self.load_calls.borrow()
    }
}

impl Ledger for FakeLedger {
    fn load(&self) -> Loaded {
        *self.load_calls.borrow_mut() += 1;
        self.loaded.clone()
    }

    fn append(&self, entries: &[Entry]) -> std::io::Result<()> {
        self.appended.borrow_mut().extend_from_slice(entries);
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Other helpers
// ---------------------------------------------------------------------

fn doi(s: &str) -> Doi {
    Doi::parse(s).unwrap()
}

fn arxiv(s: &str) -> ArxivId {
    ArxivId::parse(s).unwrap()
}

fn pmid(s: &str) -> Pmid {
    Pmid::parse(s).unwrap()
}

fn isbn(s: &str) -> Isbn {
    Isbn::parse(s).unwrap()
}

fn hash_of(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// A minimal ledger entry, matching the shape used across
/// `borax-core/tests/ledger.rs`.
fn entry(path: &str, hash_seed: &str) -> Entry {
    Entry {
        hash: hash_of(hash_seed),
        doi: None,
        arxiv: None,
        pmid: None,
        isbn: None,
        path: path.to_string(),
        entry_type: EntryType::Article,
        run: RunId::new("run-1"),
        timestamp: "2026-08-19T00:00:00Z".to_string(),
        tool_version: "0.2.0-test".to_string(),
    }
}

fn record_of(entry_type: EntryType) -> Record {
    Record::new(entry_type)
}

fn file_record(record: Record, hash: Option<ContentHash>) -> FileRecord {
    FileRecord {
        record,
        source: None,
        tier: None,
        cached: false,
        hash,
    }
}

/// Reports whether a candidate is one of `paths`, written with `/`
/// whatever the platform, following the shape of `exists_at` in
/// `tests/config.rs`.
fn exists_at(paths: &'static [&'static str]) -> impl Fn(&Path) -> bool {
    move |path| {
        paths
            .iter()
            .any(|expected| Path::new(expected).components().eq(path.components()))
    }
}

/// Write `content` at `dir.join(relative)`, creating parent
/// directories as needed, and a sidecar beside it carrying `record`.
fn write_pdf_with_sidecar(
    dir: &Path,
    relative: &str,
    content: &[u8],
    record: &Record,
) -> std::path::PathBuf {
    let path = dir.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, content).unwrap();
    std::fs::write(borax::bib::sidecar_path(&path), sidecar(record, "key2024")).unwrap();
    path
}

// ---------------------------------------------------------------------
// 2.2: FileLedger — path conventions
// ---------------------------------------------------------------------

#[test]
fn ledger_file_name_is_ledger_jsonl() {
    assert_eq!(LEDGER_FILE, "ledger.jsonl");
}

#[test]
fn accounting_dir_is_dot_borax() {
    assert_eq!(ACCOUNTING_DIR, ".borax");
}

#[test]
fn at_collection_root_places_the_ledger_under_the_accounting_directory() {
    let ledger = FileLedger::at_collection_root(Path::new("/collection"));
    assert_eq!(
        ledger.path(),
        Path::new("/collection")
            .join(ACCOUNTING_DIR)
            .join(LEDGER_FILE)
    );
}

// ---------------------------------------------------------------------
// 2.2: FileLedger — append
// ---------------------------------------------------------------------

#[test]
fn file_ledger_new_does_not_create_the_file_or_its_parent_directory() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested").join("ledger.jsonl");

    FileLedger::new(&path);

    assert!(!path.exists());
    assert!(!path.parent().unwrap().exists());
}

#[test]
fn file_ledger_append_creates_the_file_and_its_parent_directory() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested").join("ledger.jsonl");
    let ledger = FileLedger::new(&path);

    ledger.append(&[entry("a.pdf", "a")]).unwrap();

    assert!(path.exists());
}

#[test]
fn file_ledger_append_of_an_empty_slice_is_a_no_op_that_still_succeeds() {
    let dir = tempdir().unwrap();
    let ledger = FileLedger::new(dir.path().join("ledger.jsonl"));

    assert!(ledger.append(&[]).is_ok());
    assert_eq!(ledger.load().index.by_hash(&hash_of("a")), None);
}

/// The ledger is append-only: `append` never reorders what is already
/// on disk. Only `borax ledger rebuild` sorts.
#[test]
fn file_ledger_append_preserves_call_order_rather_than_sorting() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ledger.jsonl");
    let ledger = FileLedger::new(&path);

    ledger.append(&[entry("z.pdf", "z")]).unwrap();
    ledger.append(&[entry("a.pdf", "a")]).unwrap();

    let contents = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 2, "got {contents:?}");
    assert!(lines[0].contains("z.pdf"), "got {contents:?}");
    assert!(lines[1].contains("a.pdf"), "got {contents:?}");
}

#[test]
fn file_ledger_writes_one_json_object_per_line() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ledger.jsonl");
    let ledger = FileLedger::new(&path);

    ledger
        .append(&[entry("a.pdf", "a"), entry("b.pdf", "b")])
        .unwrap();

    let contents = std::fs::read_to_string(&path).unwrap();
    for line in contents.lines() {
        let value: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|error| panic!("{line:?} did not parse as JSON: {error}"));
        assert!(value.is_object(), "{line:?} did not parse as a JSON object");
    }
}

// ---------------------------------------------------------------------
// 2.2: FileLedger::load — degrade-loudly behaviour
// ---------------------------------------------------------------------

#[test]
fn load_of_an_absent_file_warns_absent_and_indexes_nothing() {
    let dir = tempdir().unwrap();
    let ledger = FileLedger::new(dir.path().join("ledger.jsonl"));

    let loaded = ledger.load();

    assert_eq!(loaded.warning, Some(LedgerWarning::Absent));
    assert_eq!(loaded.index.by_hash(&hash_of("a")), None);
}

#[test]
fn load_of_well_formed_entries_indexes_them_with_no_warning() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ledger.jsonl");
    let a = entry("a.pdf", "a");
    std::fs::write(&path, format!("{}\n", serde_json::to_string(&a).unwrap())).unwrap();
    let ledger = FileLedger::new(&path);

    let loaded = ledger.load();

    assert_eq!(loaded.warning, None);
    assert_eq!(loaded.index.by_hash(&hash_of("a")), Some(&a));
}

/// ledger spec: "A torn trailing line (interrupted append) SHALL be
/// ignored with a warning rather than failing the parse."
#[test]
fn load_ignores_a_torn_trailing_line_but_keeps_the_entries_before_it() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ledger.jsonl");
    let a = entry("a.pdf", "a");
    let text = format!(
        "{}\n{{\"hash\":\"sha256-dead",
        serde_json::to_string(&a).unwrap()
    );
    std::fs::write(&path, text).unwrap();
    let ledger = FileLedger::new(&path);

    let loaded = ledger.load();

    assert_eq!(loaded.warning, Some(LedgerWarning::TornTrailingLine));
    assert_eq!(loaded.index.by_hash(&hash_of("a")), Some(&a));
}

/// ledger spec scenario "Corrupt ledger": mid-file corruption turns
/// duplicate detection off entirely rather than losing one entry.
#[test]
fn load_of_mid_file_corruption_warns_unparsable_and_indexes_nothing() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ledger.jsonl");
    let a = entry("a.pdf", "a");
    let b = entry("b.pdf", "b");
    let text = format!(
        "{}\nnot valid json\n{}\n",
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
    std::fs::write(&path, text).unwrap();
    let ledger = FileLedger::new(&path);

    let loaded = ledger.load();

    assert_eq!(
        loaded.warning,
        Some(LedgerWarning::Unparsable(Unparsable { line: 2 }))
    );
    assert_eq!(loaded.index.by_hash(&hash_of("a")), None);
    assert_eq!(loaded.index.by_hash(&hash_of("b")), None);
}

// ---------------------------------------------------------------------
// 2.2 / cli spec "Stale entries never block re-admission":
// relative_to() and duplicate_is_live()
// ---------------------------------------------------------------------

#[test]
fn relative_to_joins_a_slash_separated_relative_path_onto_the_root() {
    let full = relative_to(Path::new("/collection"), "sub/Smith2024.pdf");
    assert_eq!(full, Path::new("/collection/sub/Smith2024.pdf"));
}

#[test]
fn relative_to_of_a_bare_filename_joins_directly_under_the_root() {
    let full = relative_to(Path::new("/collection"), "Smith2024.pdf");
    assert_eq!(full, Path::new("/collection/Smith2024.pdf"));
}

#[test]
fn duplicate_is_live_when_the_recorded_path_still_exists() {
    let duplicate = Duplicate {
        reason: DuplicateReason::Content,
        existing_path: "Smith2024.pdf".to_string(),
    };

    let live = duplicate_is_live(
        &duplicate,
        Path::new("/collection"),
        &exists_at(&["/collection/Smith2024.pdf"]),
    );

    assert!(live);
}

/// ledger spec scenario "Duplicate of an undone admission": disk is
/// the source of truth, so a recorded path that no longer exists never
/// vetoes re-admission.
#[test]
fn duplicate_is_stale_when_the_recorded_path_is_gone() {
    let duplicate = Duplicate {
        reason: DuplicateReason::Work,
        existing_path: "Smith2024.pdf".to_string(),
    };

    let live = duplicate_is_live(&duplicate, Path::new("/collection"), &exists_at(&[]));

    assert!(!live, "an undone admission must not veto re-admission");
}

// ---------------------------------------------------------------------
// 2.2: prepare() — degrade-loudly, never blocking
// ---------------------------------------------------------------------

#[test]
fn prepare_disabled_never_touches_the_ledger() {
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: None,
    });

    let prepared = prepare(false, Some(&ledger));

    assert_eq!(ledger.load_calls(), 0);
    assert_eq!(prepared.diagnostic, None);
    assert_eq!(prepared.index.by_hash(&hash_of("a")), None);
}

/// The caller passes no ledger at all when there is no collection
/// root; `prepare` must not treat that as anything worth warning
/// about.
#[test]
fn prepare_with_no_ledger_is_silent_and_off() {
    let prepared = prepare(true, None);

    assert_eq!(prepared.diagnostic, None);
    assert_eq!(prepared.index.by_hash(&hash_of("a")), None);
}

#[test]
fn prepare_of_a_clean_ledger_carries_its_index_with_no_diagnostic() {
    let a = entry("a.pdf", "a");
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(std::slice::from_ref(&a)),
        warning: None,
    });

    let prepared = prepare(true, Some(&ledger));

    assert_eq!(prepared.diagnostic, None);
    assert_eq!(prepared.index.by_hash(&hash_of("a")), Some(&a));
}

/// ledger spec scenario "Corrupt ledger" / degrade-loudly requirement:
/// absent earns exactly one warning naming the cause, and the run is
/// not blocked.
#[test]
fn prepare_of_an_absent_ledger_warns_and_leaves_duplicate_detection_off() {
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: Some(LedgerWarning::Absent),
    });

    let prepared = prepare(true, Some(&ledger));

    let diagnostic = prepared.diagnostic.expect("expected a warning");
    assert_eq!(diagnostic.level, Level::Warning);
    assert!(
        diagnostic.message.to_lowercase().contains("duplicate"),
        "got {:?}",
        diagnostic.message
    );
}

#[test]
fn prepare_of_an_unparsable_ledger_warns_and_names_rebuild() {
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: Some(LedgerWarning::Unparsable(Unparsable { line: 3 })),
    });

    let prepared = prepare(true, Some(&ledger));

    let diagnostic = prepared.diagnostic.expect("expected a warning");
    assert_eq!(diagnostic.level, Level::Warning);
    assert!(
        diagnostic.message.contains("rebuild"),
        "got {:?}",
        diagnostic.message
    );
}

/// A torn trailing line still warns, but — unlike absence or mid-file
/// corruption — duplicate detection stays on for everything the parse
/// did recover.
#[test]
fn prepare_of_a_torn_trailing_line_warns_but_keeps_duplicate_detection_on() {
    let a = entry("a.pdf", "a");
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(std::slice::from_ref(&a)),
        warning: Some(LedgerWarning::TornTrailingLine),
    });

    let prepared = prepare(true, Some(&ledger));

    assert!(prepared.diagnostic.is_some());
    assert_eq!(
        prepared.index.by_hash(&hash_of("a")),
        Some(&a),
        "entries before a torn trailing line stay indexed"
    );
}

// ---------------------------------------------------------------------
// 2.3: rebuild() — pure entry assembly
// ---------------------------------------------------------------------

#[test]
fn rebuild_produces_one_entry_per_scanned_file() {
    let scanned = vec![
        Scanned {
            path: "a.pdf".to_string(),
            hash: hash_of("a"),
            record: record_of(EntryType::Article),
        },
        Scanned {
            path: "b.pdf".to_string(),
            hash: hash_of("b"),
            record: record_of(EntryType::Book),
        },
    ];

    let entries = rebuild(
        &scanned,
        RunId::new("rebuild-1"),
        "2026-08-19T00:00:00Z",
        "0.2.0",
    );

    assert_eq!(entries.len(), 2);
}

#[test]
fn rebuild_carries_the_hash_path_and_entry_type_from_each_scanned_file() {
    let record = Record {
        title: Some("On Borax".to_string()),
        ..record_of(EntryType::Preprint)
    };
    let scanned = vec![Scanned {
        path: "sub/Smith2024.pdf".to_string(),
        hash: hash_of("smith"),
        record,
    }];

    let entries = rebuild(
        &scanned,
        RunId::new("rebuild-1"),
        "2026-08-19T00:00:00Z",
        "0.2.0",
    );

    assert_eq!(entries.len(), 1);
    let built = &entries[0];
    assert_eq!(built.hash, hash_of("smith"));
    assert_eq!(built.path, "sub/Smith2024.pdf");
    assert_eq!(built.entry_type, EntryType::Preprint);
}

#[test]
fn rebuild_carries_every_identifier_the_record_holds() {
    let record = Record {
        doi: Some(doi("10.1021/jacs.4c01234")),
        pmid: Some(pmid("12345678")),
        isbn: Some(isbn("978-1-59327-828-1")),
        borax: BoraxExt {
            arxiv: Some(arxiv("2401.12345")),
            ..BoraxExt::default()
        },
        ..record_of(EntryType::Article)
    };
    let scanned = vec![Scanned {
        path: "a.pdf".to_string(),
        hash: hash_of("a"),
        record,
    }];

    let entries = rebuild(&scanned, RunId::new("r"), "t", "v");

    let built = &entries[0];
    assert_eq!(built.doi, Some(doi("10.1021/jacs.4c01234")));
    assert_eq!(built.arxiv, Some(arxiv("2401.12345")));
    assert_eq!(built.pmid, Some(pmid("12345678")));
    assert_eq!(built.isbn, Some(isbn("978-1-59327-828-1")));
}

#[test]
fn rebuild_leaves_identifiers_the_record_does_not_carry_unset() {
    let scanned = vec![Scanned {
        path: "a.pdf".to_string(),
        hash: hash_of("a"),
        record: record_of(EntryType::Article),
    }];

    let entries = rebuild(&scanned, RunId::new("r"), "t", "v");

    let built = &entries[0];
    assert_eq!(built.doi, None);
    assert_eq!(built.arxiv, None);
    assert_eq!(built.pmid, None);
    assert_eq!(built.isbn, None);
}

#[test]
fn rebuild_stamps_every_entry_with_the_given_run_timestamp_and_tool_version() {
    let scanned = vec![
        Scanned {
            path: "a.pdf".to_string(),
            hash: hash_of("a"),
            record: record_of(EntryType::Article),
        },
        Scanned {
            path: "b.pdf".to_string(),
            hash: hash_of("b"),
            record: record_of(EntryType::Book),
        },
    ];
    let run = RunId::new("2026-08-19T00-00-00Z-rebuild");

    let entries = rebuild(&scanned, run.clone(), "2026-08-19T00:00:00Z", "0.2.0-test");

    for built in &entries {
        assert_eq!(built.run, run);
        assert_eq!(built.timestamp, "2026-08-19T00:00:00Z");
        assert_eq!(built.tool_version, "0.2.0-test");
    }
}

#[test]
fn rebuild_of_an_empty_scan_is_empty() {
    assert_eq!(rebuild(&[], RunId::new("r"), "t", "v"), Vec::new());
}

// ---------------------------------------------------------------------
// 2.3: scan_collection() — the I/O adapter
// ---------------------------------------------------------------------

#[test]
fn scan_collection_finds_a_pdf_with_a_valid_sidecar() {
    let dir = tempdir().unwrap();
    let record = Record {
        title: Some("On Borax".to_string()),
        doi: Some(doi("10.1021/jacs.4c01234")),
        ..record_of(EntryType::Article)
    };
    let path = write_pdf_with_sidecar(dir.path(), "Smith2024.pdf", b"pdf bytes", &record);

    let scanned = scan_collection(dir.path());

    assert_eq!(scanned.len(), 1, "got {scanned:?}");
    assert_eq!(scanned[0].path, "Smith2024.pdf");
    assert_eq!(scanned[0].hash, hash_file(&path).unwrap());
    assert_eq!(scanned[0].record.doi, Some(doi("10.1021/jacs.4c01234")));
}

#[test]
fn scan_collection_skips_a_pdf_with_no_sidecar() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("Orphan.pdf"), b"bytes").unwrap();

    assert_eq!(scan_collection(dir.path()), Vec::new());
}

/// The spec requires sidecars and files only: a sidecar borax cannot
/// recover a record from is the same as no sidecar.
#[test]
fn scan_collection_skips_a_pdf_whose_sidecar_carries_no_recoverable_record() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("Broken.pdf");
    std::fs::write(&path, b"bytes").unwrap();
    std::fs::write(borax::bib::sidecar_path(&path), "@article{key,}\n").unwrap();

    assert_eq!(scan_collection(dir.path()), Vec::new());
}

#[test]
fn scan_collection_reports_a_nested_path_slash_separated() {
    let dir = tempdir().unwrap();
    let record = record_of(EntryType::Article);
    write_pdf_with_sidecar(dir.path(), "sub/Smith2024.pdf", b"bytes", &record);

    let scanned = scan_collection(dir.path());

    assert_eq!(scanned.len(), 1, "got {scanned:?}");
    assert_eq!(scanned[0].path, "sub/Smith2024.pdf");
}

#[test]
fn scan_collection_after_a_deletion_no_longer_lists_that_file() {
    let dir = tempdir().unwrap();
    let record = record_of(EntryType::Article);
    let a = write_pdf_with_sidecar(dir.path(), "a.pdf", b"a-bytes", &record);
    write_pdf_with_sidecar(dir.path(), "b.pdf", b"b-bytes", &record);
    assert_eq!(scan_collection(dir.path()).len(), 2);

    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(borax::bib::sidecar_path(&a)).unwrap();

    let scanned = scan_collection(dir.path());
    assert_eq!(scanned.len(), 1, "got {scanned:?}");
    assert_eq!(scanned[0].path, "b.pdf");
}

// ---------------------------------------------------------------------
// 2.3: spec scenarios — rebuild after deletion, rebuild is idempotent
// ---------------------------------------------------------------------

/// ledger spec scenario "Rebuild after manual deletions".
#[test]
fn rebuild_after_scan_compacts_away_a_deleted_files_entry() {
    let dir = tempdir().unwrap();
    let record = record_of(EntryType::Article);
    let a = write_pdf_with_sidecar(dir.path(), "a.pdf", b"a-bytes", &record);
    write_pdf_with_sidecar(dir.path(), "b.pdf", b"b-bytes", &record);

    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(borax::bib::sidecar_path(&a)).unwrap();

    let entries = rebuild(&scan_collection(dir.path()), RunId::new("r"), "t", "v");
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();

    assert_eq!(paths, vec!["b.pdf"]);
}

/// ledger spec scenario "Rebuild is idempotent".
#[test]
fn rebuilding_an_unchanged_collection_twice_is_byte_identical() {
    let dir = tempdir().unwrap();
    let record = record_of(EntryType::Article);
    write_pdf_with_sidecar(dir.path(), "z.pdf", b"z-bytes", &record);
    write_pdf_with_sidecar(dir.path(), "a.pdf", b"a-bytes", &record);
    let run = RunId::new("rebuild-1");

    let first = serialize_jsonl(&rebuild(
        &scan_collection(dir.path()),
        run.clone(),
        "t",
        "v",
    ));
    let second = serialize_jsonl(&rebuild(&scan_collection(dir.path()), run, "t", "v"));

    assert_eq!(first, second);
    assert_eq!(first.lines().count(), 2, "got {first:?}");
}

// ---------------------------------------------------------------------
// 2.4: admission_entry() — what an applied admission appends
// ---------------------------------------------------------------------

/// An entry a later run could never match by hash is not worth
/// recording.
#[test]
fn admission_entry_is_none_when_the_files_hash_is_unknown() {
    let file = file_record(record_of(EntryType::Article), None);

    assert_eq!(
        admission_entry(&file, "a.pdf", RunId::new("r"), "t", "v"),
        None
    );
}

#[test]
fn admission_entry_carries_the_hash_path_and_entry_type() {
    let file = file_record(record_of(EntryType::Thesis), Some(hash_of("a")));

    let built = admission_entry(&file, "sub/Smith2024.pdf", RunId::new("r"), "t", "v").unwrap();

    assert_eq!(built.hash, hash_of("a"));
    assert_eq!(built.path, "sub/Smith2024.pdf");
    assert_eq!(built.entry_type, EntryType::Thesis);
}

#[test]
fn admission_entry_carries_every_identifier_the_record_holds() {
    let record = Record {
        doi: Some(doi("10.1021/jacs.4c01234")),
        pmid: Some(pmid("12345678")),
        isbn: Some(isbn("978-1-59327-828-1")),
        borax: BoraxExt {
            arxiv: Some(arxiv("2401.12345")),
            ..BoraxExt::default()
        },
        ..record_of(EntryType::Article)
    };
    let file = file_record(record, Some(hash_of("a")));

    let built = admission_entry(&file, "a.pdf", RunId::new("r"), "t", "v").unwrap();

    assert_eq!(built.doi, Some(doi("10.1021/jacs.4c01234")));
    assert_eq!(built.arxiv, Some(arxiv("2401.12345")));
    assert_eq!(built.pmid, Some(pmid("12345678")));
    assert_eq!(built.isbn, Some(isbn("978-1-59327-828-1")));
}

#[test]
fn admission_entry_stamps_the_given_run_timestamp_and_tool_version() {
    let file = file_record(record_of(EntryType::Article), Some(hash_of("a")));
    let run = RunId::new("2026-08-19T00-00-00Z-rename-apply");

    let built =
        admission_entry(&file, "a.pdf", run.clone(), "2026-08-19T00:00:00Z", "0.2.0").unwrap();

    assert_eq!(built.run, run);
    assert_eq!(built.timestamp, "2026-08-19T00:00:00Z");
    assert_eq!(built.tool_version, "0.2.0");
}
