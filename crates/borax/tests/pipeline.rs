#![allow(clippy::unwrap_used)]

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use borax::event::{Attempt, Counts, Event, SkipReason};
use borax::pipeline::{
    FileOutcome, FileRecord, Library, ResolveConfig, event_for, resolve_batch, resolve_file,
};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{Doi, Identifier};
use borax_core::record::{EntryType, Record};
use borax_pdf::source::{ExtractionError, InfoMetadata, PdfSource};
use borax_pdf::tiered::{ExtractionConfig, Tier};
use borax_sources::cache::MemoryCache;
use borax_sources::source::{Source, SourceError, SourceName};
use borax_sources::store::ContentIndex;

// ---------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------

/// A [`PdfSource`] fake driven entirely by data supplied through its
/// builder methods, following the shape of the one in
/// `borax-pdf/tests/tiered.rs`.
#[derive(Clone)]
struct FakePdf {
    pages: Vec<Result<String, ExtractionError>>,
    info: InfoMetadata,
    xmp: Option<String>,
}

impl FakePdf {
    fn new() -> FakePdf {
        FakePdf {
            pages: Vec::new(),
            info: InfoMetadata::default(),
            xmp: None,
        }
    }

    fn with_xmp(mut self, xmp: impl Into<String>) -> FakePdf {
        self.xmp = Some(xmp.into());
        self
    }

    fn with_pages(mut self, pages: Vec<Result<String, ExtractionError>>) -> FakePdf {
        self.pages = pages;
        self
    }

    fn with_title(mut self, title: impl Into<String>) -> FakePdf {
        self.info.title = Some(title.into());
        self
    }
}

impl PdfSource for FakePdf {
    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn info_metadata(&self) -> &InfoMetadata {
        &self.info
    }

    fn xmp(&self) -> Option<&str> {
        self.xmp.as_deref()
    }

    fn page_text(&self, index: usize) -> Result<String, ExtractionError> {
        self.pages[index].clone()
    }
}

/// A PDF carrying `value` as a DOI in its XMP packet, resolved on the
/// embedded-metadata pass.
fn pdf_with_embedded_doi(value: &str) -> FakePdf {
    FakePdf::new().with_xmp(format!("<prism:doi>{value}</prism:doi>"))
}

/// A PDF carrying `value` as a DOI only in its first page's text,
/// resolved on the text-layer pass.
fn pdf_with_text_doi(value: &str) -> FakePdf {
    FakePdf::new().with_pages(vec![Ok(format!("see {value} for details"))])
}

/// A PDF with no pages and no metadata: the text pass has nothing to
/// read at all.
fn pdf_with_no_text_layer() -> FakePdf {
    FakePdf::new()
}

/// A PDF with a page of ordinary prose holding no identifier.
fn pdf_with_no_identifier() -> FakePdf {
    FakePdf::new().with_pages(vec![Ok("just some prose, no identifiers here".to_string())])
}

/// What [`FakeLibrary`] answers for one path.
struct LibraryEntry {
    hash: Result<ContentHash, ExtractionError>,
    pdf: Result<FakePdf, ExtractionError>,
}

/// A [`Library`] fake backed by a map from path to a fixed `(hash, PDF
/// content or error)` pair, with call counters so a test can prove a
/// content-index hit never touched the file.
struct FakeLibrary {
    entries: BTreeMap<PathBuf, LibraryEntry>,
    hash_calls: Cell<usize>,
    open_calls: Cell<usize>,
}

