#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use borax::bib::BibFiles;
use borax::cli::{Cli, Command, LedgerAction, Settings};
use borax::config::{Layer, Origin, resolve};
use borax::event::{Event, Level, SkipReason};
use borax::journal::{Entry as JournalEntry, Journal};
use borax::ledger::{
    ACCOUNTING_DIR, FileLedger, LEDGER_FILE, Ledger, LedgerWarning, Loaded, Scanned,
    admission_entry, duplicate_is_live, prepare, rebuild, relative_to, scan_collection,
};
use borax::pipeline::{FileRecord, Library};
use borax::renaming::{Filesystem, RenameError};
use borax::run::{Adapters, Configs, Streams, dispatch, events_for};
use borax::session::Outcome;
use borax_core::bib_output::sidecar;
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{ArxivId, Doi, Identifier, Isbn, Pmid};
use borax_core::ledger::{
    Duplicate, DuplicateReason, Entry, Index, RunId, Unparsable, serialize_jsonl,
};
use borax_core::record::{BoraxExt, DateParts, EntryType, Name, Record};
use borax_pdf::source::{ExtractionError, InfoMetadata, PdfSource};
use borax_sources::cache::MemoryCache;
use borax_sources::source::{Source, SourceError, SourceName};
use borax_sources::store::{ContentIndex, hash_file};
use tempfile::tempdir;

// ---------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------

/// A [`Ledger`] fake returning a fixed [`Loaded`] from every `load`,
/// counting how many times `load` was called — the counter is what
/// proves [`prepare`] never touches a disabled or root-less ledger —
/// and recording every `append` and `replace`.
struct FakeLedger {
    loaded: Loaded,
    load_calls: RefCell<usize>,
    appended: RefCell<Vec<Entry>>,
    replaced: RefCell<Option<Vec<Entry>>>,
    replace_failure: Option<String>,
}

impl FakeLedger {
    fn new(loaded: Loaded) -> FakeLedger {
        FakeLedger {
            loaded,
            load_calls: RefCell::new(0),
            appended: RefCell::new(Vec::new()),
            replaced: RefCell::new(None),
            replace_failure: None,
        }
    }

    /// Make the next call to `replace` fail with `message`, following
    /// the failing-adapter pattern `FakeFilesystem::with_failure` uses
    /// in `renaming.rs`.
    fn with_replace_failure(mut self, message: impl Into<String>) -> FakeLedger {
        self.replace_failure = Some(message.into());
        self
    }

    fn load_calls(&self) -> usize {
        *self.load_calls.borrow()
    }

    /// Every entry passed to `append`, in call order across every call.
    fn appended(&self) -> Vec<Entry> {
        self.appended.borrow().clone()
    }

