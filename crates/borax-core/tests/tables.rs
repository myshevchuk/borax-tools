#![allow(clippy::unwrap_used)]

use borax_core::record::{DateParts, EntryType, Record};
use borax_core::tables::{Table, TableError, TableSpec, ValueKind, parse_tsv};
use borax_core::template::{RenderInput, TemplateError, slug};

// ---------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------

/// Two rows of the real curated file this feature targets
/// (`journal_titles.tsv`), header `abbreviation\ttitle\tshorttitle`: one
/// row where `title` and `shorttitle` agree verbatim, one where they
/// differ.
fn journal_titles_tsv() -> String {
    "abbreviation\ttitle\tshorttitle\n\
     AA\tAmino Acids\tAmino Acids\n\
     ABB-\tArchives of Biochemistry and Biophysics\tArch. Biochem. Biophys.\n"
        .to_string()
}

/// A table keyed on `title`, valued on `abbreviation` — the shape of
/// the `jcode` table from the spec's worked example.
fn title_to_abbreviation() -> TableSpec {
    TableSpec {
        key_columns: vec!["title".to_string()],
        value_column: "abbreviation".to_string(),
        values: ValueKind::Text,
    }
}

/// The same table, but drawing keys from both `title` and `shorttitle`.
fn title_and_shorttitle_to_abbreviation() -> TableSpec {
    TableSpec {
        key_columns: vec!["title".to_string(), "shorttitle".to_string()],
        value_column: "abbreviation".to_string(),
        values: ValueKind::Text,
    }
}

/// A table keyed on `title`, whose `abbreviation` column holds template
/// source rather than literal text.
fn title_to_fragment_abbreviation() -> TableSpec {
    TableSpec {
        key_columns: vec!["title".to_string()],
        value_column: "abbreviation".to_string(),
        values: ValueKind::Template,
    }
}

/// A minimal record carrying only `year`, for rendering fragment values
/// against `[year]`.
fn record_with_year(year: i32) -> Record {
    let mut r = Record::new(EntryType::Article);
    r.issued = Some(DateParts {
        year,
        month: None,
        day: None,
    });
    r
}

fn input(record: &Record) -> RenderInput<'_> {
    RenderInput { record, sha1: None }
}

// ---------------------------------------------------------------------
// 1.3 TSV format contract: header discovery, BOM, CRLF, blank lines
// (parse_tsv)
// ---------------------------------------------------------------------

#[test]
fn parse_tsv_finds_header_on_the_first_non_blank_line() {
    let text = "\n\nabbreviation\ttitle\tshorttitle\nAA\tAmino Acids\tAmino Acids\n";
    let (header, rows) = parse_tsv(text).expect("parses");
    assert_eq!(header, vec!["abbreviation", "title", "shorttitle"]);
    // Lines 1 and 2 are blank; the header is line 3, so the one row is
    // line 4.
    assert_eq!(
        rows,
        vec![(
            4,
            vec![
                "AA".to_string(),
                "Amino Acids".to_string(),
                "Amino Acids".to_string(),
            ]
        )]
    );
}

#[test]
fn parse_tsv_strips_a_leading_byte_order_mark_from_the_first_column() {
    let text = "\u{feff}abbreviation\ttitle\tshorttitle\nAA\tAmino Acids\tAmino Acids\n";
    let (header, _rows) = parse_tsv(text).expect("parses");
    assert_eq!(header[0], "abbreviation");
}

#[test]
fn parse_tsv_accepts_crlf_line_endings() {
    let text = "abbreviation\ttitle\tshorttitle\r\nAA\tAmino Acids\tAmino Acids\r\n";
    let (header, rows) = parse_tsv(text).expect("parses");
    assert_eq!(header, vec!["abbreviation", "title", "shorttitle"]);
    assert_eq!(
        rows,
        vec![(
            2,
            vec![
                "AA".to_string(),
                "Amino Acids".to_string(),
                "Amino Acids".to_string(),
            ]
        )]
    );
}

