#![allow(clippy::unwrap_used)]

use borax_core::identifier::Doi;
use borax_core::record::{DateParts, EntryType, Name, Source};
use borax_sources::openalex::{entry_type, parse, split_display_name};
use borax_sources::source::ParseError;

const ARTICLE: &str = include_str!("cassettes/openalex-article.json");

// --- entry_type mapping ---

#[test]
fn entry_type_maps_article_to_article() {
    assert_eq!(entry_type("article"), EntryType::Article);
}

#[test]
fn entry_type_maps_preprint_to_preprint() {
    assert_eq!(entry_type("preprint"), EntryType::Preprint);
}

#[test]
fn entry_type_maps_book_to_book() {
    assert_eq!(entry_type("book"), EntryType::Book);
}

#[test]
fn entry_type_maps_book_chapter_to_chapter() {
    assert_eq!(entry_type("book-chapter"), EntryType::Chapter);
}

#[test]
fn entry_type_maps_dissertation_to_thesis() {
    assert_eq!(entry_type("dissertation"), EntryType::Thesis);
}

#[test]
fn entry_type_maps_report_to_report() {
    assert_eq!(entry_type("report"), EntryType::Report);
}

#[test]
fn entry_type_maps_standard_to_standard() {
    assert_eq!(entry_type("standard"), EntryType::Standard);
}

#[test]
fn entry_type_maps_patent_to_patent() {
    assert_eq!(entry_type("patent"), EntryType::Patent);
}

#[test]
fn entry_type_maps_unknown_string_to_article() {
    assert_eq!(entry_type("dataset"), EntryType::Article);
}

// --- split_display_name ---

#[test]
fn split_display_name_splits_three_word_name_on_last_space() {
    assert_eq!(
        split_display_name("James Dewey Watson"),
        Name {
            family: "Watson".to_string(),
            given: Some("James Dewey".to_string()),
        }
    );
}

#[test]
fn split_display_name_single_word_has_no_given() {
    assert_eq!(
        split_display_name("Plato"),
        Name {
            family: "Plato".to_string(),
            given: None,
        }
    );
}

#[test]
fn split_display_name_trims_surrounding_whitespace() {
    assert_eq!(
        split_display_name("  James Dewey Watson  "),
        Name {
            family: "Watson".to_string(),
            given: Some("James Dewey".to_string()),
        }
    );
}

#[test]
fn split_display_name_with_internal_double_space_splits_on_last_space_character() {
    // "Jean  Paul" has two space characters between the words; the
    // split falls on the rightmost one, leaving a trailing space in
    // `given` rather than treating the run of spaces as one separator.
    assert_eq!(
        split_display_name("Jean  Paul"),
        Name {
            family: "Paul".to_string(),
            given: Some("Jean ".to_string()),
        }
    );
}

// --- parsing the article cassette ---

#[test]
fn parse_article_maps_entry_type() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(record.entry_type, EntryType::Article);
}

#[test]
fn parse_article_normalizes_doi_from_resolver_url() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(record.doi, Some(Doi::parse("10.1038/171737a0").unwrap()));
}

#[test]
fn parse_article_extracts_title() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(
        record.title.as_deref(),
        Some("Molecular Structure of Nucleic Acids: A Structure for Deoxyribose Nucleic Acid")
    );
}

#[test]
fn parse_article_extracts_container_title() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(record.container_title.as_deref(), Some("Nature"));
}

#[test]
fn parse_article_extracts_volume_issue_and_pages() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(record.volume.as_deref(), Some("171"));
    assert_eq!(record.issue.as_deref(), Some("4356"));
    assert_eq!(record.pages.as_deref(), Some("737-738"));
}

#[test]
fn parse_article_extracts_issued_date() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(
        record.issued,
        Some(DateParts {
            year: 1953,
            month: Some(4),
            day: Some(25),
        })
    );
}

#[test]
fn parse_article_first_author_family_name() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(
        record.authors.first().map(|a| a.family.as_str()),
        Some("Watson")
    );
}

#[test]
fn parse_article_attributes_fields_to_openalex() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(
        record.borax.provenance.get("title"),
        Some(&Source::OpenAlex)
    );
    assert_eq!(record.borax.confidence, None);
}

#[test]
fn parse_article_preserves_openalex_id_in_source_fields() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(
        record.borax.source_fields.get("openalex_id"),
        Some(&serde_json::json!("https://openalex.org/W2126466006"))
    );
}

// --- pages fallback ---

#[test]
fn parse_pages_from_first_page_alone_when_last_page_absent() {
    let record = parse(r#"{"type":"article","biblio":{"first_page":"5"}}"#).unwrap();
    assert_eq!(record.pages.as_deref(), Some("5"));
}

#[test]
fn parse_pages_absent_when_neither_first_nor_last_page_present() {
    let record = parse(r#"{"type":"article","biblio":{}}"#).unwrap();
    assert_eq!(record.pages, None);
}

// --- date fallback ---

#[test]
fn parse_falls_back_to_publication_year_when_date_absent() {
    let record = parse(r#"{"type":"article","publication_year":1999}"#).unwrap();
    assert_eq!(
        record.issued,
        Some(DateParts {
            year: 1999,
            month: None,
            day: None,
        })
    );
}

#[test]
fn parse_falls_back_to_publication_year_when_date_unparsable() {
    let record =
        parse(r#"{"type":"article","publication_date":"not-a-date","publication_year":2001}"#)
            .unwrap();
    assert_eq!(
        record.issued,
        Some(DateParts {
            year: 2001,
            month: None,
            day: None,
        })
    );
}

// --- title fallback ---

#[test]
fn parse_falls_back_to_display_name_when_title_absent() {
    let record = parse(r#"{"type":"article","display_name":"Some Title"}"#).unwrap();
    assert_eq!(record.title.as_deref(), Some("Some Title"));
}

// --- doi optional ---

#[test]
fn parse_accepts_null_doi() {
    let record = parse(r#"{"type":"article","doi":null}"#).unwrap();
    assert_eq!(record.doi, None);
}

#[test]
fn parse_rejects_unparsable_doi() {
    let error = parse(r#"{"type":"article","doi":"garbage"}"#).unwrap_err();
    assert!(matches!(error, ParseError::Invalid { field: "doi", .. }));
}

// --- errors ---

#[test]
fn parse_rejects_non_json_body() {
    let error = parse("not json at all").unwrap_err();
    assert!(matches!(error, ParseError::Malformed { .. }));
}

#[test]
fn parse_rejects_json_array() {
    let error = parse("[1,2,3]").unwrap_err();
    assert!(matches!(error, ParseError::Malformed { .. }));
}

#[test]
fn parse_requires_type_field() {
    let error = parse(r#"{"title":"Untyped Work"}"#).unwrap_err();
    assert_eq!(error, ParseError::MissingField { field: "type" });
}
