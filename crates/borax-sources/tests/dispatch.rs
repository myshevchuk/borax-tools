#![allow(clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use borax_core::identifier::{ArxivId, Doi, Identifier, Isbn, Pmid};
use borax_core::record::{EntryType, Record};
use borax_sources::dispatch::{Resolved, priority, resolve};
use borax_sources::source::{Source, SourceError, SourceName};

fn doi_identifier() -> Identifier {
    Identifier::Doi(Doi::parse("10.1038/171737a0").unwrap())
}

fn arxiv_identifier() -> Identifier {
    Identifier::Arxiv(ArxivId::parse("1706.03762").unwrap())
}

fn pmid_identifier() -> Identifier {
    Identifier::Pmid(Pmid::parse("13054692").unwrap())
}

fn isbn_identifier() -> Identifier {
    Identifier::Isbn(Isbn::parse("9780521663960").unwrap())
}

fn article() -> Record {
    Record::new(EntryType::Article)
}

/// A [`Source`] whose answer and support predicate are fixed at
/// construction, and which counts how many times [`Source::fetch`] was
/// called on it.
struct FakeSource {
    name: SourceName,
    supports: Box<dyn Fn(&Identifier) -> bool + Sync>,
    response: Result<Record, SourceError>,
    calls: AtomicUsize,
}

impl FakeSource {
    fn always(name: SourceName, response: Result<Record, SourceError>) -> FakeSource {
        FakeSource {
            name,
            supports: Box::new(|_identifier| true),
            response,
            calls: AtomicUsize::new(0),
        }
    }

    fn unsupported(name: SourceName, response: Result<Record, SourceError>) -> FakeSource {
        FakeSource {
            name,
            supports: Box::new(|_identifier| false),
            response,
            calls: AtomicUsize::new(0),
        }
    }
}

impl Source for FakeSource {
    fn name(&self) -> SourceName {
        self.name
    }

    fn supports(&self, identifier: &Identifier) -> bool {
        (self.supports)(identifier)
    }

    fn fetch(&self, _identifier: &Identifier) -> Result<Record, SourceError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.response.clone()
    }
}

// --- priority ---

#[test]
fn priority_for_doi_is_crossref_then_openalex_then_datacite() {
    assert_eq!(
        priority(&doi_identifier()),
        vec![
            SourceName::Crossref,
            SourceName::OpenAlex,
            SourceName::DataCite
        ]
    );
}

#[test]
fn priority_for_arxiv_is_arxiv_then_openalex() {
    assert_eq!(
        priority(&arxiv_identifier()),
        vec![SourceName::Arxiv, SourceName::OpenAlex]
    );
}

#[test]
fn priority_for_pmid_is_pubmed_then_openalex() {
    assert_eq!(
        priority(&pmid_identifier()),
        vec![SourceName::PubMed, SourceName::OpenAlex]
    );
}

#[test]
fn priority_for_isbn_is_openalex_only() {
    assert_eq!(priority(&isbn_identifier()), vec![SourceName::OpenAlex]);
}

// --- resolve: success and fallback ---

#[test]
fn resolve_succeeds_on_first_source_without_consulting_the_rest() {
    let crossref = FakeSource::always(SourceName::Crossref, Ok(article()));
    let openalex = FakeSource::always(SourceName::OpenAlex, Ok(article()));

    let sources: Vec<&dyn Source> = vec![&crossref, &openalex];
    let result = resolve(&sources, &doi_identifier());

    assert_eq!(
        result,
        Ok(Resolved {
            record: article(),
            source: SourceName::Crossref,
        })
    );
    assert_eq!(openalex.calls.load(Ordering::Relaxed), 0);
}

#[test]
fn resolve_falls_back_to_openalex_when_crossref_is_unavailable() {
    let crossref = FakeSource::always(
        SourceName::Crossref,
        Err(SourceError::Unavailable {
            message: "503".to_string(),
        }),
    );
    let openalex = FakeSource::always(SourceName::OpenAlex, Ok(article()));

    let sources: Vec<&dyn Source> = vec![&crossref, &openalex];
    let result = resolve(&sources, &doi_identifier());

    assert_eq!(
        result,
        Ok(Resolved {
            record: article(),
            source: SourceName::OpenAlex,
        })
    );
}

