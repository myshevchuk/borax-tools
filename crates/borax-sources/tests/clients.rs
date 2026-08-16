#![allow(clippy::unwrap_used)]

use std::sync::Mutex;

use borax_core::identifier::{ArxivId, Doi, Identifier, Isbn, Pmid};
use borax_sources::arxiv::ArxivClient;
use borax_sources::crossref::CrossrefClient;
use borax_sources::dispatch::resolve;
use borax_sources::http::{HttpRequest, HttpResponse, Politeness, Transport, TransportError};
use borax_sources::openalex::OpenAlexClient;
use borax_sources::source::{Source, SourceError, SourceName};

const CROSSREF_ARTICLE: &str = include_str!("cassettes/crossref-article.json");
const OPENALEX_ARTICLE: &str = include_str!("cassettes/openalex-article.json");
const ARXIV_PREPRINT: &str = include_str!("cassettes/arxiv-preprint.xml");
const ARXIV_EMPTY: &str = include_str!("cassettes/arxiv-empty.xml");

/// A [`Transport`] whose answer is fixed at construction, recording
/// every request it is asked to perform.
struct FakeTransport {
    response: Result<HttpResponse, TransportError>,
    seen: Mutex<Vec<HttpRequest>>,
}

impl FakeTransport {
    fn ok(status: u16, body: &str) -> FakeTransport {
        FakeTransport {
            response: Ok(HttpResponse {
                status,
                body: body.to_string(),
            }),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn failing(error: TransportError) -> FakeTransport {
        FakeTransport {
            response: Err(error),
            seen: Mutex::new(Vec::new()),
        }
    }
}

impl Transport for &FakeTransport {
    fn get(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        self.seen.lock().unwrap().push(request.clone());
        self.response.clone()
    }
}

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

// ================= Crossref: request building =================

#[test]
fn crossref_request_url_and_headers_for_doi() {
    let politeness = Politeness::default();
    let client = CrossrefClient::new((), politeness.clone());
    let request = client.request(&doi_identifier()).unwrap();

    assert_eq!(
        request.url,
        "https://api.crossref.org/works/10.1038/171737a0"
    );
    assert_eq!(
        request.headers,
        vec![
            ("User-Agent".to_string(), politeness.user_agent()),
            ("Accept".to_string(), "application/json".to_string()),
        ]
    );
}

#[test]
fn crossref_request_is_none_for_identifiers_it_does_not_support() {
    let client = CrossrefClient::new((), Politeness::default());
    assert!(client.request(&arxiv_identifier()).is_none());
    assert!(client.request(&pmid_identifier()).is_none());
    assert!(client.request(&isbn_identifier()).is_none());
}

// ================= OpenAlex: request building =================

#[test]
fn openalex_request_url_for_doi_with_no_mailto() {
    let client = OpenAlexClient::new((), Politeness::default());
    let request = client.request(&doi_identifier()).unwrap();
    assert_eq!(
        request.url,
        "https://api.openalex.org/works/doi:10.1038/171737a0"
    );
}

#[test]
fn openalex_request_url_with_mailto_percent_encodes_plus_and_at() {
    let politeness = Politeness {
        mailto: Some("user+tag@example.org".to_string()),
    };
    let client = OpenAlexClient::new((), politeness);
    let request = client.request(&doi_identifier()).unwrap();
    assert_eq!(
        request.url,
        "https://api.openalex.org/works/doi:10.1038/171737a0?mailto=user%2Btag%40example.org"
    );
}

#[test]
fn openalex_request_url_for_pmid() {
    let client = OpenAlexClient::new((), Politeness::default());
    let request = client
        .request(&Identifier::Pmid(Pmid::parse("12345678").unwrap()))
        .unwrap();
    assert_eq!(request.url, "https://api.openalex.org/works/pmid:12345678");
}

#[test]
fn openalex_request_is_none_for_arxiv_and_isbn() {
    let client = OpenAlexClient::new((), Politeness::default());
    assert!(client.request(&arxiv_identifier()).is_none());
    assert!(client.request(&isbn_identifier()).is_none());
}

// ================= arXiv: request building =================

#[test]
fn arxiv_request_url_uses_bare_id_stripped_of_version() {
    let client = ArxivClient::new((), Politeness::default());
    let request = client
        .request(&Identifier::Arxiv(ArxivId::parse("2401.12345v2").unwrap()))
        .unwrap();
    assert_eq!(
        request.url,
        "https://export.arxiv.org/api/query?id_list=2401.12345"
    );
}

#[test]
fn arxiv_request_headers_are_user_agent_only_even_with_mailto_configured() {
    let politeness = Politeness {
        mailto: Some("a@b.example".to_string()),
    };
    let client = ArxivClient::new((), politeness.clone());
    let request = client
        .request(&Identifier::Arxiv(ArxivId::parse("2401.12345v2").unwrap()))
        .unwrap();
    assert_eq!(
        request.headers,
        vec![("User-Agent".to_string(), politeness.user_agent())]
    );
}

#[test]
fn arxiv_request_is_none_for_doi() {
    let client = ArxivClient::new((), Politeness::default());
    assert!(client.request(&doi_identifier()).is_none());
}

// ================= Crossref: fetch =================

#[test]
fn crossref_fetch_200_returns_record_and_sends_expected_request() {
    let transport = FakeTransport::ok(200, CROSSREF_ARTICLE);
    let client = CrossrefClient::new(&transport, Politeness::default());

    let record = client.fetch(&doi_identifier()).unwrap();
    assert_eq!(record.doi, Some(Doi::parse("10.1038/171737a0").unwrap()));

    let seen = transport.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].url,
        "https://api.crossref.org/works/10.1038/171737a0"
    );
}