    /// The entries passed to `replace`, or `None` when `replace` was
    /// never called.
    fn replaced(&self) -> Option<Vec<Entry>> {
        self.replaced.borrow().clone()
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

    fn replace(&self, entries: &[Entry]) -> std::io::Result<()> {
        if let Some(message) = &self.replace_failure {
            return Err(std::io::Error::other(message.clone()));
        }
        *self.replaced.borrow_mut() = Some(entries.to_vec());
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
// Fakes for 2.5: the ledger wired into a `rename` run through
// `dispatch`/`events_for`, following the shape of the fakes in
// `tests/dispatch.rs`.
// ---------------------------------------------------------------------

/// A [`PdfSource`] fake carrying an embedded identifier in its XMP
/// packet, following the shape of the one in `dispatch.rs`.
#[derive(Clone)]
struct FakePdf {
    info: InfoMetadata,
    xmp: Option<String>,
}

impl PdfSource for FakePdf {
    fn page_count(&self) -> usize {
        0
    }

    fn info_metadata(&self) -> &InfoMetadata {
        &self.info
    }

    fn xmp(&self) -> Option<&str> {
        self.xmp.as_deref()
    }

    fn page_text(&self, _index: usize) -> Result<String, ExtractionError> {
        Ok(String::new())
    }
}

/// A PDF carrying `value` as a DOI in its XMP packet, resolved on the
/// embedded-metadata pass.
fn pdf_with_embedded_doi(value: &str) -> FakePdf {
    FakePdf {
        info: InfoMetadata::default(),
        xmp: Some(format!("<prism:doi>{value}</prism:doi>")),
    }
}

/// What [`FakeLibrary`] answers for one path.
struct LibraryEntry {
    hash: Result<ContentHash, ExtractionError>,
    pdf: Result<FakePdf, ExtractionError>,
}

/// A [`Library`] fake backed by a map from path to a fixed `(hash, PDF
/// content or error)` pair, following the shape of the one in
/// `dispatch.rs`.
struct FakeLibrary {
    entries: BTreeMap<PathBuf, LibraryEntry>,
}

impl FakeLibrary {
    fn new() -> FakeLibrary {
        FakeLibrary {
            entries: BTreeMap::new(),
        }
    }

    /// A readable file: hashing and opening both succeed.
    fn with_file(
        mut self,
        path: impl Into<PathBuf>,
        hash: ContentHash,
        pdf: FakePdf,
    ) -> FakeLibrary {
        self.entries.insert(
            path.into(),
            LibraryEntry {
                hash: Ok(hash),
                pdf: Ok(pdf),
            },
        );
        self
    }

    /// A file whose hash is known but which fails to open — used to
    /// prove a content-duplicate check never opens the file it matches.
    fn with_open_error(
        mut self,
        path: impl Into<PathBuf>,
        hash: ContentHash,
        error: ExtractionError,
    ) -> FakeLibrary {
        self.entries.insert(
            path.into(),
            LibraryEntry {
                hash: Ok(hash),
                pdf: Err(error),
            },
        );
        self
    }
}

impl Library for FakeLibrary {
    fn hash(&self, path: &Path) -> Result<ContentHash, ExtractionError> {
        self.entries.get(path).map_or_else(
            || {
                Err(ExtractionError::Unreadable {
                    message: format!("no fake entry for {}", path.display()),
                })
            },
            |entry| entry.hash.clone(),
        )
    }

    fn open(&self, path: &Path) -> Result<Box<dyn PdfSource>, ExtractionError> {
        match self.entries.get(path) {
            Some(entry) => entry
                .pdf
                .clone()
                .map(|pdf| Box::new(pdf) as Box<dyn PdfSource>),
            None => Err(ExtractionError::Unreadable {
                message: format!("no fake entry for {}", path.display()),
            }),
        }
    }
}

/// A [`Source`] whose name and canned response are fixed at
/// construction, following the shape of the one in `dispatch.rs`.
struct FakeSource {
    name: SourceName,
    response: Result<Record, SourceError>,
}

impl Source for FakeSource {
    fn name(&self) -> SourceName {
        self.name
    }

    fn supports(&self, _identifier: &Identifier) -> bool {
        true
    }

    fn fetch(&self, _identifier: &Identifier) -> Result<Record, SourceError> {
        self.response.clone()
    }
}

fn fake_source(name: SourceName, response: Result<Record, SourceError>) -> FakeSource {
    FakeSource { name, response }
}

/// A [`Filesystem`] fake backed by a map from directory to the names
/// present there, following the shape of the one in `dispatch.rs`. Every
/// [`Filesystem::rename`] call is recorded in order.
struct FakeFilesystem {
    existing: BTreeMap<PathBuf, BTreeMap<String, Option<String>>>,
    renames: RefCell<Vec<(PathBuf, PathBuf)>>,
}

impl FakeFilesystem {
    fn new() -> FakeFilesystem {
        FakeFilesystem {
            existing: BTreeMap::new(),
            renames: RefCell::new(Vec::new()),
        }
    }

    /// The same filesystem with `names` present in `directory`.
    ///
    /// A ledger entry names a file the collection is said to hold, and
    /// what decides whether it still holds it is this map: an entry
    /// whose file is absent records an admission that no longer stands.
    fn with_existing(mut self, directory: &str, names: &[&str]) -> FakeFilesystem {
        self.existing.insert(
            PathBuf::from(directory),
            names
                .iter()
                .map(|name| ((*name).to_string(), None))
                .collect(),
        );
        self
    }

    fn renames(&self) -> Vec<(PathBuf, PathBuf)> {
        self.renames.borrow().clone()
    }
}

impl Filesystem for FakeFilesystem {
    fn existing(&self, directory: &Path) -> BTreeMap<String, Option<String>> {
        self.existing.get(directory).cloned().unwrap_or_default()
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), RenameError> {
        self.renames
            .borrow_mut()
            .push((from.to_path_buf(), to.to_path_buf()));
        Ok(())
    }
}

/// A [`Journal`] fake recording every append, following the shape of the
/// one in `dispatch.rs`. Nothing in this file reads a journal back.
struct FakeJournal {
    appended: RefCell<Vec<JournalEntry>>,
}

impl FakeJournal {
    fn new() -> FakeJournal {
        FakeJournal {
            appended: RefCell::new(Vec::new()),
        }
    }
}

impl Journal for FakeJournal {
    fn append(&self, entries: &[JournalEntry]) -> std::io::Result<()> {
        self.appended.borrow_mut().extend_from_slice(entries);
        Ok(())
    }

    fn read(&self) -> Vec<JournalEntry> {
        Vec::new()
    }
}

/// A [`BibFiles`] fake that reads nothing and records nothing — none of
/// these tests configure a bibliography destination.
struct FakeBibFiles;

impl BibFiles for FakeBibFiles {
    fn read(&self, _path: &Path) -> std::io::Result<String> {
        Ok(String::new())
    }

    fn write(&self, _path: &Path, _content: &str) -> std::io::Result<()> {
        Ok(())
    }
}

/// An `Article` by one author in the given year, carrying `doi_value`.
/// Renders as `"{family}{year}"` under the `[auth][year]` template these
/// tests use, following the shape of `record_by` in `dispatch.rs`.
fn record_by(family: &str, year: i32, doi_value: &str) -> Record {
    Record {
        authors: vec![Name {
            family: family.to_string(),
            given: None,
        }],
        issued: Some(DateParts {
            year,
            month: None,
            day: None,
        }),
        doi: Some(doi(doi_value)),
        ..record_of(EntryType::Article)
    }
}

/// The `now` every fixture in this section uses: a fixed string, so a
/// ledger entry's timestamp and run id are pinned rather than depending
/// on the clock.
fn fixed_now() -> String {
    "2024-01-01T00:00:00Z".to_string()
}

/// An [`borax::config::Effective`] whose default template is `template`
/// and which is otherwise the built-in defaults, following the shape of
/// the one in `dispatch.rs`.
fn effective_with_default_template(template: &str) -> borax::config::Effective {
    effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            template.to_string(),
        )]));
    })
}