impl FakeLibrary {
    fn new() -> FakeLibrary {
        FakeLibrary {
            entries: BTreeMap::new(),
            hash_calls: Cell::new(0),
            open_calls: Cell::new(0),
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

    /// A file whose hash is known but which fails to open.
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

    /// A file that cannot be hashed but opens fine.
    fn with_hash_error(
        mut self,
        path: impl Into<PathBuf>,
        error: ExtractionError,
        pdf: FakePdf,
    ) -> FakeLibrary {
        self.entries.insert(
            path.into(),
            LibraryEntry {
                hash: Err(error),
                pdf: Ok(pdf),
            },
        );
        self
    }

    /// Number of times [`Library::hash`] has been called.
    fn hash_calls(&self) -> usize {
        self.hash_calls.get()
    }

    /// Number of times [`Library::open`] has been called. The assertion
    /// that proves a content-index hit never opened the file.
    fn open_calls(&self) -> usize {
        self.open_calls.get()
    }
}

impl Library for FakeLibrary {
    fn hash(&self, path: &Path) -> Result<ContentHash, ExtractionError> {
        self.hash_calls.set(self.hash_calls.get() + 1);
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
        self.open_calls.set(self.open_calls.get() + 1);
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

/// A [`Source`] whose name, support answer, and canned response are
/// fixed at construction, following the shape of the one in
/// `borax-sources/tests/cache.rs`. The call counter is shared through
/// an `Rc` so a test can read it after the source has been borrowed
/// into a `&[&dyn Source]` slice.
struct FakeSource {
    name: SourceName,
    supports: bool,
    response: Result<Record, SourceError>,
    calls: Rc<Cell<usize>>,
}

impl Source for FakeSource {
    fn name(&self) -> SourceName {
        self.name
    }

    fn supports(&self, _identifier: &Identifier) -> bool {
        self.supports
    }

    fn fetch(&self, _identifier: &Identifier) -> Result<Record, SourceError> {
        self.calls.set(self.calls.get() + 1);
        self.response.clone()
    }
}

fn fake_source(
    name: SourceName,
    response: Result<Record, SourceError>,
) -> (FakeSource, Rc<Cell<usize>>) {
    let calls = Rc::new(Cell::new(0));
    (
        FakeSource {
            name,
            supports: true,
            response,
            calls: calls.clone(),
        },
        calls,
    )
}

/// A [`Source`] that panics if it is ever asked anything, for tests
/// that must prove a source was never touched.
struct PanicSource {
    name: SourceName,
}

impl Source for PanicSource {
    fn name(&self) -> SourceName {
        self.name
    }

    fn supports(&self, _identifier: &Identifier) -> bool {
        panic!(
            "{} was asked to support an identifier in an offline run",
            self.name
        )
    }

    fn fetch(&self, _identifier: &Identifier) -> Result<Record, SourceError> {
        panic!("{} was asked to fetch in an offline run", self.name)
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

fn record_with_doi(value: &str) -> Record {
    Record {
        title: Some("On the Structure of Borax".to_string()),
        doi: Some(doi(value)),
        ..Record::new(EntryType::Article)
    }
}

fn record_with_doi_and_title(value: &str, title: &str) -> Record {
    Record {
        title: Some(title.to_string()),
        doi: Some(doi(value)),
        ..Record::new(EntryType::Article)
    }
}

fn config(cache: bool) -> ResolveConfig {
    ResolveConfig {
        extraction: ExtractionConfig::default(),
        cache,
    }
}

/// Render [`Tier`] the way `event_for` is expected to: a kebab-case
/// name matching the variant, consistent with the rest of the event
/// schema's kebab-case tags.
fn tier_str(tier: Tier) -> &'static str {
    match tier {
        Tier::EmbeddedMetadata => "embedded-metadata",
        Tier::TextLayer => "text-layer",
    }
}

fn resolved_outcome(outcome: FileOutcome) -> FileRecord {
    match outcome {
        FileOutcome::Resolved(record) => record,
        FileOutcome::Skipped(reason) => panic!("expected Resolved, got Skipped({reason:?})"),
    }
}

fn skipped_outcome(outcome: FileOutcome) -> SkipReason {
    match outcome {
        FileOutcome::Skipped(reason) => reason,
        FileOutcome::Resolved(record) => panic!("expected Skipped, got Resolved({record:?})"),
    }
}

// ---------------------------------------------------------------------
// resolve_file: happy path
// ---------------------------------------------------------------------

#[test]
fn embedded_metadata_identifier_resolves_via_the_first_source_uncached() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("embedded-happy-path");
    let library =
        FakeLibrary::new().with_file(path, hash, pdf_with_embedded_doi("10.1000/embedded"));
    let (crossref, _calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/embedded")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));
    let file_record = resolved_outcome(outcome);

    assert_eq!(file_record.record, record_with_doi("10.1000/embedded"));
    assert_eq!(file_record.source, Some(SourceName::Crossref));
    assert_eq!(file_record.tier, Some(Tier::EmbeddedMetadata));
    assert!(!file_record.cached);
}

#[test]
fn text_layer_identifier_reports_the_text_layer_tier() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("text-layer-happy-path");
    let library = FakeLibrary::new().with_file(path, hash, pdf_with_text_doi("10.1000/text-layer"));
    let (crossref, _calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/text-layer")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));
    let file_record = resolved_outcome(outcome);

    assert_eq!(file_record.tier, Some(Tier::TextLayer));
}

// ---------------------------------------------------------------------
// resolve_file: content index
// ---------------------------------------------------------------------

#[test]
fn content_index_hit_is_returned_without_opening_the_file() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("indexed-content");
    // The pdf field would fail loudly if opened, so an accidental open
    // shows up as a skip rather than a silently-correct resolution.
    let library = FakeLibrary::new().with_open_error(
        path,
        hash.clone(),
        ExtractionError::Unreadable {
            message: "must never be opened".to_string(),
        },
    );
    let indexed = record_with_doi("10.1000/indexed");
    let index = ContentIndex::new(MemoryCache::new());
    index.put(&hash, &indexed);
    let sources: Vec<&dyn Source> = Vec::new();

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));
    let file_record = resolved_outcome(outcome);

    assert_eq!(file_record.record, indexed);
    assert_eq!(file_record.source, None);
    assert_eq!(file_record.tier, None);
    assert!(file_record.cached);
    assert_eq!(library.open_calls(), 0);
}

#[test]
fn cache_false_bypasses_the_content_index() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("indexed-but-bypassed");
    let library =
        FakeLibrary::new().with_file(path, hash.clone(), pdf_with_embedded_doi("10.1000/live"));
    let indexed = record_with_doi("10.1000/stale-index-entry");
    let index = ContentIndex::new(MemoryCache::new());
    index.put(&hash, &indexed);
    let (crossref, calls) = fake_source(SourceName::Crossref, Ok(record_with_doi("10.1000/live")));
    let sources: Vec<&dyn Source> = vec![&crossref];

    let outcome = resolve_file(path, &library, &sources, &index, &config(false));
    let file_record = resolved_outcome(outcome);

    assert_eq!(library.open_calls(), 1);
    assert_eq!(calls.get(), 1);
    assert_eq!(file_record.record, record_with_doi("10.1000/live"));
    assert!(!file_record.cached);
}

// ---------------------------------------------------------------------
// resolve_file: a hash failure is a miss, not a skip
// ---------------------------------------------------------------------

#[test]
fn a_hash_failure_proceeds_to_open_and_extract_rather_than_skipping() {
    let path = Path::new("paper.pdf");
    let library = FakeLibrary::new().with_hash_error(
        path,
        ExtractionError::Unreadable {
            message: "cannot hash".to_string(),
        },
        pdf_with_embedded_doi("10.1000/unhashable"),
    );
    let (crossref, _calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/unhashable")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));
    let file_record = resolved_outcome(outcome);

    assert_eq!(library.hash_calls(), 1);
    assert_eq!(library.open_calls(), 1);
    assert_eq!(file_record.record, record_with_doi("10.1000/unhashable"));
    assert!(!file_record.cached);
}

// ---------------------------------------------------------------------
// resolve_file: a successful resolution is written to the index
// ---------------------------------------------------------------------

#[test]
fn a_successful_resolution_is_written_to_the_content_index() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("to-be-indexed");
    let library = FakeLibrary::new().with_file(
        path,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/to-index"),
    );
    let (crossref, _calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/to-index")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));
    let file_record = resolved_outcome(outcome);

    assert_eq!(index.get(&hash), Some(file_record.record));
}

/// The doc comment states the index write unconditionally, as a
/// consequence of the four numbered passes rather than as a fifth pass
/// gated on `cache`; only the content-index *read* (pass 1) is
/// documented as skipped when `cache` is `false`. This test pins that
/// reading: a `--no-cache` run still leaves a later, cache-enabled run
/// able to find the file offline.
#[test]
fn a_successful_resolution_is_written_to_the_index_even_with_cache_bypassed() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("indexed-despite-bypass");
    let library = FakeLibrary::new().with_file(
        path,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/bypassed"),
    );
    let (crossref, _calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/bypassed")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(false));
    let file_record = resolved_outcome(outcome);

    assert_eq!(index.get(&hash), Some(file_record.record));
}

// ---------------------------------------------------------------------
// resolve_file: skips
// ---------------------------------------------------------------------

#[test]
fn unreadable_file_is_skipped_with_the_open_error_message() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("unreadable");
    let library = FakeLibrary::new().with_open_error(
        path,
        hash,
        ExtractionError::Unreadable {
            message: "corrupt stream".to_string(),
        },
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));
    let reason = skipped_outcome(outcome);

