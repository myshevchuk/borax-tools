#![allow(clippy::unwrap_used)]

use borax_core::identifier::{ArxivId, Doi};
use borax_core::record::{BoraxExt, DateParts, EntryType, Name, Record};
use borax_core::tables::{LookupTables, Table, TableSpec, ValueKind};
use borax_core::template::{Miss, RenderInput, Rendered, Template, TemplateError, TemplateTable};

/// Lowercase hex digest used as the `sha1` field across tests.
const SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";

/// The shared fixture: an `Article` by Smith/Doe/Roe, titled "An Awesome
/// Paper on Borax", issued 2024-05 in "J. Chem. Ed.", with a DOI and a
/// versioned arXiv id.
fn record() -> Record {
    Record {
        entry_type: EntryType::Article,
        title: Some("An Awesome Paper on Borax".to_string()),
        authors: vec![
            Name {
                family: "Smith".to_string(),
                given: Some("Jane".to_string()),
            },
            Name {
                family: "Doe".to_string(),
                given: Some("John".to_string()),
            },
            Name {
                family: "Roe".to_string(),
                given: Some("Richard".to_string()),
            },
        ],
        issued: Some(DateParts {
            year: 2024,
            month: Some(5),
            day: None,
        }),
        container_title: Some("J. Chem. Ed.".to_string()),
        volume: None,
        issue: None,
        pages: None,
        publisher: None,
        doi: Some(Doi::parse("10.1021/jacs.4c01234").unwrap()),
        pmid: None,
        isbn: None,
        borax: BoraxExt {
            arxiv: Some(ArxivId::parse("2401.12345v2").unwrap()),
            ..Default::default()
        },
    }
}

fn render(template: &str, input: &RenderInput<'_>) -> String {
    Template::compile(template)
        .unwrap()
        .render(input, &LookupTables::new())
        .text
}

/// Compile `template` and render it against `tables`, keeping the full
/// [`Rendered`] (text and misses) rather than discarding the misses the
/// way [`render`] does.
fn render_with_tables(template: &str, input: &RenderInput<'_>, tables: &LookupTables) -> Rendered {
    Template::compile(template).unwrap().render(input, tables)
}

/// A `jcode` table over one row of the real curated file
/// (`journal_titles.tsv`): `JACS` maps `title` to `abbreviation`. Keyed
/// on `title` alone, so the `shorttitle` column (`J. Am. Chem. Soc.`) is
/// not a key of this table.
fn jcode_tables() -> LookupTables {
    let text = "abbreviation\ttitle\tshorttitle\n\
                 JACS\tJournal of the American Chemical Society\tJ. Am. Chem. Soc.\n";
    let spec = TableSpec {
        key_columns: vec!["title".to_string()],
        value_column: "abbreviation".to_string(),
        values: ValueKind::Text,
    };
    let (table, warnings) = Table::load(text, &spec).unwrap();
    assert_eq!(warnings, Vec::new());
    let mut tables = LookupTables::new();
    tables.insert("jcode".to_string(), table);
    tables
}

// ---------------------------------------------------------------------
// Happy paths: literals, fields, alternatives, determinism
// ---------------------------------------------------------------------

#[test]
fn spec_scenario_basic_rendering() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(
        render("[auth:lower][year]_[shorttitle3:camel]", &input),
        "smith2024_AwesomePaperBorax"
    );
}

#[test]
fn literal_text_passes_through_around_tokens() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("x [year] y", &input), "x 2024 y");
}

#[test]
fn title_field_renders_verbatim() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[title]", &input), "An Awesome Paper on Borax");
}

#[test]
fn journal_field_renders_container_title_verbatim() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[journal]", &input), "J. Chem. Ed.");
}

#[test]
fn doi_field_renders_normalized_doi() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[doi]", &input), "10.1021/jacs.4c01234");
}

#[test]
fn arxiv_field_renders_bare_id_without_version() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[arxiv]", &input), "2401.12345");
}

#[test]
fn entrytype_field_renders_csl_type_string() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[entrytype]", &input), "article-journal");
}

#[test]
fn sha1_field_truncated_renders_first_n_chars() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[sha1:trunc8]", &input), "da39a3ee");
}

#[test]
fn auth_field_renders_first_family_name() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[auth]", &input), "Smith");
}

#[test]
fn authors_field_renders_all_family_names_hyphenated() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[authors]", &input), "Smith-Doe-Roe");
}

#[test]
fn authorsn_field_appends_etal_when_authors_dropped() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[authors2]", &input), "Smith-Doe-etal");
}

