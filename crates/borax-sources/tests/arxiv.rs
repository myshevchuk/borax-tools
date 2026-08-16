#![allow(clippy::unwrap_used)]

use borax_core::record::{DateParts, EntryType, Name, Source};
use borax_sources::arxiv::parse;
use borax_sources::source::ParseError;

const PREPRINT: &str = include_str!("cassettes/arxiv-preprint.xml");
const EMPTY: &str = include_str!("cassettes/arxiv-empty.xml");

/// Wrap a single `<entry>...</entry>` fragment in a minimal, otherwise
/// valid Atom feed carrying the namespaces the arXiv reader expects.
fn feed_with_entry(entry: &str) -> String {
    format!(
        r#"<?xml version='1.0' encoding='UTF-8'?>
<feed xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/" xmlns:arxiv="http://arxiv.org/schemas/atom" xmlns="http://www.w3.org/2005/Atom">
  <id>https://arxiv.org/api/test</id>
  <title>arXiv Query: test</title>
  <updated>2024-01-01T00:00:00Z</updated>
  {entry}
</feed>"#
    )
}

// --- parsing the real preprint cassette ---

#[test]
fn parse_preprint_entry_type_is_always_preprint() {
    let record = parse(PREPRINT).unwrap();
    assert_eq!(record.entry_type, EntryType::Preprint);
}

#[test]
fn parse_preprint_extracts_arxiv_id_and_version() {
    let record = parse(PREPRINT).unwrap();
    let arxiv = record.borax.arxiv.unwrap();
    assert_eq!(arxiv.id(), "1706.03762");
    assert_eq!(arxiv.version(), Some(7));
}

#[test]
fn parse_preprint_extracts_title() {
    let record = parse(PREPRINT).unwrap();
    assert_eq!(record.title.as_deref(), Some("Attention Is All You Need"));
}

#[test]
fn parse_preprint_extracts_authors_in_feed_order() {
    let record = parse(PREPRINT).unwrap();
    let expected = vec![
        Name {
            family: "Vaswani".to_string(),
            given: Some("Ashish".to_string()),
        },
        Name {
            family: "Shazeer".to_string(),
            given: Some("Noam".to_string()),
        },
        Name {
            family: "Parmar".to_string(),
            given: Some("Niki".to_string()),
        },
        Name {
            family: "Uszkoreit".to_string(),
            given: Some("Jakob".to_string()),
        },
        Name {
            family: "Jones".to_string(),
            given: Some("Llion".to_string()),
        },
        Name {
            family: "Gomez".to_string(),
            given: Some("Aidan N.".to_string()),
        },
        Name {
            family: "Kaiser".to_string(),
            given: Some("Lukasz".to_string()),
        },
        Name {
            family: "Polosukhin".to_string(),
            given: Some("Illia".to_string()),
        },
    ];
    assert_eq!(record.authors, expected);
}

#[test]
fn parse_preprint_extracts_issued_date_from_published() {
    let record = parse(PREPRINT).unwrap();
    assert_eq!(
        record.issued,
        Some(DateParts {
            year: 2017,
            month: Some(6),
            day: Some(12),
        })
    );
}

// The cassette carries no `arxiv:doi` or `arxiv:journal_ref` element,
// so both fields are absent from the parsed record.
#[test]
fn parse_preprint_has_no_doi_or_container_title() {
    let record = parse(PREPRINT).unwrap();
    assert_eq!(record.doi, None);
    assert_eq!(record.container_title, None);
}

#[test]
fn parse_preprint_attributes_fields_to_arxiv() {
    let record = parse(PREPRINT).unwrap();
    assert_eq!(record.borax.provenance.get("title"), Some(&Source::Arxiv));
    assert_eq!(record.borax.confidence, None);
}

#[test]
fn parse_preprint_preserves_primary_category_and_categories() {
    let record = parse(PREPRINT).unwrap();
    assert_eq!(
        record.borax.source_fields.get("primary_category"),
        Some(&serde_json::json!("cs.CL"))
    );
    assert_eq!(
        record.borax.source_fields.get("categories"),
        Some(&serde_json::json!(["cs.CL", "cs.LG"]))
    );
}

// --- the empty feed is NotFound, not a malformed body ---

#[test]
fn parse_empty_feed_is_not_found() {
    let error = parse(EMPTY).unwrap_err();
    assert_eq!(error, ParseError::NotFound);
}

// --- errors ---

#[test]
fn parse_rejects_non_xml_body() {
    let error = parse("not xml at all <<<").unwrap_err();
    assert!(matches!(error, ParseError::Malformed { .. }));
}

