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
