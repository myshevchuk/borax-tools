#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use borax::event::{
    Attempt, Claim, ClaimOrigin, Counts, Event, Extraction, LibraryAnswer, SkipReason,
};
use borax::library::{ArtifactStore, ItemStore, Stores};
use borax::pipeline::{
    Documents, FileOutcome, FileRecord, Provenance, RealDocuments, ResolveConfig, claims_of,
    event_for, extraction, extraction_of, remember, resolve_batch, resolve_file, resolve_supplied,
    standing, verdict_event,
};
use borax_core::content::{ContentHash, hash_bytes};
use borax_core::identifier::{ArxivId, Doi, Identifier};
use borax_core::library::{
    ArtifactId, ArtifactRecord, DuplicateReason, HashEntry, Item, ItemId, RunId as LibraryRunId,
};
use borax_core::record::{EntryType, Record};
use borax_pdf::scan::FoundIdentifier;
use borax_pdf::source::{ExtractionError, InfoMetadata, PdfSource};
use borax_pdf::tiered::{Extracted, ExtractionConfig, Tier};
use borax_sources::cache::{Cache, MemoryCache};
use borax_sources::source::{Source, SourceError, SourceName};
use borax_sources::store::{ContentIndex, hash_file};
use tempfile::tempdir;
use uuid::Uuid;

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

/// What [`FakeDocuments`] answers for one path.
struct LibraryEntry {
    hash: Result<ContentHash, ExtractionError>,
    pdf: Result<FakePdf, ExtractionError>,
}

/// A [`Documents`] fake backed by a map from path to a fixed `(hash, PDF
/// content or error)` pair, with call counters so a test can prove a
/// content-index hit never touched the file.
struct FakeDocuments {
    entries: BTreeMap<PathBuf, LibraryEntry>,
    hash_calls: AtomicUsize,
    open_calls: AtomicUsize,
}

impl FakeDocuments {
    fn new() -> FakeDocuments {
        FakeDocuments {
            entries: BTreeMap::new(),
            hash_calls: AtomicUsize::new(0),
            open_calls: AtomicUsize::new(0),
        }
    }

    /// A readable file: hashing and opening both succeed.
    fn with_file(
        mut self,
        path: impl Into<PathBuf>,
        hash: ContentHash,
        pdf: FakePdf,
    ) -> FakeDocuments {
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
    ) -> FakeDocuments {
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
    ) -> FakeDocuments {
        self.entries.insert(
            path.into(),
            LibraryEntry {
                hash: Err(error),
                pdf: Ok(pdf),
            },
        );
        self
    }

    /// Number of times [`Documents::hash`] has been called.
    fn hash_calls(&self) -> usize {
        self.hash_calls.load(Ordering::Relaxed)
    }

    /// Number of times [`Documents::open`] has been called. The assertion
    /// that proves a content-index hit never opened the file.
    fn open_calls(&self) -> usize {
        self.open_calls.load(Ordering::Relaxed)
    }
}

impl Documents for FakeDocuments {
    fn hash(&self, path: &Path) -> Result<ContentHash, ExtractionError> {
        self.hash_calls.fetch_add(1, Ordering::Relaxed);
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
        self.open_calls.fetch_add(1, Ordering::Relaxed);
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

/// A [`Documents`] that sleeps before opening a file, keyed by path, so a
/// batch resolved concurrently has jobs that finish in a different order
/// than they were queued — the condition under which `resolve_batch`
/// restoring input order is actually being exercised, rather than
/// trivially true because nothing raced.
struct DelayedDocuments {
    inner: FakeDocuments,
    delays: BTreeMap<PathBuf, Duration>,
}

impl DelayedDocuments {
    fn new(inner: FakeDocuments) -> DelayedDocuments {
        DelayedDocuments {
            inner,
            delays: BTreeMap::new(),
        }
    }

    fn with_delay(mut self, path: impl Into<PathBuf>, delay: Duration) -> DelayedDocuments {
        self.delays.insert(path.into(), delay);
        self
    }
}

impl Documents for DelayedDocuments {
    fn hash(&self, path: &Path) -> Result<ContentHash, ExtractionError> {
        self.inner.hash(path)
    }

    fn open(&self, path: &Path) -> Result<Box<dyn PdfSource>, ExtractionError> {
        if let Some(delay) = self.delays.get(path) {
            thread::sleep(*delay);
        }
        self.inner.open(path)
    }
}

/// A [`Source`] whose name, support answer, and canned response are
/// fixed at construction, following the shape of the one in
/// `borax-sources/tests/cache.rs`. The call counter is shared through
/// an `Arc` so a test can read it after the source has been borrowed
/// into a `&[&dyn Source]` slice.
struct FakeSource {
    name: SourceName,
    supports: bool,
    response: Result<Record, SourceError>,
    calls: Arc<AtomicUsize>,
}

impl Source for FakeSource {
    fn name(&self) -> SourceName {
        self.name
    }

    fn supports(&self, _identifier: &Identifier) -> bool {
        self.supports
    }

    fn fetch(&self, _identifier: &Identifier) -> Result<Record, SourceError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.response.clone()
    }
}

fn fake_source(
    name: SourceName,
    response: Result<Record, SourceError>,
) -> (FakeSource, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
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
    let documents =
        FakeDocuments::new().with_file(path, hash, pdf_with_embedded_doi("10.1000/embedded"));
    let (crossref, _calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/embedded")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));
    let file_record = resolved_outcome(outcome);

    assert_eq!(file_record.record, record_with_doi("10.1000/embedded"));
    assert_eq!(file_record.source, Some(SourceName::Crossref));
    assert_eq!(
        file_record.tier,
        Some(Provenance::Extracted(Tier::EmbeddedMetadata))
    );
    assert!(!file_record.cached);
}

#[test]
fn text_layer_identifier_reports_the_text_layer_tier() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("text-layer-happy-path");
    let documents =
        FakeDocuments::new().with_file(path, hash, pdf_with_text_doi("10.1000/text-layer"));
    let (crossref, _calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/text-layer")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));
    let file_record = resolved_outcome(outcome);

    assert_eq!(
        file_record.tier,
        Some(Provenance::Extracted(Tier::TextLayer))
    );
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
    let documents = FakeDocuments::new().with_open_error(
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

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));
    let file_record = resolved_outcome(outcome);

    assert_eq!(file_record.record, indexed);
    assert_eq!(file_record.source, None);
    assert_eq!(file_record.tier, None);
    assert!(file_record.cached);
    assert_eq!(documents.open_calls(), 0);
}

#[test]
fn cache_false_bypasses_the_content_index() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("indexed-but-bypassed");
    let documents =
        FakeDocuments::new().with_file(path, hash.clone(), pdf_with_embedded_doi("10.1000/live"));
    let indexed = record_with_doi("10.1000/stale-index-entry");
    let index = ContentIndex::new(MemoryCache::new());
    index.put(&hash, &indexed);
    let (crossref, calls) = fake_source(SourceName::Crossref, Ok(record_with_doi("10.1000/live")));
    let sources: Vec<&dyn Source> = vec![&crossref];

    let outcome = resolve_file(path, &documents, &sources, &index, &config(false));
    let file_record = resolved_outcome(outcome);

    assert_eq!(documents.open_calls(), 1);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(file_record.record, record_with_doi("10.1000/live"));
    assert!(!file_record.cached);
}

// ---------------------------------------------------------------------
// resolve_file: a hash failure is a miss, not a skip
// ---------------------------------------------------------------------

#[test]
fn a_hash_failure_proceeds_to_open_and_extract_rather_than_skipping() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_hash_error(
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

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));
    let file_record = resolved_outcome(outcome);

    assert_eq!(documents.hash_calls(), 1);
    assert_eq!(documents.open_calls(), 1);
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
    let documents = FakeDocuments::new().with_file(
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

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));
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
    let documents = FakeDocuments::new().with_file(
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

    let outcome = resolve_file(path, &documents, &sources, &index, &config(false));
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
    let documents = FakeDocuments::new().with_open_error(
        path,
        hash,
        ExtractionError::Unreadable {
            message: "corrupt stream".to_string(),
        },
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));
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
    let documents = FakeDocuments::new().with_open_error(path, hash, ExtractionError::Encrypted);
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));
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
    let documents = FakeDocuments::new().with_file(path, hash, pdf_with_no_text_layer());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));
    let reason = skipped_outcome(outcome);

    assert_eq!(reason, SkipReason::NoIdentifier);
}

#[test]
fn a_file_with_text_but_no_identifier_is_skipped_as_having_no_identifier() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("no-identifier-found");
    let documents = FakeDocuments::new().with_file(path, hash, pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));
    let reason = skipped_outcome(outcome);

    assert_eq!(reason, SkipReason::NoIdentifier);
}

#[test]
fn an_identifier_no_source_holds_is_skipped_as_unresolvable_with_attempts_in_priority_order() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("unresolvable");
    let documents =
        FakeDocuments::new().with_file(path, hash, pdf_with_embedded_doi("10.1000/nowhere"));
    let (crossref, _) = fake_source(SourceName::Crossref, Err(SourceError::NotFound));
    let (openalex, _) = fake_source(SourceName::OpenAlex, Err(SourceError::NotFound));
    let (datacite, _) = fake_source(SourceName::DataCite, Err(SourceError::NotFound));
    let sources: Vec<&dyn Source> = vec![&crossref, &openalex, &datacite];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));
    let reason = skipped_outcome(outcome);

    assert_eq!(
        reason,
        SkipReason::Unresolvable {
            found: "doi:10.1000/nowhere".to_string(),
            tier: Some("embedded-metadata".to_string()),
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
    let documents = FakeDocuments::new().with_file(
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

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));

    assert!(!matches!(outcome, FileOutcome::Resolved(_)));
    let SkipReason::Conflict {
        field,
        extracted,
        resolved,
        similarity,
    } = skipped_outcome(outcome)
    else {
        panic!("expected a title conflict");
    };

    assert_eq!(field, "title");
    assert_eq!(extracted, "Old Title Extracted from the PDF");
    assert_eq!(
        resolved,
        "A Completely Different Title About Something Else"
    );
    // The two share only `title` out of four and six content words.
    assert_eq!(similarity, 0.2);
}

