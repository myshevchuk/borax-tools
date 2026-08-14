#![allow(clippy::unwrap_used)]

use borax_core::bibtex::{emit, entry_kind, escape};
use borax_core::identifier::{ArxivId, Doi};
use borax_core::record::{DateParts, EntryType, Name, Record};

// ---------------------------------------------------------------------
// escape
// ---------------------------------------------------------------------

#[test]
fn escape_ampersand() {
    assert_eq!(escape("&"), "\\&");
}

#[test]
fn escape_percent() {
    assert_eq!(escape("%"), "\\%");
}

#[test]
fn escape_dollar_hash_underscore() {
    assert_eq!(escape("$"), "\\$");
    assert_eq!(escape("#"), "\\#");
    assert_eq!(escape("_"), "\\_");
}

#[test]
fn escape_braces() {
    assert_eq!(escape("{"), "\\{");
    assert_eq!(escape("}"), "\\}");
}

#[test]
fn escape_tilde() {
    assert_eq!(escape("~"), "\\textasciitilde{}");
}

#[test]
fn escape_caret() {
    assert_eq!(escape("^"), "\\textasciicircum{}");
}

#[test]
fn escape_backslash() {
    assert_eq!(escape("\\"), "\\textbackslash{}");
}

#[test]
fn escape_keeps_non_ascii_unchanged_while_escaping_percent() {
    assert_eq!(escape("Gr\u{fc}\u{df}e 100% caf\u{e9}"), "Gr\u{fc}\u{df}e 100\\% caf\u{e9}");
}

#[test]
fn escape_plain_text_unchanged() {
    assert_eq!(escape("Borax and Friends"), "Borax and Friends");
}

// ---------------------------------------------------------------------
// entry_kind
// ---------------------------------------------------------------------

#[test]
fn entry_kind_mappings_match_documented_table() {
    assert_eq!(entry_kind(EntryType::Article), "article");
    assert_eq!(entry_kind(EntryType::Preprint), "misc");
    assert_eq!(entry_kind(EntryType::Book), "book");
    assert_eq!(entry_kind(EntryType::Chapter), "incollection");
    assert_eq!(entry_kind(EntryType::Thesis), "thesis");
    assert_eq!(entry_kind(EntryType::Report), "techreport");
    assert_eq!(entry_kind(EntryType::Patent), "patent");
    assert_eq!(entry_kind(EntryType::Standard), "standard");
}

// ---------------------------------------------------------------------
// emit
// ---------------------------------------------------------------------

fn article_golden_record() -> Record {
    let mut record = Record::new(EntryType::Article);
    record.authors = vec![
        Name {
            family: "Smith".to_string(),
            given: Some("Jane".to_string()),
        },
        Name {
            family: "Doe".to_string(),
            given: Some("John".to_string()),
        },
    ];
    record.title = Some("Borax & Friends: A 100% Study".to_string());
    record.container_title = Some("J. Chem. Ed.".to_string());
    record.issued = Some(DateParts {
        year: 2024,
        month: Some(5),
        day: None,
    });
    record.volume = Some("12".to_string());
    record.issue = Some("3".to_string());
    record.pages = Some("100-110".to_string());
    record.doi = Some(Doi::parse("10.1021/jacs.4c01234").unwrap());
    record
}

#[test]
fn emit_article_matches_golden_output_exactly() {
    let record = article_golden_record();
    let output = emit(&record, "smith2024");

    let expected = "@article{smith2024,\n\
  author = {Smith, Jane and Doe, John},\n\
  title = {Borax \\& Friends: A 100\\% Study},\n\
  journal = {J. Chem. Ed.},\n\
  year = {2024},\n\
  month = {5},\n\
  volume = {12},\n\
  number = {3},\n\
  pages = {100-110},\n\
  doi = {10.1021/jacs.4c01234},\n\
}\n";

    assert_eq!(output, expected);
}

#[test]
fn emit_is_deterministic_across_repeated_calls() {
    let record = article_golden_record();
    let first = emit(&record, "smith2024");
    let second = emit(&record, "smith2024");
    assert_eq!(first, second);
}

#[test]
fn emit_preprint_with_arxiv_id_uses_misc_and_bare_eprint_id() {
    let mut record = Record::new(EntryType::Preprint);
    record.title = Some("A Preprint Title".to_string());
    record.borax.arxiv = Some(ArxivId::parse("2401.12345v2").unwrap());

    let output = emit(&record, "preprint2024");

    assert!(output.starts_with("@misc{preprint2024,\n"));
    assert!(
        output.contains("  eprint = {2401.12345},\n"),
        "output was:\n{output}"
    );
    assert!(
        output.contains("  archiveprefix = {arXiv},\n"),
        "output was:\n{output}"
    );
    assert!(!output.contains("journal ="));
}

#[test]
fn emit_chapter_uses_booktitle_not_journal() {
    let mut record = Record::new(EntryType::Chapter);
    record.title = Some("A Chapter".to_string());
    record.container_title = Some("The Big Book".to_string());

    let output = emit(&record, "chapter2024");

    assert!(output.contains("booktitle = {The Big Book},"));
    assert!(!output.contains("journal ="));
}

#[test]
fn emit_omits_missing_fields() {
    let mut record = Record::new(EntryType::Report);
    record.title = Some("Just A Title".to_string());

    let output = emit(&record, "onlytitle");

    assert!(!output.contains("author ="));
    assert!(!output.contains("year ="));
    assert!(!output.contains("volume ="));
    assert!(output.contains("title = {Just A Title},"));
}

#[test]
fn emit_escapes_special_characters_in_title() {
    let mut record = Record::new(EntryType::Article);
    record.title = Some("100% Caf\u{e9} & Study".to_string());

    let output = emit(&record, "special2024");

    assert!(
        output.contains("title = {100\\% Caf\u{e9} \\& Study},"),
        "output was:\n{output}"
    );
}