#[test]
fn parse_rejects_truncated_xml() {
    let error = parse("<?xml version='1.0'?><feed><entry><id>abc").unwrap_err();
    assert!(matches!(error, ParseError::Malformed { .. }));
}

#[test]
fn parse_entry_without_id_is_missing_field() {
    let feed = feed_with_entry("<entry><title>No Id</title></entry>");
    let error = parse(&feed).unwrap_err();
    assert_eq!(error, ParseError::MissingField { field: "id" });
}

#[test]
fn parse_entry_with_non_arxiv_id_is_invalid() {
    let feed =
        feed_with_entry("<entry><id>http://example.com/x</id><title>Not arXiv</title></entry>");
    let error = parse(&feed).unwrap_err();
    assert!(matches!(error, ParseError::Invalid { field: "id", .. }));
}

// --- entity decoding ---

#[test]
fn parse_decodes_xml_entities_in_title() {
    let feed = feed_with_entry(
        "<entry><id>http://arxiv.org/abs/2401.00001</id><title>Foo &amp; Bar</title></entry>",
    );
    let record = parse(&feed).unwrap();
    assert_eq!(record.title.as_deref(), Some("Foo & Bar"));
}

// --- whitespace collapsing ---

// The real cassette's title happens to fit on one line, so it does not
// exercise the wrap-collapse behavior the doc comment describes. This
// synthetic feed pins that behavior directly.
#[test]
fn parse_collapses_internal_whitespace_runs_in_title() {
    let feed = feed_with_entry(
        "<entry><id>http://arxiv.org/abs/2401.00002</id><title>\n      Long   Title\n      That Wraps\n    </title></entry>",
    );
    let record = parse(&feed).unwrap();
    assert_eq!(record.title.as_deref(), Some("Long Title That Wraps"));
}

// --- multiple entries ---

#[test]
fn parse_uses_first_entry_when_feed_has_several() {
    let first = "<entry><id>http://arxiv.org/abs/2401.00003</id><title>First</title></entry>";
    let second = "<entry><id>http://arxiv.org/abs/2401.00004</id><title>Second</title></entry>";
    let feed = feed_with_entry(&format!("{first}{second}"));
    let record = parse(&feed).unwrap();
    assert_eq!(record.title.as_deref(), Some("First"));
    assert_eq!(record.borax.arxiv.unwrap().id(), "2401.00003");
}

// ================= old-style identifiers =================

// An identifier from before April 2007 is `archive.subject/YYMMNNN`,
// and the `/` in it is part of the identifier. Reading the abstract
// URL's last path segment drops the archive and leaves a bare number
// that is not an arXiv identifier at all.
#[test]
fn parse_reads_an_old_style_identifier_whose_archive_contains_a_slash() {
    let feed = feed_with_entry(
        "<entry>\
           <id>http://arxiv.org/abs/math.GT/0309136v1</id>\
           <title>An Older Submission</title>\
           <published>2003-09-08T00:00:00Z</published>\
           <author><name>Petra Nowak</name></author>\
         </entry>",
    );

    let record = parse(&feed).unwrap();

    let arxiv = record.borax.arxiv.unwrap();
    assert_eq!(
        arxiv.id(),
        "math.GT/0309136",
        "the archive is part of the identifier"
    );
    assert_eq!(arxiv.version(), Some(1));
}

#[test]
fn parse_still_reads_a_new_style_identifier_from_its_abstract_url() {
    let feed = feed_with_entry(
        "<entry>\
           <id>http://arxiv.org/abs/2401.12345v2</id>\
           <title>Preprints and Their Stamps</title>\
           <published>2024-01-03T00:00:00Z</published>\
         </entry>",
    );

    let record = parse(&feed).unwrap();

    let arxiv = record.borax.arxiv.unwrap();
    assert_eq!(arxiv.id(), "2401.12345");
    assert_eq!(arxiv.version(), Some(2));
}

// A bare identifier where an abstract URL was expected is still read,
// so a feed that omits the URL form is not a parse failure.
#[test]
fn parse_reads_an_identifier_that_is_not_an_abstract_url() {
    let feed = feed_with_entry(
        "<entry>\
           <id>2401.12345v2</id>\
           <title>Preprints and Their Stamps</title>\
         </entry>",
    );

    let record = parse(&feed).unwrap();

    let arxiv = record.borax.arxiv.unwrap();
    assert_eq!(arxiv.id(), "2401.12345");
    assert_eq!(arxiv.version(), Some(2));
}
