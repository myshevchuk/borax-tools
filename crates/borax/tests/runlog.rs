#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use borax::bib::BibFiles;
use borax::cli::{Cli, Command, LedgerAction, Settings};
use borax::config::{Layer, Origin, resolve};
use borax::ledger::ACCOUNTING_DIR;
use borax::pipeline::Library;
use borax::renaming::{Filesystem, RenameError};
use borax::run::{Adapters, Configs, Streams, dispatch};
use borax::runlog::{RUNS_DIR, destination, log_name, state_root};
use borax::session::Outcome;
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{Doi, Identifier};
use borax_core::record::{DateParts, EntryType, Name, Record};
use borax_pdf::source::{ExtractionError, InfoMetadata, PdfSource};
use borax_sources::cache::MemoryCache;
use borax_sources::source::{Source, SourceError, SourceName};
use borax_sources::store::ContentIndex;
use tempfile::tempdir;

// ---------------------------------------------------------------------
// Fakes, following the shape of the ones in `tests/ledger.rs` and
// `tests/dispatch.rs`.
// ---------------------------------------------------------------------

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

struct LibraryEntry {
    hash: Result<ContentHash, ExtractionError>,
    pdf: Result<FakePdf, ExtractionError>,
}

struct FakeLibrary {
    entries: BTreeMap<PathBuf, LibraryEntry>,
}

impl FakeLibrary {
    fn new() -> FakeLibrary {
        FakeLibrary {
            entries: BTreeMap::new(),
        }
    }

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

struct FakeFilesystem {
    renames: RefCell<Vec<(PathBuf, PathBuf)>>,
}

impl FakeFilesystem {
    fn new() -> FakeFilesystem {
        FakeFilesystem {
            renames: RefCell::new(Vec::new()),
        }
    }

    fn renames(&self) -> Vec<(PathBuf, PathBuf)> {
        self.renames.borrow().clone()
    }
}

impl Filesystem for FakeFilesystem {
    fn existing(&self, _directory: &Path) -> BTreeMap<String, Option<String>> {
        BTreeMap::new()
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), RenameError> {
        self.renames
            .borrow_mut()
            .push((from.to_path_buf(), to.to_path_buf()));
        Ok(())
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

// ---------------------------------------------------------------------
// Other helpers
// ---------------------------------------------------------------------

fn doi(value: &str) -> Doi {
    Doi::parse(value).unwrap()
}

fn hash_of(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// An `Article` by one author in the given year, carrying `doi_value`.
/// Renders as `"{family}{year}"` under the `[auth][year]` template these
/// tests use.
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
        ..Record::new(EntryType::Article)
    }
}

/// The `now` most fixtures in this file use: a fixed *extended* ISO
/// string, deliberately not already in basic form, so a filename built
/// from it also exercises `log_name`'s normalization.
fn fixed_now() -> String {
    "2024-01-01T00:00:00Z".to_string()
}

fn effective_with(customize: impl FnOnce(&mut Layer)) -> borax::config::Effective {
    let mut layer = Layer::default();
    customize(&mut layer);
    resolve(vec![(Origin::Flag("test".to_string()), layer)]).unwrap()
}

fn effective_with_default_template(template: &str) -> borax::config::Effective {
    effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            template.to_string(),
        )]));
    })
}

fn cli(command: Command, json: bool) -> Cli {
    Cli {
        command,
        settings: Settings::default(),
        json,
    }
}