    assert_eq!(
        reason,
        SkipReason::Unreadable {
            message: "corrupt stream".to_string(),
        }
    );
}

#[test]
fn encrypted_file_is_skipped_as_unreadable() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("encrypted");
    let library = FakeLibrary::new().with_open_error(path, hash, ExtractionError::Encrypted);
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));
    let reason = skipped_outcome(outcome);

    assert_eq!(
        reason,
        SkipReason::Unreadable {
            message: ExtractionError::Encrypted.to_string(),
        }
    );
}

#[test]
fn a_file_with_no_text_layer_is_skipped_as_having_no_identifier() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("no-text-layer");
    let library = FakeLibrary::new().with_file(path, hash, pdf_with_no_text_layer());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));
    let reason = skipped_outcome(outcome);

    assert_eq!(reason, SkipReason::NoIdentifier);
}

#[test]
fn a_file_with_text_but_no_identifier_is_skipped_as_having_no_identifier() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("no-identifier-found");
    let library = FakeLibrary::new().with_file(path, hash, pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));
    let reason = skipped_outcome(outcome);

    assert_eq!(reason, SkipReason::NoIdentifier);
}

#[test]
fn an_identifier_no_source_holds_is_skipped_as_unresolvable_with_attempts_in_priority_order() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("unresolvable");
    let library =
        FakeLibrary::new().with_file(path, hash, pdf_with_embedded_doi("10.1000/nowhere"));
    let (crossref, _) = fake_source(SourceName::Crossref, Err(SourceError::NotFound));
    let (openalex, _) = fake_source(SourceName::OpenAlex, Err(SourceError::NotFound));
    let (datacite, _) = fake_source(SourceName::DataCite, Err(SourceError::NotFound));
    let sources: Vec<&dyn Source> = vec![&crossref, &openalex, &datacite];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));
    let reason = skipped_outcome(outcome);

    assert_eq!(
        reason,
        SkipReason::Unresolvable {
            attempts: vec![
                Attempt {
                    source: "crossref".to_string(),
                    error: SourceError::NotFound.to_string(),
                },
                Attempt {
                    source: "openalex".to_string(),
                    error: SourceError::NotFound.to_string(),
                },
                Attempt {
                    source: "datacite".to_string(),
                    error: SourceError::NotFound.to_string(),
                },
            ],
        }
    );
}