#[test]
fn parse_tsv_skips_blank_lines_between_rows() {
    let text = "abbreviation\ttitle\tshorttitle\n\
                 AA\tAmino Acids\tAmino Acids\n\
                 \n\
                 ABB-\tArchives of Biochemistry and Biophysics\tArch. Biochem. Biophys.\n";
    let (_header, rows) = parse_tsv(text).expect("parses");
    let lines: Vec<usize> = rows.iter().map(|(line, _)| *line).collect();
    // Line 3 is blank and contributes no row.
    assert_eq!(lines, vec![2, 4]);
}

#[test]
fn parse_tsv_pads_a_short_row_with_empty_cells() {
    let text = "abbreviation\ttitle\tshorttitle\nAA\n";
    let (header, rows) = parse_tsv(text).expect("parses");
    assert_eq!(header, vec!["abbreviation", "title", "shorttitle"]);
    assert_eq!(
        rows,
        vec![(2, vec!["AA".to_string(), String::new(), String::new()])]
    );
}

#[test]
fn parse_tsv_of_only_blank_lines_fails_with_no_header() {
    let err = parse_tsv("\n\n\n").expect_err("no header row at all");
    assert_eq!(err, TableError::NoHeader);
}

#[test]
fn parse_tsv_of_empty_text_fails_with_no_header() {
    let err = parse_tsv("").expect_err("no header row at all");
    assert_eq!(err, TableError::NoHeader);
}

// ---------------------------------------------------------------------
// 1.3 TSV format contract: Table::load over the parsed rows — ignored
// columns, missing cells, and configuration errors
// ---------------------------------------------------------------------

#[test]
fn table_load_ignores_a_header_column_the_spec_does_not_name() {
    let text = "abbreviation\ttitle\tshorttitle\tnotes\n\
                 AA\tAmino Acids\tAmino Acids\tsee also X\n";
    let (table, warnings) = Table::load(text, &title_to_abbreviation()).expect("loads");
    assert_eq!(warnings, Vec::new());
    assert_eq!(table.get("Amino Acids").expect("hit").source(), "AA");
}

#[test]
fn table_load_treats_a_short_row_as_having_empty_cells_for_the_rest() {
    // The row supplies only `abbreviation`; `title` (the key column) is
    // therefore empty and the row is skipped and warned about rather
    // than panicking on a missing cell.
    let text = "abbreviation\ttitle\tshorttitle\nAA\n";
    let (table, warnings) = Table::load(text, &title_to_abbreviation()).expect("loads");
    assert_eq!(table.len(), 0);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 2);
}

#[test]
fn a_row_with_an_empty_value_cell_is_skipped_and_warned_about() {
    let text = "abbreviation\ttitle\tshorttitle\n\
                 AA\tAmino Acids\tAmino Acids\n\
                 \tArchives of Biochemistry and Biophysics\tArch. Biochem. Biophys.\n";
    let (table, warnings) = Table::load(text, &title_to_abbreviation()).expect("loads");

    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 3);
    // The malformed row is dropped; the well-formed one still loads.
    assert_eq!(table.len(), 1);
    assert_eq!(table.get("Amino Acids").expect("hit").source(), "AA");
    assert!(
        table
            .get("Archives of Biochemistry and Biophysics")
            .is_none()
    );
}

#[test]
fn a_row_with_an_empty_key_cell_is_skipped_and_warned_about() {
    let text = "abbreviation\ttitle\tshorttitle\n\
                 AA\tAmino Acids\tAmino Acids\n\
                 ABB-\t\tArch. Biochem. Biophys.\n";
    let (table, warnings) = Table::load(text, &title_to_abbreviation()).expect("loads");

    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 3);
    assert_eq!(table.len(), 1);
    assert_eq!(table.get("Amino Acids").expect("hit").source(), "AA");
}