#[test]
fn crossref_fetch_404_is_not_found() {
    let transport = FakeTransport::ok(404, "");
    let client = CrossrefClient::new(&transport, Politeness::default());
    assert_eq!(client.fetch(&doi_identifier()), Err(SourceError::NotFound));
}

#[test]
fn crossref_fetch_429_is_rate_limited() {
    let transport = FakeTransport::ok(429, "");
    let client = CrossrefClient::new(&transport, Politeness::default());
    assert_eq!(
        client.fetch(&doi_identifier()),
        Err(SourceError::RateLimited)
    );
}

#[test]
fn crossref_fetch_500_is_unavailable() {
    let transport = FakeTransport::ok(500, "");
    let client = CrossrefClient::new(&transport, Politeness::default());
    assert!(matches!(
        client.fetch(&doi_identifier()),
        Err(SourceError::Unavailable { .. })
    ));
}

#[test]
fn crossref_fetch_200_with_unparsable_body_is_malformed() {
    let transport = FakeTransport::ok(200, "not json at all");
    let client = CrossrefClient::new(&transport, Politeness::default());
    assert!(matches!(
        client.fetch(&doi_identifier()),
        Err(SourceError::Malformed { .. })
    ));
}

#[test]
fn crossref_fetch_transport_timeout_is_unavailable() {
    let transport = FakeTransport::failing(TransportError::Timeout);
    let client = CrossrefClient::new(&transport, Politeness::default());
    assert!(matches!(
        client.fetch(&doi_identifier()),
        Err(SourceError::Unavailable { .. })
    ));
}

#[test]
fn crossref_fetch_unsupported_identifier_sends_no_request() {
    let transport = FakeTransport::ok(200, CROSSREF_ARTICLE);
    let client = CrossrefClient::new(&transport, Politeness::default());
    assert!(matches!(
        client.fetch(&arxiv_identifier()),
        Err(SourceError::Unavailable { .. })
    ));
    assert!(transport.seen.lock().unwrap().is_empty());
}

// ================= OpenAlex: fetch =================

#[test]
fn openalex_fetch_200_returns_normalized_doi() {
    let transport = FakeTransport::ok(200, OPENALEX_ARTICLE);
    let client = OpenAlexClient::new(&transport, Politeness::default());
    let record = client.fetch(&doi_identifier()).unwrap();
    assert_eq!(record.doi, Some(Doi::parse("10.1038/171737a0").unwrap()));
}