#[test]
fn a_title_conflict_is_a_skip_and_the_record_is_not_returned() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("conflicting-title");
    let library = FakeLibrary::new().with_file(
        path,
        hash,
        pdf_with_embedded_doi("10.1000/conflict").with_title("Old Title Extracted from the PDF"),
    );
    let (crossref, _) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi_and_title(
            "10.1000/conflict",
            "A Completely Different Title About Something Else",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));

    assert!(!matches!(outcome, FileOutcome::Resolved(_)));
    let reason = skipped_outcome(outcome);
    assert_eq!(
        reason,
        SkipReason::Conflict {
            field: "title".to_string(),
            extracted: "Old Title Extracted from the PDF".to_string(),
            resolved: "A Completely Different Title About Something Else".to_string(),
        }
    );
}

// ---------------------------------------------------------------------
// resolve_file: fallback and unknown-everywhere spec scenarios
// ---------------------------------------------------------------------

/// Spec scenario: "Crossref outage falls back to OpenAlex".
#[test]
fn crossref_outage_falls_back_to_openalex() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("crossref-outage");
    let library = FakeLibrary::new().with_file(path, hash, pdf_with_embedded_doi("10.1000/outage"));
    let (crossref, _) = fake_source(
        SourceName::Crossref,
        Err(SourceError::Unavailable {
            message: "503".to_string(),
        }),
    );
    let (openalex, _) = fake_source(SourceName::OpenAlex, Ok(record_with_doi("10.1000/outage")));
    let sources: Vec<&dyn Source> = vec![&crossref, &openalex];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));
    let file_record = resolved_outcome(outcome);

    assert_eq!(file_record.source, Some(SourceName::OpenAlex));
    assert_eq!(file_record.record, record_with_doi("10.1000/outage"));
}

