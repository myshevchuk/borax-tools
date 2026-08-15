#![allow(clippy::unwrap_used)]

use borax_core::identifier::{Doi, Isbn};
use borax_core::record::{DateParts, EntryType, Name, Source};
use borax_sources::crossref::{entry_type, parse};
use borax_sources::source::ParseError;

const ARTICLE: &str = include_str!("cassettes/crossref-article.json");
const BOOK: &str = include_str!("cassettes/crossref-book.json");

// --- entry_type mapping ---

#[test]
fn entry_type_maps_journal_article_to_article() {
    assert_eq!(entry_type("journal-article"), EntryType::Article);
}

#[test]
fn entry_type_maps_posted_content_to_preprint() {
    assert_eq!(entry_type("posted-content"), EntryType::Preprint);
}

#[test]
fn entry_type_maps_book_to_book() {
    assert_eq!(entry_type("book"), EntryType::Book);
}

#[test]
fn entry_type_maps_monograph_to_book() {
    assert_eq!(entry_type("monograph"), EntryType::Book);
}

#[test]
fn entry_type_maps_edited_book_to_book() {
    assert_eq!(entry_type("edited-book"), EntryType::Book);
}

#[test]
fn entry_type_maps_reference_book_to_book() {
    assert_eq!(entry_type("reference-book"), EntryType::Book);
}

#[test]
fn entry_type_maps_book_chapter_to_chapter() {
    assert_eq!(entry_type("book-chapter"), EntryType::Chapter);
}

#[test]
fn entry_type_maps_book_part_to_chapter() {
    assert_eq!(entry_type("book-part"), EntryType::Chapter);
}

#[test]
fn entry_type_maps_book_section_to_chapter() {
    assert_eq!(entry_type("book-section"), EntryType::Chapter);
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
fn entry_type_maps_report_component_to_report() {
    assert_eq!(entry_type("report-component"), EntryType::Report);
}

#[test]
fn entry_type_maps_standard_to_standard() {
    assert_eq!(entry_type("standard"), EntryType::Standard);
}

#[test]
fn entry_type_maps_unknown_string_to_article() {
    assert_eq!(entry_type("some-future-type"), EntryType::Article);
}

// --- parsing the article cassette ---

#[test]
fn parse_article_maps_entry_type() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(record.entry_type, EntryType::Article);
}

#[test]
fn parse_article_extracts_doi() {
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
fn parse_article_extracts_volume_issue_pages() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(record.volume.as_deref(), Some("171"));
    assert_eq!(record.issue.as_deref(), Some("4356"));
    assert_eq!(record.pages.as_deref(), Some("737-738"));
}

#[test]
fn parse_article_extracts_publisher() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(
        record.publisher.as_deref(),
        Some("Springer Science and Business Media LLC")
    );
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
fn parse_article_extracts_authors_preserving_case() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(record.authors.len(), 2);
    assert_eq!(
        record.authors[0],
        Name {
            family: "WATSON".to_string(),
            given: Some("J. D.".to_string()),
        }
    );
    assert_eq!(
        record.authors[1],
        Name {
            family: "CRICK".to_string(),
            given: Some("F. H. C.".to_string()),
        }
    );
}

#[test]
fn parse_article_attributes_fields_to_crossref() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(
        record.borax.provenance.get("title"),
        Some(&Source::Crossref)
    );
    assert_eq!(record.borax.provenance.get("DOI"), Some(&Source::Crossref));
    assert_eq!(
        record.borax.provenance.get("author"),
        Some(&Source::Crossref)
    );
    assert_eq!(
        record.borax.provenance.get("container-title"),
        Some(&Source::Crossref)
    );
}

#[test]
fn parse_article_leaves_confidence_unset() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(record.borax.confidence, None);
}

// The cassette carries `"subject": []`, which is what Crossref sends for
// most works. An empty array is not data, so it is dropped rather than
// carried into every record and sidecar.
#[test]
fn parse_article_drops_an_empty_subject_array() {
    let record = parse(ARTICLE).unwrap();
    assert_eq!(record.borax.source_fields.get("subject"), None);
}

#[test]
fn parse_preserves_a_populated_subject_array() {
    let body = r#"{"message":{"DOI":"10.1038/171737a0","type":"journal-article",
        "subject":["Genetics","Molecular Biology"]}}"#;
    let record = parse(body).unwrap();
    assert_eq!(
        record.borax.source_fields.get("subject"),
        Some(&serde_json::json!(["Genetics", "Molecular Biology"]))
    );
}

// --- parsing the book cassette ---

#[test]
fn parse_book_maps_entry_type() {
    let record = parse(BOOK).unwrap();
    assert_eq!(record.entry_type, EntryType::Book);
}

#[test]
fn parse_book_extracts_isbn_from_first_entry() {
    let record = parse(BOOK).unwrap();
    assert_eq!(record.isbn, Some(Isbn::parse("9780521663960").unwrap()));
}

#[test]
fn parse_book_extracts_publisher_and_title() {
    let record = parse(BOOK).unwrap();
    assert_eq!(
        record.publisher.as_deref(),
        Some("Cambridge University Press")
    );
    assert_eq!(
        record.title.as_deref(),
        Some("An Introduction to Fluid Dynamics")
    );
}

// --- error cases ---

#[test]
fn parse_rejects_non_json_body() {
    let error = parse("not json at all").unwrap_err();
    assert!(matches!(error, ParseError::Malformed { .. }));
}

#[test]
fn parse_requires_message_field() {
    let error = parse(r#"{"status":"ok"}"#).unwrap_err();
    assert_eq!(error, ParseError::MissingField { field: "message" });
}

#[test]
fn parse_requires_doi_in_message() {
    let error = parse(r#"{"message":{"type":"journal-article"}}"#).unwrap_err();
    assert_eq!(error, ParseError::MissingField { field: "DOI" });
}

#[test]
fn parse_rejects_unparsable_doi() {
    let error = parse(r#"{"message":{"DOI":"nonsense","type":"journal-article"}}"#).unwrap_err();
    assert!(matches!(error, ParseError::Invalid { field: "DOI", .. }));
}

#[test]
fn parse_requires_type_in_message() {
    let error = parse(r#"{"message":{"DOI":"10.1038/171737a0"}}"#).unwrap_err();
    assert_eq!(error, ParseError::MissingField { field: "type" });
}

#[test]
fn parse_tolerates_malformed_issued_shape() {
    let record = parse(
        r#"{"message":{"DOI":"10.1038/171737a0","type":"journal-article","issued":{"date-parts":[]}}}"#,
    )
    .unwrap();
    assert_eq!(record.issued, None);
}

#[test]
fn parse_tolerates_invalid_isbn() {
    let record =
        parse(r#"{"message":{"DOI":"10.1038/171737a0","type":"book","ISBN":["not-an-isbn"]}}"#)
            .unwrap();
    assert_eq!(record.isbn, None);
}

#[test]
fn parse_tolerates_extra_unknown_fields() {
    let record = parse(
        r#"{"message":{"DOI":"10.1038/171737a0","type":"journal-article","totally-unknown":42,"another":{"nested":true}}}"#,
    )
    .unwrap();
    assert_eq!(record.doi, Some(Doi::parse("10.1038/171737a0").unwrap()));
}