/// [`effective_with_default_template`] with the run log switched off.
///
/// These fixtures use `collection_root: Some("/lib")` as a stand-in
/// collection that is never actually on disk — fine for a `Ledger`
/// fake, which never touches it, but a real path an optional run log
/// would now try to write under. Turning the run log off keeps these
/// tests' stderr assertions about the ledger's own warnings, undiluted
/// by an unrelated "could not write the run log" one.
fn effective_with_default_template_and_no_run_log(template: &str) -> borax::config::Effective {
    effective_with(|layer| {
        layer.run_log = Some(false);
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            template.to_string(),
        )]));
    })
}

/// An [`borax::config::Effective`] built from a single layer, for tests
/// that need to steer one or two settings away from the built-in
/// defaults, following the shape of the one in `dispatch.rs`.
fn effective_with(customize: impl FnOnce(&mut Layer)) -> borax::config::Effective {
    let mut layer = Layer::default();
    customize(&mut layer);
    resolve(vec![(Origin::Flag("test".to_string()), layer)]).unwrap()
}

/// A [`Cli`] running `command` in the given output format, following the
/// shape of the one in `dispatch.rs`.
fn cli(command: Command, json: bool) -> Cli {
    Cli {
        command,
        settings: Settings::default(),
        json,
    }
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

// ---------------------------------------------------------------------
// 2.5: the ledger and the collection root on `Adapters` — a `rename`
// run's duplicate checks and applied admissions, driven end to end
// through `events_for`/`dispatch`.
// ---------------------------------------------------------------------

/// design "Duplicate detection is two distinct checks": the content
/// check runs before resolution, so a byte-identical duplicate is caught
/// without opening the file, is reported with the existing file's full
/// path, and the source file itself is left where it is.
#[test]
fn a_content_duplicate_is_skipped_with_the_existing_files_full_path_and_the_source_untouched() {
    let path = PathBuf::from("/lib/original.pdf");
    let hash = hash_of("dup-content");
    let library = FakeLibrary::new().with_open_error(
        &path,
        hash,
        ExtractionError::Unreadable {
            message: "a content duplicate must never be opened".to_string(),
        },
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new().with_existing("/lib", &["Smith2024.pdf"]);
    let bib_files = FakeBibFiles;
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[entry("Smith2024.pdf", "dup-content")]),
        warning: None,
    });
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/lib")),
        state_root: None,
    };

    let events = events_for(
        &Command::Rename {
            paths: vec![path.clone()],
            apply: false,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert_eq!(
        events,
        vec![Event::Skipped {
            path,
            reason: SkipReason::Duplicate {
                reason: DuplicateReason::Content,
                existing_path: PathBuf::from("/lib/Smith2024.pdf"),
            },
        }],
        "got {events:?}"
    );
    assert!(filesystem.renames().is_empty());
}

/// design "Duplicate detection is two distinct checks": a work duplicate
/// is only visible after resolution, since it is the resolved record's
/// identifier — not the incoming file's hash — that matches the ledger.
#[test]
fn a_work_duplicate_is_skipped_with_the_work_reason() {
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("work-dup-incoming"),
        pdf_with_embedded_doi("10.1000/work-dup"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/work-dup")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new().with_existing("/lib", &["Archived.pdf"]);
    let bib_files = FakeBibFiles;
    let existing = Entry {
        doi: Some(doi("10.1000/work-dup")),
        ..entry("Archived.pdf", "work-dup-existing")
    };
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[existing]),
        warning: None,
    });
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/lib")),
        state_root: None,
    };

    let events = events_for(
        &Command::Rename {
            paths: vec![path.clone()],
            apply: false,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert_eq!(
        events,
        vec![Event::Skipped {
            path,
            reason: SkipReason::Duplicate {
                reason: DuplicateReason::Work,
                existing_path: PathBuf::from("/lib/Archived.pdf"),
            },
        }],
        "got {events:?}"
    );
}

/// ledger spec scenario "Duplicate of an undone admission", exercised
/// through a whole `rename` run rather than at `duplicate_is_live`'s own
/// level: a ledger entry whose file is no longer on disk must not keep a
/// re-admission out, so the file is resolved and planned exactly as it
/// would be with no ledger at all.
#[test]
fn a_duplicate_whose_recorded_file_is_gone_is_processed_normally() {
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("stale-dup"),
        pdf_with_embedded_doi("10.1000/stale-dup"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/stale-dup")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    // "Gone.pdf" is recorded in the ledger but the fake filesystem
    // reports nothing under that name, so the admission it names no
    // longer holds.
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[entry("Gone.pdf", "stale-dup")]),
        warning: None,
    });
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/lib")),
        state_root: None,
    };

    let events = events_for(
        &Command::Rename {
            paths: vec![path.clone()],
            apply: false,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            Event::Resolved {
                path: path.clone(),
                identifier: "doi:10.1000/stale-dup".to_string(),
                record: Box::new(record_by("Smith", 2024, "10.1000/stale-dup")),
                source: "crossref".to_string(),
                tier: Some("embedded-metadata".to_string()),
                cached: false,
            },
            Event::Planned {
                path,
                target: PathBuf::from("/lib/Smith2024.pdf"),
            },
        ],
        "an undone admission must not veto re-admission, got {events:?}"
    );
}