/// Spec scenario: "Identifier unknown everywhere".
#[test]
fn identifier_unknown_everywhere_is_skipped_as_unresolvable() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("unknown-everywhere");
    let library =
        FakeLibrary::new().with_file(path, hash, pdf_with_embedded_doi("10.1000/unknown"));
    let (crossref, _) = fake_source(SourceName::Crossref, Err(SourceError::NotFound));
    let (openalex, _) = fake_source(SourceName::OpenAlex, Err(SourceError::NotFound));
    let (datacite, _) = fake_source(SourceName::DataCite, Err(SourceError::NotFound));
    let sources: Vec<&dyn Source> = vec![&crossref, &openalex, &datacite];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &library, &sources, &index, &config(true));

    assert!(matches!(
        outcome,
        FileOutcome::Skipped(SkipReason::Unresolvable { .. })
    ));
}

// ---------------------------------------------------------------------
// event_for
// ---------------------------------------------------------------------

#[test]
fn a_resolved_file_produces_a_resolved_event_with_path_identifier_source_tier_and_cached() {
    let path = PathBuf::from("paper.pdf");
    let outcome = FileOutcome::Resolved(FileRecord {
        record: record_with_doi("10.1000/xyz"),
        source: Some(SourceName::Crossref),
        tier: Some(Tier::EmbeddedMetadata),
        cached: false,
    });

    let event = event_for(&path, &outcome);

    assert_eq!(
        event,
        Event::Resolved {
            path: path.clone(),
            identifier: "10.1000/xyz".to_string(),
            source: "crossref".to_string(),
            tier: Some(tier_str(Tier::EmbeddedMetadata).to_string()),
            cached: false,
        }
    );
}