#[test]
fn authorsn_field_omits_etal_when_n_covers_all_authors() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[authors3]", &input), "Smith-Doe-Roe");
}

#[test]
fn missing_fields_render_empty_and_are_skipped_in_context() {
    let r = Record::new(EntryType::Report);
    let input = RenderInput {
        record: &r,
        sha1: None,
    };
    assert_eq!(render("[auth][year][doi]", &input), "");
    assert_eq!(render("a[title]b", &input), "ab");
}

// ---------------------------------------------------------------------
// Publication fields
// ---------------------------------------------------------------------

#[test]
fn volume_field_renders_record_value() {
    let mut r = record();
    r.volume = Some("146".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[volume]", &input), "146");
}

#[test]
fn spec_scenario_absent_volume_renders_empty() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[volume]", &input), "");
}

#[test]
fn issue_field_renders_record_value() {
    let mut r = record();
    r.issue = Some("3".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[issue]", &input), "3");
}

#[test]
fn issue_field_renders_empty_when_absent() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[issue]", &input), "");
}

#[test]
fn pages_field_renders_record_value_verbatim() {
    let mut r = record();
    r.pages = Some("1234-1245".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[pages]", &input), "1234-1245");
}

#[test]
fn pages_field_renders_empty_when_absent() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[pages]", &input), "");
}

#[test]
fn publisher_field_renders_record_value() {
    let mut r = record();
    r.publisher = Some("ACS".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[publisher]", &input), "ACS");
}

#[test]
fn publisher_field_renders_empty_when_absent() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[publisher]", &input), "");
}