/// The regression this check was rewritten for: `2011ASC(353)575.pdf`,
/// whose typesetter could not encode the alpha or the hyphens that the
/// publisher's record keeps. One dropped letter is not a different work.
#[test]
fn a_title_the_producer_could_not_encode_is_not_a_conflict() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("lossy-title");
    let documents = FakeDocuments::new().with_file(
        path,
        hash,
        pdf_with_embedded_doi("10.1002/adsc.201000846").with_title(
            "Synthesis of Diazo Carbonyl Compounds with the ShelfStable Diazo Transfer \
             Reagent Nonafluorobutanesulfonyl Azide",
        ),
    );
    let (crossref, _) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi_and_title(
            "10.1002/adsc.201000846",
            "Synthesis of \u{3b1}\u{2010}Diazo Carbonyl Compounds with the Shelf\u{2010}Stable \
             Diazo Transfer Reagent Nonafluorobutanesulfonyl Azide",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));

    assert!(matches!(outcome, FileOutcome::Resolved(_)));
}

/// The same file's XMP packet says `untitled` while its Info dictionary
/// carries the real title. One source of evidence agreeing is enough.
#[test]
fn a_placeholder_xmp_title_does_not_override_an_agreeing_info_title() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("placeholder-xmp");
    let documents = FakeDocuments::new().with_file(
        path,
        hash,
        pdf_with_text_doi("10.1000/placeholder")
            .with_title("The Real Title of This Particular Work")
            .with_xmp("<dc:title><rdf:Alt><rdf:li>untitled</rdf:li></rdf:Alt></dc:title>"),
    );
    let (crossref, _) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi_and_title(
            "10.1000/placeholder",
            "The Real Title of This Particular Work",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));

    assert!(matches!(outcome, FileOutcome::Resolved(_)));
}

// ---------------------------------------------------------------------
// resolve_file: fallback and unknown-everywhere spec scenarios
// ---------------------------------------------------------------------

/// Spec scenario: "Crossref outage falls back to OpenAlex".
#[test]
fn crossref_outage_falls_back_to_openalex() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("crossref-outage");
    let documents =
        FakeDocuments::new().with_file(path, hash, pdf_with_embedded_doi("10.1000/outage"));
    let (crossref, _) = fake_source(
        SourceName::Crossref,
        Err(SourceError::Unavailable {
            message: "503".to_string(),
        }),
    );
    let (openalex, _) = fake_source(SourceName::OpenAlex, Ok(record_with_doi("10.1000/outage")));
    let sources: Vec<&dyn Source> = vec![&crossref, &openalex];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));
    let file_record = resolved_outcome(outcome);

    assert_eq!(file_record.source, Some(SourceName::OpenAlex));
    assert_eq!(file_record.record, record_with_doi("10.1000/outage"));
}

/// Spec scenario: "Identifier unknown everywhere".
#[test]
fn identifier_unknown_everywhere_is_skipped_as_unresolvable() {
    let path = Path::new("paper.pdf");
    let hash = hash_for("unknown-everywhere");
    let documents =
        FakeDocuments::new().with_file(path, hash, pdf_with_embedded_doi("10.1000/unknown"));
    let (crossref, _) = fake_source(SourceName::Crossref, Err(SourceError::NotFound));
    let (openalex, _) = fake_source(SourceName::OpenAlex, Err(SourceError::NotFound));
    let (datacite, _) = fake_source(SourceName::DataCite, Err(SourceError::NotFound));
    let sources: Vec<&dyn Source> = vec![&crossref, &openalex, &datacite];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));

    assert!(matches!(
        outcome,
        FileOutcome::Skipped(SkipReason::Unresolvable { .. })
    ));
}

// ---------------------------------------------------------------------
// event_for
// ---------------------------------------------------------------------

#[test]
fn a_resolved_file_produces_a_resolved_event_with_path_identifier_record_source_tier_and_cached() {
    let path = PathBuf::from("paper.pdf");
    let record = record_with_doi("10.1000/xyz");
    let outcome = FileOutcome::Resolved(FileRecord {
        record: record.clone(),
        source: Some(SourceName::Crossref),
        found: None,

        claims: Vec::new(),

        tier: Some(Provenance::Extracted(Tier::EmbeddedMetadata)),
        cached: false,
        hash: Some(hash_for("paper")),
        overrode: None,
        library: None,
    });

    let event = event_for(&path, &outcome);

    assert_eq!(
        event,
        Event::Resolved {
            path: path.clone(),
            identifier: "doi:10.1000/xyz".to_string(),
            record: Box::new(record),
            source: "crossref".to_string(),
            found: "doi:10.1000/xyz".to_string(),

            claims: Vec::new(),

            tier: Some(tier_str(Tier::EmbeddedMetadata).to_string()),
            cached: false,
            overrode: None,
            library: None,
        }
    );
}

#[test]
fn a_content_index_hit_reports_its_source_as_cache() {
    let path = PathBuf::from("paper.pdf");
    let outcome = FileOutcome::Resolved(FileRecord {
        record: record_with_doi("10.1000/cached-record"),
        source: None,
        found: None,

        claims: Vec::new(),

        tier: None,
        cached: true,
        hash: Some(hash_for("paper")),
        overrode: None,
        library: None,
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
            library: None,
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
    let documents = FakeDocuments::new()
        .with_file(&p1, hash_for("order-a"), pdf_with_embedded_doi("10.1000/a"))
        .with_file(&p2, hash_for("order-b"), pdf_with_no_text_layer())
        .with_file(&p3, hash_for("order-c"), pdf_with_embedded_doi("10.1000/c"));
    let (crossref, _) = fake_source(SourceName::Crossref, Ok(record_with_doi("10.1000/a")));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(
        &[p1.clone(), p2.clone(), p3.clone()],
        &documents,
        &sources,
        &index,
        None,
        &|_: &Path| config(true),
        1,
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
    let documents = FakeDocuments::new()
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

    let run = resolve_batch(
        &[p1, p2, p3],
        &documents,
        &sources,
        &index,
        None,
        &|_: &Path| config(true),
        1,
    );

    assert_eq!(
        run.counts,
        Counts {
            resolved: 2,
            renamed: 0,
            skipped: 1,
            named: 0,
            unmatched: 0,
            unreached: 0,
            findings: 0,
        }
    );
}

#[test]
fn the_final_event_carries_the_same_counts_as_run_counts() {
    let p1 = PathBuf::from("a.pdf");
    let documents =
        FakeDocuments::new().with_file(&p1, hash_for("final-event"), pdf_with_no_text_layer());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(
        &[p1],
        &documents,
        &sources,
        &index,
        None,
        &|_: &Path| config(true),
        1,
    );

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
    let documents = FakeDocuments::new()
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
        &documents,
        &sources,
        &index,
        None,
        &|_: &Path| config(true),
        1,
    );

    assert_eq!(run.events.len(), 5);
    assert_eq!(
        run.counts,
        Counts {
            resolved: 2,
            renamed: 0,
            skipped: 2,
            named: 0,
            unmatched: 0,
            unreached: 0,
            findings: 0,
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
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(
        &[],
        &documents,
        &sources,
        &index,
        None,
        &|_: &Path| config(true),
        1,
    );

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
    let documents = FakeDocuments::new()
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
        &documents,
        &live_sources,
        &index,
        None,
        &|_: &Path| conf,
        1,
    );
    assert_eq!(
        first.counts,
        Counts {
            resolved: 2,
            renamed: 0,
            skipped: 0,
            named: 0,
            unmatched: 0,
            unreached: 0,
            findings: 0,
        }
    );
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    let opens_after_first_run = documents.open_calls();

    let panic_crossref = PanicSource {
        name: SourceName::Crossref,
    };
    let panic_sources: Vec<&dyn Source> = vec![&panic_crossref];

    let second = resolve_batch(
        &[p1, p2],
        &documents,
        &panic_sources,
        &index,
        None,
        &|_: &Path| conf,
        1,
    );

    assert_eq!(second.counts, first.counts);
    assert_eq!(documents.open_calls(), opens_after_first_run);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    // Both runs end with the summary event; the per-file events are
    // everything before it.
    let first_files = &first.events[..first.events.len() - 1];
    let second_files = &second.events[..second.events.len() - 1];
    for (first_event, second_event) in first_files.iter().zip(second_files.iter()) {
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
    let documents = FakeDocuments::new()
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
        &documents,
        &sources,
        &index,
        None,
        &|_: &Path| config(true),
        1,
    );

    assert_eq!(documents.open_calls(), 1);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
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

// ---------------------------------------------------------------------
// resolve_batch: concurrency
// ---------------------------------------------------------------------

#[test]
fn a_batch_resolved_on_many_threads_reports_its_files_in_input_order() {
    let paths: Vec<PathBuf> = (0..12)
        .map(|i| PathBuf::from(format!("many-threads-{i:02}.pdf")))
        .collect();
    let mut documents = FakeDocuments::new();
    for (i, path) in paths.iter().enumerate() {
        documents = if i % 2 == 0 {
            documents.with_file(
                path,
                hash_for(&format!("many-threads-{i}")),
                pdf_with_embedded_doi(&format!("10.1000/many-threads-{i}")),
            )
        } else {
            documents.with_file(
                path,
                hash_for(&format!("many-threads-{i}")),
                pdf_with_no_identifier(),
            )
        };
    }
    // Earlier files sleep longer, so a job-order-dependent implementation
    // would report them last rather than first.
    let mut documents = DelayedDocuments::new(documents);
    for (i, path) in paths.iter().enumerate() {
        documents = documents.with_delay(path, Duration::from_millis((paths.len() - i) as u64 * 5));
    }
    let (crossref, _) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/many-threads")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(
        &paths,
        &documents,
        &sources,
        &index,
        None,
        &|_: &Path| config(true),
        8,
    );

    assert_eq!(run.events.len(), paths.len() + 1);
    let reported: Vec<&PathBuf> = run.events[..paths.len()]
        .iter()
        .map(|event| match event {
            Event::Resolved { path, .. } | Event::Skipped { path, .. } => path,
            other => panic!("unexpected event {other:?}"),
        })
        .collect();
    assert_eq!(reported, paths.iter().collect::<Vec<_>>());
}

#[test]
fn a_batch_resolved_with_one_worker_or_eight_produces_the_same_run() {
    let paths: Vec<PathBuf> = (0..12)
        .map(|i| PathBuf::from(format!("same-run-{i:02}.pdf")))
        .collect();
    let mut documents = FakeDocuments::new();
    for (i, path) in paths.iter().enumerate() {
        documents = if i % 3 == 0 {
            documents.with_file(
                path,
                hash_for(&format!("same-run-{i}")),
                pdf_with_no_identifier(),
            )
        } else {
            documents.with_file(
                path,
                hash_for(&format!("same-run-{i}")),
                pdf_with_embedded_doi(&format!("10.1000/same-run-{i}")),
            )
        };
    }
    let (crossref, _) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/same-run")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];

    // A fresh index per run: sharing one would make the second run a
    // cache hit and give it a different `cached`/`source` than the
    // first, which is not the property under test.
    let index_one = ContentIndex::new(MemoryCache::new());
    let run_one = resolve_batch(
        &paths,
        &documents,
        &sources,
        &index_one,
        None,
        &|_: &Path| config(true),
        1,
    );
    let index_eight = ContentIndex::new(MemoryCache::new());
    let run_eight = resolve_batch(
        &paths,
        &documents,
        &sources,
        &index_eight,
        None,
        &|_: &Path| config(true),
        8,
    );

    assert_eq!(run_one.events, run_eight.events);
    assert_eq!(run_one.counts, run_eight.counts);
}

#[test]
fn every_file_in_a_concurrent_batch_reaches_the_network_exactly_once() {
    let paths: Vec<PathBuf> = (0..12)
        .map(|i| PathBuf::from(format!("exactly-once-{i:02}.pdf")))
        .collect();
    let mut documents = FakeDocuments::new();
    for (i, path) in paths.iter().enumerate() {
        documents = documents.with_file(
            path,
            hash_for(&format!("exactly-once-{i}")),
            pdf_with_embedded_doi(&format!("10.1000/exactly-once-{i}")),
        );
    }
    let (crossref, calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/exactly-once")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(
        &paths,
        &documents,
        &sources,
        &index,
        None,
        &|_: &Path| config(true),
        8,
    );

    assert_eq!(calls.load(Ordering::Relaxed), paths.len());
    assert_eq!(run.counts.resolved, paths.len());
    assert_eq!(run.counts.skipped, 0);
}

#[test]
fn a_concurrency_of_zero_still_resolves_the_whole_batch() {
    let p1 = PathBuf::from("zero-a.pdf");
    let p2 = PathBuf::from("zero-b.pdf");
    let p3 = PathBuf::from("zero-c.pdf");
    let documents = FakeDocuments::new()
        .with_file(
            &p1,
            hash_for("zero-a"),
            pdf_with_embedded_doi("10.1000/zero-a"),
        )
        .with_file(&p2, hash_for("zero-b"), pdf_with_no_text_layer())
        .with_file(
            &p3,
            hash_for("zero-c"),
            pdf_with_embedded_doi("10.1000/zero-c"),
        );
    let (crossref, _) = fake_source(SourceName::Crossref, Ok(record_with_doi("10.1000/zero-a")));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(
        &[p1.clone(), p2.clone(), p3.clone()],
        &documents,
        &sources,
        &index,
        None,
        &|_: &Path| config(true),
        0,
    );

    assert_eq!(run.events.len(), 4);
    assert_eq!(
        run.counts,
        Counts {
            resolved: 2,
            renamed: 0,
            skipped: 1,
            named: 0,
            unmatched: 0,
            unreached: 0,
            findings: 0,
        }
    );
    assert!(matches!(run.events[3], Event::RunFinished { .. }));
}

#[test]
fn an_empty_batch_with_high_concurrency_still_produces_just_the_finishing_event() {
    let documents = FakeDocuments::new();
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let run = resolve_batch(
        &[],
        &documents,
        &sources,
        &index,
        None,
        &|_: &Path| config(true),
        8,
    );

    assert_eq!(
        run.events,
        vec![Event::RunFinished {
            counts: Counts::default(),
        }]
    );
    assert_eq!(run.counts, Counts::default());
}

// ---------------------------------------------------------------------
// RealDocuments
// ---------------------------------------------------------------------

/// The path to a fixture in `borax-pdf`'s corpus, resolved from this
/// crate's manifest directory so the suite runs the same way regardless
/// of where `cargo test` is invoked from.
fn corpus_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../borax-pdf/tests/corpus")
        .join(name)
}

#[test]
fn hash_of_a_real_file_matches_hashing_its_bytes_directly() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("document.pdf");
    fs::write(&path, b"some file contents").unwrap();
    let documents = RealDocuments;

    let hash = documents.hash(&path).unwrap();

    assert_eq!(hash, hash_for("some file contents"), "got {hash:?}");
    assert_eq!(hash, hash_file(&path).unwrap());
}

#[test]
fn hash_of_a_missing_path_is_unreadable() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("does-not-exist.pdf");
    let documents = RealDocuments;

    let result = documents.hash(&path);

    assert!(
        matches!(result, Err(ExtractionError::Unreadable { .. })),
        "got {result:?}"
    );
}