#[test]
fn a_content_index_hit_reports_its_source_as_cache() {
    let path = PathBuf::from("paper.pdf");
    let outcome = FileOutcome::Resolved(FileRecord {
        record: record_with_doi("10.1000/cached-record"),
        source: None,
        tier: None,
        cached: true,
    });

    let event = event_for(&path, &outcome);

    match event {
        Event::Resolved {
            source,
            tier,
            cached,
            ..
        } => {
            assert_eq!(source, "cache");
            assert_eq!(tier, None);
            assert!(cached);
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

#[test]
fn a_skipped_file_produces_a_skipped_event_carrying_the_same_reason() {
    let path = PathBuf::from("mystery.pdf");
    let outcome = FileOutcome::Skipped(SkipReason::NoIdentifier);

    let event = event_for(&path, &outcome);

    assert_eq!(
        event,
        Event::Skipped {
            path,
            reason: SkipReason::NoIdentifier,
        }
    );
}

// ---------------------------------------------------------------------
// resolve_batch
// ---------------------------------------------------------------------

#[test]
fn events_come_in_input_order_ending_with_run_finished_and_nothing_after() {
    let p1 = PathBuf::from("a.pdf");
    let p2 = PathBuf::from("b.pdf");
    let p3 = PathBuf::from("c.pdf");
    let library = FakeLibrary::new()
        .with_file(&p1, hash_for("order-a"), pdf_with_embedded_doi("10.1000/a"))
        .with_file(&p2, hash_for("order-b"), pdf_with_no_text_layer())
        .with_file(&p3, hash_for("order-c"), pdf_with_embedded_doi("10.1000/c"));
    let (crossref, _) = fake_source(SourceName::Crossref, Ok(record_with_doi("10.1000/a")));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(
        &[p1.clone(), p2.clone(), p3.clone()],
        &library,
        &sources,
        &index,
        &config(true),
    );

    assert_eq!(run.events.len(), 4);
    match &run.events[0] {
        Event::Resolved { path, .. } => assert_eq!(path, &p1),
        other => panic!("expected Resolved for a.pdf, got {other:?}"),
    }
    match &run.events[1] {
        Event::Skipped { path, .. } => assert_eq!(path, &p2),
        other => panic!("expected Skipped for b.pdf, got {other:?}"),
    }
    match &run.events[2] {
        Event::Resolved { path, .. } => assert_eq!(path, &p3),
        other => panic!("expected Resolved for c.pdf, got {other:?}"),
    }
    assert!(matches!(run.events[3], Event::RunFinished { .. }));
}

#[test]
fn counts_reflect_the_outcomes_and_renamed_is_always_zero() {
    let p1 = PathBuf::from("a.pdf");
    let p2 = PathBuf::from("b.pdf");
    let p3 = PathBuf::from("c.pdf");
    let library = FakeLibrary::new()
        .with_file(
            &p1,
            hash_for("counts-a"),
            pdf_with_embedded_doi("10.1000/a"),
        )
        .with_file(&p2, hash_for("counts-b"), pdf_with_no_text_layer())
        .with_file(
            &p3,
            hash_for("counts-c"),
            pdf_with_embedded_doi("10.1000/c"),
        );
    let (crossref, _) = fake_source(SourceName::Crossref, Ok(record_with_doi("10.1000/a")));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(&[p1, p2, p3], &library, &sources, &index, &config(true));

    assert_eq!(
        run.counts,
        Counts {
            resolved: 2,
            renamed: 0,
            skipped: 1,
        }
    );
}

#[test]
fn the_final_event_carries_the_same_counts_as_run_counts() {
    let p1 = PathBuf::from("a.pdf");
    let library =
        FakeLibrary::new().with_file(&p1, hash_for("final-event"), pdf_with_no_text_layer());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(&[p1], &library, &sources, &index, &config(true));

    match run.events.last() {
        Some(Event::RunFinished { counts }) => assert_eq!(*counts, run.counts),
        other => panic!("expected the last event to be RunFinished, got {other:?}"),
    }
}

#[test]
fn a_mixed_batch_completes_and_an_unreadable_file_does_not_curtail_it() {
    let p1 = PathBuf::from("resolved-first.pdf");
    let p2 = PathBuf::from("unreadable.pdf");
    let p3 = PathBuf::from("resolved-second.pdf");
    let p4 = PathBuf::from("no-identifier.pdf");
    let library = FakeLibrary::new()
        .with_file(
            &p1,
            hash_for("mixed-1"),
            pdf_with_embedded_doi("10.1000/mixed-1"),
        )
        .with_open_error(
            &p2,
            hash_for("mixed-2"),
            ExtractionError::Unreadable {
                message: "broken file".to_string(),
            },
        )
        .with_file(
            &p3,
            hash_for("mixed-3"),
            pdf_with_embedded_doi("10.1000/mixed-3"),
        )
        .with_file(&p4, hash_for("mixed-4"), pdf_with_no_identifier());
    let (crossref, _) = fake_source(SourceName::Crossref, Ok(record_with_doi("10.1000/mixed")));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(
        &[p1.clone(), p2.clone(), p3.clone(), p4.clone()],
        &library,
        &sources,
        &index,
        &config(true),
    );

    assert_eq!(run.events.len(), 5);
    assert_eq!(
        run.counts,
        Counts {
            resolved: 2,
            renamed: 0,
            skipped: 2,
        }
    );
    let paths: Vec<PathBuf> = run.events[..4]
        .iter()
        .map(|event| match event {
            Event::Resolved { path, .. } | Event::Skipped { path, .. } => path.clone(),
            other => panic!("unexpected event {other:?}"),
        })
        .collect();
    assert_eq!(paths, vec![p1, p2, p3, p4]);
}

#[test]
fn an_empty_batch_produces_just_the_finishing_event_with_zero_counts() {
    let library = FakeLibrary::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(&[], &library, &sources, &index, &config(true));

    assert_eq!(
        run.events,
        vec![Event::RunFinished {
            counts: Counts::default(),
        }]
    );
    assert_eq!(run.counts, Counts::default());
}

/// Spec scenario: "Re-run over the same directory is offline".
#[test]
fn a_second_identical_batch_is_served_from_the_index_and_never_touches_a_source() {
    let p1 = PathBuf::from("one.pdf");
    let p2 = PathBuf::from("two.pdf");
    let library = FakeLibrary::new()
        .with_file(
            &p1,
            hash_for("offline-one"),
            pdf_with_embedded_doi("10.4000/offline-one"),
        )
        .with_file(
            &p2,
            hash_for("offline-two"),
            pdf_with_embedded_doi("10.4000/offline-two"),
        );
    let (crossref, calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.9000/offline-record")),
    );
    let live_sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let conf = config(true);

    let first = resolve_batch(
        &[p1.clone(), p2.clone()],
        &library,
        &live_sources,
        &index,
        &conf,
    );
    assert_eq!(
        first.counts,
        Counts {
            resolved: 2,
            renamed: 0,
            skipped: 0,
        }
    );
    assert_eq!(calls.get(), 2);
    let opens_after_first_run = library.open_calls();

    let panic_crossref = PanicSource {
        name: SourceName::Crossref,
    };
    let panic_sources: Vec<&dyn Source> = vec![&panic_crossref];

    let second = resolve_batch(&[p1, p2], &library, &panic_sources, &index, &conf);

    assert_eq!(second.counts, first.counts);
    assert_eq!(library.open_calls(), opens_after_first_run);
    assert_eq!(calls.get(), 2);
    for (first_event, second_event) in first.events.iter().zip(second.events.iter()) {
        let (
            Event::Resolved {
                identifier: first_identifier,
                ..
            },
            Event::Resolved {
                identifier: second_identifier,
                source: second_source,
                cached: second_cached,
                ..
            },
        ) = (first_event, second_event)
        else {
            panic!("expected both events to be Resolved: {first_event:?}, {second_event:?}");
        };
        assert_eq!(first_identifier, second_identifier);
        assert_eq!(second_source, "cache");
        assert!(*second_cached);
    }
}

/// Spec scenario: "Renamed file, same content" — a second path with
/// identical content to the first is served from the index within the
/// same batch, without ever being opened or dispatched to a source.
#[test]
fn a_renamed_file_with_identical_content_is_served_from_the_index_without_opening_or_resolving() {
    let original = PathBuf::from("original.pdf");
    let renamed = PathBuf::from("renamed.pdf");
    let shared_hash = hash_for("same bytes, different name");
    let library = FakeLibrary::new()
        .with_file(
            &original,
            shared_hash.clone(),
            pdf_with_embedded_doi("10.5000/renamed-scenario"),
        )
        .with_open_error(
            &renamed,
            shared_hash,
            ExtractionError::Unreadable {
                message: "must never be opened".to_string(),
            },
        );
    let (crossref, calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.5000/renamed-scenario")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(
        &[original, renamed.clone()],
        &library,
        &sources,
        &index,
        &config(true),
    );

    assert_eq!(library.open_calls(), 1);
    assert_eq!(calls.get(), 1);
    match &run.events[1] {
        Event::Resolved {
            path,
            source,
            cached,
            ..
        } => {
            assert_eq!(path, &renamed);
            assert_eq!(source, "cache");
            assert!(*cached);
        }
        other => panic!("expected Resolved for the renamed file, got {other:?}"),
    }
}
