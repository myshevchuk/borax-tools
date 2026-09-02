#![allow(clippy::unwrap_used)]

//! The two contracts `stream-per-file-events` adds around `dispatch`'s
//! envelope.
//!
//! **Liveness**: a file's line reaches `Streams.out` when that file is
//! decided, not after the whole run has been assembled. Proved without
//! timing, by an adapter whose call for file N observes what the writer
//! already holds — a buffered implementation leaves that observation
//! empty for every file, however many came before it.
//!
//! **The fatal envelope survives streaming**: `dispatch` must write
//! `RunStarted` before it knows whether the run can proceed, since
//! streaming means it can no longer wait for the whole body to succeed
//! first. `preflight` is what keeps a fatal run from opening the stream
//! at all — these tests pin that a `Diagnostic`, however it arises,
//! still leaves stdout without `run-started` or `run-finished`.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use borax::bib::BibFiles;
use borax::cli::{Cli, Command};
use borax::config::{Effective, Layer, Origin, resolve};
use borax::pipeline::Library;
use borax::renaming::{Filesystem, RenameError};
use borax::run::{Adapters, Configs, Streams, dispatch};
use borax::session::Outcome;
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{Doi, Identifier};
use borax_core::record::{DateParts, EntryType, Name, Record};
use borax_pdf::source::{ExtractionError, InfoMetadata, PdfSource};
use borax_sources::cache::MemoryCache;
use borax_sources::source::{Source, SourceError, SourceName};
use borax_sources::store::ContentIndex;
use serde_json::Value;

// ---------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------

/// A [`PdfSource`] fake carrying an embedded DOI in its XMP packet,
/// following the shape of the one in `dispatch.rs`.
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

/// What [`LiveLibrary`] answers for one path.
struct LibraryEntry {
    hash: ContentHash,
    pdf: FakePdf,
}

/// A [`Library`] fake that, each time a file's hash is requested,
/// records everything `out` holds at that moment before answering.
///
/// The record is what proves events reach the writer as they happen
/// rather than being assembled into a value first: a run that buffers
/// its whole stream and writes it only at the end leaves every snapshot
/// here identical, however many files came before the one being hashed.
struct LiveLibrary {
    entries: BTreeMap<PathBuf, LibraryEntry>,
    out: Arc<Mutex<Vec<u8>>>,
    snapshots: Mutex<Vec<(PathBuf, String)>>,
}

impl LiveLibrary {
    fn new(out: Arc<Mutex<Vec<u8>>>) -> LiveLibrary {
        LiveLibrary {
            entries: BTreeMap::new(),
            out,
            snapshots: Mutex::new(Vec::new()),
        }
    }

    fn with_file(
        mut self,
        path: impl Into<PathBuf>,
        hash: ContentHash,
        pdf: FakePdf,
    ) -> LiveLibrary {
        self.entries.insert(path.into(), LibraryEntry { hash, pdf });
        self
    }

    /// What `out` held, in call order, the moment each file's hash was
    /// requested.
    fn snapshots(&self) -> Vec<(PathBuf, String)> {
        self.snapshots.lock().unwrap().clone()
    }
}

impl Library for LiveLibrary {
    fn hash(&self, path: &Path) -> Result<ContentHash, ExtractionError> {
        let seen = String::from_utf8(self.out.lock().unwrap().clone()).unwrap();
        self.snapshots
            .lock()
            .unwrap()
            .push((path.to_path_buf(), seen));

        self.entries
            .get(path)
            .map(|entry| entry.hash.clone())
            .ok_or_else(|| ExtractionError::Unreadable {
                message: format!("no fake entry for {}", path.display()),
            })
    }

