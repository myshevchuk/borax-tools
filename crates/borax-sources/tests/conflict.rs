#![allow(clippy::unwrap_used)]

use borax_core::record::{EntryType, Record};
use borax_sources::conflict::{
    Conflict, TITLE_AGREEMENT, check_title, comparison_tokens, is_title_evidence, title_similarity,
    titles_agree,
};

fn article_with_title(title: &str) -> Record {
    let mut record = Record::new(EntryType::Article);
    record.title = Some(title.to_string());
    record
}

fn article_without_title() -> Record {
    Record::new(EntryType::Article)
}

fn tokens(title: &str) -> Vec<String> {
    comparison_tokens(title)
}

/// The Info `/Title` of `2011ASC(353)575.pdf`, whose typesetter dropped
/// the alpha and both hyphens.
const ASC_EXTRACTED: &str = "Synthesis of Diazo Carbonyl Compounds with the ShelfStable Diazo \
                             Transfer Reagent Nonafluorobutanesulfonyl Azide";

/// The Crossref title for the same work, which keeps the alpha and the
/// U+2010 hyphens.
const ASC_RESOLVED: &str = "Synthesis of \u{3b1}\u{2010}Diazo Carbonyl Compounds with the \
                            Shelf\u{2010}Stable Diazo Transfer Reagent Nonafluorobutanesulfonyl \
                            Azide";

// ================= comparison_tokens =================

#[test]
fn comparison_tokens_lowercases_drops_punctuation_and_function_words() {
    assert_eq!(
        tokens("Molecular Structure of Nucleic Acids: A Structure"),
        ["molecular", "structure", "nucleic", "acids", "structure"]
    );
}

#[test]
fn comparison_tokens_drops_curly_quotes_em_dash_and_trailing_period() {
    assert_eq!(
        tokens("\u{201c}Curly\u{201d} Quotes \u{2014} and a period."),
        ["curly", "quotes", "period"]
    );
}

#[test]
fn comparison_tokens_collapses_and_trims_whitespace_runs() {
    assert_eq!(tokens("  Spaced   Out  "), ["spaced", "out"]);
}

// A hyphen separates rather than vanishes, so a source writing
// `COVID-19` and one writing `COVID 19` reduce to the same tokens.
#[test]
fn comparison_tokens_splits_on_hyphens_rather_than_deleting_them() {
    assert_eq!(tokens("COVID-19 in 2020"), ["covid", "19", "2020"]);
}

#[test]
fn comparison_tokens_folds_latin_letters_to_ascii() {
    assert_eq!(tokens("Gr\u{fc}\u{df}e"), ["gruesse"]);
}

// A letter with no Latin folding is dropped, which is what the PDF
// producers that cannot encode it do to it.
#[test]
fn comparison_tokens_drops_letters_without_a_latin_folding() {
    assert_eq!(
        tokens("\u{3b1}\u{2010}Diazo Carbonyl"),
        ["diazo", "carbonyl"]
    );
}

#[test]
fn comparison_tokens_of_a_title_with_no_content_words_is_empty() {
    assert!(tokens("of the and").is_empty());
}

// ================= title_similarity =================

#[test]
fn title_similarity_of_identical_token_lists_is_one() {
    let both = tokens("Molecular Structure of Nucleic Acids");
    assert_eq!(title_similarity(&both, &both), 1.0);
}

#[test]
fn title_similarity_of_disjoint_token_lists_is_zero() {
    assert_eq!(
        title_similarity(&tokens("Alpha Beta"), &tokens("Gamma Delta")),
        0.0
    );
}

#[test]
fn title_similarity_of_the_asc_pair_clears_the_threshold() {
    let similarity = title_similarity(&tokens(ASC_EXTRACTED), &tokens(ASC_RESOLVED));
    assert!(
        similarity >= TITLE_AGREEMENT,
        "one dropped letter must not read as disagreement, got {similarity}"
    );
}

#[test]
fn title_similarity_of_two_different_works_falls_below_the_threshold() {
    let similarity = title_similarity(
        &tokens("Thermal Stability of RNA Structures with Bulky Cations"),
        &tokens("Aerosols and Hydrocarbons in the Atmosphere of a White Dwarf Planet"),
    );
    assert!(
        similarity < TITLE_AGREEMENT,
        "unrelated works must not agree, got {similarity}"
    );
}

// ================= titles_agree =================

#[test]
fn titles_agree_when_either_side_has_no_content_words() {
    assert!(titles_agree(&tokens("of the"), &tokens("A Real Title")));
    assert!(titles_agree(&tokens("A Real Title"), &tokens("of the")));
}

