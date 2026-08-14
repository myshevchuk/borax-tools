#![allow(clippy::unwrap_used)]

use borax_core::identifier::{ArxivId, Doi};
use borax_core::record::{BoraxExt, DateParts, EntryType, Name, Record};
use borax_core::template::{RenderInput, Template, TemplateError, TemplateTable};

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
    Template::compile(template).unwrap().render(input)
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
    assert_eq!(template.render(&input), template.render(&input));
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
    assert_eq!(template.render(&input), "hello ] world");
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
    assert_eq!(table.render(&article_input), "2024");

    let mut thesis_record = record();
    thesis_record.entry_type = EntryType::Thesis;
    let thesis_input = RenderInput {
        record: &thesis_record,
        sha1: Some(SHA1),
    };
    assert_eq!(table.render(&thesis_input), "Smith");
}