#[test]
fn firstpage_field_renders_before_a_hyphen() {
    let mut r = record();
    r.pages = Some("1234-1245".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[firstpage]", &input), "1234");
}

#[test]
fn firstpage_field_renders_before_an_en_dash() {
    let mut r = record();
    r.pages = Some("1234–1245".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[firstpage]", &input), "1234");
}

#[test]
fn firstpage_field_renders_before_an_em_dash() {
    let mut r = record();
    r.pages = Some("1234—1245".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[firstpage]", &input), "1234");
}

#[test]
fn firstpage_field_trims_whitespace_around_the_separator() {
    let mut r = record();
    r.pages = Some("1234 - 1245".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[firstpage]", &input), "1234");
}

#[test]
fn spec_scenario_article_number_is_not_a_range() {
    let mut r = record();
    r.pages = Some("e0123456".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[firstpage]", &input), "e0123456");
}

#[test]
fn firstpage_field_passes_through_a_bare_numeric_article_number() {
    let mut r = record();
    r.pages = Some("045301".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[firstpage]", &input), "045301");
}

#[test]
fn firstpage_field_renders_empty_when_pages_absent() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[firstpage]", &input), "");
}

#[test]
fn spec_scenario_volume_and_first_page_render() {
    let mut r = record();
    r.volume = Some("146".to_string());
    r.pages = Some("1234-1245".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(
        render("[year]-[volume]-[firstpage]", &input),
        "2024-146-1234"
    );
}

// ---------------------------------------------------------------------
// Filters
// ---------------------------------------------------------------------

#[test]
fn lower_filter_lowercases_whole_string() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[title:lower]", &input), "an awesome paper on borax");
}

#[test]
fn upper_filter_uppercases_whole_string() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[title:upper]", &input), "AN AWESOME PAPER ON BORAX");
}

#[test]
fn capitalize_filter_uppercases_first_char_and_lowercases_rest() {
    let mut r = Record::new(EntryType::Article);
    r.authors.push(Name {
        family: "VAN DIJK".to_string(),
        given: None,
    });
    let input = RenderInput {
        record: &r,
        sha1: None,
    };
    assert_eq!(render("[auth:capitalize]", &input), "Van dijk");
}

#[test]
fn titlecase_filter_capitalizes_each_word_preserving_spaces() {
    let mut r = Record::new(EntryType::Article);
    r.title = Some("an AWESOME paper".to_string());
    let input = RenderInput {
        record: &r,
        sha1: None,
    };
    assert_eq!(render("[title:titlecase]", &input), "An Awesome Paper");
}

#[test]
fn camel_filter_titlecases_then_removes_separators() {
    let mut r = Record::new(EntryType::Article);
    r.title = Some("an AWESOME paper".to_string());
    let input = RenderInput {
        record: &r,
        sha1: None,
    };
    assert_eq!(render("[title:camel]", &input), "AnAwesomePaper");
}

#[test]
fn abbr_filter_takes_first_char_of_each_word_preserving_case() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[journal:abbr]", &input), "JCE");
}

#[test]
fn trunc_filter_takes_first_n_chars() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[title:trunc10]", &input), "An Awesome");
}

#[test]
fn trunc_filter_counts_unicode_scalar_values_not_bytes() {
    let mut r = Record::new(EntryType::Article);
    r.title = Some("Grüße aus Wien".to_string());
    let input = RenderInput {
        record: &r,
        sha1: None,
    };
    assert_eq!(render("[title:trunc5]", &input), "Grüße");
}

#[test]
fn slug_filter_transliterates_lowercases_and_dashes_punctuation_runs() {
    let mut r = Record::new(EntryType::Article);
    r.title = Some("Grüße & Co: eine Studie!".to_string());
    let input = RenderInput {
        record: &r,
        sha1: None,
    };
    assert_eq!(render("[title:slug]", &input), "gruesse-co-eine-studie");
}

#[test]
fn slug_filter_trims_leading_and_trailing_dashes() {
    let mut r = Record::new(EntryType::Article);
    r.title = Some("  ...borax...  ".to_string());
    let input = RenderInput {
        record: &r,
        sha1: None,
    };
    assert_eq!(render("[title:slug]", &input), "borax");
}

#[test]
fn transliterate_filter_folds_latin_diacritics_to_ascii() {
    let mut r = Record::new(EntryType::Article);
    r.title = Some("Grüße æther Ñandú".to_string());
    let input = RenderInput {
        record: &r,
        sha1: None,
    };
    assert_eq!(
        render("[title:transliterate]", &input),
        "Gruesse aether Nandu"
    );
}

#[test]
fn regex_filter_replaces_all_occurrences() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(
        render(r#"[title:regex("Paper","Note")]"#, &input),
        "An Awesome Note on Borax"
    );
}

#[test]
fn regex_filter_supports_group_references_in_replacement() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(
        render(r#"[year:regex("(\d{2})(\d{2})","$2-$1")]"#, &input),
        "24-20"
    );
}

#[test]
fn regex_filter_pattern_may_contain_an_escaped_quote() {
    let mut r = Record::new(EntryType::Article);
    r.title = Some(r#"A "Quoted" Title"#.to_string());
    let input = RenderInput {
        record: &r,
        sha1: None,
    };
    // The pattern is a single literal `"` (written `\"` inside the
    // quoted pattern argument); replacing it with the empty string
    // strips both quote characters from the title.
    assert_eq!(
        render(r#"[title:regex("\"","")]"#, &input),
        "A Quoted Title"
    );
}

// ---------------------------------------------------------------------
// Affix filters
// ---------------------------------------------------------------------

#[test]
fn spec_scenario_suffix_on_present_value() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render(r#"[auth:suffix("-")]"#, &input), "Smith-");
}

#[test]
fn prefix_filter_wraps_a_present_value() {
    let mut r = record();
    r.volume = Some("146".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render(r#"[volume:prefix("-")]"#, &input), "-146");
}

#[test]
fn prefix_filter_leaves_the_empty_string_unchanged() {
    // Load-bearing: a missing volume must contribute nothing, not a
    // stray "-", so the separator belongs to the optional segment it
    // separates rather than to the field.
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render(r#"[volume:prefix("-")]"#, &input), "");
}

#[test]
fn suffix_filter_leaves_the_empty_string_unchanged() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render(r#"[volume:suffix("-")]"#, &input), "");
}

#[test]
fn affix_filter_composes_to_the_right_of_another_filter() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render(r#"[auth:lower:prefix("-")]"#, &input), "-smith");
}

#[test]
fn spec_scenario_optional_segment_carries_its_separator() {
    let mut r = record();
    r.container_title = Some("J. Am. Chem. Soc.".to_string());
    r.volume = Some("146".to_string());
    r.pages = Some("1234-1245".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(
        render(
            r#"[year]-[journal:abbr][volume:prefix("-")]-[firstpage]"#,
            &input
        ),
        "2024-JACS-146-1234"
    );
}

#[test]
fn spec_scenario_same_template_with_no_volume() {
    let mut r = record();
    r.container_title = Some("J. Am. Chem. Soc.".to_string());
    r.pages = Some("1234-1245".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(
        render(
            r#"[year]-[journal:abbr][volume:prefix("-")]-[firstpage]"#,
            &input
        ),
        "2024-JACS-1234"
    );
}

// ---------------------------------------------------------------------
// Filter chaining
// ---------------------------------------------------------------------

#[test]
fn filters_chain_left_to_right_slug_then_trunc() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    // slug(title) = "an-awesome-paper-on-borax"; its first 7 chars keep
    // the dash produced by slug, since trunc runs after slug.
    assert_eq!(render("[title:slug:trunc7]", &input), "an-awes");
}

#[test]
fn filters_chain_left_to_right_trunc_then_slug_differs_from_reverse() {
    let mut r = Record::new(EntryType::Article);
    r.title = Some("Über Borax".to_string());
    let input = RenderInput {
        record: &r,
        sha1: None,
    };
    // slug("Über Borax") = "ueber-borax" (Ü transliterates to "Ue"
    // before the length-6 truncation applies), so trunc6 keeps the
    // trailing dash: "ueber-".
    assert_eq!(render("[title:slug:trunc6]", &input), "ueber-");
    // trunc6("Über Borax") = "Über B" (6 Unicode scalar values), then
    // slug transliterates and dashes it: "ueber-b" — one character
    // longer than the reverse order, because transliteration expands
    // Ü to two ASCII characters only once slug runs on the (shorter)
    // truncated fragment.
    assert_eq!(render("[title:trunc6:slug]", &input), "ueber-b");
}

// ---------------------------------------------------------------------
// Alternatives
// ---------------------------------------------------------------------

#[test]
fn alternatives_pick_first_non_empty_chain() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(
        render("[doi:slug || sha1:trunc8]", &input),
        "10-1021-jacs-4c01234"
    );
}

#[test]
fn alternatives_fall_through_to_later_chain_when_earlier_is_empty() {
    let mut r = record();
    r.doi = None;
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[doi:slug || sha1:trunc8]", &input), "da39a3ee");
}

#[test]
fn alternatives_render_empty_when_every_chain_is_empty() {
    let r = Record::new(EntryType::Report);
    let input = RenderInput {
        record: &r,
        sha1: None,
    };
    assert_eq!(render("[doi:slug || sha1:trunc8]", &input), "");
}

#[test]
fn alternatives_compile_without_surrounding_whitespace() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    assert_eq!(render("[doi||sha1]", &input), "10.1021/jacs.4c01234");
}

// ---------------------------------------------------------------------
// Lookup filter and misses
// ---------------------------------------------------------------------

#[test]
fn lookup_filter_hit_substitutes_the_tables_value() {
    let mut r = record();
    r.container_title = Some("Journal of the American Chemical Society".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    let tables = jcode_tables();
    let rendered = render_with_tables(r#"[journal:lookup("jcode")]"#, &input, &tables);
    assert_eq!(rendered.text, "JACS");
}

#[test]
fn lookup_filter_hit_records_no_miss() {
    let mut r = record();
    r.container_title = Some("Journal of the American Chemical Society".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    let tables = jcode_tables();
    let rendered = render_with_tables(r#"[journal:lookup("jcode")]"#, &input, &tables);
    assert_eq!(rendered.misses, Vec::new());
}

#[test]
fn lookup_filter_composes_with_a_following_filter() {
    let mut r = record();
    r.container_title = Some("Journal of the American Chemical Society".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    let tables = jcode_tables();
    let rendered = render_with_tables(r#"[journal:lookup("jcode"):lower]"#, &input, &tables);
    assert_eq!(rendered.text, "jacs");
}

#[test]
fn lookup_filter_matching_folds_both_sides() {
    // Differs from the table's key only by trailing punctuation and
    // case; the fold that builds the table's keys is applied to the
    // input too, so this still hits.
    let mut r = record();
    r.container_title = Some("journal of the american chemical society.".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    let tables = jcode_tables();
    let rendered = render_with_tables(r#"[journal:lookup("jcode")]"#, &input, &tables);
    assert_eq!(rendered.text, "JACS");
    assert_eq!(rendered.misses, Vec::new());
}

#[test]
fn lookup_filter_miss_renders_empty_and_records_a_miss() {
    // The shared fixture's container title, "J. Chem. Ed.", is not a key
    // of `jcode_tables`.
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    let tables = jcode_tables();
    let rendered = render_with_tables(r#"[journal:lookup("jcode")]"#, &input, &tables);
    assert_eq!(rendered.text, "");
    assert_eq!(
        rendered.misses,
        vec![Miss {
            table: "jcode".to_string(),
            input: "J. Chem. Ed.".to_string(),
        }]
    );
}

#[test]
fn lookup_filter_miss_carries_the_input_as_the_record_held_it() {
    // "J. Am. Chem. Soc." folds differently from the table's title-only
    // key ("Journal of the American Chemical Society"), so it misses;
    // the recorded miss carries the record's own spelling, not the
    // folded form ("j-am-chem-soc").
    let mut r = record();
    r.container_title = Some("J. Am. Chem. Soc.".to_string());
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    let tables = jcode_tables();
    let rendered = render_with_tables(r#"[journal:lookup("jcode")]"#, &input, &tables);
    assert_eq!(
        rendered.misses,
        vec![Miss {
            table: "jcode".to_string(),
            input: "J. Am. Chem. Soc.".to_string(),
        }]
    );
}

#[test]
fn lookup_filter_miss_falls_through_to_an_alternative() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    let tables = jcode_tables();
    let rendered = render_with_tables(
        r#"[journal:lookup("jcode") || journal:abbr]"#,
        &input,
        &tables,
    );
    assert_eq!(rendered.text, "JCE");
}

#[test]
fn lookup_filter_miss_is_recorded_even_when_an_alternative_supplied_the_output() {
    // Load-bearing: the table lacking this journal is worth knowing
    // about even though `abbr` covered for it in the rendered text.
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    let tables = jcode_tables();
    let rendered = render_with_tables(
        r#"[journal:lookup("jcode") || journal:abbr]"#,
        &input,
        &tables,
    );
    assert_eq!(rendered.text, "JCE");
    assert_eq!(
        rendered.misses,
        vec![Miss {
            table: "jcode".to_string(),
            input: "J. Chem. Ed.".to_string(),
        }]
    );
}

#[test]
fn lookup_filter_misses_appear_in_template_evaluation_order() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    let tables = jcode_tables();
    let rendered = render_with_tables(
        r#"[journal:lookup("jcode")]-[auth:lookup("jcode")]"#,
        &input,
        &tables,
    );
    assert_eq!(
        rendered.misses,
        vec![
            Miss {
                table: "jcode".to_string(),
                input: "J. Chem. Ed.".to_string(),
            },
            Miss {
                table: "jcode".to_string(),
                input: "Smith".to_string(),
            },
        ]
    );
}

// ---------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------

#[test]
fn render_is_deterministic_across_repeated_calls() {
    let r = record();
    let input = RenderInput {
        record: &r,
        sha1: Some(SHA1),
    };
    let template = Template::compile("[auth:lower][year]_[shorttitle3:camel]").unwrap();
    assert_eq!(
        template.render(&input, &LookupTables::new()).text,
        template.render(&input, &LookupTables::new()).text
    );
}

// Fail-fast contract: `Template::compile` performs all validation, so a
// successfully compiled `Template::render` cannot fail — its return type
// is a plain `String`, not a `Result`, which the type system already
// enforces. No test is needed for that half of the contract.

// ---------------------------------------------------------------------
// Compile failures
// ---------------------------------------------------------------------

#[test]
fn spec_scenario_typo_in_filter_name_is_unknown_filter() {
    let err = Template::compile("[title:lwoer]").unwrap_err();
    match err {
        TemplateError::UnknownFilter { token } => assert_eq!(token, "lwoer"),
        other => panic!("expected UnknownFilter, got {other:?}"),
    }
}

#[test]
fn unknown_field_name_is_rejected() {
    let err = Template::compile("[titel]").unwrap_err();
    match err {
        TemplateError::UnknownField { token } => assert_eq!(token, "titel"),
        other => panic!("expected UnknownField, got {other:?}"),
    }
}

#[test]
fn unclosed_bracket_is_syntax_error() {
    let err = Template::compile("[title").unwrap_err();
    assert!(matches!(err, TemplateError::Syntax { .. }));
}

#[test]
fn empty_token_is_syntax_error() {
    let err = Template::compile("[]").unwrap_err();
    assert!(matches!(err, TemplateError::Syntax { .. }));
}

#[test]
fn whitespace_inside_chain_is_syntax_error() {
    let err = Template::compile("[title: lower]").unwrap_err();
    assert!(matches!(err, TemplateError::Syntax { .. }));
}

#[test]
fn stray_closing_bracket_outside_token_is_literal() {
    let template = Template::compile("hello ] world").unwrap();
    let r = Record::new(EntryType::Report);
    let input = RenderInput {
        record: &r,
        sha1: None,
    };
    assert_eq!(
        template.render(&input, &LookupTables::new()).text,
        "hello ] world"
    );
}

#[test]
fn filter_name_used_as_a_field_is_unknown_field() {
    let err = Template::compile("[trunc3]").unwrap_err();
    match err {
        TemplateError::UnknownField { token } => assert_eq!(token, "trunc3"),
        other => panic!("expected UnknownField, got {other:?}"),
    }
}

#[test]
fn trunc_filter_without_n_is_rejected() {
    let err = Template::compile("[title:trunc]").unwrap_err();
    assert!(matches!(
        err,
        TemplateError::Syntax { .. } | TemplateError::UnknownFilter { .. }
    ));
}

#[test]
fn trunc_filter_with_zero_n_is_syntax_error() {
    let err = Template::compile("[title:trunc0]").unwrap_err();
    assert!(matches!(err, TemplateError::Syntax { .. }));
}

#[test]
fn malformed_regex_pattern_is_bad_regex() {
    let err = Template::compile(r#"[title:regex("(unclosed","x")]"#).unwrap_err();
    assert!(matches!(err, TemplateError::BadRegex { .. }));
}

#[test]
fn affix_filter_unterminated_quoted_argument_is_syntax_error() {
    let err = Template::compile(r#"[auth:prefix("-]"#).unwrap_err();
    assert!(matches!(err, TemplateError::Syntax { .. }));
}

#[test]
fn affix_filter_missing_closing_paren_is_syntax_error() {
    let err = Template::compile(r#"[auth:prefix("-"]"#).unwrap_err();
    assert!(matches!(err, TemplateError::Syntax { .. }));
}

// ---------------------------------------------------------------------
// Template::tables
// ---------------------------------------------------------------------

#[test]
fn template_tables_reports_tables_in_order_of_first_appearance() {
    let template = Template::compile(r#"[journal:lookup("b")]-[auth:lookup("a")]"#).unwrap();
    assert_eq!(template.tables(), vec!["b", "a"]);
}

#[test]
fn template_tables_deduplicates_a_table_looked_up_twice() {
    let template =
        Template::compile(r#"[journal:lookup("jcode")]-[auth:lookup("jcode")]"#).unwrap();
    assert_eq!(template.tables(), vec!["jcode"]);
}

#[test]
fn template_tables_is_empty_when_the_template_has_no_lookup() {
    let template = Template::compile("[journal:abbr]").unwrap();
    assert_eq!(template.tables(), Vec::<&str>::new());
}

#[test]
fn template_tables_reports_tables_from_every_alternative() {
    let template =
        Template::compile(r#"[journal:lookup("jcode") || auth:lookup("author-table")]"#).unwrap();
    assert_eq!(template.tables(), vec!["jcode", "author-table"]);
}

// ---------------------------------------------------------------------
// TemplateTable
// ---------------------------------------------------------------------

#[test]
fn template_table_uses_default_for_every_type_when_unset() {
    let default = Template::compile("[year]").unwrap();
    let table = TemplateTable::new(default);
    for entry_type in [
        EntryType::Article,
        EntryType::Book,
        EntryType::Thesis,
        EntryType::Report,
    ] {
        assert_eq!(table.get(entry_type).source(), "[year]");
    }
}

#[test]
fn spec_scenario_type_specific_template_wins() {
    let default = Template::compile("[year]").unwrap();
    let thesis = Template::compile("[auth]").unwrap();
    let mut table = TemplateTable::new(default);
    table.insert(EntryType::Thesis, thesis);

    assert_eq!(table.get(EntryType::Thesis).source(), "[auth]");
    assert_eq!(table.get(EntryType::Book).source(), "[year]");
}

#[test]
fn template_table_render_dispatches_on_record_entry_type() {
    let default = Template::compile("[year]").unwrap();
    let thesis = Template::compile("[auth]").unwrap();
    let mut table = TemplateTable::new(default);
    table.insert(EntryType::Thesis, thesis);

    let article = record();
    let article_input = RenderInput {
        record: &article,
        sha1: Some(SHA1),
    };
    assert_eq!(
        table.render(&article_input, &LookupTables::new()).text,
        "2024"
    );

    let mut thesis_record = record();
    thesis_record.entry_type = EntryType::Thesis;
    let thesis_input = RenderInput {
        record: &thesis_record,
        sha1: Some(SHA1),
    };
    assert_eq!(
        table.render(&thesis_input, &LookupTables::new()).text,
        "Smith"
    );
}