/// ledger spec scenario "Duplicate of an undone admission", the warning
/// half: "the run warns that the ledger holds stale entries" and the
/// warning suggests `borax ledger rebuild`. `events_for` cannot observe
/// this — the warning is a stderr [`borax::event::Diagnostic`], never an
/// event — so this drives the run through `dispatch`, matching the
/// absent-ledger / unparsable-ledger warning tests above.
#[test]
fn a_stale_duplicate_warns_that_the_ledger_holds_stale_entries() {
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("stale-warns"),
        pdf_with_embedded_doi("10.1000/stale-warns"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/stale-warns")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    // "Gone.pdf" is recorded in the ledger but the fake filesystem
    // reports nothing under that name, so the matched entry is stale.
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[entry("Gone.pdf", "stale-warns")]),
        warning: None,
    });
    let effective = effective_with_default_template_and_no_run_log("[auth][year]");
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/lib")),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::Rename {
                paths: vec![path.clone()],
                apply: false,
            },
            true,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    let err_text = String::from_utf8(err).unwrap();
    let lines: Vec<&str> = err_text.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one stale-entry warning, got {err_text:?}"
    );
    assert!(lines[0].starts_with("warning:"), "got {err_text:?}");
    assert!(
        lines[0].to_lowercase().contains("stale"),
        "the warning must say the ledger holds stale entries, got {err_text:?}"
    );
    assert!(
        lines[0].contains("borax ledger rebuild"),
        "the warning must suggest borax ledger rebuild, got {err_text:?}"
    );

    let out_text = String::from_utf8(out).unwrap();
    let events: Vec<Event> = out_text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Planned { path: planned, .. } if planned == &path)),
        "a stale duplicate must still be processed normally, got {events:?}"
    );
}

/// A matched entry whose file IS present is a live duplicate, not a
/// stale one, so nothing about staleness is said.
#[test]
fn a_live_duplicate_emits_no_stale_warning() {
    let path = PathBuf::from("/lib/original.pdf");
    let hash = hash_of("live-dup-no-warning");
    let library = FakeLibrary::new().with_open_error(
        &path,
        hash,
        ExtractionError::Unreadable {
            message: "a content duplicate must never be opened".to_string(),
        },
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new().with_existing("/lib", &["Smith2024.pdf"]);
    let bib_files = FakeBibFiles;
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[entry("Smith2024.pdf", "live-dup-no-warning")]),
        warning: None,
    });
    let effective = effective_with(|layer| {
        layer.run_log = Some(false);
    });
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/lib")),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::Rename {
                paths: vec![path],
                apply: false,
            },
            true,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert!(
        err.is_empty(),
        "a live duplicate must not warn about staleness: {:?}",
        String::from_utf8_lossy(&err)
    );
}

/// A file matching no ledger entry at all is a miss, not staleness, so
/// nothing about stale entries is said.
#[test]
fn no_match_emits_no_stale_warning() {
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("no-match-incoming"),
        pdf_with_embedded_doi("10.1000/no-match"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/no-match")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new().with_existing("/lib", &["Unrelated.pdf"]);
    let bib_files = FakeBibFiles;
    // An entry for an unrelated file and identifier: the incoming file
    // matches nothing here, by hash or by identifier.
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[entry("Unrelated.pdf", "unrelated-seed")]),
        warning: None,
    });
    let effective = effective_with_default_template_and_no_run_log("[auth][year]");
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/lib")),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::Rename {
                paths: vec![path],
                apply: false,
            },
            true,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert!(
        err.is_empty(),
        "a miss is not staleness, must not warn: {:?}",
        String::from_utf8_lossy(&err)
    );
}