#[test]
fn open_of_a_file_that_is_not_a_pdf_is_unreadable() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("not-a-pdf.pdf");
    fs::write(&path, b"this is plain text, not a PDF").unwrap();
    let documents = RealDocuments;

    match documents.open(&path) {
        Err(ExtractionError::Unreadable { .. }) => {}
        Err(other) => panic!("expected Unreadable, got Err({other:?})"),
        Ok(_) => panic!("expected opening a non-PDF file to fail"),
    }
}

#[test]
fn open_of_a_missing_path_is_an_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("does-not-exist.pdf");
    let documents = RealDocuments;

    match documents.open(&path) {
        Err(_) => {}
        Ok(_) => panic!("expected opening a missing path to fail"),
    }
}

#[test]
fn open_of_a_real_pdf_fixture_succeeds_with_a_nonzero_page_count() {
    let documents = RealDocuments;

    let pdf = documents
        .open(&corpus_fixture("publisher-info-doi.pdf"))
        .unwrap();

    assert!(pdf.page_count() > 0, "got {}", pdf.page_count());
}

// ---------------------------------------------------------------------
// event_for: the resolved record travels with the event
// ---------------------------------------------------------------------

// The CLI spec defines `resolve` as "extract + resolve, emit records".
// An identifier alone is not a record: a consumer would have to go back
// to the network for what borax already had in hand.
#[test]
fn a_resolved_event_carries_the_whole_record() {
    let path = Path::new("paper.pdf");
    let record = record_with_doi_and_title("10.1000/x", "A Title Worth Keeping");
    let outcome = FileOutcome::Resolved(FileRecord {
        record: record.clone(),
        source: Some(SourceName::Crossref),
        found: None,

        claims: Vec::new(),

        tier: Some(Provenance::Extracted(Tier::TextLayer)),
        cached: false,
        hash: None,
        overrode: None,
        library: None,
    });

    let Event::Resolved {
        record: reported, ..
    } = event_for(path, &outcome)
    else {
        panic!("expected a resolved event");
    };

    assert_eq!(*reported, record);
}

// The record survives the JSON rendering, so a `--json` consumer reads
// the same record the run resolved.
#[test]
fn a_resolved_event_round_trips_its_record_through_json() {
    let path = Path::new("paper.pdf");
    let record = record_with_doi_and_title("10.1000/x", "A Title Worth Keeping");
    let event = event_for(
        path,
        &FileOutcome::Resolved(FileRecord {
            record: record.clone(),
            source: Some(SourceName::Crossref),
            found: None,

            claims: Vec::new(),

            tier: Some(Provenance::Extracted(Tier::TextLayer)),
            cached: false,
            hash: None,
            overrode: None,
            library: None,
        }),
    );

    let line = borax::event::json_line(&event);
    let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();

    assert_eq!(
        parsed["record"]["title"],
        serde_json::json!("A Title Worth Keeping")
    );
}

// ---------------------------------------------------------------------
// 2.4: standing() — duplicate detection wiring against the library
//
// ledger spec: "content check runs after hashing and before any
// resolution" (Content) / "work duplicate ... after resolution" and
// "Stale entries never block re-admission" (disk is the source of
// truth over the artifact store).
//
// What each check answers is `tests/library.rs`'s subject; what these
// pin is the wiring — which pass each check runs between, and what the
// verdict becomes.
// ---------------------------------------------------------------------

/// A UUIDv7 for a record or an item, distinct per call, following the
/// shape of the fixed identities in `tests/library.rs`.
fn fresh_uuid() -> Uuid {
    Uuid::now_v7()
}

/// Writes an artifact record under `root` naming `relative`, holding
/// `hash`, and linked to `item` where one is given.
fn record_at(root: &Path, relative: &str, hash: ContentHash, item: Option<ItemId>) {
    let record = ArtifactRecord {
        id: ArtifactId::from_uuid(fresh_uuid()),
        item,
        path: relative.to_string(),
        size: 100,
        modified_millis: 0,
        history: vec![HashEntry {
            hash,
            run: LibraryRunId::new("earlier-run"),
            timestamp: "2026-08-01T00:00:00Z".to_string(),
            tool_version: "0.2.0-test".to_string(),
        }],
    };
    let dir = root.join(".borax").join("artifacts");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{}.toml", record.id)), record.to_toml()).unwrap();
}

/// Writes an item under `root` carrying `doi_value`, and hands back its
/// identity so a record can be linked to it.
fn item_with(root: &Path, doi_value: &str) -> ItemId {
    let item = Item {
        id: ItemId::from_uuid(fresh_uuid()),
        record: record_with_doi(doi_value),
    };
    let items = root.join("items");
    fs::create_dir_all(&items).unwrap();
    fs::write(items.join(format!("{}.toml", item.id)), item.to_toml()).unwrap();
    item.id.clone()
}

/// The verdict `standing` reaches for `path` against the library rooted
/// at `root`, with `live` deciding whether a recorded path still holds
/// a file — the seam disk occupies in a run, held here so a stale
/// record is a property of the test rather than of the temporary
/// directory.
fn checked<C: borax_sources::cache::Cache>(
    path: &Path,
    documents: &dyn Documents,
    sources: &[&dyn Source],
    index: &ContentIndex<C>,
    root: &Path,
    live: bool,
) -> FileOutcome {
    let stores = Stores::read(root);
    let exists = |_: &Path| live;
    standing(
        path,
        documents,
        sources,
        index,
        &config(true),
        Some(&stores.account(&exists)),
        None,
    )
    .verdict
}

