#![allow(clippy::unwrap_used)]

use borax_core::record::{EntryType, Record};
use borax_sources::conflict::{Conflict, check_title, normalize_title};

fn article_with_title(title: &str) -> Record {
    let mut record = Record::new(EntryType::Article);
    record.title = Some(title.to_string());
    record
}

fn article_without_title() -> Record {
    Record::new(EntryType::Article)
}

// ================= normalize_title =================

#[test]
fn normalize_title_lowercases_drops_punctuation_and_collapses_spacing() {
    assert_eq!(
        normalize_title("Molecular Structure of Nucleic Acids: A Structure"),
        "molecular structure of nucleic acids a structure"
    );
}

#[test]
fn normalize_title_drops_curly_quotes_em_dash_and_trailing_period() {
    assert_eq!(
        normalize_title("\u{201c}Curly\u{201d} Quotes \u{2014} and a period."),
        "curly quotes and a period"
    );
}

#[test]
fn normalize_title_collapses_and_trims_whitespace_runs() {
    assert_eq!(normalize_title("  Spaced   Out  "), "spaced out");
}

#[test]
fn normalize_title_keeps_digits_and_drops_the_hyphen_without_a_space() {
    assert_eq!(normalize_title("COVID-19 in 2020"), "covid19 in 2020");
}

#[test]
fn normalize_title_keeps_non_ascii_letters() {
    assert_eq!(normalize_title("Gr\u{fc}\u{df}e"), "gr\u{fc}\u{df}e");
}

// ================= check_title: no conflict =================

#[test]
fn check_title_none_when_extracted_is_none() {
    assert_eq!(check_title(None, &article_with_title("Some Title")), None);
}

#[test]
fn check_title_none_when_extracted_is_empty() {
    assert_eq!(
        check_title(Some(""), &article_with_title("Some Title")),
        None
    );
}

#[test]
fn check_title_none_when_extracted_is_only_whitespace() {
    assert_eq!(
        check_title(Some("   "), &article_with_title("Some Title")),
        None
    );
}

#[test]
fn check_title_none_when_record_has_no_title() {
    assert_eq!(
        check_title(Some("Some Title"), &article_without_title()),
        None
    );
}

#[test]
fn check_title_none_when_normalized_forms_are_equal_though_raw_differ() {
    assert_eq!(
        check_title(Some("The Title."), &article_with_title("the title")),
        None
    );
}

#[test]
fn check_title_none_when_extracted_is_a_word_boundary_prefix_of_resolved() {
    let resolved = article_with_title(
        "Molecular Structure of Nucleic Acids: A Structure for Deoxyribose Nucleic Acid",
    );
    assert_eq!(
        check_title(Some("Molecular Structure of Nucleic Acids"), &resolved),
        None
    );
}

#[test]
fn check_title_none_when_resolved_is_a_word_boundary_prefix_of_extracted() {
    let resolved = article_with_title("Molecular Structure of Nucleic Acids");
    assert_eq!(
        check_title(
            Some("Molecular Structure of Nucleic Acids: A Structure for Deoxyribose Nucleic Acid"),
            &resolved
        ),
        None
    );
}

// ================= check_title: conflict =================

#[test]
fn check_title_some_conflict_when_titles_genuinely_differ() {
    let resolved = article_with_title("A Completely Different Title");
    let conflict = check_title(Some("Some Other Title"), &resolved).unwrap();

    assert_eq!(
        conflict,
        Conflict {
            field: "title",
            extracted: "Some Other Title".to_string(),
            resolved: "A Completely Different Title".to_string(),
        }
    );
}

// A character-level prefix ("molecular structure" is a prefix of
// "molecular structures of nucleic acids") is not enough: the next
// character in the resolved title is `s`, not a word boundary, so this
// must read as a conflict rather than a subtitle.
#[test]
fn check_title_conflict_when_prefix_match_is_not_at_a_word_boundary() {
    let resolved = article_with_title("Molecular Structures of Nucleic Acids");
    let conflict = check_title(Some("Molecular Structure"), &resolved).unwrap();

    assert_eq!(
        conflict,
        Conflict {
            field: "title",
            extracted: "Molecular Structure".to_string(),
            resolved: "Molecular Structures of Nucleic Acids".to_string(),
        }
    );
}
