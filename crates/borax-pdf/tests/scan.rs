#![allow(clippy::unwrap_used)]

use borax_core::identifier::{ArxivId, Doi, Identifier};
use borax_pdf::scan::{FoundIdentifier, scan_info, scan_text, scan_xmp, xmp_title};
use borax_pdf::source::InfoMetadata;

// ---------------------------------------------------------------------
// scan_text: DOI
// ---------------------------------------------------------------------

#[test]
fn scan_text_finds_bare_doi_in_a_sentence() {
    let found = scan_text("see 10.1021/jacs.4c01234 for details");
    assert_eq!(
        found,
        Some(FoundIdentifier::Doi(
            Doi::parse("10.1021/jacs.4c01234").unwrap()
        ))
    );
}

#[test]
fn scan_text_normalizes_doi_url_with_trailing_punctuation() {
    let found = scan_text("https://doi.org/10.1021/JACS.4C01234.");
    assert_eq!(
        found,
        Some(FoundIdentifier::Doi(
            Doi::parse("10.1021/jacs.4c01234").unwrap()
        ))
    );
}

#[test]
fn scan_text_strips_punctuation_around_doi_in_parentheses() {
    let found = scan_text("(10.1021/x1)");
    assert_eq!(
        found,
        Some(FoundIdentifier::Doi(Doi::parse("10.1021/x1").unwrap()))
    );
}

#[test]
fn scan_text_strips_trailing_comma_and_semicolon() {
    for text in ["see 10.1021/x1, next", "see 10.1021/x1; next"] {
        let found = scan_text(text);
        assert_eq!(
            found,
            Some(FoundIdentifier::Doi(Doi::parse("10.1021/x1").unwrap())),
            "text {text:?}"
        );
    }
}

#[test]
fn scan_text_skips_invalid_doi_candidate_and_returns_the_next_valid_one() {
    let found = scan_text("bad candidate 10.1/bad then good 10.1021/good");
    assert_eq!(
        found,
        Some(FoundIdentifier::Doi(Doi::parse("10.1021/good").unwrap()))
    );
}

#[test]
fn scan_text_prefers_the_earlier_of_two_valid_dois() {
    let found = scan_text("first 10.1021/first then 10.2000/second");
    assert_eq!(
        found,
        Some(FoundIdentifier::Doi(Doi::parse("10.1021/first").unwrap()))
    );
}

// ---------------------------------------------------------------------
// scan_text: arXiv
// ---------------------------------------------------------------------

#[test]
fn scan_text_finds_arxiv_id_with_version_marker() {
    let found = scan_text("Preprint arXiv:2401.12345v2 submitted");
    let Some(FoundIdentifier::Arxiv(id)) = found else {
        panic!("expected an arXiv identifier, got {found:?}");
    };
    assert_eq!(id.id(), "2401.12345");
    assert_eq!(id.version(), Some(2));
}

#[test]
fn scan_text_finds_arxiv_id_with_space_after_colon() {
    let found = scan_text("arXiv: 2401.12345");
    let Some(FoundIdentifier::Arxiv(id)) = found else {
        panic!("expected an arXiv identifier, got {found:?}");
    };
    assert_eq!(id.id(), "2401.12345");
}

#[test]
fn scan_text_finds_arxiv_marker_case_insensitively() {
    let found = scan_text("ARXIV:2401.12345");
    let Some(FoundIdentifier::Arxiv(id)) = found else {
        panic!("expected an arXiv identifier, got {found:?}");
    };
    assert_eq!(id.id(), "2401.12345");
}

#[test]
fn scan_text_finds_arxiv_abs_url_form() {
    let found = scan_text("see https://arxiv.org/abs/2401.12345 for the preprint");
    let Some(FoundIdentifier::Arxiv(id)) = found else {
        panic!("expected an arXiv identifier, got {found:?}");
    };
    assert_eq!(id.id(), "2401.12345");
}

#[test]
fn scan_text_finds_old_style_arxiv_id() {
    let found = scan_text("arXiv:math.GT/0309136");
    let Some(FoundIdentifier::Arxiv(id)) = found else {
        panic!("expected an arXiv identifier, got {found:?}");
    };
    assert_eq!(id.id(), "math.GT/0309136");
}

#[test]
fn scan_text_bare_number_is_not_treated_as_an_identifier() {
    assert_eq!(scan_text("as shown in 2401.12345 above"), None);
    assert_eq!(scan_text("Figure 1203.0023 shows"), None);
}

// ---------------------------------------------------------------------
// scan_text: precedence, edge cases
// ---------------------------------------------------------------------

#[test]
fn scan_text_doi_wins_over_arxiv_regardless_of_order() {
    let found = scan_text("arXiv:2401.12345 later 10.1021/jacs.4c01234 appears");
    assert_eq!(
        found,
        Some(FoundIdentifier::Doi(
            Doi::parse("10.1021/jacs.4c01234").unwrap()
        ))
    );
}