#[test]
fn a_live_content_duplicate_is_skipped_before_any_network_work() {
    let library = tempdir().unwrap();
    let root = library.path();
    let path = root.join("incoming.pdf");
    let hash = hash_for("same-bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/whatever"),
    );
    let panics = PanicSource {
        name: SourceName::Crossref,
    };
    let sources: Vec<&dyn Source> = vec![&panics];
    let index = ContentIndex::new(MemoryCache::new());
    record_at(root, "archived/Smith2024.pdf", hash, None);

    let outcome = checked(&path, &documents, &sources, &index, root, true);

    assert_eq!(
        outcome,
        FileOutcome::Skipped(SkipReason::Duplicate {
            reason: DuplicateReason::Content,
            existing_path: root.join("archived").join("Smith2024.pdf"),
        })
    );
    assert_eq!(
        documents.open_calls(),
        0,
        "a content duplicate must be caught before the file is even opened"
    );
}

/// ledger spec scenario "Duplicate of a vanished admission".
#[test]
fn a_stale_content_duplicate_does_not_block_re_admission() {
    let library = tempdir().unwrap();
    let root = library.path();
    let path = root.join("incoming.pdf");
    let hash = hash_for("same-bytes-again");
    let documents =
        FakeDocuments::new().with_file(&path, hash.clone(), pdf_with_embedded_doi("10.1000/again"));
    let (crossref, _) = fake_source(SourceName::Crossref, Ok(record_with_doi("10.1000/again")));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    record_at(root, "gone/Smith2024.pdf", hash, None);

    // Nothing on disk any more: the recorded path is stale.
    let outcome = checked(&path, &documents, &sources, &index, root, false);

    let file_record = resolved_outcome(outcome);
    assert_eq!(file_record.record, record_with_doi("10.1000/again"));
}

/// ledger spec scenario "Second PDF of an archived paper".
#[test]
fn a_live_work_duplicate_is_skipped_after_resolution() {
    let library = tempdir().unwrap();
    let root = library.path();
    let path = root.join("incoming.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("different-bytes"),
        pdf_with_embedded_doi("10.1000/reprint"),
    );
    let (crossref, calls) =
        fake_source(SourceName::Crossref, Ok(record_with_doi("10.1000/reprint")));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let item = item_with(root, "10.1000/reprint");
    record_at(
        root,
        "archived/Reprint2024.pdf",
        hash_for("original-bytes"),
        Some(item),
    );

    let outcome = checked(&path, &documents, &sources, &index, root, true);

    assert_eq!(
        outcome,
        FileOutcome::Skipped(SkipReason::Duplicate {
            reason: DuplicateReason::Work,
            existing_path: root.join("archived").join("Reprint2024.pdf"),
        })
    );
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "resolution still has to run once to learn the identifier the work check matches on"
    );
}

#[test]
fn a_stale_work_duplicate_does_not_block_re_admission() {
    let library = tempdir().unwrap();
    let root = library.path();
    let path = root.join("incoming.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("different-bytes-again"),
        pdf_with_embedded_doi("10.1000/reprint-again"),
    );
    let (crossref, _) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/reprint-again")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let item = item_with(root, "10.1000/reprint-again");
    record_at(
        root,
        "gone/Reprint2024.pdf",
        hash_for("original-bytes-again"),
        Some(item),
    );

    let outcome = checked(&path, &documents, &sources, &index, root, false);

    let file_record = resolved_outcome(outcome);
    assert_eq!(file_record.record, record_with_doi("10.1000/reprint-again"));
}

#[test]
fn standing_resolves_normally_when_the_library_holds_nothing() {
    let library = tempdir().unwrap();
    let root = library.path();
    let path = root.join("incoming.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("brand-new"),
        pdf_with_embedded_doi("10.1000/new"),
    );
    let (crossref, _) = fake_source(SourceName::Crossref, Ok(record_with_doi("10.1000/new")));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = checked(&path, &documents, &sources, &index, root, true);

    let file_record = resolved_outcome(outcome);
    assert_eq!(file_record.record, record_with_doi("10.1000/new"));
}

/// A resolution failure passes through untouched: the library has
/// nothing to add to a file that never produced a record.
#[test]
fn standing_passes_through_a_resolution_failure() {
    let library = tempdir().unwrap();
    let root = library.path();
    let path = root.join("incoming.pdf");
    let documents =
        FakeDocuments::new().with_file(&path, hash_for("unresolvable"), pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = vec![];
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = checked(&path, &documents, &sources, &index, root, true);

    assert_eq!(outcome, FileOutcome::Skipped(SkipReason::NoIdentifier));
}

// ---------------------------------------------------------------------
// design D2: a file is not its own duplicate
// ---------------------------------------------------------------------

/// A content match whose recorded path is the incoming file's own path
/// is not a duplicate: the run continues through resolution and, in the
/// usual case, is answered by the content index with no source asked
/// and the file never opened.
#[test]
fn a_content_match_at_the_incoming_path_is_not_a_duplicate_and_resolves_from_the_index() {
    let library = tempdir().unwrap();
    let root = library.path();
    let path = root.join("archived").join("Smith2024.pdf");
    let hash = hash_for("own-bytes");
    let documents = FakeDocuments::new().with_open_error(
        &path,
        hash.clone(),
        ExtractionError::Unreadable {
            message: "must never be opened".to_string(),
        },
    );
    let panics = PanicSource {
        name: SourceName::Crossref,
    };
    let sources: Vec<&dyn Source> = vec![&panics];
    let index = ContentIndex::new(MemoryCache::new());
    let indexed = record_with_doi("10.1000/own");
    index.put(&hash, &indexed);
    record_at(root, "archived/Smith2024.pdf", hash, None);

    let outcome = checked(&path, &documents, &sources, &index, root, true);

    let file_record = resolved_outcome(outcome);
    assert_eq!(file_record.record, indexed);
    assert!(file_record.cached, "the file's own record is not a query");
    assert_eq!(documents.open_calls(), 0);
}

/// A content match at a *different* live path is still reported as a
/// duplicate — the contrast to the case above, so passing over a file's
/// own record cannot be mistaken for passing over every record.
#[test]
fn a_content_match_at_another_live_path_is_still_a_duplicate() {
    let library = tempdir().unwrap();
    let root = library.path();
    let path = root.join("incoming").join("Copy.pdf");
    let hash = hash_for("shared-bytes");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/whatever"),
    );
    let panics = PanicSource {
        name: SourceName::Crossref,
    };
    let sources: Vec<&dyn Source> = vec![&panics];
    let index = ContentIndex::new(MemoryCache::new());
    record_at(root, "archived/Smith2024.pdf", hash, None);

    let outcome = checked(&path, &documents, &sources, &index, root, true);

    assert_eq!(
        outcome,
        FileOutcome::Skipped(SkipReason::Duplicate {
            reason: DuplicateReason::Content,
            existing_path: root.join("archived").join("Smith2024.pdf"),
        })
    );
}

/// The work check has the same hole and the same fix: a file at its
/// admitted path, annotated afterwards so its bytes (and therefore its
/// hash) changed, still resolves to the identifier the item its own
/// record links to carries, and is not reported a work duplicate of
/// itself.
#[test]
fn a_work_match_at_the_incoming_path_is_not_a_duplicate_after_a_changed_hash() {
    let library = tempdir().unwrap();
    let root = library.path();
    let path = root.join("archived").join("Reprint2024.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("annotated-bytes"),
        pdf_with_embedded_doi("10.1000/reprint"),
    );
    let (crossref, calls) =
        fake_source(SourceName::Crossref, Ok(record_with_doi("10.1000/reprint")));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());
    let item = item_with(root, "10.1000/reprint");
    record_at(
        root,
        "archived/Reprint2024.pdf",
        hash_for("original-bytes"),
        Some(item),
    );

    let outcome = checked(&path, &documents, &sources, &index, root, true);

    let file_record = resolved_outcome(outcome);
    assert_eq!(file_record.record, record_with_doi("10.1000/reprint"));
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "resolution still runs once to learn the identifier the work check matches on"
    );
}

// ---------------------------------------------------------------------
// claims_of: a file's own titles, read on their own (design D2a, task 2)
// ---------------------------------------------------------------------

#[test]
fn claims_of_reads_the_xmp_and_info_titles_of_a_file_never_resolved() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_file(
        path,
        hash_for("claims-of-no-identifier"),
        pdf_with_no_identifier()
            .with_title("The Info Title")
            .with_xmp("<dc:title><rdf:Alt><rdf:li>The Xmp Title</rdf:li></rdf:Alt></dc:title>"),
    );

    let claims = claims_of(path, &documents);

    assert_eq!(
        claims,
        vec![
            Claim {
                from: ClaimOrigin::Xmp,
                title: "The Xmp Title".to_string(),
            },
            Claim {
                from: ClaimOrigin::Info,
                title: "The Info Title".to_string(),
            },
        ]
    );
}

/// A file the content index answered for was never opened at all, so a
/// caller comparing a supplied record against it has to be able to read
/// its titles independently of whatever the content index said.
#[test]
fn claims_of_reads_titles_for_a_file_the_content_index_would_have_answered_for() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_file(
        path,
        hash_for("claims-of-indexed"),
        pdf_with_embedded_doi("10.1000/indexed-but-still-readable")
            .with_title("A Title The Index Never Saw"),
    );

    let claims = claims_of(path, &documents);

    assert_eq!(
        claims,
        vec![Claim {
            from: ClaimOrigin::Info,
            title: "A Title The Index Never Saw".to_string(),
        }]
    );
}

#[test]
fn claims_of_is_empty_for_a_file_that_cannot_be_opened() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_open_error(
        path,
        hash_for("claims-of-unreadable"),
        ExtractionError::Unreadable {
            message: "corrupt stream".to_string(),
        },
    );

    let claims = claims_of(path, &documents);

    assert_eq!(claims, Vec::new());
}

// ---------------------------------------------------------------------
// resolve_supplied: resolving an identifier somebody typed (design D2,
// D2a, D3; task 2.3)
// ---------------------------------------------------------------------