#[test]
fn titles_agree_when_one_is_a_token_prefix_of_the_other() {
    assert!(titles_agree(
        &tokens("Molecular Structure of Nucleic Acids"),
        &tokens("Molecular Structure of Nucleic Acids: A Structure for Deoxyribose Nucleic Acid"),
    ));
}

// A word split on one side and joined on the other is the same title:
// `ShelfStable` and `Shelf-Stable` differ only in where the producer
// put a separator.
#[test]
fn titles_agree_when_tokens_differ_only_in_where_words_were_split() {
    assert!(titles_agree(
        &tokens("The Shelf Stable Reagent"),
        &tokens("The ShelfStable Reagent")
    ));
    assert!(titles_agree(
        &tokens("COVID-19 Vaccines"),
        &tokens("COVID19 Vaccines")
    ));
}

#[test]
fn titles_agree_on_the_asc_pair() {
    assert!(titles_agree(&tokens(ASC_EXTRACTED), &tokens(ASC_RESOLVED)));
}

#[test]
fn titles_disagree_when_the_works_are_different() {
    assert!(!titles_agree(
        &tokens("Some Other Title"),
        &tokens("A Completely Different Title")
    ));
}

// A character-level prefix is not a token prefix: `structure` is not
// `structures`, so this stays a disagreement.
#[test]
fn titles_disagree_when_a_prefix_match_is_not_a_whole_token() {
    assert!(!titles_agree(
        &tokens("Molecular Structure"),
        &tokens("Molecular Structures of Nucleic Acids")
    ));
}

// ================= is_title_evidence =================

#[test]
fn a_plausible_title_is_evidence() {
    let record = article_with_title("Aerosols and Hydrocarbons in the Atmosphere");
    assert!(is_title_evidence(
        "Aerosols and Hydrocarbons in the Atmosphere",
        &record
    ));
}

#[test]
fn a_known_producer_default_is_not_evidence() {
    let record = article_with_title("Aerosols and Hydrocarbons in the Atmosphere");
    for candidate in ["untitled", "Untitled", "unknown", "no title", "document"] {
        assert!(
            !is_title_evidence(candidate, &record),
            "{candidate:?} must not count as evidence"
        );
    }
}

// The German PowerPoint default. No locale list catches this; the
// short-and-disjoint rule does.
#[test]
fn a_localized_producer_default_is_not_evidence() {
    let record = article_with_title("Aerosols and Hydrocarbons in the Atmosphere");
    assert!(!is_title_evidence("PowerPoint-Pr\u{e4}sentation", &record));
}

#[test]
fn a_filename_is_not_evidence() {
    let record = article_with_title("Aerosols and Hydrocarbons in the Atmosphere");
    for candidate in [
        "Microsoft Word - manuscript_rev2.docx",
        "manuscript_final.doc",
        "EJOC_201900.indd",
        "paper.tex",
    ] {
        assert!(
            !is_title_evidence(candidate, &record),
            "{candidate:?} must not count as evidence"
        );
    }
}

#[test]
fn a_bare_identifier_is_not_evidence() {
    let record = article_with_title("Aerosols and Hydrocarbons in the Atmosphere");
    assert!(!is_title_evidence("10.1002/adsc.201000846", &record));
    assert!(!is_title_evidence("arXiv:2401.12345", &record));
}

// The rule is short *and* disjoint. A short title that overlaps the
// record is a real title, and a long disjoint one is a real
// disagreement worth reporting.
#[test]
fn a_short_title_overlapping_the_record_is_still_evidence() {
    let record = article_with_title("Aerosols and Hydrocarbons in the Atmosphere");
    assert!(is_title_evidence("Aerosols", &record));
}

#[test]
fn a_long_disjoint_title_is_still_evidence() {
    let record = article_with_title("Aerosols and Hydrocarbons in the Atmosphere");
    assert!(is_title_evidence(
        "Thermal Stability of RNA Structures with Bulky Cations",
        &record
    ));
}

// ================= check_title: no conflict =================

#[test]
fn check_title_none_when_there_are_no_candidates() {
    assert_eq!(check_title(&[], &article_with_title("Some Title")), None);
}

#[test]
fn check_title_none_when_the_candidate_is_empty() {
    assert_eq!(check_title(&[""], &article_with_title("Some Title")), None);
}

#[test]
fn check_title_none_when_the_candidate_is_only_whitespace() {
    assert_eq!(
        check_title(&["   "], &article_with_title("Some Title")),
        None
    );
}

#[test]
fn check_title_none_when_record_has_no_title() {
    assert_eq!(check_title(&["Some Title"], &article_without_title()), None);
}