#[test]
fn a_value_column_absent_from_the_header_fails_with_missing_column() {
    let text = "abbreviation\ttitle\tshorttitle\nAA\tAmino Acids\tAmino Acids\n";
    let spec = TableSpec {
        key_columns: vec!["title".to_string()],
        value_column: "code".to_string(),
        values: ValueKind::Text,
    };
    let err = Table::load(text, &spec).expect_err("value column not in header");
    assert_eq!(
        err,
        TableError::MissingColumn {
            column: "code".to_string()
        }
    );
}

#[test]
fn a_key_column_absent_from_the_header_fails_with_missing_column() {
    let text = "abbreviation\ttitle\tshorttitle\nAA\tAmino Acids\tAmino Acids\n";
    let spec = TableSpec {
        key_columns: vec!["keywords".to_string()],
        value_column: "abbreviation".to_string(),
        values: ValueKind::Text,
    };
    let err = Table::load(text, &spec).expect_err("key column not in header");
    assert_eq!(
        err,
        TableError::MissingColumn {
            column: "keywords".to_string()
        }
    );
}

#[test]
fn table_load_fails_with_no_header_when_the_file_has_no_header_row() {
    let err = Table::load("\n\n", &title_to_abbreviation()).expect_err("no header row at all");
    assert_eq!(err, TableError::NoHeader);
}

// ---------------------------------------------------------------------
// 1.5 Fold and lookup: punctuation, abbreviation, diacritics, and
// the empty fold
// ---------------------------------------------------------------------

#[test]
fn punctuation_and_case_do_not_affect_matching() {
    let text = "abbreviation\ttitle\tshorttitle\n\
                 JACS\tJournal of the American Chemical Society\tJ. Am. Chem. Soc.\n";
    let (table, warnings) = Table::load(text, &title_to_abbreviation()).expect("loads");
    assert_eq!(warnings, Vec::new());
    assert_eq!(
        table
            .get("Journal of the American Chemical Society.")
            .expect("hit")
            .source(),
        "JACS"
    );
}

#[test]
fn abbreviated_forms_fold_alike() {
    let text = "abbreviation\ttitle\tshorttitle\nJCS\tJ. Am. Chem. Soc.\tJ. Am. Chem. Soc.\n";
    let (table, warnings) = Table::load(text, &title_to_abbreviation()).expect("loads");
    assert_eq!(warnings, Vec::new());
    assert_eq!(table.get("J Am Chem Soc").expect("hit").source(), "JCS");
}

#[test]
fn diacritics_fold_in_the_german_manner_and_the_lookup_matches() {
    // The fold itself is `slug`'s contract; here we confirm the table
    // reaches the same row through it.
    assert_eq!(slug("Zeitschrift für Chemie"), "zeitschrift-fuer-chemie");

    let text = "abbreviation\ttitle\tshorttitle\nZFC\tZeitschrift für Chemie\tZ. Chem.\n";
    let (table, warnings) = Table::load(text, &title_to_abbreviation()).expect("loads");
    assert_eq!(warnings, Vec::new());
    assert_eq!(
        table.get("Zeitschrift für Chemie").expect("hit").source(),
        "ZFC"
    );
    // The already-folded spelling matches too.
    assert_eq!(
        table.get("zeitschrift-fuer-chemie").expect("hit").source(),
        "ZFC"
    );
}

#[test]
fn a_row_whose_key_folds_to_empty_is_dropped_with_a_warning() {
    // "!!!" is entirely punctuation: every character is replaced by the
    // fold's dash step and then trimmed away, leaving nothing.
    let text = "abbreviation\ttitle\tshorttitle\nXX\t!!!\t!!!\n";
    let (table, warnings) = Table::load(text, &title_to_abbreviation()).expect("loads");
    assert_eq!(table.len(), 0);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 2);
}