/// design: the ledger is read once per run and has at most one thing to
/// say about it — several files each hitting a stale entry still
/// produce exactly one warning, not one per file.
#[test]
fn several_stale_duplicates_in_one_run_still_produce_exactly_one_warning() {
    let first = PathBuf::from("/lib/first.pdf");
    let second = PathBuf::from("/lib/second.pdf");
    let library = FakeLibrary::new()
        .with_file(
            &first,
            hash_of("stale-first"),
            pdf_with_embedded_doi("10.1000/stale-several"),
        )
        .with_file(
            &second,
            hash_of("stale-second"),
            pdf_with_embedded_doi("10.1000/stale-several"),
        );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/stale-several")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    // Neither "Gone1.pdf" nor "Gone2.pdf" is reported present, so both
    // matched entries are stale.
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[
            entry("Gone1.pdf", "stale-first"),
            entry("Gone2.pdf", "stale-second"),
        ]),
        warning: None,
    });
    let effective = effective_with_default_template_and_no_run_log("[auth][year]");
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/lib")),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::Rename {
                paths: vec![first, second],
                apply: false,
            },
            true,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    let err_text = String::from_utf8(err).unwrap();
    let lines: Vec<&str> = err_text.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "two files hitting stale entries must still warn once, got {err_text:?}"
    );

    let out_text = String::from_utf8(out).unwrap();
    let events: Vec<Event> = out_text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        !events.iter().any(|event| matches!(
            event,
            Event::Skipped {
                reason: SkipReason::Duplicate { .. },
                ..
            }
        )),
        "a stale entry must not be reported as a duplicate, got {events:?}"
    );
}

/// design "The ledger is an append-only JSONL file": an applied rename
/// records the file's new path relative to the collection root — not
/// relative to the directory it happened to be renamed within — and
/// `/`-separated whatever the platform.
#[test]
fn an_applied_rename_appends_the_files_new_path_relative_to_the_collection_root() {
    let path = PathBuf::from("/collection/sub/original.pdf");
    let hash = hash_of("apply-admission");
    let library = FakeLibrary::new().with_file(
        &path,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/apply-admission"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/apply-admission")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let journal = FakeJournal::new();
    let bib_files = FakeBibFiles;
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: None,
    });
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: Some(&journal as &dyn Journal),
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/collection")),
        state_root: None,
    };

    let events = events_for(
        &Command::Rename {
            paths: vec![path.clone()],
            apply: true,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert_eq!(
        events,
        vec![
            Event::Resolved {
                path: path.clone(),
                identifier: "doi:10.1000/apply-admission".to_string(),
                record: Box::new(record_by("Smith", 2024, "10.1000/apply-admission")),
                source: "crossref".to_string(),
                tier: Some("embedded-metadata".to_string()),
                cached: false,
            },
            Event::Renamed {
                path,
                target: PathBuf::from("/collection/sub/Smith2024.pdf"),
                hash: hash.clone(),
            },
        ],
        "got {events:?}"
    );
    assert_eq!(
        ledger.appended(),
        vec![Entry {
            hash,
            doi: Some(doi("10.1000/apply-admission")),
            arxiv: None,
            pmid: None,
            isbn: None,
            path: "sub/Smith2024.pdf".to_string(),
            entry_type: EntryType::Article,
            run: RunId::new(fixed_now()),
            timestamp: fixed_now(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
        }],
        "got {:?}",
        ledger.appended()
    );
}

/// design "Appends happen only on applied admissions": a preview run
/// plans the identical rename but writes nothing to the ledger.
#[test]
fn a_preview_run_appends_nothing_to_the_ledger_even_when_it_plans_a_rename() {
    let path = PathBuf::from("/collection/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("preview-no-append"),
        pdf_with_embedded_doi("10.1000/preview-no-append"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/preview-no-append")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: None,
    });
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/collection")),
        state_root: None,
    };

    let events = events_for(
        &Command::Rename {
            paths: vec![path],
            apply: false,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert!(
        matches!(events.last(), Some(Event::Planned { .. })),
        "expected a planned rename, got {events:?}"
    );
    assert!(
        ledger.appended().is_empty(),
        "a preview must append nothing, got {:?}",
        ledger.appended()
    );
}

/// design "`--no-ledger` disables it explicitly": with the ledger
/// disabled, a file whose hash the ledger already holds is neither
/// reported as a duplicate nor is anything appended for it.
#[test]
fn a_disabled_ledger_neither_checks_nor_appends() {
    let path = PathBuf::from("/collection/original.pdf");
    let hash = hash_of("disabled-ledger");
    let library = FakeLibrary::new().with_file(
        &path,
        hash,
        pdf_with_embedded_doi("10.1000/disabled-ledger"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/disabled-ledger")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let journal = FakeJournal::new();
    let bib_files = FakeBibFiles;
    // The incoming file's hash is already in the ledger — if duplicate
    // detection ran despite being disabled, this would be reported as a
    // content duplicate rather than renamed.
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[entry("Smith2024.pdf", "disabled-ledger")]),
        warning: None,
    });
    let effective = effective_with(|layer| {
        layer.ledger = Some(false);
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            "[auth][year]".to_string(),
        )]));
    });
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: Some(&journal as &dyn Journal),
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/collection")),
        state_root: None,
    };

    let events = events_for(
        &Command::Rename {
            paths: vec![path],
            apply: true,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert!(
        matches!(events.last(), Some(Event::Renamed { .. })),
        "a disabled ledger must not report the file as a duplicate, got {events:?}"
    );
    assert!(
        ledger.appended().is_empty(),
        "a disabled ledger must not be written to, got {:?}",
        ledger.appended()
    );
}

/// design "Outside a collection... the ledger is simply inactive": with
/// no collection root and no ledger, a run does the same as a disabled
/// one — no check, no append — and, unlike a ledger that failed to load,
/// says nothing about it on stderr.
#[test]
fn outside_a_collection_the_run_checks_nothing_appends_nothing_and_warns_nothing() {
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("no-collection"),
        pdf_with_embedded_doi("10.1000/no-collection"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/no-collection")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = effective_with_default_template("[auth][year]");
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::Rename {
                paths: vec![path],
                apply: false,
            },
            false,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert!(
        err.is_empty(),
        "a run with no ledger at all must not warn: {:?}",
        String::from_utf8_lossy(&err)
    );
}

/// design "degrades loudly": an absent ledger earns exactly one warning
/// line on stderr, and the run's events are unaffected by it.
#[test]
fn an_absent_ledger_warns_exactly_once_and_the_run_proceeds_unaffected() {
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("absent-ledger"),
        pdf_with_embedded_doi("10.1000/absent-ledger"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/absent-ledger")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: Some(LedgerWarning::Absent),
    });
    let effective = effective_with_default_template_and_no_run_log("[auth][year]");
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/lib")),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::Rename {
                paths: vec![path.clone()],
                apply: false,
            },
            true,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    let err_text = String::from_utf8(err).unwrap();
    let lines: Vec<&str> = err_text.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one warning, got {err_text:?}"
    );
    assert!(lines[0].starts_with("warning:"), "got {err_text:?}");

    let out_text = String::from_utf8(out).unwrap();
    let events: Vec<Event> = out_text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Planned { path: planned, .. } if planned == &path)),
        "the run must still plan the rename despite the warning, got {events:?}"
    );
}

