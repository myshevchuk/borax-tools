#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use borax_core::identifier::{ArxivId, Doi, Isbn, Pmid};
use borax_core::record::{BoraxExt, DateParts, EntryType, Name, Record, Source};

fn maximal_record() -> Record {
    let mut provenance = BTreeMap::new();
    provenance.insert("title".to_string(), Source::Crossref);
    provenance.insert("DOI".to_string(), Source::Extraction);

    let mut source_fields = BTreeMap::new();
    source_fields.insert(
        "crossref-extra".to_string(),
        serde_json::json!({"funder": ["NSF"], "is-referenced-by-count": 3}),
    );

    Record {
        entry_type: EntryType::Article,
        title: Some("Bor\u{e1}x \u{2014} a study".to_string()),
        authors: vec![
            Name {
                family: "Smith".to_string(),
                given: Some("Jane".to_string()),
            },
            Name {
                family: "Doe".to_string(),
                given: None,
            },
        ],
        issued: Some(DateParts {
            year: 2024,
            month: Some(5),
            day: Some(17),
        }),
        container_title: Some("Journal of Chemical Education".to_string()),
        volume: Some("12".to_string()),
        issue: Some("3".to_string()),
        pages: Some("100-110".to_string()),
        publisher: Some("ACS".to_string()),
        doi: Some(Doi::parse("10.1021/jacs.4c01234").unwrap()),
        pmid: Some(Pmid::parse("12345678").unwrap()),
        isbn: Some(Isbn::parse("978-1-59327-828-1").unwrap()),
        borax: BoraxExt {
            arxiv: Some(ArxivId::parse("2401.12345v2").unwrap()),
            confidence: Some(0.97),
            provenance,
            source_fields,
        },
    }
}

#[test]
fn maximal_record_round_trips_losslessly() {
    let record = maximal_record();
    let json = serde_json::to_string(&record).unwrap();
    let parsed: Record = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, record);
}

#[test]
fn minimal_record_round_trips_losslessly() {
    let record = Record::new(EntryType::Report);
    let json = serde_json::to_string(&record).unwrap();
    let parsed: Record = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, record);
}

#[test]
fn serialized_type_key_uses_csl_string() {
    let record = maximal_record();
    let value: serde_json::Value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["type"], "article-journal");
}

#[test]
fn serialized_authors_appear_under_author_key_as_family_given_objects() {
    let record = maximal_record();
    let value: serde_json::Value = serde_json::to_value(&record).unwrap();
    let authors = value["author"].as_array().unwrap();
    assert_eq!(authors.len(), 2);
    assert_eq!(authors[0]["family"], "Smith");
    assert_eq!(authors[0]["given"], "Jane");
    assert_eq!(authors[1]["family"], "Doe");
    assert!(authors[1].get("given").is_none());
}

#[test]
fn serialized_container_title_uses_hyphenated_key() {
    let record = maximal_record();
    let value: serde_json::Value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["container-title"], "Journal of Chemical Education");
}

#[test]
fn serialized_pages_use_page_key() {
    let record = maximal_record();
    let value: serde_json::Value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["page"], "100-110");
}

#[test]
fn serialized_doi_uses_uppercase_key_holding_plain_string() {
    let record = maximal_record();
    let value: serde_json::Value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["DOI"], "10.1021/jacs.4c01234");
}

#[test]
fn minimal_record_omits_empty_optional_fields_from_json() {
    let record = Record::new(EntryType::Report);
    let value: serde_json::Value = serde_json::to_value(&record).unwrap();
    let object = value.as_object().unwrap();
    assert!(!object.contains_key("title"));
    assert!(!object.contains_key("author"));
    assert!(!object.contains_key("borax"));
    assert!(!object.contains_key("DOI"));
    assert!(!object.contains_key("PMID"));
    assert!(!object.contains_key("ISBN"));
}

// ---------------------------------------------------------------------
// DateParts
// ---------------------------------------------------------------------

#[test]
fn date_parts_year_only_serializes_to_single_element_array() {
    let date = DateParts {
        year: 2024,
        month: None,
        day: None,
    };
    let value = serde_json::to_value(date).unwrap();
    assert_eq!(value, serde_json::json!({"date-parts": [[2024]]}));
}

#[test]
fn date_parts_year_month_serializes_to_two_element_array() {
    let date = DateParts {
        year: 2024,
        month: Some(5),
        day: None,
    };
    let value = serde_json::to_value(date).unwrap();
    assert_eq!(value, serde_json::json!({"date-parts": [[2024, 5]]}));
}