#[test]
fn an_input_folding_to_empty_matches_nothing() {
    let (table, _warnings) =
        Table::load(&journal_titles_tsv(), &title_to_abbreviation()).expect("loads");
    assert!(table.get("!!!").is_none());
}

#[test]
fn a_plain_miss_returns_none() {
    let (table, _warnings) =
        Table::load(&journal_titles_tsv(), &title_to_abbreviation()).expect("loads");
    assert!(table.get("Some Unlisted Journal").is_none());
}

// ---------------------------------------------------------------------
// 1.7 Multi-column keys and conflicts
// ---------------------------------------------------------------------

#[test]
fn a_row_is_reachable_by_either_of_two_key_columns() {
    let text = "abbreviation\ttitle\tshorttitle\n\
                 ABB-\tArchives of Biochemistry and Biophysics\tArch. Biochem. Biophys.\n";
    let (table, warnings) =
        Table::load(text, &title_and_shorttitle_to_abbreviation()).expect("loads");
    assert_eq!(warnings, Vec::new());
    assert_eq!(
        table
            .get("Archives of Biochemistry and Biophysics")
            .expect("hit by title")
            .source(),
        "ABB-"
    );
    assert_eq!(
        table
            .get("Arch. Biochem. Biophys.")
            .expect("hit by shorttitle")
            .source(),
        "ABB-"
    );
}

#[test]
fn identical_key_cells_after_folding_contribute_one_key_without_complaint() {
    // The real curated file has rows exactly like this one, where
    // `title` and `shorttitle` repeat the same form: AA / Amino Acids /
    // Amino Acids.
    let text = "abbreviation\ttitle\tshorttitle\nAA\tAmino Acids\tAmino Acids\n";
    let (table, warnings) =
        Table::load(text, &title_and_shorttitle_to_abbreviation()).expect("loads");
    assert_eq!(warnings, Vec::new());
    assert_eq!(table.len(), 1);
    assert_eq!(table.get("Amino Acids").expect("hit").source(), "AA");
}