/// design "degrades loudly": an unparsable ledger warns the same way an
/// absent one does — exactly once, run unaffected.
#[test]
fn an_unparsable_ledger_warns_exactly_once_and_the_run_proceeds_unaffected() {
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("unparsable-ledger"),
        pdf_with_embedded_doi("10.1000/unparsable-ledger"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/unparsable-ledger")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: Some(LedgerWarning::Unparsable(Unparsable { line: 3 })),
    });
    let effective = effective_with_default_template_and_no_run_log("[auth][year]");
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/lib")),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::Rename {
                paths: vec![path.clone()],
                apply: false,
            },
            true,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    let err_text = String::from_utf8(err).unwrap();
    let lines: Vec<&str> = err_text.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one warning, got {err_text:?}"
    );
    assert!(lines[0].starts_with("warning:"), "got {err_text:?}");

    let out_text = String::from_utf8(out).unwrap();
    let events: Vec<Event> = out_text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Planned { path: planned, .. } if planned == &path)),
        "the run must still plan the rename despite the warning, got {events:?}"
    );
}

/// cli spec: a skipped duplicate is a skip like any other, so it counts
/// toward the run's totals and its exit is `PARTIAL` rather than
/// `SUCCESS`.
#[test]
fn a_content_duplicate_skip_counts_toward_a_partial_outcome() {
    let path = PathBuf::from("/lib/original.pdf");
    let hash = hash_of("partial-dup");
    let library = FakeLibrary::new().with_open_error(
        &path,
        hash,
        ExtractionError::Unreadable {
            message: "a content duplicate must never be opened".to_string(),
        },
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new().with_existing("/lib", &["Smith2024.pdf"]);
    let bib_files = FakeBibFiles;
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[entry("Smith2024.pdf", "partial-dup")]),
        warning: None,
    });
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(PathBuf::from("/lib")),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(
            Command::Rename {
                paths: vec![path],
                apply: false,
            },
            true,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Partial, "got {outcome:?}");
    let out_text = String::from_utf8(out).unwrap();
    let events: Vec<Event> = out_text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let Some(Event::RunFinished { counts }) = events.last() else {
        panic!("expected the last event to be RunFinished, got {events:?}")
    };
    assert_eq!(counts.skipped, 1, "got {counts:?}");
}