#[test]
fn scan_text_empty_text_returns_none() {
    assert_eq!(scan_text(""), None);
}

#[test]
fn scan_text_no_identifiers_returns_none() {
    assert_eq!(scan_text("nothing interesting here at all"), None);
}

#[test]
fn scan_text_finds_doi_on_a_later_line() {
    let found = scan_text("line one\nline two\nsee 10.1021/jacs.4c01234 here");
    assert_eq!(
        found,
        Some(FoundIdentifier::Doi(
            Doi::parse("10.1021/jacs.4c01234").unwrap()
        ))
    );
}

// ---------------------------------------------------------------------
// scan_xmp
// ---------------------------------------------------------------------

#[test]
fn scan_xmp_finds_doi_in_prism_doi_element() {
    let xmp = r#"<rdf:RDF><prism:doi>10.1021/jacs.4c01234</prism:doi></rdf:RDF>"#;
    assert_eq!(
        scan_xmp(xmp),
        Some(FoundIdentifier::Doi(
            Doi::parse("10.1021/jacs.4c01234").unwrap()
        ))
    );
}

#[test]
fn scan_xmp_accepts_any_prefix_on_doi_element() {
    assert_eq!(
        scan_xmp("<pdfx:doi>10.1021/x1</pdfx:doi>"),
        Some(FoundIdentifier::Doi(Doi::parse("10.1021/x1").unwrap()))
    );
    assert_eq!(
        scan_xmp("<doi>10.1021/x1</doi>"),
        Some(FoundIdentifier::Doi(Doi::parse("10.1021/x1").unwrap()))
    );
}

#[test]
fn scan_xmp_normalizes_doi_url_in_dc_identifier() {
    let xmp = "<dc:identifier>https://doi.org/10.1021/x1</dc:identifier>";
    assert_eq!(
        scan_xmp(xmp),
        Some(FoundIdentifier::Doi(Doi::parse("10.1021/x1").unwrap()))
    );
}

#[test]
fn scan_xmp_finds_doi_in_attribute_form() {
    let xmp = r#"<rdf:Description prism:doi="10.1021/x1"/>"#;
    assert_eq!(
        scan_xmp(xmp),
        Some(FoundIdentifier::Doi(Doi::parse("10.1021/x1").unwrap()))
    );
}

#[test]
fn scan_xmp_finds_arxiv_id_in_dc_identifier() {
    let xmp = "<dc:identifier>arXiv:2401.12345</dc:identifier>";
    let Some(FoundIdentifier::Arxiv(id)) = scan_xmp(xmp) else {
        panic!("expected an arXiv identifier");
    };
    assert_eq!(id.id(), "2401.12345");
}

#[test]
fn scan_xmp_ignores_elements_that_are_not_doi_or_identifier() {
    assert_eq!(scan_xmp("<dc:title>10.1021/x1</dc:title>"), None);
}

#[test]
fn scan_xmp_tolerates_malformed_xml() {
    let xmp = "<prism:doi>10.1021/x1</prism:doi><broken";
    assert_eq!(
        scan_xmp(xmp),
        Some(FoundIdentifier::Doi(Doi::parse("10.1021/x1").unwrap()))
    );
}

#[test]
fn scan_xmp_empty_input_returns_none() {
    assert_eq!(scan_xmp(""), None);
}

#[test]
fn scan_xmp_no_match_returns_none() {
    assert_eq!(
        scan_xmp("<rdf:RDF><dc:title>Some Title</dc:title></rdf:RDF>"),
        None
    );
}

#[test]
fn scan_xmp_document_order_skips_invalid_element_for_valid_later_one() {
    let xmp = "<prism:doi>10.1/bad</prism:doi><prism:doi>10.1021/x1</prism:doi>";
    assert_eq!(
        scan_xmp(xmp),
        Some(FoundIdentifier::Doi(Doi::parse("10.1021/x1").unwrap()))
    );
}

// ---------------------------------------------------------------------
// scan_info
// ---------------------------------------------------------------------

fn info_with_custom(pairs: &[(&str, &str)]) -> InfoMetadata {
    let mut info = InfoMetadata::default();
    for (key, value) in pairs {
        info.custom.insert((*key).to_string(), (*value).to_string());
    }
    info
}

#[test]
fn scan_info_finds_doi_in_custom_key() {
    let info = info_with_custom(&[("WPS-ARTICLEDOI", "10.1021/jacs.4c01234")]);
    assert_eq!(
        scan_info(&info),
        Some(FoundIdentifier::Doi(
            Doi::parse("10.1021/jacs.4c01234").unwrap()
        ))
    );
}

