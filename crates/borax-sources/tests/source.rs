#![allow(clippy::unwrap_used)]

use borax_sources::source::SourceName;

// --- SourceName::parse ---

#[test]
fn parse_round_trips_every_variant_through_as_str() {
    for name in SourceName::ALL {
        assert_eq!(SourceName::parse(name.as_str()), Some(name));
    }
}

#[test]
fn parse_is_case_sensitive() {
    assert_eq!(SourceName::parse("Crossref"), None);
    assert_eq!(SourceName::parse("arXiv"), None);
}

#[test]
fn parse_rejects_an_unknown_name() {
    assert_eq!(SourceName::parse("semanticscholar"), None);
}

#[test]
fn parse_rejects_the_empty_string() {
    assert_eq!(SourceName::parse(""), None);
}

// --- SourceName::ALL ---

#[test]
fn all_contains_every_variant_exactly_once() {
    let mut sorted = SourceName::ALL.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), SourceName::ALL.len());
}

#[test]
fn all_is_in_declaration_order() {
    assert_eq!(
        SourceName::ALL,
        [
            SourceName::Crossref,
            SourceName::OpenAlex,
            SourceName::Arxiv,
            SourceName::DataCite,
            SourceName::PubMed,
        ]
    );
}

// ---------------------------------------------------------------------
// SUPPORTED
// ---------------------------------------------------------------------

// Every supported source is one borax has a client for. `ALL` still
// lists the rest, because the dispatch table routes to them.
#[test]
fn supported_lists_only_the_sources_with_a_client() {
    assert_eq!(
        SourceName::SUPPORTED.to_vec(),
        vec![
            SourceName::Crossref,
            SourceName::OpenAlex,
            SourceName::Arxiv
        ]
    );
}

#[test]
fn every_supported_source_is_also_in_all() {
    for source in SourceName::SUPPORTED {
        assert!(SourceName::ALL.contains(&source));
    }
}

#[test]
fn is_supported_answers_for_every_variant() {
    assert!(SourceName::Crossref.is_supported());
    assert!(SourceName::OpenAlex.is_supported());
    assert!(SourceName::Arxiv.is_supported());
    assert!(!SourceName::DataCite.is_supported());
    assert!(!SourceName::PubMed.is_supported());
}