// ---------------------------------------------------------------------
// 2.6: `borax ledger rebuild` dispatched — scan_collection + rebuild +
// ledger.replace, wired behind `preflight`/`emit_events` the same way
// `borax cache` is: the whole work is one event, and the work is what
// can fail.
// ---------------------------------------------------------------------

/// A rebuild over a collection of PDFs-with-sidecars replaces the
/// ledger with exactly what `scan_collection` + `rebuild` — already
/// tested on their own above — produce from it, and reports the count.
#[test]
fn rebuild_replaces_the_ledger_with_one_entry_per_scanned_file_and_reports_the_count() {
    let dir = tempdir().unwrap();
    write_pdf_with_sidecar(
        dir.path(),
        "a.pdf",
        b"a-bytes",
        &record_of(EntryType::Article),
    );
    write_pdf_with_sidecar(dir.path(), "b.pdf", b"b-bytes", &record_of(EntryType::Book));

    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: None,
    });
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(dir.path().to_path_buf()),
        state_root: None,
    };

    let events = events_for(
        &Command::Ledger {
            action: LedgerAction::Rebuild,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert_eq!(
        events,
        vec![Event::LedgerRebuilt {
            root: dir.path().to_path_buf(),
            entries: 2,
        }],
        "got {events:?}"
    );
    assert_eq!(
        ledger.replaced(),
        Some(rebuild(
            &scan_collection(dir.path()),
            RunId::new(fixed_now()),
            &fixed_now(),
            env!("CARGO_PKG_VERSION"),
        )),
        "got {:?}",
        ledger.replaced()
    );
}

/// ledger spec scenario "Rebuild after manual deletions": an entry for a
/// file no longer on disk does not survive the rebuild.
#[test]
fn rebuild_compacts_away_entries_for_files_no_longer_on_disk() {
    let dir = tempdir().unwrap();
    let record = record_of(EntryType::Article);
    let a = write_pdf_with_sidecar(dir.path(), "a.pdf", b"a-bytes", &record);
    write_pdf_with_sidecar(dir.path(), "b.pdf", b"b-bytes", &record);
    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(borax::bib::sidecar_path(&a)).unwrap();

    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: None,
    });
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(dir.path().to_path_buf()),
        state_root: None,
    };

    let events = events_for(
        &Command::Ledger {
            action: LedgerAction::Rebuild,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert_eq!(
        events,
        vec![Event::LedgerRebuilt {
            root: dir.path().to_path_buf(),
            entries: 1,
        }],
        "got {events:?}"
    );
    let replaced = ledger
        .replaced()
        .unwrap_or_else(|| panic!("replace was never called"));
    assert_eq!(
        replaced
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        vec!["b.pdf"],
        "got {replaced:?}"
    );
}

/// ledger spec scenario "Rebuild is idempotent", proven against a real
/// [`FileLedger`] rather than a fake: a fake cannot show that the bytes
/// written are stable, only that the entries passed to it are.
#[test]
fn rebuilding_an_unchanged_collection_twice_through_dispatch_is_byte_identical() {
    let dir = tempdir().unwrap();
    let record = record_of(EntryType::Article);
    write_pdf_with_sidecar(dir.path(), "z.pdf", b"z-bytes", &record);
    write_pdf_with_sidecar(dir.path(), "a.pdf", b"a-bytes", &record);

    let ledger_path = dir.path().join(ACCOUNTING_DIR).join(LEDGER_FILE);
    let real_ledger = FileLedger::new(&ledger_path);
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&real_ledger),
        collection_root: Some(dir.path().to_path_buf()),
        state_root: None,
    };
    let command = Command::Ledger {
        action: LedgerAction::Rebuild,
    };

    events_for(&command, &Configs::uniform(effective.clone()), &adapters).unwrap();
    let first = std::fs::read(&ledger_path).unwrap();

    events_for(&command, &Configs::uniform(effective), &adapters).unwrap();
    let second = std::fs::read(&ledger_path).unwrap();

    assert_eq!(
        first, second,
        "two rebuilds of an unchanged collection must be byte identical"
    );
    assert_eq!(
        String::from_utf8(first).unwrap().lines().count(),
        2,
        "expected one line per scanned file"
    );
}