#[test]
fn scan_info_scans_custom_keys_in_key_order() {
    let info = info_with_custom(&[("aaa", "10.1000/first"), ("zzz", "10.2000/second")]);
    assert_eq!(
        scan_info(&info),
        Some(FoundIdentifier::Doi(Doi::parse("10.1000/first").unwrap()))
    );
}

#[test]
fn scan_info_custom_precedes_subject() {
    let mut info = info_with_custom(&[("zzz", "10.2000/x")]);
    info.subject = Some("10.1000/y".to_string());
    assert_eq!(
        scan_info(&info),
        Some(FoundIdentifier::Doi(Doi::parse("10.2000/x").unwrap()))
    );
}

#[test]
fn scan_info_keywords_precede_title() {
    let info = InfoMetadata {
        keywords: Some("10.1000/kw".to_string()),
        title: Some("10.2000/ti".to_string()),
        ..Default::default()
    };
    assert_eq!(
        scan_info(&info),
        Some(FoundIdentifier::Doi(Doi::parse("10.1000/kw").unwrap()))
    );
}

#[test]
fn scan_info_subject_precedes_keywords() {
    let info = InfoMetadata {
        subject: Some("10.3000/su".to_string()),
        keywords: Some("10.1000/kw".to_string()),
        ..Default::default()
    };
    assert_eq!(
        scan_info(&info),
        Some(FoundIdentifier::Doi(Doi::parse("10.3000/su").unwrap()))
    );
}

#[test]
fn scan_info_default_empty_metadata_returns_none() {
    assert_eq!(scan_info(&InfoMetadata::default()), None);
}

#[test]
fn scan_info_finds_arxiv_id_in_a_field() {
    let info = InfoMetadata {
        subject: Some("arXiv:2401.12345".to_string()),
        ..Default::default()
    };
    let Some(FoundIdentifier::Arxiv(id)) = scan_info(&info) else {
        panic!("expected an arXiv identifier");
    };
    assert_eq!(id.id(), "2401.12345");
}

// ---------------------------------------------------------------------
// From<FoundIdentifier> for Identifier
// ---------------------------------------------------------------------

#[test]
fn doi_found_identifier_widens_to_a_doi_identifier_carrying_the_same_value() {
    let doi = Doi::parse("10.1021/jacs.4c01234").unwrap();
    let widened: Identifier = FoundIdentifier::Doi(doi.clone()).into();

    assert_eq!(widened, Identifier::Doi(doi));
}

#[test]
fn arxiv_found_identifier_widens_to_an_arxiv_identifier_carrying_the_same_value() {
    let id = ArxivId::parse("2401.12345v2").unwrap();
    let widened: Identifier = FoundIdentifier::Arxiv(id.clone()).into();

    assert_eq!(widened, Identifier::Arxiv(id));
}

// ================= xmp_title =================

/// The `dc:title` of `2011ASC(353)575.pdf`: a Distiller default, which
/// is a value the caller has to be able to see in order to dismiss.
const ASC_XMP: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
   <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/">
         <dc:format>application/pdf</dc:format>
         <dc:title>
            <rdf:Alt>
               <rdf:li xml:lang="x-default">untitled</rdf:li>
            </rdf:Alt>
         </dc:title>
      </rdf:Description>
   </rdf:RDF>
</x:xmpmeta>"#;

#[test]
fn xmp_title_reads_the_alt_default_of_dc_title() {
    assert_eq!(xmp_title(ASC_XMP), Some("untitled".to_string()));
}

#[test]
fn xmp_title_reads_a_dc_title_holding_text_directly() {
    let xmp = "<rdf:Description><dc:title>A Real Title</dc:title></rdf:Description>";
    assert_eq!(xmp_title(xmp), Some("A Real Title".to_string()));
}

#[test]
fn xmp_title_takes_the_first_rdf_li_when_several_languages_are_present() {
    let xmp = r#"<dc:title><rdf:Alt>
        <rdf:li xml:lang="x-default">Default Title</rdf:li>
        <rdf:li xml:lang="de">Deutscher Titel</rdf:li>
    </rdf:Alt></dc:title>"#;
    assert_eq!(xmp_title(xmp), Some("Default Title".to_string()));
}

#[test]
fn xmp_title_ignores_an_element_whose_local_name_is_not_title() {
    let xmp = "<dc:creator>Someone</dc:creator><dc:format>application/pdf</dc:format>";
    assert_eq!(xmp_title(xmp), None);
}

#[test]
fn xmp_title_none_when_the_packet_holds_no_title() {
    assert_eq!(xmp_title(""), None);
}

#[test]
fn xmp_title_none_when_the_title_is_empty() {
    assert_eq!(xmp_title("<dc:title></dc:title>"), None);
}

#[test]
fn xmp_title_trims_the_surrounding_whitespace() {
    assert_eq!(
        xmp_title("<dc:title>\n   Spaced Title \n</dc:title>"),
        Some("Spaced Title".to_string())
    );
}
