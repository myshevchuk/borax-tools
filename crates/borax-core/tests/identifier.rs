#![allow(clippy::unwrap_used)]

use borax_core::identifier::{ArxivId, Doi, IdentifierError, Isbn, Pmid};

// ---------------------------------------------------------------------
// Doi
// ---------------------------------------------------------------------

#[test]
fn doi_parse_bare_form_accepted_as_is() {
    let doi = Doi::parse("10.1021/jacs.4c01234").unwrap();
    assert_eq!(doi.as_str(), "10.1021/jacs.4c01234");
}

#[test]
fn doi_parse_strips_https_resolver_prefix_lowercases_and_strips_trailing_dot() {
    let doi = Doi::parse("https://doi.org/10.1021/JACS.4C01234.").unwrap();
    assert_eq!(doi.as_str(), "10.1021/jacs.4c01234");
}

#[test]
fn doi_parse_accepts_doi_scheme_prefix_case_insensitively() {
    let doi = Doi::parse("doi:10.5555/abc").unwrap();
    assert_eq!(doi.as_str(), "10.5555/abc");

    let doi = Doi::parse("DOI:10.5555/abc").unwrap();
    assert_eq!(doi.as_str(), "10.5555/abc");
}

#[test]
fn doi_parse_accepts_dx_doi_org_url_form() {
    let doi = Doi::parse("https://dx.doi.org/10.5555/abc").unwrap();
    assert_eq!(doi.as_str(), "10.5555/abc");
}

#[test]
fn doi_parse_strips_trailing_punctuation() {
    for trailer in [",", ";", ")"] {
        let input = format!("10.5555/abc{trailer}");
        let doi = Doi::parse(&input).unwrap();
        assert_eq!(doi.as_str(), "10.5555/abc", "trailer {trailer:?}");
    }
}

#[test]
fn doi_parse_rejects_empty_string() {
    let err = Doi::parse("").unwrap_err();
    assert!(matches!(err, IdentifierError::Invalid { .. }));
}

#[test]
fn doi_parse_rejects_wrong_leading_digits() {
    let err = Doi::parse("11.1234/x").unwrap_err();
    assert!(matches!(err, IdentifierError::Invalid { .. }));
}

#[test]
fn doi_parse_rejects_too_few_registrant_digits() {
    let err = Doi::parse("10.12/short-prefix").unwrap_err();
    assert!(matches!(err, IdentifierError::Invalid { .. }));
}

#[test]
fn doi_parse_rejects_missing_suffix() {
    let err = Doi::parse("10.1234").unwrap_err();
    assert!(matches!(err, IdentifierError::Invalid { .. }));
}

#[test]
fn doi_display_and_as_str_agree_on_normalized_form() {
    let doi = Doi::parse("https://doi.org/10.1021/JACS.4C01234.").unwrap();
    assert_eq!(doi.to_string(), "10.1021/jacs.4c01234");
    assert_eq!(doi.to_string(), doi.as_str());
}

#[test]
fn doi_serializes_to_json_string_of_normalized_form() {
    let doi = Doi::parse("https://doi.org/10.1021/JACS.4C01234.").unwrap();
    let json = serde_json::to_string(&doi).unwrap();
    assert_eq!(json, "\"10.1021/jacs.4c01234\"");
}

#[test]
fn doi_deserializing_invalid_string_fails() {
    let result: Result<Doi, _> = serde_json::from_str("\"not a doi\"");
    assert!(result.is_err());
}

#[test]
fn doi_valid_round_trips_to_equal_value() {
    let doi = Doi::parse("10.1021/jacs.4c01234").unwrap();
    let json = serde_json::to_string(&doi).unwrap();
    let parsed: Doi = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, doi);
}

// ---------------------------------------------------------------------
// ArxivId
// ---------------------------------------------------------------------

#[test]
fn arxiv_id_parse_new_style_no_version() {
    let id = ArxivId::parse("2401.12345").unwrap();
    assert_eq!(id.id(), "2401.12345");
    assert_eq!(id.version(), None);
}

#[test]
fn arxiv_id_parse_with_prefix_and_version() {
    let id = ArxivId::parse("arXiv:2401.12345v2").unwrap();
    assert_eq!(id.id(), "2401.12345");
    assert_eq!(id.version(), Some(2));
}

#[test]
fn arxiv_id_parse_old_style_no_version() {
    let id = ArxivId::parse("math.GT/0309136").unwrap();
    assert_eq!(id.id(), "math.GT/0309136");
    assert_eq!(id.version(), None);
}

#[test]
fn arxiv_id_parse_old_style_with_version() {
    let id = ArxivId::parse("math.GT/0309136v3").unwrap();
    assert_eq!(id.id(), "math.GT/0309136");
    assert_eq!(id.version(), Some(3));
}

