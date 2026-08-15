//! Contract tests against the live services.
//!
//! Ignored by default: `cargo test` stays offline and deterministic.
//! Run them deliberately — `cargo test -p borax-sources -- --ignored` —
//! or from the scheduled CI job, whose purpose is to notice when a
//! service's responses drift away from the recorded cassettes the rest
//! of the suite trusts.
//!
//! These are the only tests that exercise [`UreqTransport`], which is
//! why it has no offline coverage: everything above it is decided
//! before a request is sent.

#![allow(clippy::unwrap_used)]

use std::time::Duration;

use borax_sources::http::{HttpRequest, Politeness, Transport, classify};
use borax_sources::transport::UreqTransport;
use borax_sources::{arxiv, crossref, openalex};

/// The identifier every live test asks about: a 1953 Nature paper that
/// will not be retracted, re-registered, or removed.
const DOI: &str = "10.1038/171737a0";

fn request(url: &str) -> HttpRequest {
    HttpRequest {
        url: url.to_string(),
        headers: vec![
            ("User-Agent".to_string(), Politeness::default().user_agent()),
            ("Accept".to_string(), "application/json".to_string()),
        ],
    }
}

fn fetch(url: &str) -> String {
    let transport = UreqTransport::new(Duration::from_secs(30));
    let response = transport.get(&request(url)).unwrap();
    assert_eq!(classify(response.status), None, "unexpected status");
    response.body
}

#[test]
#[ignore = "hits the live Crossref API"]
fn crossref_still_answers_in_the_shape_the_reader_expects() {
    let body = fetch(&format!("https://api.crossref.org/works/{DOI}"));
    let record = crossref::parse(&body).unwrap();

    assert_eq!(record.doi.unwrap().as_str(), DOI);
    assert!(record.title.unwrap().contains("Molecular Structure"));
    assert!(!record.authors.is_empty());
    assert_eq!(record.issued.unwrap().year, 1953);
}

#[test]
#[ignore = "hits the live OpenAlex API"]
fn openalex_still_answers_in_the_shape_the_reader_expects() {
    let body = fetch(&format!("https://api.openalex.org/works/doi:{DOI}"));
    let record = openalex::parse(&body).unwrap();

    assert_eq!(record.doi.unwrap().as_str(), DOI);
    assert!(record.title.is_some());
    assert_eq!(record.issued.unwrap().year, 1953);
}

#[test]
#[ignore = "hits the live arXiv API"]
fn arxiv_still_answers_in_the_shape_the_reader_expects() {
    let body = fetch("https://export.arxiv.org/api/query?id_list=1706.03762");
    let record = arxiv::parse(&body).unwrap();

    assert_eq!(record.borax.arxiv.unwrap().id(), "1706.03762");
    assert!(record.title.unwrap().contains("Attention"));
    assert_eq!(record.authors.len(), 8);
}

#[test]
#[ignore = "hits the live arXiv API"]
fn arxiv_reports_an_unknown_identifier_as_an_empty_feed() {
    let body = fetch("https://export.arxiv.org/api/query?id_list=2401.99999");
    assert!(matches!(
        arxiv::parse(&body),
        Err(borax_sources::source::ParseError::NotFound)
    ));
}

#[test]
#[ignore = "hits the live Crossref API"]
fn crossref_reports_an_unknown_doi_as_404() {
    let transport = UreqTransport::new(Duration::from_secs(30));
    let response = transport
        .get(&request(
            "https://api.crossref.org/works/10.9999/definitely-not-a-real-doi",
        ))
        .unwrap();

    assert_eq!(
        classify(response.status),
        Some(borax_sources::source::SourceError::NotFound)
    );
}