#[test]
fn resolve_supplied_reports_a_title_conflict_rather_than_refusing() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_file(
        path,
        hash_for("resolve-supplied-conflict"),
        pdf_with_no_identifier().with_title("Old Title Extracted from the PDF"),
    );
    let identifier = Identifier::Doi(doi("10.1000/supplied-conflict"));
    let (crossref, _calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi_and_title(
            "10.1000/supplied-conflict",
            "A Completely Different Title About Something Else",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];

    let supplied = resolve_supplied(path, &identifier, &documents, &sources).unwrap();

    assert_eq!(
        supplied.file.record,
        record_with_doi_and_title(
            "10.1000/supplied-conflict",
            "A Completely Different Title About Something Else",
        ),
        "a conflicting record is still handed back, not refused"
    );
    let Some(SkipReason::Conflict { field, .. }) = supplied.conflict else {
        panic!(
            "expected the disagreement reported as a conflict, got {:?}",
            supplied.conflict
        );
    };
    assert_eq!(field, "title");
}

#[test]
fn resolve_supplied_reports_no_conflict_when_the_titles_agree() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_file(
        path,
        hash_for("resolve-supplied-agrees"),
        pdf_with_no_identifier().with_title("On the Structure of Borax"),
    );
    let identifier = Identifier::Doi(doi("10.1000/supplied-agrees"));
    let (crossref, _calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/supplied-agrees")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];

    let supplied = resolve_supplied(path, &identifier, &documents, &sources).unwrap();

    assert_eq!(supplied.conflict, None);
}

#[test]
fn resolve_supplied_returns_the_attempts_when_no_service_holds_the_identifier() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_file(
        path,
        hash_for("resolve-supplied-unresolvable"),
        pdf_with_no_identifier(),
    );
    let identifier = Identifier::Doi(doi("10.1000/nowhere-supplied"));
    let (crossref, _calls) = fake_source(SourceName::Crossref, Err(SourceError::NotFound));
    let (openalex, _calls) = fake_source(SourceName::OpenAlex, Err(SourceError::NotFound));
    let sources: Vec<&dyn Source> = vec![&crossref, &openalex];

    let unresolved = resolve_supplied(path, &identifier, &documents, &sources).unwrap_err();

    assert_eq!(
        unresolved.attempts,
        vec![
            (SourceName::Crossref, SourceError::NotFound),
            (SourceName::OpenAlex, SourceError::NotFound),
        ]
    );
}

/// The doc comment states this explicitly: "nothing is written to the
/// content index here". `resolve_supplied` is not handed one at all, so
/// there is no seam through which it could write — this pins the
/// signature itself as the guarantee, rather than a side effect a future
/// change could quietly add back.
#[test]
fn resolve_supplied_takes_no_content_index_to_write_to() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_file(
        path,
        hash_for("resolve-supplied-no-index-seam"),
        pdf_with_no_identifier(),
    );
    let identifier = Identifier::Doi(doi("10.1000/no-index-seam"));
    let (crossref, _calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/no-index-seam")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];

    // If this line compiles at all, it compiles without an index
    // argument — the property under test.
    let supplied = resolve_supplied(path, &identifier, &documents, &sources).unwrap();

    assert_eq!(
        supplied.file.record,
        record_with_doi("10.1000/no-index-seam")
    );
}

// ---------------------------------------------------------------------
// remember: keeping an operator's accepted record (design D7, task 4.3)
// ---------------------------------------------------------------------

#[test]
fn remember_writes_a_record_a_later_resolve_then_answers_with() {
    let hash = hash_for("remembered-by-hand");
    let index = ContentIndex::new(MemoryCache::new());
    let record = record_with_doi("10.1000/remembered-by-hand");
    assert_eq!(index.get(&hash), None, "nothing written yet");

    remember(&index, Some(&hash), &record);

    assert_eq!(index.get(&hash), Some(record));
}

/// A hash `resolve_file` never learned — the file could not be hashed —
/// has nowhere to be written. `remember` is handed `None` rather than
/// panicking or inventing a place to keep the record.
#[test]
fn remember_with_no_hash_does_not_panic() {
    let index = ContentIndex::new(MemoryCache::new());
    let record = record_with_doi("10.1000/no-hash-to-remember-under");

    remember(&index, None, &record);
}

// ---------------------------------------------------------------------
// consult-library-first, task 3.1: standing() consults the library
// first (design D1, D3, D6, D8, D9, D10, D11)
// ---------------------------------------------------------------------

/// A [`Cache`] wrapping a [`MemoryCache`], counting every `get` and
/// `put` — what proves a library answer neither reads nor writes the
/// content index (design D6).
#[derive(Clone, Default)]
struct CountingCache {
    inner: Arc<MemoryCache>,
    gets: Arc<AtomicUsize>,
    puts: Arc<AtomicUsize>,
}

impl CountingCache {
    fn new() -> CountingCache {
        CountingCache::default()
    }

    fn gets(&self) -> usize {
        self.gets.load(Ordering::Relaxed)
    }

    fn puts(&self) -> usize {
        self.puts.load(Ordering::Relaxed)
    }
}

impl Cache for CountingCache {
    fn get(&self, key: &str) -> Option<Record> {
        self.gets.fetch_add(1, Ordering::Relaxed);
        self.inner.get(key)
    }

    fn put(&self, key: &str, record: &Record) {
        self.puts.fetch_add(1, Ordering::Relaxed);
        self.inner.put(key, record);
    }
}

/// Writes an item under `root` carrying `record`, and hands back the
/// whole item — [`item_with`] hands back only the identity, but a
/// library-answer test needs the record and provenance too, to check
/// that they travel unchanged into the verdict.
fn library_item(root: &Path, record: Record) -> Item {
    let item = Item {
        id: ItemId::from_uuid(fresh_uuid()),
        record,
    };
    let items = root.join("items");
    fs::create_dir_all(&items).unwrap();
    fs::write(items.join(format!("{}.toml", item.id)), item.to_toml()).unwrap();
    item
}

/// A library whose one artifact record names `relative`, holds `hash`,
/// and links `item`.
fn tracked_library(root: &Path, relative: &str, hash: ContentHash, item: &Item) -> Stores {
    record_at(root, relative, hash, Some(item.id.clone()));
    Stores::read(root)
}

/// design D1, D3, D6, D9: a tracked file's verdict is the item's own
/// record, reached with no extraction, no source and no content-index
/// traffic — under `cache` both true and false, since the library is
/// not the cache (design D7).
#[test]
fn a_tracked_file_resolves_from_its_item_with_no_extraction_no_source_and_no_index_traffic() {
    for cache in [true, false] {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let path = root.join("paper.pdf");
        let hash = hash_for("tracked bytes");
        let item = library_item(root, record_with_doi("10.1000/tracked"));
        let stores = tracked_library(root, "paper.pdf", hash.clone(), &item);
        let documents =
            FakeDocuments::new().with_file(&path, hash.clone(), pdf_with_no_text_layer());
        let (crossref, calls) =
            fake_source(SourceName::Crossref, Ok(record_with_doi("10.1000/wrong")));
        let sources: Vec<&dyn Source> = vec![&crossref];
        let index = CountingCache::new();
        let content_index = ContentIndex::new(index.clone());

        let result = standing(
            &path,
            &documents,
            &sources,
            &content_index,
            &config(cache),
            None,
            Some(&stores),
        );

        match &result.verdict {
            FileOutcome::Resolved(file) => {
                assert_eq!(file.record, item.record, "cache={cache}");
                assert_eq!(file.tier, Some(Provenance::Library), "cache={cache}");
                assert!(!file.cached, "cache={cache}");
                assert_eq!(file.claims, Vec::new(), "cache={cache}");
                assert_eq!(file.found, None, "cache={cache}");
                assert_eq!(
                    result.library,
                    Some(LibraryAnswer::Tracked {
                        artifact: file_artifact_id(&stores, &path, &hash),
                        item: item.id.to_string(),
                    }),
                    "cache={cache}"
                );
            }
            other => panic!("expected Resolved for a tracked file, got {other:?} (cache={cache})"),
        }

        assert_eq!(
            documents.open_calls(),
            0,
            "cache={cache}: the file must not be opened"
        );
        assert_eq!(
            calls.load(Ordering::Relaxed),
            0,
            "cache={cache}: no source may be asked"
        );
        assert_eq!(
            index.gets(),
            0,
            "cache={cache}: the content index must not be read"
        );
        assert_eq!(
            index.puts(),
            0,
            "cache={cache}: the content index must not be written"
        );
    }
}

/// The artifact identity a fixture's own [`tracked_library`] wrote,
/// read back from the store so the expected [`LibraryAnswer`] does not
/// have to hard-code a UUID the fixture generates fresh.
fn file_artifact_id(stores: &Stores, path: &Path, hash: &ContentHash) -> String {
    stores
        .consult(path, Some(hash))
        .and_then(|consulted| match consulted.answer {
            LibraryAnswer::Tracked { artifact, .. } => Some(artifact),
            _ => None,
        })
        .expect("the fixture must build a tracked file")
}