#[test]
fn arxiv_id_parse_new_style_four_digit_id() {
    let id = ArxivId::parse("1203.0023").unwrap();
    assert_eq!(id.id(), "1203.0023");
    assert_eq!(id.version(), None);
}

#[test]
fn arxiv_id_parse_rejects_three_digit_group() {
    let err = ArxivId::parse("2401.123").unwrap_err();
    assert!(matches!(err, IdentifierError::Invalid { .. }));
}

#[test]
fn arxiv_id_parse_rejects_non_id_text() {
    let err = ArxivId::parse("notanid").unwrap_err();
    assert!(matches!(err, IdentifierError::Invalid { .. }));
}

#[test]
fn arxiv_id_parse_rejects_empty_string() {
    let err = ArxivId::parse("").unwrap_err();
    assert!(matches!(err, IdentifierError::Invalid { .. }));
}

#[test]
fn arxiv_id_display_includes_version_when_present() {
    let id = ArxivId::parse("arXiv:2401.12345v2").unwrap();
    assert_eq!(id.to_string(), "2401.12345v2");
}

#[test]
fn arxiv_id_display_omits_version_when_absent() {
    let id = ArxivId::parse("2401.12345").unwrap();
    assert_eq!(id.to_string(), "2401.12345");
}

#[test]
fn arxiv_id_serde_string_round_trip() {
    let id = ArxivId::parse("arXiv:2401.12345v2").unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "\"2401.12345v2\"");
    let parsed: ArxivId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, id);
}

// ---------------------------------------------------------------------
// Pmid
// ---------------------------------------------------------------------

#[test]
fn pmid_parse_bare_digits() {
    let pmid = Pmid::parse("12345678").unwrap();
    assert_eq!(pmid.value(), 12345678);
}

#[test]
fn pmid_parse_with_prefix_and_space() {
    let pmid = Pmid::parse("PMID: 12345678").unwrap();
    assert_eq!(pmid.value(), 12345678);
}

#[test]
fn pmid_parse_with_lowercase_prefix_no_space() {
    let pmid = Pmid::parse("pmid:42").unwrap();
    assert_eq!(pmid.value(), 42);
}

#[test]
fn pmid_parse_rejects_zero() {
    let err = Pmid::parse("0").unwrap_err();
    assert!(matches!(err, IdentifierError::Invalid { .. }));
}

#[test]
fn pmid_parse_rejects_non_numeric() {
    let err = Pmid::parse("abc").unwrap_err();
    assert!(matches!(err, IdentifierError::Invalid { .. }));
}

#[test]
fn pmid_parse_rejects_empty_string() {
    let err = Pmid::parse("").unwrap_err();
    assert!(matches!(err, IdentifierError::Invalid { .. }));
}

#[test]
fn pmid_value_and_display_agree() {
    let pmid = Pmid::parse("12345678").unwrap();
    assert_eq!(pmid.to_string(), "12345678");
    assert_eq!(pmid.to_string(), pmid.value().to_string());
}

#[test]
fn pmid_serializes_as_json_string() {
    let pmid = Pmid::parse("12345678").unwrap();
    let json = serde_json::to_string(&pmid).unwrap();
    assert_eq!(json, "\"12345678\"");
}

// ---------------------------------------------------------------------
// Isbn
// ---------------------------------------------------------------------

#[test]
fn isbn_parse_isbn13_with_hyphens() {
    let isbn = Isbn::parse("978-1-59327-828-1").unwrap();
    assert_eq!(isbn.as_str(), "9781593278281");
    assert_eq!(isbn.as_str().len(), 13);
}

#[test]
fn isbn_parse_isbn10_with_x_check_digit_uppercased() {
    let isbn = Isbn::parse("0-8044-2957-X").unwrap();
    assert_eq!(isbn.as_str(), "080442957X");
    assert_eq!(isbn.as_str().len(), 10);
}

#[test]
fn isbn_parse_isbn10_lowercase_x_accepted_and_uppercased() {
    let isbn = Isbn::parse("0-8044-2957-x").unwrap();
    assert_eq!(isbn.as_str(), "080442957X");
}

#[test]
fn isbn_parse_accepts_isbn_prefix() {
    let isbn = Isbn::parse("ISBN: 978-1-59327-828-1").unwrap();
    assert_eq!(isbn.as_str(), "9781593278281");
}

#[test]
fn isbn_parse_reports_checksum_failure() {
    let err = Isbn::parse("9781593278282").unwrap_err();
    assert!(matches!(err, IdentifierError::Checksum { .. }));
}

#[test]
fn isbn_parse_rejects_wrong_length_as_invalid() {
    let err = Isbn::parse("12345").unwrap_err();
    assert!(matches!(err, IdentifierError::Invalid { .. }));
}