    fn open(&self, path: &Path) -> Result<Box<dyn PdfSource>, ExtractionError> {
        self.entries
            .get(path)
            .map(|entry| Box::new(entry.pdf.clone()) as Box<dyn PdfSource>)
            .ok_or_else(|| ExtractionError::Unreadable {
                message: format!("no fake entry for {}", path.display()),
            })
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

/// A [`Filesystem`] fake with nothing already on disk and no rename
/// recording, following the shape of the one in `dispatch.rs` — the
/// tests here never inspect what was renamed.
struct FakeFilesystem;

impl Filesystem for FakeFilesystem {
    fn existing(&self, _directory: &Path) -> BTreeMap<String, Option<String>> {
        BTreeMap::new()
    }

    fn rename(&self, _from: &Path, _to: &Path) -> Result<(), RenameError> {
        Ok(())
    }
}

/// A [`BibFiles`] fake that reads as empty and discards every write,
/// following the shape of the one in `dispatch.rs` — no test here
/// configures a bibliography destination.
struct FakeBibFiles;

impl BibFiles for FakeBibFiles {
    fn read(&self, _path: &Path) -> io::Result<String> {
        Ok(String::new())
    }

    fn write(&self, _path: &Path, _content: &str) -> io::Result<()> {
        Ok(())
    }
}

/// A [`Write`] that appends into a buffer shared with a fake elsewhere
/// in the run, so that fake can read what has reached "stdout" mid-run.
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Other helpers
// ---------------------------------------------------------------------

fn doi(value: &str) -> Doi {
    Doi::parse(value).unwrap()
}

fn hash_for(seed: &str) -> ContentHash {
    hash_bytes(seed.as_bytes())
}

/// An `Article` by one author in the given year, carrying `doi_value`.
/// Renders as `"{family}{year}"` under the `[auth][year]` template used
/// throughout this file.
fn record_by(family: &str, year: i32, doi_value: &str) -> Record {
    Record {
        title: None,
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

/// The `now` every fixture in this file uses.
fn fixed_now() -> String {
    "2024-01-01T00:00:00Z".to_string()
}

/// An [`Effective`] built from a single layer, for tests that need to
/// steer one or two settings away from the built-in defaults.
fn effective_with(customize: impl FnOnce(&mut Layer)) -> Effective {
    let mut layer = Layer::default();
    customize(&mut layer);
    resolve(vec![(Origin::Flag("test".to_string()), layer)]).unwrap()
}

/// An [`Effective`] whose default template is `template` and which is
/// otherwise the built-in defaults.
fn effective_with_default_template(template: &str) -> Effective {
    effective_with(|layer| {
        layer.templates = Some(BTreeMap::from([(
            "default".to_string(),
            template.to_string(),
        )]));
    })
}

fn cli(command: Command, json: bool) -> Cli {
    Cli { command, json }
}

/// The JSON lines `out` decodes to, one [`Value`] per line.
fn json_lines(out: &[u8]) -> Vec<Value> {
    String::from_utf8(out.to_vec())
        .unwrap()
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("{line:?} did not parse: {error}"))
        })
        .collect()
}

/// The number of `resolved` events among `lines`.
fn resolved_count(lines: &[Value]) -> usize {
    lines
        .iter()
        .filter(|line| line["event"] == "resolved")
        .count()
}

/// Assert that `out` carries neither `run-started` nor `run-finished`,
/// which is what a fatal run must never write once `RunStarted` moves
/// ahead of the checks that can end a run before it starts.
fn assert_stream_never_opened(out: &[u8]) {
    let lines = json_lines(out);
    assert!(
        !lines.iter().any(|line| line["event"] == "run-started"),
        "expected no run-started, got {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line["event"] == "run-finished"),
        "expected no run-finished, got {lines:?}"
    );
}

// ---------------------------------------------------------------------
// Liveness (task 2.1)
// ---------------------------------------------------------------------

#[test]
fn rename_preview_writes_each_file_s_resolved_line_before_the_next_file_is_hashed() {
    let a = PathBuf::from("/lib/a.pdf");
    let b = PathBuf::from("/lib/b.pdf");
    let c = PathBuf::from("/lib/c.pdf");

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let library = LiveLibrary::new(Arc::clone(&buffer))
        .with_file(
            &a,
            hash_for("streaming-a"),
            pdf_with_embedded_doi("10.1000/streaming-a"),
        )
        .with_file(
            &b,
            hash_for("streaming-b"),
            pdf_with_embedded_doi("10.1000/streaming-b"),
        )
        .with_file(
            &c,
            hash_for("streaming-c"),
            pdf_with_embedded_doi("10.1000/streaming-c"),
        );
    // One canned response for every identifier: this test is about when
    // a line reaches the writer, not about which record each file gets.
    let crossref = fake_source(
        SourceName::Crossref,
        Ok(record_by("Smith", 2024, "10.1000/streaming-a")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem;
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

    let mut out = SharedWriter(Arc::clone(&buffer));
    let mut err = Vec::new();
    let mut streams = Streams {
        out: &mut out,
        err: &mut err,
    };

    dispatch(
        &cli(
            Command::rename(vec![a.clone(), b.clone(), c.clone()], false),
            true,
        ),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    let snapshots = library.snapshots();
    assert_eq!(
        snapshots.len(),
        3,
        "expected one hash() call per file, got {snapshots:?}"
    );

    assert_eq!(snapshots[0].0, a);
    assert_eq!(
        resolved_count(&json_lines(snapshots[0].1.as_bytes())),
        0,
        "nothing has resolved yet when the first file is hashed:\n{}",
        snapshots[0].1
    );

    assert_eq!(snapshots[1].0, b);
    assert_eq!(
        resolved_count(&json_lines(snapshots[1].1.as_bytes())),
        1,
        "{}'s resolved line must already be on the writer before {} is hashed:\n{}",
        a.display(),
        b.display(),
        snapshots[1].1
    );

    assert_eq!(snapshots[2].0, c);
    assert_eq!(
        resolved_count(&json_lines(snapshots[2].1.as_bytes())),
        2,
        "{}'s and {}'s resolved lines must already be on the writer before {} is hashed:\n{}",
        a.display(),
        b.display(),
        c.display(),
        snapshots[2].1
    );
}

// ---------------------------------------------------------------------
// The fatal envelope survives streaming (task 2.2)
// ---------------------------------------------------------------------

#[test]
fn an_uncompilable_template_is_fatal_and_opens_no_stream() {
    let path = PathBuf::from("/lib/paper.pdf");
    let library = LiveLibrary::new(Arc::new(Mutex::new(Vec::new())));
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem;
    let bib_files = FakeBibFiles;
    let effective = effective_with_default_template("[nonexistentfield]");
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
        &cli(Command::rename(vec![path], false), true),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Fatal, "got {outcome:?}");
    assert_stream_never_opened(&out);
    let err_text = String::from_utf8(err).unwrap();
    assert!(
        err_text.contains("nonexistentfield"),
        "expected the offending field in stderr, got {err_text:?}"
    );
}

/// design "retires the journal from the write path": the message names
/// the run log rather than the journal, since nothing checks for a
/// journal here any more — a run with neither a collection nor a state
/// directory has nowhere to record what it moves.
#[test]
fn apply_with_nowhere_to_record_itself_is_fatal_and_opens_no_stream() {
    let path = PathBuf::from("/lib/paper.pdf");
    let library = LiveLibrary::new(Arc::new(Mutex::new(Vec::new())));
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem;
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
        &cli(Command::rename(vec![path], true), true),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Fatal, "got {outcome:?}");
    assert_stream_never_opened(&out);
    let err_text = String::from_utf8(err).unwrap();
    assert!(
        err_text.contains("record"),
        "expected the run's inability to record itself named in stderr, got {err_text:?}"
    );
    assert!(
        !err_text.to_lowercase().contains("journal"),
        "the journal is retired from this gate, got {err_text:?}"
    );
}

#[test]
fn cache_with_no_cache_root_is_fatal_and_opens_no_stream() {
    let library = LiveLibrary::new(Arc::new(Mutex::new(Vec::new())));
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem;
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
        &cli(Command::cache(false), true),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Fatal, "got {outcome:?}");
    assert_stream_never_opened(&out);
    let err_text = String::from_utf8(err).unwrap();
    assert!(
        err_text.contains("cache directory"),
        "expected the missing cache directory named in stderr, got {err_text:?}"
    );
}

/// The positive control for the three tests above: a run with nothing
/// fatal about it does open the stream, on both ends.
#[test]
fn a_normal_json_run_emits_both_run_started_and_run_finished() {
    let library = LiveLibrary::new(Arc::new(Mutex::new(Vec::new())));
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());
    let filesystem = FakeFilesystem;
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
        &cli(Command::config(), true),
        &Configs::uniform(effective),
        &adapters,
        &mut streams,
    );

    assert_eq!(outcome, Outcome::Success, "got {outcome:?}");
    let lines = json_lines(&out);
    assert!(
        lines.iter().any(|line| line["event"] == "run-started"),
        "got {lines:?}"
    );
    assert!(
        lines.iter().any(|line| line["event"] == "run-finished"),
        "got {lines:?}"
    );
}