/// `scan_collection`'s own contract — a file with no sidecar, and one
/// whose sidecar carries no recoverable record, each contribute no
/// entry — has to survive being wired into the subcommand.
#[test]
fn rebuild_skips_files_scan_collection_would_not_count() {
    let dir = tempdir().unwrap();
    write_pdf_with_sidecar(
        dir.path(),
        "good.pdf",
        b"good-bytes",
        &record_of(EntryType::Article),
    );
    std::fs::write(dir.path().join("orphan.pdf"), b"bytes").unwrap();
    let broken_path = dir.path().join("broken.pdf");
    std::fs::write(&broken_path, b"bytes").unwrap();
    std::fs::write(borax::bib::sidecar_path(&broken_path), "@article{key,}\n").unwrap();

    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: None,
    });
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(dir.path().to_path_buf()),
        state_root: None,
    };

    let events = events_for(
        &Command::Ledger {
            action: LedgerAction::Rebuild,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert_eq!(
        events,
        vec![Event::LedgerRebuilt {
            root: dir.path().to_path_buf(),
            entries: 1,
        }],
        "got {events:?}"
    );
    let replaced = ledger
        .replaced()
        .unwrap_or_else(|| panic!("replace was never called"));
    assert_eq!(
        replaced
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        vec!["good.pdf"],
        "got {replaced:?}"
    );
}

/// Outside any collection there is nothing to scan and nowhere for a
/// rebuilt ledger to go, so the run is refused before anything is
/// written — the same shape as `borax cache` with no cache directory.
#[test]
fn rebuild_outside_a_collection_is_refused_and_writes_nothing() {
    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: None,
    });
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: None,
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(
            Command::Ledger {
                action: LedgerAction::Rebuild,
            },
            false,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Fatal, "got {outcome:?}");
    assert!(
        out.is_empty(),
        "a refused rebuild must write no event stream: {:?}",
        String::from_utf8_lossy(&out)
    );
    assert!(
        !err.is_empty(),
        "expected a diagnostic naming the missing collection"
    );
    assert!(
        ledger.replaced().is_none(),
        "nothing must be written when there is no collection to rebuild"
    );
}

/// A ledger that cannot be written refuses the run rather than
/// reporting a clean rebuild that never landed — the same shape as
/// `borax cache` over an unreadable cache directory.
#[test]
fn a_failing_replace_refuses_the_run_rather_than_reporting_a_clean_rebuild() {
    let dir = tempdir().unwrap();
    write_pdf_with_sidecar(
        dir.path(),
        "a.pdf",
        b"a-bytes",
        &record_of(EntryType::Article),
    );

    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: None,
    })
    .with_replace_failure("disk full");
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(dir.path().to_path_buf()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(
            Command::Ledger {
                action: LedgerAction::Rebuild,
            },
            false,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Fatal, "got {outcome:?}");
    assert!(
        out.is_empty(),
        "a failed rebuild must not report success: {:?}",
        String::from_utf8_lossy(&out)
    );
    let err_text = String::from_utf8(err).unwrap();
    assert!(!err_text.is_empty(), "expected a diagnostic on stderr");
}

/// Precedent: `--no-cache` governs the pipeline's use of the response
/// cache and has no bearing on `borax cache`. `ledger rebuild` is an
/// explicit instruction to do ledger work, so `ledger = false` /
/// `--no-ledger` must not stop it either.
#[test]
fn a_disabled_ledger_setting_does_not_prevent_a_rebuild() {
    let dir = tempdir().unwrap();
    write_pdf_with_sidecar(
        dir.path(),
        "a.pdf",
        b"a-bytes",
        &record_of(EntryType::Article),
    );

    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: None,
    });
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = effective_with(|layer| {
        layer.ledger = Some(false);
    });
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(dir.path().to_path_buf()),
        state_root: None,
    };

    let events = events_for(
        &Command::Ledger {
            action: LedgerAction::Rebuild,
        },
        &Configs::uniform(effective),
        &adapters,
    )
    .unwrap();

    assert_eq!(
        events,
        vec![Event::LedgerRebuilt {
            root: dir.path().to_path_buf(),
            entries: 1,
        }],
        "a disabled ledger setting must not stop an explicit rebuild, got {events:?}"
    );
    assert!(
        ledger.replaced().is_some(),
        "the rebuild must still write, even with ledger = false"
    );
}

/// The rebuild's `--json` line is the event stream and nothing else: it
/// carries the schema field every other event does, under the
/// `ledger-rebuilt` tag, with no diagnostic mixed onto stdout.
#[test]
fn dispatching_a_rebuild_writes_ledger_rebuilt_on_stdout_carrying_the_schema_field() {
    let dir = tempdir().unwrap();
    write_pdf_with_sidecar(
        dir.path(),
        "a.pdf",
        b"a-bytes",
        &record_of(EntryType::Article),
    );

    let ledger = FakeLedger::new(Loaded {
        index: Index::build(&[]),
        warning: None,
    });
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        journal: None,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: Some(&ledger),
        collection_root: Some(dir.path().to_path_buf()),
        state_root: None,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::Ledger {
                action: LedgerAction::Rebuild,
            },
            true,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert!(
        err.is_empty(),
        "a clean rebuild must not write to stderr: {:?}",
        String::from_utf8_lossy(&err)
    );
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let rebuilt = lines
        .iter()
        .find(|line| line["event"] == "ledger-rebuilt")
        .unwrap_or_else(|| panic!("no ledger-rebuilt line in {lines:?}"));
    assert_eq!(rebuilt["schema"], serde_json::json!(1));
    assert_eq!(rebuilt["entries"], serde_json::json!(1));
    assert_eq!(rebuilt["root"], serde_json::to_value(dir.path()).unwrap());
}