#[test]
fn check_title_none_when_normalized_forms_are_equal_though_raw_differ() {
    assert_eq!(
        check_title(&["The Title."], &article_with_title("the title")),
        None
    );
}

#[test]
fn check_title_none_when_the_candidate_is_a_prefix_of_the_resolved_title() {
    let resolved = article_with_title(
        "Molecular Structure of Nucleic Acids: A Structure for Deoxyribose Nucleic Acid",
    );
    assert_eq!(
        check_title(&["Molecular Structure of Nucleic Acids"], &resolved),
        None
    );
}

#[test]
fn check_title_none_when_the_resolved_title_is_a_prefix_of_the_candidate() {
    let resolved = article_with_title("Molecular Structure of Nucleic Acids");
    assert_eq!(
        check_title(
            &["Molecular Structure of Nucleic Acids: A Structure for Deoxyribose Nucleic Acid"],
            &resolved
        ),
        None
    );
}

// The regression this rewrite exists for.
#[test]
fn check_title_none_for_the_asc_paper() {
    assert_eq!(
        check_title(&[ASC_EXTRACTED], &article_with_title(ASC_RESOLVED)),
        None
    );
}

// `2011ASC(353)575.pdf` carries both: XMP says `untitled`, the Info
// dictionary carries the lossy real title. The junk one is discarded
// and the real one agrees.
#[test]
fn check_title_none_when_a_junk_candidate_accompanies_an_agreeing_one() {
    assert_eq!(
        check_title(
            &["untitled", ASC_EXTRACTED],
            &article_with_title(ASC_RESOLVED)
        ),
        None
    );
}

#[test]
fn check_title_none_when_every_candidate_is_junk() {
    assert_eq!(
        check_title(
            &["untitled", "PowerPoint-Pr\u{e4}sentation"],
            &article_with_title("Aerosols and Hydrocarbons in the Atmosphere")
        ),
        None
    );
}

// Any agreeing candidate clears the file, whichever order they arrive
// in: one source of evidence agreeing is enough.
#[test]
fn check_title_none_when_one_of_two_real_candidates_agrees() {
    let resolved = article_with_title("Aerosols and Hydrocarbons in the Atmosphere");
    assert_eq!(
        check_title(
            &[
                "Thermal Stability of RNA Structures with Bulky Cations",
                "Aerosols and Hydrocarbons in the Atmosphere",
            ],
            &resolved
        ),
        None
    );
}

// ================= check_title: conflict =================

#[test]
fn check_title_some_conflict_when_titles_genuinely_differ() {
    let resolved = article_with_title("A Completely Different Title");
    let conflict = check_title(&["Some Other Title"], &resolved).unwrap();

    assert_eq!(conflict.field, "title");
    assert_eq!(conflict.extracted, "Some Other Title");
    assert_eq!(conflict.resolved, "A Completely Different Title");
    assert!(conflict.similarity < TITLE_AGREEMENT);
}

#[test]
fn check_title_conflict_when_a_prefix_match_is_not_a_whole_token() {
    let resolved = article_with_title("Molecular Structures of Nucleic Acids");
    let conflict = check_title(&["Molecular Structure"], &resolved).unwrap();

    assert_eq!(conflict.extracted, "Molecular Structure");
    assert_eq!(conflict.resolved, "Molecular Structures of Nucleic Acids");
}

// The reported candidate is the closest one, so a person judging the
// skip sees the strongest case against it rather than an arbitrary one.
#[test]
fn check_title_reports_the_closest_candidate() {
    let resolved = article_with_title("Aerosols and Hydrocarbons in the Atmosphere");
    let conflict = check_title(
        &[
            "Thermal Stability of RNA Structures with Bulky Cations",
            "Aerosols and Hydrocarbons in the Stratosphere of Mars",
        ],
        &resolved,
    )
    .unwrap();

    assert_eq!(
        conflict.extracted,
        "Aerosols and Hydrocarbons in the Stratosphere of Mars"
    );
}

#[test]
fn check_title_conflict_carries_the_raw_strings_not_the_normalized_ones() {
    let resolved = article_with_title("A Completely Different Title");
    let conflict = check_title(&["  Some Other Title.  "], &resolved).unwrap();

    assert_eq!(conflict.extracted, "  Some Other Title.  ");
}

// ================= the Conflict value =================

#[test]
fn conflict_equality_ignores_nothing() {
    let a = Conflict {
        field: "title",
        extracted: "x".to_string(),
        resolved: "y".to_string(),
        similarity: 0.0,
    };
    assert_eq!(a.clone(), a);
}