// --- resolve: failure reporting ---

#[test]
fn resolve_reports_unresolved_when_every_source_says_not_found() {
    let crossref = FakeSource::always(SourceName::Crossref, Err(SourceError::NotFound));
    let openalex = FakeSource::always(SourceName::OpenAlex, Err(SourceError::NotFound));
    let datacite = FakeSource::always(SourceName::DataCite, Err(SourceError::NotFound));

    let sources: Vec<&dyn Source> = vec![&crossref, &openalex, &datacite];
    let unresolved = resolve(&sources, &doi_identifier()).unwrap_err();

    assert_eq!(
        unresolved.attempts,
        vec![
            (SourceName::Crossref, SourceError::NotFound),
            (SourceName::OpenAlex, SourceError::NotFound),
            (SourceName::DataCite, SourceError::NotFound),
        ]
    );
    assert!(unresolved.is_conclusive());
}

#[test]
fn resolve_mixed_failures_are_not_conclusive() {
    let crossref = FakeSource::always(
        SourceName::Crossref,
        Err(SourceError::Unavailable {
            message: "network error".to_string(),
        }),
    );
    let openalex = FakeSource::always(SourceName::OpenAlex, Err(SourceError::NotFound));

    let sources: Vec<&dyn Source> = vec![&crossref, &openalex];
    let unresolved = resolve(&sources, &doi_identifier()).unwrap_err();

    assert!(!unresolved.is_conclusive());
}

#[test]
fn resolve_with_no_sources_is_not_conclusive() {
    let sources: Vec<&dyn Source> = vec![];
    let unresolved = resolve(&sources, &doi_identifier()).unwrap_err();

    assert!(unresolved.attempts.is_empty());
    assert!(!unresolved.is_conclusive());
}

// --- resolve: filtering ---

#[test]
fn resolve_never_fetches_a_source_that_does_not_support_the_identifier() {
    let crossref = FakeSource::unsupported(SourceName::Crossref, Ok(article()));

    let sources: Vec<&dyn Source> = vec![&crossref];
    let unresolved = resolve(&sources, &doi_identifier()).unwrap_err();

    assert_eq!(crossref.calls.load(Ordering::Relaxed), 0);
    assert!(unresolved.attempts.is_empty());
}

#[test]
fn resolve_never_consults_a_source_outside_priority_order() {
    let pubmed = FakeSource::always(SourceName::PubMed, Ok(article()));

    let sources: Vec<&dyn Source> = vec![&pubmed];
    let unresolved = resolve(&sources, &doi_identifier()).unwrap_err();

    assert_eq!(pubmed.calls.load(Ordering::Relaxed), 0);
    assert!(unresolved.attempts.is_empty());
}

// --- resolve: ordering and determinism ---

#[test]
fn resolve_consults_sources_in_priority_order_regardless_of_input_order() {
    let crossref = FakeSource::always(SourceName::Crossref, Err(SourceError::NotFound));
    let openalex = FakeSource::always(SourceName::OpenAlex, Err(SourceError::NotFound));

    // Passed in reverse of priority order.
    let sources: Vec<&dyn Source> = vec![&openalex, &crossref];
    let unresolved = resolve(&sources, &doi_identifier()).unwrap_err();

    assert_eq!(
        unresolved.attempts,
        vec![
            (SourceName::Crossref, SourceError::NotFound),
            (SourceName::OpenAlex, SourceError::NotFound),
        ]
    );
}

#[test]
fn resolve_is_deterministic() {
    let crossref_a = FakeSource::always(SourceName::Crossref, Ok(article()));
    let openalex_a = FakeSource::always(SourceName::OpenAlex, Ok(article()));
    let sources_a: Vec<&dyn Source> = vec![&crossref_a, &openalex_a];

    let crossref_b = FakeSource::always(SourceName::Crossref, Ok(article()));
    let openalex_b = FakeSource::always(SourceName::OpenAlex, Ok(article()));
    let sources_b: Vec<&dyn Source> = vec![&crossref_b, &openalex_b];

    assert_eq!(
        resolve(&sources_a, &doi_identifier()),
        resolve(&sources_b, &doi_identifier())
    );
}