/// design D3: `verdict_event` of a tracked standing is a `resolved`
/// event whose `found` is the record's own identifier, whose `source`
/// is read from the item's provenance, and whose `library` is
/// `Tracked`.
#[test]
fn verdict_event_of_a_tracked_standing_reports_found_and_source_from_the_item() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    let hash = hash_for("tracked bytes 2");
    let mut record = record_with_doi("10.1000/tracked-verdict");
    record.borax.provenance = [("title".to_string(), borax_core::record::Source::Crossref)]
        .into_iter()
        .collect();
    let item = library_item(root, record);
    let stores = tracked_library(root, "paper.pdf", hash.clone(), &item);
    let documents = FakeDocuments::new().with_file(&path, hash, pdf_with_no_text_layer());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let result = standing(
        &path,
        &documents,
        &sources,
        &index,
        &config(true),
        None,
        Some(&stores),
    );
    let event = verdict_event(&path, &result);

    match event {
        Event::Resolved {
            found,
            source,
            identifier,
            library,
            ..
        } => {
            assert_eq!(
                found, identifier,
                "found must be the record's own identifier"
            );
            assert_eq!(source, "crossref");
            assert!(matches!(library, Some(LibraryAnswer::Tracked { .. })));
        }
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// design D3: an item whose provenance names no service reports
/// `source: "library"`, not `"cache"` — `library` appears in `source`
/// only where `cache` would otherwise appear (design D3, "Rejected:
/// `source: \"library\"` on every library answer").
#[test]
fn verdict_event_of_a_provenance_less_item_names_library_as_the_source() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    let hash = hash_for("provenance-less bytes");
    let item = library_item(root, record_with_doi("10.1000/provenance-less"));
    let stores = tracked_library(root, "paper.pdf", hash.clone(), &item);
    let documents = FakeDocuments::new().with_file(&path, hash, pdf_with_no_text_layer());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let result = standing(
        &path,
        &documents,
        &sources,
        &index,
        &config(true),
        None,
        Some(&stores),
    );
    let event = verdict_event(&path, &result);

    match event {
        Event::Resolved { source, .. } => assert_eq!(source, "library"),
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// design D2: a `dangling-item` problem — the linked item is not in the
/// store — falls back through the content index, extraction and the
/// services exactly as an untracked file would, and the fresh success
/// is written to the content index. The problem travels onto the
/// verdict and onto `verdict_event`'s `resolved` event.
#[test]
fn a_dangling_item_problem_falls_back_and_writes_the_content_index() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    let hash = hash_for("dangling bytes");
    let dangling_item = ItemId::from_uuid(fresh_uuid());
    record_at(root, "paper.pdf", hash.clone(), Some(dangling_item.clone()));
    let stores = Stores::read(root);
    let documents = FakeDocuments::new().with_file(
        &path,
        hash.clone(),
        pdf_with_embedded_doi("10.1000/fallback"),
    );
    let (crossref, calls) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/fallback")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let result = standing(
        &path,
        &documents,
        &sources,
        &index,
        &config(true),
        None,
        Some(&stores),
    );

    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "the fallback must ask a source"
    );
    assert_eq!(
        index.get(&hash),
        Some(record_with_doi("10.1000/fallback")),
        "a fresh success must still be written to the content index"
    );
    let expected_problem = LibraryAnswer::DanglingItem {
        artifact: record_id_at(root, "paper.pdf"),
        item: dangling_item.to_string(),
    };
    match &result.verdict {
        FileOutcome::Resolved(file) => {
            assert_eq!(result.library, Some(expected_problem.clone()));
            let _ = file;
        }
        other => panic!("expected Resolved for the fallback, got {other:?}"),
    }
    match verdict_event(&path, &result) {
        Event::Resolved { library, .. } => assert_eq!(library, Some(expected_problem)),
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// The artifact identity of the one record `record_at` wrote at
/// `relative` under `root`, read back from disk.
fn record_id_at(root: &Path, relative: &str) -> String {
    ArtifactStore::read(root)
        .by_path(relative)
        .expect("record_at must have written a record at this path")
        .id
        .to_string()
}

/// A dangling-linked record whose file carries no identifier at all:
/// the verdict is `NoIdentifier`, and its `verdict_event` still carries
/// the `DanglingItem` problem on the `skipped` event.
#[test]
fn a_dangling_item_problem_on_a_file_with_no_identifier_carries_through_to_the_skip() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("mystery.pdf");
    let hash = hash_for("dangling, no identifier");
    let dangling_item = ItemId::from_uuid(fresh_uuid());
    record_at(
        root,
        "mystery.pdf",
        hash.clone(),
        Some(dangling_item.clone()),
    );
    let stores = Stores::read(root);
    let documents = FakeDocuments::new().with_file(&path, hash, pdf_with_no_identifier());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let result = standing(
        &path,
        &documents,
        &sources,
        &index,
        &config(true),
        None,
        Some(&stores),
    );

    assert!(matches!(
        result.verdict,
        FileOutcome::Skipped(SkipReason::NoIdentifier)
    ));
    let expected_problem = LibraryAnswer::DanglingItem {
        artifact: record_id_at(root, "mystery.pdf"),
        item: dangling_item.to_string(),
    };
    assert_eq!(result.library, Some(expected_problem.clone()));
    match verdict_event(&path, &result) {
        Event::Skipped { library, .. } => assert_eq!(library, Some(expected_problem)),
        other => panic!("expected Event::Skipped, got {other:?}"),
    }
}

/// design D1: the content-duplicate check runs before the library is
/// consulted at all, so a file whose bytes another live record already
/// holds is a duplicate with `library: None` — the library is never
/// asked about it.
#[test]
fn a_content_duplicate_is_skipped_before_the_library_is_asked() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let _existing = write_file_for_test(root, "existing.pdf", b"shared bytes");
    let incoming = root.join("incoming.pdf");
    let hash = hash_bytes(b"shared bytes");
    record_at(root, "existing.pdf", hash.clone(), None);
    let stores = Stores::read(root);
    let exists = |_: &Path| true;
    let account = stores.account(&exists);
    let documents = FakeDocuments::new().with_file(&incoming, hash, pdf_with_no_text_layer());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let result = standing(
        &incoming,
        &documents,
        &sources,
        &index,
        &config(true),
        Some(&account),
        Some(&stores),
    );

    assert!(
        matches!(
            result.verdict,
            FileOutcome::Skipped(SkipReason::Duplicate {
                reason: DuplicateReason::Content,
                ..
            })
        ),
        "got {:?}",
        result.verdict
    );
    assert_eq!(
        result.library, None,
        "a content duplicate is decided before the library is ever asked"
    );
    assert_eq!(documents.open_calls(), 0);
}

/// Writes `bytes` at `relative` under `root`, for a fixture that needs
/// a real file on disk (the incoming file's own hash is computed by
/// the fake `Documents`, but the *existing* file the duplicate check
/// asks `exists` about need not be real here since `exists` is stubbed
/// `true` unconditionally).
fn write_file_for_test(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, bytes).unwrap();
    path
}

