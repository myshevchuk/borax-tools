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
//!
//! Most tests here go through [`UreqTransport`] plus a bare `parse`
//! function, pinning the format each service answers in. The ones
//! under "source clients" go one layer up, through
//! [`CrossrefClient`], [`OpenAlexClient`], and [`ArxivClient`]
//! themselves: those build the request URL and map HTTP statuses to
//! [`SourceError`](borax_sources::source::SourceError), neither of
//! which the parse-level tests touch.

#![allow(clippy::unwrap_used)]

use std::time::Duration;

use borax_core::identifier::{ArxivId, Doi, Identifier};
use borax_sources::arxiv::{self, ArxivClient};
use borax_sources::crossref::{self, CrossrefClient};
use borax_sources::dispatch::resolve;
use borax_sources::http::{HttpRequest, Politeness, Transport, classify};
use borax_sources::openalex::{self, OpenAlexClient};
use borax_sources::source::{Source, SourceName};
use borax_sources::transport::UreqTransport;

/// The identifier every live test asks about: a 1953 Nature paper that
/// will not be retracted, re-registered, or removed.
const DOI: &str = "10.1038/171737a0";

/// How these tests identify themselves, taking a contact address from
/// `BORAX_MAILTO` when the environment sets one.
///
/// The scheduled job that runs these is a recurring automated caller of
/// three services that answer for free, and Crossref and OpenAlex offer
/// their polite pools to callers who say how to reach them. A developer
/// running the suite by hand sets nothing and is anonymous beyond the
/// User-Agent, which is the right default for a one-off run.
fn politeness() -> Politeness {
    Politeness {
        mailto: std::env::var("BORAX_MAILTO")
            .ok()
            .filter(|address| !address.is_empty()),
    }
}

fn request(url: &str) -> HttpRequest {
    HttpRequest {
        url: url.to_string(),
        headers: vec![
            ("User-Agent".to_string(), politeness().user_agent()),
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

// ================= Source clients: the URL builders, live =================
//
// The tests above go through a bare `parse` function fed a body this
// file fetched by hand; they say nothing about whether a client's own
// URL still lands on the service. These do: they build a real
// [`UreqTransport`] and call [`Source::fetch`] on the client itself, so
// a URL scheme or status mapping that has drifted shows up here.

fn transport() -> UreqTransport {
    UreqTransport::new(Duration::from_secs(30))
}

fn doi_identifier() -> Identifier {
    Identifier::Doi(Doi::parse(DOI).unwrap())
}

#[test]
#[ignore = "hits the live Crossref API"]
fn crossref_client_still_resolves_the_shared_doi_over_a_real_transport() {
    let client = CrossrefClient::new(transport(), politeness());
    let record = client.fetch(&doi_identifier()).unwrap();

    assert_eq!(record.doi.unwrap().as_str(), DOI);
    assert!(record.title.unwrap().contains("Molecular Structure"));
}

#[test]
#[ignore = "hits the live OpenAlex API"]
fn openalex_client_still_resolves_the_shared_doi_over_a_real_transport() {
    let client = OpenAlexClient::new(transport(), politeness());
    let record = client.fetch(&doi_identifier()).unwrap();

    assert_eq!(record.doi.unwrap().as_str(), DOI);
    assert!(record.title.is_some());
}

#[test]
#[ignore = "hits the live arXiv API"]
fn arxiv_client_still_resolves_the_shared_preprint_over_a_real_transport() {
    let client = ArxivClient::new(transport(), politeness());
    let identifier = Identifier::Arxiv(ArxivId::parse("1706.03762").unwrap());
    let record = client.fetch(&identifier).unwrap();

    assert_eq!(record.borax.arxiv.unwrap().id(), "1706.03762");
    assert!(record.title.unwrap().contains("Attention"));
    assert!(!record.authors.is_empty());
}

/// arXiv identifiers issued before April 2007 keep an archive prefix —
/// `hep-th/9711200`, not `9711200` — and the `/` is part of the id, not
/// a path separator. A reader that read the abstract URL's last path
/// segment instead of everything after `/abs/` would silently drop the
/// prefix; this pins the fix against the live API rather than a frozen
/// cassette, since it is exactly the kind of format only the service
/// itself can confirm.
///
/// `hep-th/9711200` is Maldacena's 1997 AdS/CFT paper — permanent, and
/// not going to be withdrawn.
#[test]
#[ignore = "hits the live arXiv API"]
fn arxiv_client_keeps_the_archive_prefix_of_a_pre_2007_identifier() {
    let client = ArxivClient::new(transport(), politeness());
    let identifier = Identifier::Arxiv(ArxivId::parse("hep-th/9711200").unwrap());
    let record = client.fetch(&identifier).unwrap();

    assert_eq!(record.borax.arxiv.unwrap().id(), "hep-th/9711200");
    assert!(record.title.is_some());
    assert!(!record.authors.is_empty());
}

#[test]
#[ignore = "hits the live Crossref API"]
fn crossref_client_accepts_a_configured_mailto_in_the_polite_pool() {
    let politeness = Politeness {
        mailto: Some("borax-tools-test@example.invalid".to_string()),
    };
    let client = CrossrefClient::new(transport(), politeness);
    let record = client.fetch(&doi_identifier()).unwrap();

    assert_eq!(record.doi.unwrap().as_str(), DOI);
}

#[test]
#[ignore = "hits the live Crossref and OpenAlex APIs"]
fn dispatch_resolve_finds_the_shared_doi_through_the_live_crossref_and_openalex_clients() {
    let crossref = CrossrefClient::new(transport(), politeness());
    let openalex = OpenAlexClient::new(transport(), politeness());
    let sources: Vec<&dyn Source> = vec![&crossref, &openalex];

    let resolved = resolve(&sources, &doi_identifier()).unwrap();

    assert_eq!(resolved.source, SourceName::Crossref);
    assert_eq!(resolved.record.doi.unwrap().as_str(), DOI);
}