#[test]
fn date_parts_full_serializes_to_three_element_array() {
    let date = DateParts {
        year: 2024,
        month: Some(5),
        day: Some(17),
    };
    let value = serde_json::to_value(date).unwrap();
    assert_eq!(value, serde_json::json!({"date-parts": [[2024, 5, 17]]}));
}

#[test]
fn date_parts_year_only_round_trips() {
    let date = DateParts {
        year: 2024,
        month: None,
        day: None,
    };
    let json = serde_json::to_string(&date).unwrap();
    let parsed: DateParts = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, date);
}

#[test]
fn date_parts_year_month_round_trips() {
    let date = DateParts {
        year: 2024,
        month: Some(5),
        day: None,
    };
    let json = serde_json::to_string(&date).unwrap();
    let parsed: DateParts = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, date);
}

#[test]
fn date_parts_full_round_trips() {
    let date = DateParts {
        year: 2024,
        month: Some(5),
        day: Some(17),
    };
    let json = serde_json::to_string(&date).unwrap();
    let parsed: DateParts = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, date);
}

#[test]
fn date_parts_deserializing_full_shape_gives_expected_struct() {
    let parsed: DateParts =
        serde_json::from_value(serde_json::json!({"date-parts": [[2024, 5, 17]]})).unwrap();
    assert_eq!(
        parsed,
        DateParts {
            year: 2024,
            month: Some(5),
            day: Some(17),
        }
    );
}

#[test]
fn date_parts_deserializing_null_month_with_day_fails() {
    let result: Result<DateParts, _> =
        serde_json::from_value(serde_json::json!({"date-parts": [[2024, null, 17]]}));
    assert!(result.is_err());
}

#[test]
fn date_parts_deserializing_four_element_array_fails() {
    // Only [year], [year, month], and [year, month, day] are valid
    // truncated shapes; a fourth element is malformed.
    let result: Result<DateParts, _> =
        serde_json::from_value(serde_json::json!({"date-parts": [[2024, 5, 17, 1]]}));
    assert!(result.is_err());
}

// ---------------------------------------------------------------------
// Provenance / spec scenarios
// ---------------------------------------------------------------------

#[test]
fn provenance_survives_round_trip() {
    let record = maximal_record();
    let json = serde_json::to_string(&record).unwrap();
    let parsed: Record = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.borax.provenance.get("title"), Some(&Source::Crossref));
    assert_eq!(
        parsed.borax.provenance.get("DOI"),
        Some(&Source::Extraction)
    );
}

#[test]
fn preprint_with_published_doi_round_trips_and_has_distinct_csl_type() {
    let mut record = Record::new(EntryType::Preprint);
    record.borax.arxiv = Some(ArxivId::parse("2401.12345").unwrap());
    record.doi = Some(Doi::parse("10.1021/jacs.4c01234").unwrap());

    let json = serde_json::to_string(&record).unwrap();
    let parsed: Record = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, record);

    let value: serde_json::Value = serde_json::to_value(&record).unwrap();
    assert_eq!(value["type"], "article");
    assert_ne!(value["type"], "article-journal");
}

// ---------------------------------------------------------------------
// EntryType
// ---------------------------------------------------------------------

#[test]
fn entry_type_csl_strings_match_documented_mapping() {
    assert_eq!(EntryType::Article.csl(), "article-journal");
    assert_eq!(EntryType::Preprint.csl(), "article");
    assert_eq!(EntryType::Book.csl(), "book");
    assert_eq!(EntryType::Chapter.csl(), "chapter");
    assert_eq!(EntryType::Thesis.csl(), "thesis");
    assert_eq!(EntryType::Report.csl(), "report");
    assert_eq!(EntryType::Patent.csl(), "patent");
    assert_eq!(EntryType::Standard.csl(), "standard");
}

#[test]
fn entry_type_csl_strings_are_pairwise_distinct() {
    let variants = [
        EntryType::Article,
        EntryType::Preprint,
        EntryType::Book,
        EntryType::Chapter,
        EntryType::Thesis,
        EntryType::Report,
        EntryType::Patent,
        EntryType::Standard,
    ];
    let strings: Vec<&'static str> = variants.iter().map(|v| v.csl()).collect();
    for i in 0..strings.len() {
        for j in (i + 1)..strings.len() {
            assert_ne!(strings[i], strings[j], "{:?} vs {:?}", strings[i], strings[j]);
        }
    }
}

#[test]
fn entry_type_serde_round_trip_preserves_variant() {
    let variants = [
        EntryType::Article,
        EntryType::Preprint,
        EntryType::Book,
        EntryType::Chapter,
        EntryType::Thesis,
        EntryType::Report,
        EntryType::Patent,
        EntryType::Standard,
    ];
    for variant in variants {
        let json = serde_json::to_string(&variant).unwrap();
        let parsed: EntryType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, variant);
    }
}