/// `library: None` as the argument: behaviour and events are exactly
/// today's, `library: None` on every event — the run outside any
/// library.
#[test]
fn passing_no_library_leaves_behaviour_and_events_unchanged() {
    let path = PathBuf::from("paper.pdf");
    let documents = FakeDocuments::new().with_file(
        &path,
        hash_for("no-library"),
        pdf_with_embedded_doi("10.1000/no-library"),
    );
    let (crossref, _) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/no-library")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let result = standing(
        &path,
        &documents,
        &sources,
        &index,
        &config(true),
        None,
        None,
    );

    assert_eq!(result.library, None);
    match verdict_event(&path, &result) {
        Event::Resolved { library, .. } => assert_eq!(library, None),
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// `resolve_batch` with a fixture library gives the same events at
/// concurrency 1 and at concurrency 8 — the library answer travels
/// through `map_bounded` like any other part of the verdict (design
/// D8: shared by reference, nothing learned).
#[test]
fn resolve_batch_with_a_library_gives_the_same_events_at_concurrency_one_and_eight() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let paths: Vec<PathBuf> = (0..6).map(|i| root.join(format!("paper{i}.pdf"))).collect();
    let mut documents = FakeDocuments::new();
    for (i, path) in paths.iter().enumerate() {
        let hash = hash_for(&format!("batch-{i}"));
        let item = library_item(root, record_with_doi(&format!("10.1000/batch-{i}")));
        record_at(root, &format!("paper{i}.pdf"), hash.clone(), Some(item.id));
        documents = documents.with_file(path, hash, pdf_with_no_text_layer());
    }
    let stores = Stores::read(root);
    let sources: Vec<&dyn Source> = Vec::new();

    let index_one = ContentIndex::new(MemoryCache::new());
    let run_one = resolve_batch(
        &paths,
        &documents,
        &sources,
        &index_one,
        Some(&stores),
        &|_: &Path| config(true),
        1,
    );

    let index_eight = ContentIndex::new(MemoryCache::new());
    let run_eight = resolve_batch(
        &paths,
        &documents,
        &sources,
        &index_eight,
        Some(&stores),
        &|_: &Path| config(true),
        8,
    );

    assert_eq!(run_one.events, run_eight.events);
    assert_eq!(run_one.counts, run_eight.counts);
    for event in &run_one.events {
        if let Event::Resolved { library, .. } = event {
            assert!(
                matches!(library, Some(LibraryAnswer::Tracked { .. })),
                "got {event:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------
// consult-library-first, second pass on task 3.1/5.3: the dangling-item
// problem survives every fallback outcome, not only a fresh success
// (design D2, D11) — a blocking critic finding: the verdict/retry paths
// must not silently drop the library's answer.
// ---------------------------------------------------------------------

/// A dangling-item problem whose fallback ends in
/// [`SkipReason::Unresolvable`] (no source holds the identifier): the
/// standing and its `verdict_event` still carry `DanglingItem`.
#[test]
fn a_dangling_item_problem_on_an_unresolvable_skip_carries_through() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("unresolvable.pdf");
    let hash = hash_for("dangling unresolvable");
    let dangling_item = ItemId::from_uuid(fresh_uuid());
    record_at(
        root,
        "unresolvable.pdf",
        hash.clone(),
        Some(dangling_item.clone()),
    );
    let stores = Stores::read(root);
    let documents = FakeDocuments::new().with_file(
        &path,
        hash,
        pdf_with_embedded_doi("10.1000/nobody-holds-this"),
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let result = standing(
        &path,
        &documents,
        &sources,
        &index,
        &config(true),
        None,
        Some(&stores),
    );

    assert!(
        matches!(
            result.verdict,
            FileOutcome::Skipped(SkipReason::Unresolvable { .. })
        ),
        "got {:?}",
        result.verdict
    );
    let expected_problem = LibraryAnswer::DanglingItem {
        artifact: record_id_at(root, "unresolvable.pdf"),
        item: dangling_item.to_string(),
    };
    assert_eq!(result.library, Some(expected_problem.clone()));
    match verdict_event(&path, &result) {
        Event::Skipped { library, .. } => assert_eq!(library, Some(expected_problem)),
        other => panic!("expected Event::Skipped, got {other:?}"),
    }
}

/// A dangling-item problem whose fallback resolves to a record the
/// file's own title disagrees with (`SkipReason::Conflict`): the
/// standing and its `verdict_event` still carry `DanglingItem`.
#[test]
fn a_dangling_item_problem_on_a_conflict_skip_carries_through() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("conflict.pdf");
    let hash = hash_for("dangling conflict");
    let dangling_item = ItemId::from_uuid(fresh_uuid());
    record_at(
        root,
        "conflict.pdf",
        hash.clone(),
        Some(dangling_item.clone()),
    );
    let stores = Stores::read(root);
    let documents = FakeDocuments::new().with_file(
        &path,
        hash,
        pdf_with_embedded_doi("10.1000/dangling-conflict")
            .with_title("Old Title Extracted from the PDF"),
    );
    let (crossref, _) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi_and_title(
            "10.1000/dangling-conflict",
            "A Completely Different Title About Something Else",
        )),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let result = standing(
        &path,
        &documents,
        &sources,
        &index,
        &config(true),
        None,
        Some(&stores),
    );

    assert!(
        matches!(
            result.verdict,
            FileOutcome::Skipped(SkipReason::Conflict { .. })
        ),
        "got {:?}",
        result.verdict
    );
    let expected_problem = LibraryAnswer::DanglingItem {
        artifact: record_id_at(root, "conflict.pdf"),
        item: dangling_item.to_string(),
    };
    assert_eq!(result.library, Some(expected_problem.clone()));
    match verdict_event(&path, &result) {
        Event::Skipped { library, .. } => assert_eq!(library, Some(expected_problem)),
        other => panic!("expected Event::Skipped, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// consult-library-first, third pass: every one of D1's problem kinds
// drives `standing` against a real fixture library, not only
// `DanglingItem` (reviewer finding).
// ---------------------------------------------------------------------

/// Runs `standing` for `path` against `stores` with a fixture that
/// resolves successfully by fallback, and asserts `expected` is carried
/// on the resolved `FileRecord`, on `Standing::library`, and on
/// `verdict_event`'s `resolved` event alike.
fn assert_problem_propagates_through_a_successful_fallback(
    path: &Path,
    hash: ContentHash,
    stores: &Stores,
    doi_value: &str,
    expected: LibraryAnswer,
) {
    let documents = FakeDocuments::new().with_file(path, hash, pdf_with_embedded_doi(doi_value));
    let (crossref, _) = fake_source(SourceName::Crossref, Ok(record_with_doi(doi_value)));
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let result = standing(
        path,
        &documents,
        &sources,
        &index,
        &config(true),
        None,
        Some(stores),
    );

    match &result.verdict {
        FileOutcome::Resolved(file) => {
            assert_eq!(file.library, Some(expected.clone()), "on the FileRecord");
        }
        other => panic!("expected a resolved fallback, got {other:?}"),
    }
    assert_eq!(
        result.library,
        Some(expected.clone()),
        "on Standing::library"
    );
    match verdict_event(path, &result) {
        Event::Resolved { library, .. } => assert_eq!(library, Some(expected), "on verdict_event"),
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// `UnrecognisedContent`: a record names the path, but none of its
/// history holds the file's current hash.
#[test]
fn unrecognised_content_propagates_through_standing_and_verdict_event() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    record_at(
        root,
        "paper.pdf",
        hash_for("stale bytes"),
        Some(ItemId::from_uuid(fresh_uuid())),
    );
    let artifact = ArtifactStore::read(root)
        .by_path("paper.pdf")
        .expect("record_at must have written a record")
        .id
        .to_string();
    let stores = Stores::read(root);

    assert_problem_propagates_through_a_successful_fallback(
        &path,
        hash_for("current bytes"),
        &stores,
        "10.1000/unrecognised-content",
        LibraryAnswer::UnrecognisedContent {
            artifacts: vec![artifact],
        },
    );
}

/// `Ambiguous`: two records at the path both hold the file's hash.
#[test]
fn ambiguous_propagates_through_standing_and_verdict_event() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    let hash = hash_for("shared bytes");
    record_at(root, "paper.pdf", hash.clone(), None);
    record_at(root, "paper.pdf", hash.clone(), None);
    let mut artifacts: Vec<String> = ArtifactStore::read(root)
        .iter()
        .map(|record| record.id.to_string())
        .collect();
    artifacts.sort();
    let stores = Stores::read(root);

    assert_problem_propagates_through_a_successful_fallback(
        &path,
        hash,
        &stores,
        "10.1000/ambiguous",
        LibraryAnswer::Ambiguous { artifacts },
    );
}

/// `NoItem`: the one record holding the hash links no item.
#[test]
fn no_item_propagates_through_standing_and_verdict_event() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    let hash = hash_for("no item bytes");
    record_at(root, "paper.pdf", hash.clone(), None);
    let artifact = ArtifactStore::read(root)
        .by_path("paper.pdf")
        .expect("record_at must have written a record")
        .id
        .to_string();
    let stores = Stores::read(root);

    assert_problem_propagates_through_a_successful_fallback(
        &path,
        hash,
        &stores,
        "10.1000/no-item",
        LibraryAnswer::NoItem { artifact },
    );
}

/// `UnreadableItem`: the linked item's own file does not parse.
#[test]
fn unreadable_item_propagates_through_standing_and_verdict_event() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    let hash = hash_for("unreadable item bytes");
    let bad_item = ItemId::from_uuid(fresh_uuid());
    record_at(root, "paper.pdf", hash.clone(), Some(bad_item.clone()));
    let items = root.join("items");
    fs::create_dir_all(&items).unwrap();
    let item_path = items.join(format!("badkey.{bad_item}.toml"));
    fs::write(&item_path, "not valid toml {{{").unwrap();
    let artifact = ArtifactStore::read(root)
        .by_path("paper.pdf")
        .expect("record_at must have written a record")
        .id
        .to_string();
    let message = ItemStore::read(root)
        .faults
        .iter()
        .find(|fault| fault.path == item_path)
        .expect("the bad item file must be a fault")
        .message
        .clone();
    let stores = Stores::read(root);

    assert_problem_propagates_through_a_successful_fallback(
        &path,
        hash,
        &stores,
        "10.1000/unreadable-item",
        LibraryAnswer::UnreadableItem {
            artifact,
            item: bad_item.to_string(),
            path: item_path,
            message,
        },
    );
}

/// `AmbiguousItem`: two item files carry the linked identity.
#[test]
fn ambiguous_item_propagates_through_standing_and_verdict_event() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    let hash = hash_for("ambiguous item bytes");
    let shared_item = ItemId::from_uuid(fresh_uuid());
    record_at(root, "paper.pdf", hash.clone(), Some(shared_item.clone()));
    let item = Item {
        id: shared_item.clone(),
        record: record_with_doi("10.1000/ambiguous-item"),
    };
    let items = root.join("items");
    fs::create_dir_all(&items).unwrap();
    fs::write(
        items.join(format!("a-first.{shared_item}.toml")),
        item.to_toml(),
    )
    .unwrap();
    fs::write(
        items.join(format!("b-second.{shared_item}.toml")),
        item.to_toml(),
    )
    .unwrap();
    let artifact = ArtifactStore::read(root)
        .by_path("paper.pdf")
        .expect("record_at must have written a record")
        .id
        .to_string();
    let stores = Stores::read(root);

    assert_problem_propagates_through_a_successful_fallback(
        &path,
        hash,
        &stores,
        "10.1000/ambiguous-item",
        LibraryAnswer::AmbiguousItem {
            artifact,
            item: shared_item.to_string(),
            files: vec![
                items.join(format!("a-first.{shared_item}.toml")),
                items.join(format!("b-second.{shared_item}.toml")),
            ],
        },
    );
}

/// `Unhashable`: a record names the path, but the file could not be
/// hashed — `Documents::hash` fails, so `from_index` reports `None`.
#[test]
fn unhashable_propagates_through_standing_and_verdict_event() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("paper.pdf");
    record_at(root, "paper.pdf", hash_for("irrelevant"), None);
    let artifact = ArtifactStore::read(root)
        .by_path("paper.pdf")
        .expect("record_at must have written a record")
        .id
        .to_string();
    let stores = Stores::read(root);
    let documents = FakeDocuments::new().with_hash_error(
        &path,
        ExtractionError::Unreadable {
            message: "cannot hash".to_string(),
        },
        pdf_with_embedded_doi("10.1000/unhashable"),
    );
    let (crossref, _) = fake_source(
        SourceName::Crossref,
        Ok(record_with_doi("10.1000/unhashable")),
    );
    let sources: Vec<&dyn Source> = vec![&crossref];
    let index = ContentIndex::new(MemoryCache::new());

    let result = standing(
        &path,
        &documents,
        &sources,
        &index,
        &config(true),
        None,
        Some(&stores),
    );

    let expected = LibraryAnswer::Unhashable {
        artifacts: vec![artifact],
    };
    match &result.verdict {
        FileOutcome::Resolved(file) => assert_eq!(file.library, Some(expected.clone())),
        other => panic!("expected a resolved fallback, got {other:?}"),
    }
    assert_eq!(result.library, Some(expected.clone()));
    match verdict_event(&path, &result) {
        Event::Resolved { library, .. } => assert_eq!(library, Some(expected)),
        other => panic!("expected Event::Resolved, got {other:?}"),
    }
}

/// `UnreadableRecords { listed: true, .. }`: no readable record names
/// the path, but an unrelated record file does not parse.
#[test]
fn unreadable_records_listed_propagates_through_standing_and_verdict_event() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("unrecorded.pdf");
    let records = root.join(".borax").join("artifacts");
    fs::create_dir_all(&records).unwrap();
    fs::write(records.join("broken.toml"), "not valid toml {{{").unwrap();
    let stores = Stores::read(root);

    assert_problem_propagates_through_a_successful_fallback(
        &path,
        hash_for("unrecorded bytes"),
        &stores,
        "10.1000/unreadable-records-listed",
        LibraryAnswer::UnreadableRecords {
            listed: true,
            unreadable: 1,
        },
    );
}

