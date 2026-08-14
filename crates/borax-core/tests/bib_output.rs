#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use borax_core::bib_output::{
    DuplicatePolicy, MergeOutcome, SIDECAR_MARKER, merge, parse_sidecar_record, sidecar,
};
use borax_core::bibtex::emit;
use borax_core::identifier::{ArxivId, Doi, Isbn, Pmid};
use borax_core::record::{BoraxExt, DateParts, EntryType, Name, Record, Source};

fn article_with_doi(doi: &str) -> Record {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("A Title".to_string());
    record.doi = Some(Doi::parse(doi).unwrap());
    record
}

fn maximal_record() -> Record {
    let mut provenance = BTreeMap::new();
    provenance.insert("title".to_string(), Source::Crossref);
    provenance.insert("DOI".to_string(), Source::Extraction);

    let mut source_fields = BTreeMap::new();
    source_fields.insert(
        "crossref-extra".to_string(),
        serde_json::json!({"funder": ["NSF"]}),
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

// ---------------------------------------------------------------------
// merge: appending
// ---------------------------------------------------------------------

#[test]
fn merge_into_empty_file_yields_exactly_the_emitted_entry() {
    let record = article_with_doi("10.1000/new");
    let result = merge("", &[("smith2024", &record)], DuplicatePolicy::Skip);

    assert_eq!(result.content, emit(&record, "smith2024"));
    assert_eq!(
        result.outcomes,
        vec![MergeOutcome::Added {
            key: "smith2024".to_string()
        }]
    );
}

#[test]
fn merge_appends_after_hand_edited_content_preserving_its_bytes() {
    let existing = concat!(
        "% A hand-edited bibliography\n",
        "@comment{ this predates borax }\n",
        "\n",
        "@article{doe2020,\n",
        "  doi = {10.1000/xyz},\n",
        "}\n",
    );
    let record = article_with_doi("10.2000/other");

    let result = merge(existing, &[("newkey", &record)], DuplicatePolicy::Skip);

    assert!(
        result.content.starts_with(existing),
        "content was:\n{}",
        result.content
    );
    let appended = &result.content[existing.len()..];
    assert_eq!(appended, format!("\n{}", emit(&record, "newkey")));
    assert_eq!(
        result.outcomes,
        vec![MergeOutcome::Added {
            key: "newkey".to_string()
        }]
    );
}

#[test]
fn merge_adds_missing_trailing_newline_before_the_blank_line_separator() {
    let existing = "@article{a1,\n}";
    let record = article_with_doi("10.9000/newone");

    let result = merge(existing, &[("b2", &record)], DuplicatePolicy::Skip);

    let expected = format!("{existing}\n\n{}", emit(&record, "b2"));
    assert_eq!(result.content, expected);
}

// ---------------------------------------------------------------------
// merge: dedup by identifier
// ---------------------------------------------------------------------

#[test]
fn merge_skips_duplicate_doi_reporting_the_existing_key() {
    let existing = "@article{orig2020,\n  doi = {10.1021/jacs.4c01234},\n}\n";
    let record = article_with_doi("10.1021/jacs.4c01234");

    let result = merge(existing, &[("newkey", &record)], DuplicatePolicy::Skip);

    assert_eq!(
        result.outcomes,
        vec![MergeOutcome::AlreadyPresent {
            existing_key: "orig2020".to_string()
        }]
    );
    assert_eq!(result.content, existing);
}

#[test]
fn merge_dedup_matches_doi_case_insensitively() {
    let existing = "@article{orig2020,\n  doi = {10.1021/JACS.4C01234},\n}\n";
    let record = article_with_doi("10.1021/jacs.4c01234");

    let result = merge(existing, &[("newkey", &record)], DuplicatePolicy::Skip);

    assert_eq!(
        result.outcomes,
        vec![MergeOutcome::AlreadyPresent {
            existing_key: "orig2020".to_string()
        }]
    );
}

#[test]
fn merge_dedup_matches_by_eprint_bare_id_ignoring_version() {
    let existing = concat!(
        "@misc{p1,\n",
        "  eprint = {2401.12345},\n",
        "  archiveprefix = {arXiv},\n",
        "}\n",
    );
    let mut record = Record::new(EntryType::Preprint);
    record.title = Some("A Preprint".to_string());
    record.borax.arxiv = Some(ArxivId::parse("2401.12345v2").unwrap());

    let result = merge(existing, &[("newkey", &record)], DuplicatePolicy::Skip);

    assert_eq!(
        result.outcomes,
        vec![MergeOutcome::AlreadyPresent {
            existing_key: "p1".to_string()
        }]
    );
}

#[test]
fn merge_dedup_tolerates_quote_delimited_doi_field() {
    let existing = "@article{q1,\n  doi = \"10.5555/abc\",\n}\n";
    let record = article_with_doi("10.5555/abc");

    let result = merge(existing, &[("newkey", &record)], DuplicatePolicy::Skip);

    assert_eq!(
        result.outcomes,
        vec![MergeOutcome::AlreadyPresent {
            existing_key: "q1".to_string()
        }]
    );
}

// ---------------------------------------------------------------------
// merge: update policy
// ---------------------------------------------------------------------

#[test]
fn merge_update_policy_replaces_entry_in_place_keeping_its_key() {
    let existing = concat!(
        "@article{oldkey,\n",
        "  doi = {10.1000/xyz},\n",
        "  pages = {1},\n",
        "}\n",
        "\n",
        "@book{other2020,\n",
        "  title = {Another Work},\n",
        "}\n",
    );
    let other_entry = "@book{other2020,\n  title = {Another Work},\n}\n";

    let mut record = article_with_doi("10.1000/xyz");
    record.title = Some("A Richer Title".to_string());
    record.pages = Some("200-210".to_string());

    let result = merge(
        existing,
        &[("requestedkey", &record)],
        DuplicatePolicy::Update,
    );

    assert_eq!(
        result.outcomes,
        vec![MergeOutcome::Updated {
            key: "oldkey".to_string()
        }]
    );
    assert!(
        result.content.contains(&emit(&record, "oldkey")),
        "content was:\n{}",
        result.content
    );
    assert!(
        result.content.contains(other_entry),
        "content was:\n{}",
        result.content
    );
    assert!(!result.content.contains("pages = {1}"));
}

// ---------------------------------------------------------------------
// merge: no identifier never dedups
// ---------------------------------------------------------------------

#[test]
fn merge_records_without_an_identifier_are_never_treated_as_duplicates() {
    let mut record = Record::new(EntryType::Book);
    record.title = Some("Same Book".to_string());

    let first = merge("", &[("book1", &record)], DuplicatePolicy::Skip);
    assert_eq!(
        first.outcomes,
        vec![MergeOutcome::Added {
            key: "book1".to_string()
        }]
    );

    let second = merge(&first.content, &[("book1", &record)], DuplicatePolicy::Skip);
    assert_eq!(
        second.outcomes,
        vec![MergeOutcome::Added {
            key: "book1a".to_string()
        }]
    );
}

// ---------------------------------------------------------------------
// merge: key uniqueness
// ---------------------------------------------------------------------

#[test]
fn merge_suffixes_a_requested_key_already_taken_by_a_different_identifier() {
    let existing = "@article{smith2024,\n  doi = {10.1000/aaa},\n}\n";
    let record = article_with_doi("10.1000/bbb");

    let result = merge(existing, &[("smith2024", &record)], DuplicatePolicy::Skip);

    assert_eq!(
        result.outcomes,
        vec![MergeOutcome::Added {
            key: "smith2024a".to_string()
        }]
    );
    assert!(result.content.contains("@article{smith2024a,"));
}

#[test]
fn merge_key_suffix_ladder_advances_past_first_taken_letter() {
    let existing = concat!(
        "@article{smith2024,\n",
        "  doi = {10.1000/aaa},\n",
        "}\n",
        "@article{smith2024a,\n",
        "  doi = {10.1000/ccc},\n",
        "}\n",
    );
    let record = article_with_doi("10.1000/bbb");

    let result = merge(existing, &[("smith2024", &record)], DuplicatePolicy::Skip);

    assert_eq!(
        result.outcomes,
        vec![MergeOutcome::Added {
            key: "smith2024b".to_string()
        }]
    );
}

#[test]
fn merge_key_clash_is_resolved_batch_internally() {
    let record_a = article_with_doi("10.1000/aaa");
    let record_b = article_with_doi("10.1000/bbb");

    let result = merge(
        "",
        &[("k", &record_a), ("k", &record_b)],
        DuplicatePolicy::Skip,
    );

    assert_eq!(
        result.outcomes,
        vec![
            MergeOutcome::Added {
                key: "k".to_string()
            },
            MergeOutcome::Added {
                key: "ka".to_string()
            },
        ]
    );
}

// ---------------------------------------------------------------------
// merge: dedup against earlier additions in the same call
// ---------------------------------------------------------------------

#[test]
fn merge_dedups_against_an_earlier_addition_in_the_same_call() {
    let record_a = article_with_doi("10.1000/shared");
    let record_b = article_with_doi("10.1000/shared");

    let result = merge(
        "",
        &[("dup1", &record_a), ("dup2", &record_b)],
        DuplicatePolicy::Skip,
    );

    assert_eq!(
        result.outcomes,
        vec![
            MergeOutcome::Added {
                key: "dup1".to_string()
            },
            MergeOutcome::AlreadyPresent {
                existing_key: "dup1".to_string()
            },
        ]
    );
}

// ---------------------------------------------------------------------
// merge: outcomes parallel the additions
// ---------------------------------------------------------------------

#[test]
fn merge_outcomes_are_parallel_to_additions_in_order_and_count() {
    let record_a = article_with_doi("10.1000/aaa");
    let record_b = article_with_doi("10.1000/bbb");
    let record_c = article_with_doi("10.1000/ccc");

    let result = merge(
        "",
        &[("one", &record_a), ("two", &record_b), ("three", &record_c)],
        DuplicatePolicy::Skip,
    );

    assert_eq!(result.outcomes.len(), 3);
    assert_eq!(
        result.outcomes,
        vec![
            MergeOutcome::Added {
                key: "one".to_string()
            },
            MergeOutcome::Added {
                key: "two".to_string()
            },
            MergeOutcome::Added {
                key: "three".to_string()
            },
        ]
    );
}

// ---------------------------------------------------------------------
// merge: determinism
// ---------------------------------------------------------------------

#[test]
fn merge_is_deterministic_across_repeated_calls() {
    let existing = "@article{smith2024,\n  doi = {10.1000/aaa},\n}\n";
    let record = article_with_doi("10.1000/bbb");

    let first = merge(existing, &[("smith2024", &record)], DuplicatePolicy::Skip);
    let second = merge(existing, &[("smith2024", &record)], DuplicatePolicy::Skip);

    assert_eq!(first, second);
}

// ---------------------------------------------------------------------
// sidecar
// ---------------------------------------------------------------------

#[test]
fn sidecar_renders_entry_blank_line_marker_json_and_trailing_newline() {
    let record = article_with_doi("10.1000/aaa");
    let emitted = emit(&record, "k");
    let json = serde_json::to_string(&record).unwrap();

    let expected = format!("{emitted}\n{SIDECAR_MARKER}{json}\n");
    assert_eq!(sidecar(&record, "k"), expected);
    assert!(!json.contains('\n'));
}

#[test]
fn sidecar_round_trips_a_maximal_record_losslessly() {
    let record = maximal_record();
    let content = sidecar(&record, "k");

    assert_eq!(parse_sidecar_record(&content), Some(record));
}

#[test]
fn parse_sidecar_record_returns_none_without_a_marker_line() {
    let content = "@article{a,\n  title = {No Sidecar Here},\n}\n";
    assert_eq!(parse_sidecar_record(content), None);
}

#[test]
fn parse_sidecar_record_returns_none_on_invalid_json_after_marker() {
    let content = format!("{SIDECAR_MARKER}not valid json\n");
    assert_eq!(parse_sidecar_record(&content), None);
}

#[test]
fn parse_sidecar_record_finds_marker_line_not_at_file_start() {
    let record = article_with_doi("10.1000/aaa");
    let json = serde_json::to_string(&record).unwrap();
    let content = format!("@article{{k,\n  doi = {{10.1000/aaa}},\n}}\n\n{SIDECAR_MARKER}{json}\n");

    assert_eq!(parse_sidecar_record(&content), Some(record));
}