#[test]
fn openalex_fetch_unsupported_identifier_sends_no_request() {
    let transport = FakeTransport::ok(200, OPENALEX_ARTICLE);
    let client = OpenAlexClient::new(&transport, Politeness::default());
    assert!(matches!(
        client.fetch(&arxiv_identifier()),
        Err(SourceError::Unavailable { .. })
    ));
    assert!(transport.seen.lock().unwrap().is_empty());
}

// ================= arXiv: fetch =================

#[test]
fn arxiv_fetch_200_returns_record_with_arxiv_id() {
    let transport = FakeTransport::ok(200, ARXIV_PREPRINT);
    let client = ArxivClient::new(&transport, Politeness::default());
    let record = client.fetch(&arxiv_identifier()).unwrap();
    assert_eq!(
        record.borax.arxiv.as_ref().map(ArxivId::id),
        Some("1706.03762")
    );
}

#[test]
fn arxiv_fetch_200_with_empty_feed_is_not_found() {
    let transport = FakeTransport::ok(200, ARXIV_EMPTY);
    let client = ArxivClient::new(&transport, Politeness::default());
    assert_eq!(
        client.fetch(&arxiv_identifier()),
        Err(SourceError::NotFound)
    );
}

#[test]
fn arxiv_fetch_unsupported_identifier_sends_no_request() {
    let transport = FakeTransport::ok(200, ARXIV_PREPRINT);
    let client = ArxivClient::new(&transport, Politeness::default());
    assert!(matches!(
        client.fetch(&doi_identifier()),
        Err(SourceError::Unavailable { .. })
    ));
    assert!(transport.seen.lock().unwrap().is_empty());
}

// ================= name() / supports() =================

#[test]
fn crossref_name_and_supports() {
    let transport = FakeTransport::ok(200, CROSSREF_ARTICLE);
    let client = CrossrefClient::new(&transport, Politeness::default());
    assert_eq!(client.name(), SourceName::Crossref);
    assert!(client.supports(&doi_identifier()));
    assert!(!client.supports(&arxiv_identifier()));
    assert!(!client.supports(&pmid_identifier()));
    assert!(!client.supports(&isbn_identifier()));
}

#[test]
fn openalex_name_and_supports() {
    let transport = FakeTransport::ok(200, OPENALEX_ARTICLE);
    let client = OpenAlexClient::new(&transport, Politeness::default());
    assert_eq!(client.name(), SourceName::OpenAlex);
    assert!(client.supports(&doi_identifier()));
    assert!(client.supports(&pmid_identifier()));
    assert!(!client.supports(&arxiv_identifier()));
    assert!(!client.supports(&isbn_identifier()));
}

#[test]
fn arxiv_name_and_supports() {
    let transport = FakeTransport::ok(200, ARXIV_PREPRINT);
    let client = ArxivClient::new(&transport, Politeness::default());
    assert_eq!(client.name(), SourceName::Arxiv);
    assert!(client.supports(&arxiv_identifier()));
    assert!(!client.supports(&doi_identifier()));
    assert!(!client.supports(&pmid_identifier()));
    assert!(!client.supports(&isbn_identifier()));
}

// ================= dispatch integration =================

#[test]
fn all_three_clients_plug_into_dispatch_and_crossref_wins_for_a_doi() {
    let crossref_transport = FakeTransport::ok(200, CROSSREF_ARTICLE);
    let openalex_transport = FakeTransport::ok(200, OPENALEX_ARTICLE);
    let arxiv_transport = FakeTransport::ok(200, ARXIV_PREPRINT);

    let crossref = CrossrefClient::new(&crossref_transport, Politeness::default());
    let openalex = OpenAlexClient::new(&openalex_transport, Politeness::default());
    let arxiv = ArxivClient::new(&arxiv_transport, Politeness::default());

    let sources: Vec<&dyn Source> = vec![&crossref, &openalex, &arxiv];
    let resolved = resolve(&sources, &doi_identifier()).unwrap();
    assert_eq!(resolved.source, SourceName::Crossref);
}