/// Every file directly under `runs`, by name, sorted.
fn run_log_names(runs: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(runs)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

// ---------------------------------------------------------------------
// 3.1: log_name() — pure filename construction
// ---------------------------------------------------------------------

#[test]
fn log_name_of_an_already_basic_timestamp() {
    assert_eq!(
        log_name("20240101T000000Z", "rename", true),
        "20240101T000000Z-rename-apply.jsonl"
    );
}

#[test]
fn log_name_strips_hyphens_and_colons_from_an_extended_iso_timestamp() {
    assert_eq!(
        log_name("2024-01-01T00:00:00Z", "rename", true),
        "20240101T000000Z-rename-apply.jsonl"
    );
}

#[test]
fn log_name_suffix_is_dry_when_not_applying() {
    assert_eq!(
        log_name("20240101T000000Z", "rename", false),
        "20240101T000000Z-rename-dry.jsonl"
    );
}

#[test]
fn log_name_turns_spaces_in_a_two_word_command_into_a_hyphen() {
    assert_eq!(
        log_name("20240101T000000Z", "ledger rebuild", true),
        "20240101T000000Z-ledger-rebuild-apply.jsonl"
    );
}

#[test]
fn log_name_never_contains_a_character_windows_rejects_in_a_filename() {
    let name = log_name("2024-01-01T00:00:00Z", "ledger rebuild", true);
    for forbidden in [':', '<', '>', '"', '/', '\\', '|', '?', '*'] {
        assert!(
            !name.contains(forbidden),
            "{forbidden:?} is not valid in a Windows filename, got {name:?}"
        );
    }
}

// ---------------------------------------------------------------------
// 3.1/3.2/3.3: destination() — placement and mandatoriness, pure
// ---------------------------------------------------------------------

#[test]
fn destination_of_an_apply_rename_in_a_collection_is_mandatory_under_the_collection_root() {
    let root = PathBuf::from("/collection");
    let command = Command::rename(vec![], true);

    let found = destination(&command, true, true, "20240101T000000Z", Some(&root), None)
        .unwrap_or_else(|| panic!("expected a destination"));

    assert!(found.mandatory);
    assert_eq!(
        found.path,
        root.join(ACCOUNTING_DIR)
            .join(RUNS_DIR)
            .join(log_name("20240101T000000Z", "rename", true))
    );
}

#[test]
fn destination_of_an_apply_rename_outside_a_collection_falls_back_to_the_state_root() {
    let state_root = PathBuf::from("/state");
    let command = Command::rename(vec![], true);

    let found = destination(
        &command,
        true,
        true,
        "20240101T000000Z",
        None,
        Some(&state_root),
    )
    .unwrap_or_else(|| panic!("expected a destination"));

    assert!(found.mandatory);
    assert_eq!(
        found.path,
        state_root
            .join(RUNS_DIR)
            .join(log_name("20240101T000000Z", "rename", true))
    );
}

#[test]
fn destination_of_an_apply_rename_with_neither_root_is_none() {
    let command = Command::rename(vec![], true);

    assert!(destination(&command, true, true, "20240101T000000Z", None, None).is_none());
}

#[test]
fn destination_of_an_apply_rename_is_written_even_when_run_log_is_disabled() {
    let root = PathBuf::from("/collection");
    let command = Command::rename(vec![], true);

    let found = destination(&command, true, false, "20240101T000000Z", Some(&root), None)
        .unwrap_or_else(|| panic!("an apply log cannot be disabled by the run-log setting"));

    assert!(found.mandatory);
}

#[test]
fn destination_of_a_non_apply_run_in_a_collection_with_run_log_on_is_optional() {
    let root = PathBuf::from("/collection");
    let command = Command::bib(vec![]);

    let found = destination(&command, true, true, "20240101T000000Z", Some(&root), None)
        .unwrap_or_else(|| panic!("expected a destination"));

    assert!(!found.mandatory);
    assert_eq!(
        found.path,
        root.join(ACCOUNTING_DIR)
            .join(RUNS_DIR)
            .join(log_name("20240101T000000Z", "bib", true))
    );
}

#[test]
fn destination_of_a_non_apply_run_with_run_log_off_is_none() {
    let root = PathBuf::from("/collection");
    let command = Command::bib(vec![]);

    assert!(destination(&command, true, false, "20240101T000000Z", Some(&root), None).is_none());
}

#[test]
fn destination_of_a_non_apply_run_outside_a_collection_is_none_even_with_a_state_root() {
    let state_root = PathBuf::from("/state");
    let command = Command::bib(vec![]);

    assert!(
        destination(
            &command,
            true,
            true,
            "20240101T000000Z",
            None,
            Some(&state_root)
        )
        .is_none(),
        "an optional run log has no XDG fallback"
    );
}

#[test]
fn destination_of_a_preview_rename_in_a_collection_is_optional_with_the_dry_suffix() {
    let root = PathBuf::from("/collection");
    let command = Command::rename(vec![], false);

    let found = destination(&command, false, true, "20240101T000000Z", Some(&root), None)
        .unwrap_or_else(|| panic!("expected a destination"));

    assert!(!found.mandatory);
    assert!(
        found.path.to_string_lossy().ends_with("rename-dry.jsonl"),
        "got {:?}",
        found.path
    );
}

/// design: mandatory means exactly `rename --apply`, not "the run
/// mutates something" — `ledger rebuild` mutates the ledger, but the
/// ledger is derived and rebuildable, so its log stays best-effort.
#[test]
fn destination_of_ledger_rebuild_is_optional_not_mandatory() {
    let root = PathBuf::from("/collection");
    let command = Command::Ledger {
        action: LedgerAction::Rebuild,
    };

    let found = destination(&command, true, true, "20240101T000000Z", Some(&root), None)
        .unwrap_or_else(|| panic!("expected a destination"));

    assert!(!found.mandatory, "only rename --apply is mandatory");
    assert!(
        found
            .path
            .to_string_lossy()
            .ends_with("ledger-rebuild-apply.jsonl"),
        "got {:?}",
        found.path
    );
}

// ---------------------------------------------------------------------
// destination() on Windows path shapes
//
// The cases above spell their roots POSIX-style (`/collection`,
// `/state`). Windows accepts those — rooted, merely drive-less — so
// they pass there without the composed path ever being one a Windows
// user would have. These are the same compositions over the roots
// Windows really produces: a drive-rooted collection, a
// `LOCALAPPDATA`-shaped state root, a collection on a UNC share, and
// a root that already ends in a separator — which is what
// `collection_root` hands back for a collection anchored at a drive
// root or a share root.
//
// Asserted as the whole string a user would see in an error message
// rather than through `join`, which would reproduce whatever separator
// the code chose and so agree with it either way.
// ---------------------------------------------------------------------

#[cfg(windows)]
#[test]
fn destination_in_a_drive_rooted_collection_is_under_that_drive_on_windows() {
    let root = PathBuf::from(r"C:\collection");
    let command = Command::rename(vec![], true);

    let found = destination(&command, true, true, "20240101T000000Z", Some(&root), None)
        .unwrap_or_else(|| panic!("expected a destination"));

    assert_eq!(
        found.path.to_string_lossy(),
        r"C:\collection\.borax\runs\20240101T000000Z-rename-apply.jsonl"
    );
    assert!(
        found.path.is_absolute(),
        "an apply log must not depend on the working directory"
    );
}

#[cfg(windows)]
#[test]
fn destination_outside_a_collection_lands_under_the_local_appdata_state_root_on_windows() {
    // The shape `state_root` builds from `LOCALAPPDATA` on Windows.
    let state_root = PathBuf::from(r"C:\Users\example\AppData\Local\borax\v1");
    let command = Command::rename(vec![], true);

    let found = destination(
        &command,
        true,
        true,
        "20240101T000000Z",
        None,
        Some(&state_root),
    )
    .unwrap_or_else(|| panic!("expected a destination"));

    assert_eq!(
        found.path.to_string_lossy(),
        r"C:\Users\example\AppData\Local\borax\v1\runs\20240101T000000Z-rename-apply.jsonl"
    );
}

/// A collection on a network share keeps its accounting on the share,
/// beside the files it is about, rather than anywhere local.
#[cfg(windows)]
#[test]
fn destination_in_a_collection_on_a_unc_share_stays_on_the_share_on_windows() {
    let root = PathBuf::from(r"\\server\share\collection");
    let command = Command::rename(vec![], true);

    let found = destination(&command, true, true, "20240101T000000Z", Some(&root), None)
        .unwrap_or_else(|| panic!("expected a destination"));

    assert_eq!(
        found.path.to_string_lossy(),
        r"\\server\share\collection\.borax\runs\20240101T000000Z-rename-apply.jsonl"
    );
    assert!(
        found.path.is_absolute(),
        "a UNC path is absolute; an apply log on a share must not depend on the working directory"
    );
}

/// A collection anchored at a drive root: the root already ends in a
/// separator, and the composition must not double it. `\\server\share\`
/// is the same shape and the same risk.
#[cfg(windows)]
#[test]
fn destination_in_a_collection_at_a_drive_root_does_not_double_the_separator_on_windows() {
    let root = PathBuf::from(r"C:\");
    let command = Command::rename(vec![], true);

    let found = destination(&command, true, true, "20240101T000000Z", Some(&root), None)
        .unwrap_or_else(|| panic!("expected a destination"));

    assert_eq!(
        found.path.to_string_lossy(),
        r"C:\.borax\runs\20240101T000000Z-rename-apply.jsonl"
    );
}

// ---------------------------------------------------------------------
// 3.1-3.3: dispatched end to end, against a real collection-root
// tempdir (the log write is real disk I/O, unlike the ledger's Adapter
// seam) with fake resolution/filesystem underneath.
// ---------------------------------------------------------------------

#[test]
fn run_log_contains_exactly_the_json_stdout_stream_including_framing_events() {
    let dir = tempdir().unwrap();
    let path = PathBuf::from("/lib/paper.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("run-log-equals-json"),
        pdf_with_embedded_doi("10.1000/run-log-equals-json"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/run-log-equals-json")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
        &cli(Command::bib(vec![path]), true),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert!(
        err.is_empty(),
        "a clean run must not warn: {:?}",
        String::from_utf8_lossy(&err)
    );
    let stdout_text = String::from_utf8(out).unwrap();
    assert!(!stdout_text.is_empty());

    let runs = dir.path().join(ACCOUNTING_DIR).join(RUNS_DIR);
    let names = run_log_names(&runs);
    assert_eq!(names.len(), 1, "got {names:?}");
    let log_text = std::fs::read_to_string(runs.join(&names[0])).unwrap();

    assert_eq!(
        log_text, stdout_text,
        "the run log must contain exactly the --json stream"
    );
}

#[test]
fn a_human_format_run_still_writes_a_json_run_log_identical_to_the_json_runs() {
    let json_dir = tempdir().unwrap();
    let human_dir = tempdir().unwrap();
    let path = PathBuf::from("/lib/paper.pdf");
    let record = record_by("Smith", 2024, "10.1000/human-still-json");

    for (dir, json) in [(&json_dir, true), (&human_dir, false)] {
        let library = FakeLibrary::new().with_file(
            &path,
            hash_of("human-still-json"),
            pdf_with_embedded_doi("10.1000/human-still-json"),
        );
        let crossref = fake_source(SourceName::Crossref, Ok(record.clone()));
        let sources: Vec<&dyn Source> = vec![&crossref];
        let index = ContentIndex::new(MemoryCache::new());
        let filesystem = FakeFilesystem::new();
        let bib_files = FakeBibFiles;
        let effective = resolve(Vec::new()).unwrap();
        let adapters = Adapters {
            library: &library,
            sources: &sources,
            index: &index,
            filesystem: &filesystem,
            bib_files: &bib_files,
            cache_root: None,
            now: fixed_now,
            ledger: None,
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
            &cli(Command::bib(vec![path.clone()]), json),
            &Configs::uniform(effective),
            &adapters,
            &mut streams,
        );
    }

    let json_runs = json_dir.path().join(ACCOUNTING_DIR).join(RUNS_DIR);
    let human_runs = human_dir.path().join(ACCOUNTING_DIR).join(RUNS_DIR);
    let json_names = run_log_names(&json_runs);
    let human_names = run_log_names(&human_runs);
    assert_eq!(json_names.len(), 1, "got {json_names:?}");
    assert_eq!(human_names.len(), 1, "got {human_names:?}");

    let json_log = std::fs::read_to_string(json_runs.join(&json_names[0])).unwrap();
    let human_log = std::fs::read_to_string(human_runs.join(&human_names[0])).unwrap();

    assert_eq!(
        json_log, human_log,
        "the record is the same JSONL regardless of the terminal format"
    );
}

#[test]
fn a_preview_followed_by_its_apply_leaves_two_files_that_sort_adjacently() {
    let dir = tempdir().unwrap();
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("dry-apply-pair"),
        pdf_with_embedded_doi("10.1000/dry-apply-pair"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/dry-apply-pair")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = effective_with_default_template("[auth][year]");

    let preview_adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: || "20240101T000000Z".to_string(),
        ledger: None,
        collection_root: Some(dir.path().to_path_buf()),
        state_root: None,
    };
    let mut preview_out = Vec::new();
    let mut preview_err = Vec::new();
    let mut preview_streams = Streams {
        out: &mut preview_out,
        err: &mut preview_err,
    };
    dispatch(
        &cli(Command::rename(vec![path.clone()], false), false),
        &Configs::uniform(effective.clone()),
        &preview_adapters,
        &mut preview_streams,
    );

    let apply_adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: || "20240101T000100Z".to_string(),
        ledger: None,
        collection_root: Some(dir.path().to_path_buf()),
        state_root: None,
    };
    let mut apply_out = Vec::new();
    let mut apply_err = Vec::new();
    let mut apply_streams = Streams {
        out: &mut apply_out,
        err: &mut apply_err,
    };
    dispatch(
        &cli(Command::rename(vec![path], true), false),
        &Configs::uniform(effective),
        &apply_adapters,
        &mut apply_streams,
    );

    let runs = dir.path().join(ACCOUNTING_DIR).join(RUNS_DIR);
    let names = run_log_names(&runs);

    assert_eq!(names.len(), 2, "got {names:?}");
    assert!(names[0].ends_with("rename-dry.jsonl"), "got {names:?}");
    assert!(names[1].ends_with("rename-apply.jsonl"), "got {names:?}");
}

/// The invariant that matters most in this group: a mandatory log that
/// cannot be created must abort the run before the first rename, not
/// after some of them.
#[test]
fn an_unwritable_mandatory_log_aborts_before_any_rename() {
    let dir = tempdir().unwrap();
    // ".borax" is a plain file rather than a directory, so nothing can
    // ever be created under it — a deterministic, cross-platform way to
    // force the run-log write to fail without touching permissions.
    std::fs::write(dir.path().join(ACCOUNTING_DIR), b"not a directory").unwrap();

    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("unwritable-log"),
        pdf_with_embedded_doi("10.1000/unwritable-log"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/unwritable-log")),
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
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
        &cli(Command::rename(vec![path], true), false),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Fatal, "got {outcome:?}");
    assert!(
        filesystem.renames().is_empty(),
        "no file may be renamed when the mandatory log cannot be created"
    );

    // The message has to name the thing that failed, and the thing that
    // failed is the run log. The journal it replaced is gone; a message
    // still naming it would send a user looking for a file this build
    // never writes.
    let message = String::from_utf8(err).unwrap();
    assert!(message.contains("run log"), "got {message:?}");
    assert!(message.contains(".jsonl"), "got {message:?}");
    assert!(
        !message.to_lowercase().contains("journal"),
        "the message must not name the removed journal: {message:?}"
    );
    assert!(
        out.is_empty(),
        "a refused run must write no event stream: {:?}",
        String::from_utf8_lossy(&out)
    );
}

/// The other way a mandatory log fails to open: the directories above
/// it are made without trouble and the file itself cannot be created,
/// because a directory already holds the name. Distinct from the case
/// above, where `.borax` is a file and nothing beneath it can be made
/// at all — this one gets as far as `File::create` and is refused
/// there, and must abort just as completely.
#[test]
fn a_mandatory_log_whose_own_name_is_taken_by_a_directory_aborts_before_any_rename() {
    let dir = tempdir().unwrap();
    // `fixed_now` is what the run stamps its log with, so the name is
    // known ahead of the run and can be occupied before it starts.
    let runs = dir.path().join(ACCOUNTING_DIR).join(RUNS_DIR);
    std::fs::create_dir_all(&runs).unwrap();
    std::fs::create_dir(runs.join(log_name(&fixed_now(), "rename", true))).unwrap();

    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("occupied-log"),
        pdf_with_embedded_doi("10.1000/occupied-log"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/occupied-log")),
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
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
        &cli(Command::rename(vec![path], true), false),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Fatal, "got {outcome:?}");
    assert!(
        filesystem.renames().is_empty(),
        "no file may be renamed when the mandatory log cannot be created"
    );
    let message = String::from_utf8(err).unwrap();
    assert!(message.contains("run log"), "got {message:?}");
    assert!(
        !message.to_lowercase().contains("journal"),
        "the message must not name the removed journal: {message:?}"
    );
}

#[test]
fn no_run_log_with_apply_still_writes_the_mandatory_log() {
    let dir = tempdir().unwrap();
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("no-run-log-apply"),
        pdf_with_embedded_doi("10.1000/no-run-log-apply"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/no-run-log-apply")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = effective_with(|layer| {
        layer.run_log = Some(false);
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
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
        &cli(Command::rename(vec![path], true), false),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
    let runs = dir.path().join(ACCOUNTING_DIR).join(RUNS_DIR);
    let names = run_log_names(&runs);
    assert_eq!(names.len(), 1, "got {names:?}");
    assert!(names[0].ends_with("rename-apply.jsonl"), "got {names:?}");
}

#[test]
fn no_run_log_on_a_preview_writes_nothing_and_the_run_still_succeeds() {
    let dir = tempdir().unwrap();
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("no-run-log-preview"),
        pdf_with_embedded_doi("10.1000/no-run-log-preview"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/no-run-log-preview")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = effective_with(|layer| {
        layer.run_log = Some(false);
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
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
        &cli(Command::rename(vec![path], false), false),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
    assert!(
        err.is_empty(),
        "a suppressed dry-run log is not a failure: {:?}",
        String::from_utf8_lossy(&err)
    );
    let runs = dir.path().join(ACCOUNTING_DIR).join(RUNS_DIR);
    let count = std::fs::read_dir(&runs)
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(count, 0, "--no-run-log on a preview must write nothing");
}

#[test]
fn a_failed_optional_log_warns_but_the_run_still_succeeds() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join(ACCOUNTING_DIR), b"not a directory").unwrap();

    let path = PathBuf::from("/lib/paper.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("failed-optional-log"),
        pdf_with_embedded_doi("10.1000/failed-optional-log"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/failed-optional-log")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem::new();
    let bib_files = FakeBibFiles;
    let effective = resolve(Vec::new()).unwrap();
    let adapters = Adapters {
        library: &library,
        sources: &sources,
        index: &index,
        filesystem: &filesystem,
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
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
        &cli(Command::bib(vec![path]), true),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(
        outcome,
        Outcome::Success,
        "a best-effort log failure must not fail the run, got {outcome:?}"
    );
    assert!(!err.is_empty(), "expected a warning about the run log");
    assert!(
        !out.is_empty(),
        "the run's own events must still be reported"
    );
}

#[test]
fn an_apply_rename_outside_a_collection_writes_its_log_under_the_state_root() {
    let state_dir = tempdir().unwrap();
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("xdg-fallback"),
        pdf_with_embedded_doi("10.1000/xdg-fallback"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/xdg-fallback")),
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
        bib_files: &bib_files,
        cache_root: None,
        now: fixed_now,
        ledger: None,
        collection_root: None,
        state_root: Some(state_dir.path().to_path_buf()),
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    let outcome = dispatch(
        &cli(Command::rename(vec![path], true), false),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
    let runs = state_dir.path().join(RUNS_DIR);
    let names = run_log_names(&runs);
    assert_eq!(names.len(), 1, "got {names:?}");
    assert!(names[0].ends_with("rename-apply.jsonl"), "got {names:?}");
}

#[test]
fn an_apply_rename_with_no_collection_and_no_state_root_is_refused_before_moving_anything() {
    let path = PathBuf::from("/lib/original.pdf");
    let library = FakeLibrary::new().with_file(
        &path,
        hash_of("no-root-at-all"),
        pdf_with_embedded_doi("10.1000/no-root-at-all"),
    );
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/no-root-at-all")),
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

    let outcome = dispatch(
        &cli(Command::rename(vec![path], true), false),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Fatal, "got {outcome:?}");
    assert!(filesystem.renames().is_empty());
    assert!(out.is_empty(), "a refused run must write no event stream");

    // This refusal is about placement rather than a failed write, so it
    // names no path — but it still has to say what was wanted and why.
    let message = String::from_utf8(err).unwrap();
    assert!(message.contains("--apply"), "got {message:?}");
    assert!(message.contains("record"), "got {message:?}");
    assert!(
        !message.to_lowercase().contains("journal"),
        "the message must not name the removed journal: {message:?}"
    );
}

// ---------------------------------------------------------------------
// state_root(): ported from `tests/journal.rs` — moved here with the
// function itself, since run logs are now its only consumer. The
// produced path is unchanged (".../borax/v1"), so logs written before
// this change are still found; "v1" is asserted as a literal here
// rather than through a re-exported constant, so these tests do not
// depend on whatever the implementer renames it to.
// ---------------------------------------------------------------------

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
fn state_root_honors_xdg_state_home_on_unix() {
    let root = state_root(env(&[
        ("XDG_STATE_HOME", "/base/home/.local/state"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home/.local/state")
                .join("borax")
                .join("v1")
        )
    );
}

#[cfg(unix)]
#[test]
fn state_root_falls_back_to_home_dot_local_state_on_unix() {
    let root = state_root(env(&[("HOME", "/base/home")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home")
                .join(".local")
                .join("state")
                .join("borax")
                .join("v1")
        )
    );
}

#[cfg(unix)]
#[test]
fn state_root_prefers_xdg_state_home_over_home_on_unix() {
    let root = state_root(env(&[
        ("XDG_STATE_HOME", "/xdg/state"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        root,
        Some(PathBuf::from("/xdg/state").join("borax").join("v1"))
    );
}

#[cfg(unix)]
#[test]
fn state_root_skips_an_empty_xdg_state_home_on_unix() {
    let root = state_root(env(&[("XDG_STATE_HOME", ""), ("HOME", "/base/home")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home")
                .join(".local")
                .join("state")
                .join("borax")
                .join("v1")
        )
    );
}

#[cfg(unix)]
#[test]
fn state_root_skips_a_relative_xdg_state_home_on_unix() {
    let root = state_root(env(&[
        ("XDG_STATE_HOME", "relative/state"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from("/base/home")
                .join(".local")
                .join("state")
                .join("borax")
                .join("v1")
        )
    );
}

#[cfg(unix)]
#[test]
fn state_root_is_none_when_nothing_qualifies_on_unix() {
    assert_eq!(state_root(env(&[])), None);
}

#[cfg(unix)]
#[test]
fn state_root_is_none_for_an_empty_home_with_no_xdg_state_home_on_unix() {
    assert_eq!(state_root(env(&[("HOME", "")])), None);
}

#[cfg(unix)]
#[test]
fn state_root_is_none_for_a_relative_home_with_no_xdg_state_home_on_unix() {
    assert_eq!(state_root(env(&[("HOME", "relative")])), None);
}

#[cfg(windows)]
#[test]
fn state_root_honors_localappdata_on_windows() {
    let root = state_root(env(&[("LOCALAPPDATA", r"C:\Users\test\AppData\Local")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\AppData\Local")
                .join("borax")
                .join("v1")
        )
    );
}

#[cfg(windows)]
#[test]
fn state_root_falls_back_to_xdg_state_home_on_windows() {
    let root = state_root(env(&[("XDG_STATE_HOME", r"C:\Users\test\.state")]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\.state")
                .join("borax")
                .join("v1")
        )
    );
}

#[cfg(windows)]
#[test]
fn state_root_prefers_localappdata_over_xdg_state_home_on_windows() {
    let root = state_root(env(&[
        ("LOCALAPPDATA", r"C:\Users\test\AppData\Local"),
        ("XDG_STATE_HOME", r"C:\Users\test\.state"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\AppData\Local")
                .join("borax")
                .join("v1")
        )
    );
}

#[cfg(windows)]
#[test]
fn state_root_skips_an_empty_localappdata_on_windows() {
    let root = state_root(env(&[
        ("LOCALAPPDATA", ""),
        ("XDG_STATE_HOME", r"C:\Users\test\.state"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\.state")
                .join("borax")
                .join("v1")
        )
    );
}

#[cfg(windows)]
#[test]
fn state_root_skips_a_relative_localappdata_on_windows() {
    let root = state_root(env(&[
        ("LOCALAPPDATA", r"AppData\Local"),
        ("XDG_STATE_HOME", r"C:\Users\test\.state"),
    ]));

    assert_eq!(
        root,
        Some(
            PathBuf::from(r"C:\Users\test\.state")
                .join("borax")
                .join("v1")
        )
    );
}

#[cfg(windows)]
#[test]
fn state_root_is_none_when_nothing_qualifies_on_windows() {
    assert_eq!(state_root(env(&[])), None);
}

#[cfg(windows)]
#[test]
fn state_root_is_none_for_an_empty_xdg_state_home_with_no_localappdata_on_windows() {
    assert_eq!(state_root(env(&[("XDG_STATE_HOME", "")])), None);
}

#[cfg(windows)]
#[test]
fn state_root_is_none_for_a_relative_xdg_state_home_with_no_localappdata_on_windows() {
    assert_eq!(state_root(env(&[("XDG_STATE_HOME", "relative")])), None);
}