/// `UnreadableRecords { listed: false, .. }`: `.borax/artifacts/`
/// itself cannot be listed.
#[cfg(unix)]
#[test]
fn unreadable_records_unlistable_propagates_through_standing_and_verdict_event() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let root = dir.path();
    let path = root.join("unrecorded.pdf");
    let records = root.join(".borax").join("artifacts");
    fs::create_dir_all(&records).unwrap();
    fs::set_permissions(&records, fs::Permissions::from_mode(0o000)).unwrap();

    if fs::read_dir(&records).is_ok() {
        fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping unreadable_records_unlistable_propagates_through_standing_and_verdict_event: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }
    let stores = Stores::read(root);

    assert_problem_propagates_through_a_successful_fallback(
        &path,
        hash_for("unrecorded bytes 2"),
        &stores,
        "10.1000/unreadable-records-unlisted",
        LibraryAnswer::UnreadableRecords {
            listed: false,
            unreadable: 0,
        },
    );

    fs::set_permissions(&records, fs::Permissions::from_mode(0o755)).unwrap();
}

// ---------------------------------------------------------------------
// report-extraction-per-file, task 1.2: extraction_of and extraction
// (design D2, D3, D10)
// ---------------------------------------------------------------------

/// A PDF whose only page is blank and whose metadata carries a title
/// holding no identifier — the "no text layer" controlled case design
/// D2 fixes the boundary with.
fn pdf_blank_with_title() -> FakePdf {
    FakePdf::new()
        .with_pages(vec![Ok(" \n".to_string())])
        .with_title("A Title")
}

/// A PDF with a page of readable prose and no identifier, whose
/// metadata also carries a title holding no identifier — the "text
/// without identifier" controlled case beside [`pdf_blank_with_title`].
fn pdf_prose_with_title() -> FakePdf {
    pdf_with_no_identifier().with_title("A Title")
}

/// The blank-page fake whose Info title itself holds a DOI: `scan_info`
/// scans the title, so this is `found` with `embedded-metadata`.
fn pdf_blank_with_doi_title(value: &str) -> FakePdf {
    FakePdf::new()
        .with_pages(vec![Ok(" \n".to_string())])
        .with_title(value)
}

/// A PDF carrying `value` as an arXiv identifier in its first page's
/// text, resolved on the text-layer pass.
fn pdf_with_text_arxiv(value: &str) -> FakePdf {
    FakePdf::new().with_pages(vec![Ok(format!("see arXiv:{value} for details"))])
}

/// A PDF whose first page cannot be read at all.
fn pdf_with_page_error(error: ExtractionError) -> FakePdf {
    FakePdf::new().with_pages(vec![Err(error)])
}

#[test]
fn extraction_of_maps_an_embedded_doi_to_found_with_its_tier() {
    let result: Result<Extracted, ExtractionError> = Ok(Extracted {
        identifier: FoundIdentifier::Doi(doi("10.1234/embedded")),
        tier: Tier::EmbeddedMetadata,
    });

    assert_eq!(
        extraction_of(&result),
        Extraction::Found {
            identifier: "doi:10.1234/embedded".to_string(),
            tier: "embedded-metadata".to_string(),
        }
    );
}

#[test]
fn extraction_of_maps_a_versioned_arxiv_id_to_found_with_text_layer() {
    let result: Result<Extracted, ExtractionError> = Ok(Extracted {
        identifier: FoundIdentifier::Arxiv(ArxivId::parse("2401.12345v2").unwrap()),
        tier: Tier::TextLayer,
    });

    assert_eq!(
        extraction_of(&result),
        Extraction::Found {
            identifier: "arXiv:2401.12345v2".to_string(),
            tier: "text-layer".to_string(),
        }
    );
}

/// design D2's table, mapped exhaustively: `Unreadable` keeps its
/// message unchanged, `Encrypted` becomes `Encrypted`, `NoTextLayer`
/// stays `NoTextLayer`, and `NoIdentifierFound` becomes
/// `TextWithoutIdentifier` — the one row that renames its
/// `ExtractionError` counterpart.
#[test]
fn extraction_of_maps_every_extraction_error_to_its_design_d2_row() {
    let cases: Vec<(ExtractionError, Extraction)> = vec![
        (
            ExtractionError::Unreadable {
                message: "corrupt stream".to_string(),
            },
            Extraction::Unreadable {
                message: "corrupt stream".to_string(),
            },
        ),
        (ExtractionError::Encrypted, Extraction::Encrypted),
        (ExtractionError::NoTextLayer, Extraction::NoTextLayer),
        (
            ExtractionError::NoIdentifierFound,
            Extraction::TextWithoutIdentifier,
        ),
    ];

    for (error, expected) in cases {
        let result: Result<Extracted, ExtractionError> = Err(error.clone());
        assert_eq!(extraction_of(&result), expected, "error {error:?}");
    }
}

#[test]
fn extraction_over_a_blank_page_with_a_title_is_no_text_layer() {
    let path = Path::new("paper.pdf");
    let documents =
        FakeDocuments::new().with_file(path, hash_for("blank-with-title"), pdf_blank_with_title());

    let result = extraction(path, &documents, &ExtractionConfig::default());

    assert_eq!(result, Extraction::NoTextLayer);
}

#[test]
fn extraction_over_readable_prose_with_a_title_is_text_without_identifier() {
    let path = Path::new("paper.pdf");
    let documents =
        FakeDocuments::new().with_file(path, hash_for("prose-with-title"), pdf_prose_with_title());

    let result = extraction(path, &documents, &ExtractionConfig::default());

    assert_eq!(result, Extraction::TextWithoutIdentifier);
}

/// design D2: a blank page whose Info title itself holds a DOI is
/// `found` with `embedded-metadata`, because `scan_info` scans the
/// title among its other fields.
#[test]
fn extraction_over_a_blank_page_whose_title_holds_a_doi_is_found() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_file(
        path,
        hash_for("blank-with-doi-title"),
        pdf_blank_with_doi_title("10.1234/example"),
    );

    let result = extraction(path, &documents, &ExtractionConfig::default());

    assert_eq!(
        result,
        Extraction::Found {
            identifier: "doi:10.1234/example".to_string(),
            tier: "embedded-metadata".to_string(),
        }
    );
}

#[test]
fn extraction_over_a_document_with_no_pages_is_no_text_layer() {
    let path = Path::new("paper.pdf");
    let documents =
        FakeDocuments::new().with_file(path, hash_for("no-pages"), pdf_with_no_text_layer());

    let result = extraction(path, &documents, &ExtractionConfig::default());

    assert_eq!(result, Extraction::NoTextLayer);
}

#[test]
fn extraction_over_an_xmp_doi_is_found_with_embedded_metadata() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_file(
        path,
        hash_for("xmp-doi"),
        pdf_with_embedded_doi("10.1234/xmp"),
    );

    let result = extraction(path, &documents, &ExtractionConfig::default());

    assert_eq!(
        result,
        Extraction::Found {
            identifier: "doi:10.1234/xmp".to_string(),
            tier: "embedded-metadata".to_string(),
        }
    );
}

#[test]
fn extraction_over_an_open_error_of_encrypted_is_encrypted() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_open_error(
        path,
        hash_for("encrypted"),
        ExtractionError::Encrypted,
    );

    let result = extraction(path, &documents, &ExtractionConfig::default());

    assert_eq!(result, Extraction::Encrypted);
}

#[test]
fn extraction_over_an_open_error_of_unreadable_keeps_its_message() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_open_error(
        path,
        hash_for("unreadable"),
        ExtractionError::Unreadable {
            message: "corrupt stream".to_string(),
        },
    );

    let result = extraction(path, &documents, &ExtractionConfig::default());

    assert_eq!(
        result,
        Extraction::Unreadable {
            message: "corrupt stream".to_string(),
        }
    );
}

#[test]
fn extraction_over_a_page_text_error_of_unreadable_on_the_first_page_is_unreadable() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_file(
        path,
        hash_for("page-text-unreadable"),
        pdf_with_page_error(ExtractionError::Unreadable {
            message: "malformed content stream".to_string(),
        }),
    );

    let result = extraction(path, &documents, &ExtractionConfig::default());

    assert_eq!(
        result,
        Extraction::Unreadable {
            message: "malformed content stream".to_string(),
        }
    );
}

/// design D2's "no page it read held text, including when it read none
/// at all" clause, from the `page_limit: 0` side: a document whose only
/// page would print a DOI gives `no-text-layer` when the text pass is
/// disabled before it can read that page.
#[test]
fn extraction_with_page_limit_zero_over_a_doi_bearing_page_is_no_text_layer() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_file(
        path,
        hash_for("page-limit-zero"),
        pdf_with_text_arxiv("2401.12345"),
    );

    let result = extraction(path, &documents, &ExtractionConfig { page_limit: 0 });

    assert_eq!(result, Extraction::NoTextLayer);
}

// ---------------------------------------------------------------------
// report-extraction-per-file, task 1.2: regression guard
// ---------------------------------------------------------------------
//
// `resolve_file` must keep collapsing `NoTextLayer` and
// `NoIdentifierFound` into `SkipReason::NoIdentifier`, and `Encrypted`
// into `SkipReason::Unreadable`, exactly as it does today. This change
// adds a per-file report beside resolution; it does not touch
// resolution's own skip reasons. The collapse is a known deviation that
// Phase 3 owns (design D9, `openspec/STATE.md`).

#[test]
fn resolve_file_still_skips_a_blank_page_with_a_title_as_no_identifier() {
    let path = Path::new("paper.pdf");
    let documents =
        FakeDocuments::new().with_file(path, hash_for("regression-blank"), pdf_blank_with_title());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));

    assert_eq!(skipped_outcome(outcome), SkipReason::NoIdentifier);
}

#[test]
fn resolve_file_still_skips_prose_with_a_title_as_no_identifier() {
    let path = Path::new("paper.pdf");
    let documents =
        FakeDocuments::new().with_file(path, hash_for("regression-prose"), pdf_prose_with_title());
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));

    assert_eq!(skipped_outcome(outcome), SkipReason::NoIdentifier);
}

#[test]
fn resolve_file_still_skips_an_encrypted_file_as_unreadable_with_its_message() {
    let path = Path::new("paper.pdf");
    let documents = FakeDocuments::new().with_open_error(
        path,
        hash_for("regression-encrypted"),
        ExtractionError::Encrypted,
    );
    let sources: Vec<&dyn Source> = Vec::new();
    let index = ContentIndex::new(MemoryCache::new());

    let outcome = resolve_file(path, &documents, &sources, &index, &config(true));

    assert_eq!(
        skipped_outcome(outcome),
        SkipReason::Unreadable {
            message: "PDF is encrypted".to_string(),
        }
    );
}