#[test]
fn two_rows_folding_alike_with_different_values_is_a_conflict() {
    let text = "abbreviation\ttitle\tshorttitle\n\
                 JCS\tJ. Chem. Soc.\tJ. Chem. Soc.\n\
                 JCHS\tJ Chem Soc\tJ Chem Soc\n";
    let err = Table::load(text, &title_to_abbreviation()).expect_err("conflicting values");
    match err {
        TableError::Conflict {
            key,
            first,
            second,
            line: _,
        } => {
            assert_eq!(key, "j-chem-soc");
            let values = [first.as_str(), second.as_str()];
            assert!(values.contains(&"JCS"), "values were {values:?}");
            assert!(values.contains(&"JCHS"), "values were {values:?}");
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
}

#[test]
fn two_rows_folding_alike_with_the_same_value_load_cleanly() {
    let text = "abbreviation\ttitle\tshorttitle\n\
                 JCS\tJ. Chem. Soc.\tJ. Chem. Soc.\n\
                 JCS\tJ Chem Soc\tJ Chem Soc\n";
    let (table, warnings) = Table::load(text, &title_to_abbreviation()).expect("loads");
    assert_eq!(warnings, Vec::new());
    assert_eq!(table.get("J Chem Soc").expect("hit").source(), "JCS");
}

// ---------------------------------------------------------------------
// 1.9 Value kinds: literal text vs. compiled template fragments
// ---------------------------------------------------------------------

#[test]
fn a_text_valued_table_substitutes_a_bracketed_value_verbatim() {
    // Literal text is the default: a value containing bracket
    // characters is data, not a token to render.
    let text = "abbreviation\ttitle\tshorttitle\nAA[note]\tAmino Acids\tAmino Acids\n";
    let (table, warnings) = Table::load(text, &title_to_abbreviation()).expect("loads");
    assert_eq!(warnings, Vec::new());

    let value = table.get("Amino Acids").expect("hit");
    assert_eq!(value.source(), "AA[note]");
    assert_eq!(value.render(&input(&record_with_year(2024))), "AA[note]");
}

#[test]
fn a_template_valued_table_compiles_its_values_at_load() {
    let text = "abbreviation\ttitle\tshorttitle\nAA[year]\tAmino Acids\tAmino Acids\n";
    let (table, warnings) = Table::load(text, &title_to_fragment_abbreviation()).expect("loads");
    assert_eq!(warnings, Vec::new());

    let value = table.get("Amino Acids").expect("hit");
    assert_eq!(value.source(), "AA[year]");
}

#[test]
fn value_render_of_a_template_fragment_produces_the_rendered_fragment() {
    let text = "abbreviation\ttitle\tshorttitle\nAA[year]\tAmino Acids\tAmino Acids\n";
    let (table, warnings) = Table::load(text, &title_to_fragment_abbreviation()).expect("loads");
    assert_eq!(warnings, Vec::new());

    let value = table.get("Amino Acids").expect("hit");
    assert_eq!(value.render(&input(&record_with_year(2024))), "AA2024");
}

#[test]
fn a_template_valued_cell_with_an_unclosed_bracket_fails_with_bad_fragment() {
    let text = "abbreviation\ttitle\tshorttitle\nAA[\tAmino Acids\tAmino Acids\n";
    let err = Table::load(text, &title_to_fragment_abbreviation())
        .expect_err("unclosed bracket does not compile");
    match err {
        TableError::BadFragment {
            line,
            source,
            error,
        } => {
            assert_eq!(line, 2);
            assert_eq!(source, "AA[");
            assert!(matches!(error, TemplateError::Syntax { .. }));
        }
        other => panic!("expected BadFragment, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// 5.1 Nested lookup: a fragment may not contain `lookup`
// ---------------------------------------------------------------------

#[test]
fn a_fragment_containing_lookup_fails_to_load_with_nested_lookup() {
    let text =
        "abbreviation\ttitle\tshorttitle\n[journal:lookup(\"other\")]\tAmino Acids\tAmino Acids\n";
    let err = Table::load(text, &title_to_fragment_abbreviation())
        .expect_err("a fragment may not look up another table");
    match err {
        TableError::NestedLookup {
            line,
            source,
            table,
        } => {
            assert_eq!(line, 2);
            assert_eq!(source, "[journal:lookup(\"other\")]");
            assert_eq!(table, "other");
        }
        other => panic!("expected NestedLookup, got {other:?}"),
    }
}

#[test]
fn a_fragment_with_no_lookup_still_loads() {
    // Guards against over-refusing: a fragment using other filters, with
    // no `lookup` anywhere in it, is not caught by the same check.
    let text =
        "abbreviation\ttitle\tshorttitle\nABB[volume:prefix(\"-\")]\tAmino Acids\tAmino Acids\n";
    let (table, warnings) = Table::load(text, &title_to_fragment_abbreviation()).expect("loads");
    assert_eq!(warnings, Vec::new());
    assert_eq!(
        table.get("Amino Acids").expect("hit").source(),
        "ABB[volume:prefix(\"-\")]"
    );
}

#[test]
fn a_text_valued_table_with_a_lookup_looking_cell_loads_and_substitutes_it_verbatim() {
    // Literal values are data and are never inspected for `lookup`: the
    // check only applies where `spec.values` is `ValueKind::Template`.
    let text =
        "abbreviation\ttitle\tshorttitle\n[journal:lookup(\"other\")]\tAmino Acids\tAmino Acids\n";
    let (table, warnings) = Table::load(text, &title_to_abbreviation()).expect("loads");
    assert_eq!(warnings, Vec::new());

    let value = table.get("Amino Acids").expect("hit");
    assert_eq!(value.source(), "[journal:lookup(\"other\")]");
    assert_eq!(
        value.render(&input(&record_with_year(2024))),
        "[journal:lookup(\"other\")]"
    );
}
