#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use borax_core::identifier::Doi;
use borax_pdf::pure::PurePdf;
use borax_pdf::scan::FoundIdentifier;
use borax_pdf::source::{ExtractionError, PdfSource};
use borax_pdf::tiered::{ExtractionConfig, Tier, extract};

/// Every `.pdf` fixture the corpus is supposed to hold, hard-coded so a
/// fixture added to the directory without a matching test fails the
/// coverage check below rather than passing silently.
const FIXTURES: [&str; 12] = [
    "publisher-info-doi.pdf",
    "publisher-xmp-doi.pdf",
    "publisher-text-doi.pdf",
    "arxiv-new-id.pdf",
    "arxiv-old-id.pdf",
    "doi-on-third-page.pdf",
    "doi-past-page-range.pdf",
    "encrypted-owner-only.pdf",
    "encrypted-user-password.pdf",
    "no-text-layer.pdf",
    "no-identifier.pdf",
    "malformed-truncated.pdf",
];

/// The path to a fixture named `name`, resolved from the crate manifest
/// directory rather than the process working directory so the suite
/// runs the same way regardless of where `cargo test` is invoked from.
fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(name)
}

/// Open a fixture with the pure backend, panicking with a message that
/// names the file when opening fails. Callers that expect `open` to
/// fail (encryption, truncation) call [`PurePdf::open`] directly
/// instead.
fn open_fixture(name: &str) -> PurePdf {
    PurePdf::open(&fixture_path(name))
        .unwrap_or_else(|error| panic!("failed to open `{name}`: {error}"))
}

fn doi(s: &str) -> Doi {
    Doi::parse(s).unwrap()
}

// ---------------------------------------------------------------------
// Embedded metadata
// ---------------------------------------------------------------------

#[test]
fn publisher_info_doi_is_found_in_the_info_dictionary() {
    let pdf = open_fixture("publisher-info-doi.pdf");

    let result = extract(&pdf, &ExtractionConfig::default());

    assert_eq!(
        result,
        Ok(borax_pdf::tiered::Extracted {
            identifier: FoundIdentifier::Doi(doi("10.1234/borax.2024.001")),
            tier: Tier::EmbeddedMetadata,
        }),
        "got {result:?}"
    );
}

#[test]
fn publisher_xmp_doi_is_found_in_the_xmp_packet() {
    let pdf = open_fixture("publisher-xmp-doi.pdf");

    let result = extract(&pdf, &ExtractionConfig::default());

    assert_eq!(
        result,
        Ok(borax_pdf::tiered::Extracted {
            identifier: FoundIdentifier::Doi(doi("10.1234/borax.2024.002")),
            tier: Tier::EmbeddedMetadata,
        }),
        "got {result:?}"
    );
}

// ---------------------------------------------------------------------
// Text layer
// ---------------------------------------------------------------------

#[test]
fn arxiv_new_id_is_found_on_the_first_line_of_the_first_page() {
    let pdf = open_fixture("arxiv-new-id.pdf");

    let result = extract(&pdf, &ExtractionConfig::default()).unwrap();

    assert_eq!(result.tier, Tier::TextLayer);
    let FoundIdentifier::Arxiv(id) = result.identifier else {
        panic!("expected an arXiv identifier, got {:?}", result.identifier);
    };
    assert_eq!(id.id(), "2401.12345");
    assert_eq!(id.version(), Some(2));
}

#[test]
fn arxiv_old_id_is_found_on_the_first_line_of_the_first_page() {
    let pdf = open_fixture("arxiv-old-id.pdf");

    let result = extract(&pdf, &ExtractionConfig::default()).unwrap();

    assert_eq!(result.tier, Tier::TextLayer);
    let FoundIdentifier::Arxiv(id) = result.identifier else {
        panic!("expected an arXiv identifier, got {:?}", result.identifier);
    };
    assert_eq!(id.id(), "math.GT/0309136");
    assert_eq!(id.version(), Some(1));
}

#[test]
fn doi_past_page_range_is_not_found_under_the_default_page_limit() {
    let pdf = open_fixture("doi-past-page-range.pdf");

    let result = extract(&pdf, &ExtractionConfig::default());

    assert_eq!(
        result,
        Err(ExtractionError::NoIdentifierFound),
        "got {result:?}"
    );
}

#[test]
fn doi_past_page_range_is_found_once_the_page_limit_covers_it() {
    let pdf = open_fixture("doi-past-page-range.pdf");

    let result = extract(&pdf, &ExtractionConfig { page_limit: 5 });

    assert_eq!(
        result,
        Ok(borax_pdf::tiered::Extracted {
            identifier: FoundIdentifier::Doi(doi("10.1234/borax.2024.007")),
            tier: Tier::TextLayer,
        }),
        "got {result:?}"
    );
}

// ---------------------------------------------------------------------
// Errors that do not involve the `'`/`"` operator gap
// ---------------------------------------------------------------------

#[test]
fn no_text_layer_pdf_is_reported_as_having_no_text_layer() {
    let pdf = open_fixture("no-text-layer.pdf");

    let result = extract(&pdf, &ExtractionConfig::default());

    assert_eq!(result, Err(ExtractionError::NoTextLayer), "got {result:?}");
}

#[test]
fn no_identifier_pdf_yields_no_identifier_found() {
    let pdf = open_fixture("no-identifier.pdf");

    let result = extract(&pdf, &ExtractionConfig::default());

    assert_eq!(
        result,
        Err(ExtractionError::NoIdentifierFound),
        "got {result:?}"
    );
}

// ---------------------------------------------------------------------
// Encryption
// ---------------------------------------------------------------------

#[test]
fn encrypted_owner_only_opens_and_yields_its_info_dictionary_doi() {
    let pdf = open_fixture("encrypted-owner-only.pdf");

    let result = extract(&pdf, &ExtractionConfig::default());

    assert_eq!(
        result,
        Ok(borax_pdf::tiered::Extracted {
            identifier: FoundIdentifier::Doi(doi("10.1234/borax.2024.008")),
            tier: Tier::EmbeddedMetadata,
        }),
        "got {result:?}"
    );
}

#[test]
fn encrypted_owner_only_info_dictionary_is_readable_through_the_encryption() {
    let pdf = open_fixture("encrypted-owner-only.pdf");

    let info = pdf.info_metadata();

    assert!(
        !info.custom.is_empty(),
        "expected a custom Info entry to survive decryption, got {info:?}"
    );
}

#[test]
fn encrypted_user_password_cannot_be_opened_without_the_password() {
    let result = PurePdf::open(&fixture_path("encrypted-user-password.pdf"));

    assert_eq!(result.err(), Some(ExtractionError::Encrypted));
}

// ---------------------------------------------------------------------
// Unreadable input
// ---------------------------------------------------------------------

#[test]
fn malformed_truncated_pdf_fails_to_open_as_unreadable() {
    let result = PurePdf::open(&fixture_path("malformed-truncated.pdf"));

    assert!(
        matches!(result, Err(ExtractionError::Unreadable { .. })),
        "got {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------
// Red: the `'` text-showing operator is not implemented by the pure
// backend, so a DOI below the first line of a text object is invisible
// to it. These pin the correct behaviour and fail until the backend is
// fixed.
// ---------------------------------------------------------------------

#[test]
fn publisher_text_doi_is_found_on_the_third_line_of_the_masthead() {
    let pdf = open_fixture("publisher-text-doi.pdf");

    let result = extract(&pdf, &ExtractionConfig::default());

    assert_eq!(
        result,
        Ok(borax_pdf::tiered::Extracted {
            identifier: FoundIdentifier::Doi(doi("10.1234/borax.2024.003")),
            tier: Tier::TextLayer,
        }),
        "got {result:?}"
    );
}

#[test]
fn doi_on_third_page_is_found_within_the_default_page_limit() {
    let pdf = open_fixture("doi-on-third-page.pdf");

    let result = extract(&pdf, &ExtractionConfig::default());

    assert_eq!(
        result,
        Ok(borax_pdf::tiered::Extracted {
            identifier: FoundIdentifier::Doi(doi("10.1234/borax.2024.006")),
            tier: Tier::TextLayer,
        }),
        "got {result:?}"
    );
}

// ---------------------------------------------------------------------
// Corpus/suite drift
// ---------------------------------------------------------------------

/// Every `.pdf` file in the corpus directory has a matching entry in
/// [`FIXTURES`], so a fixture added to the directory without a test
/// fails this rather than going untested.
#[test]
fn every_corpus_pdf_is_covered_by_the_fixture_list() {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut present: Vec<String> = std::fs::read_dir(&corpus_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "pdf"))
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    present.sort();

    let mut expected: Vec<String> = FIXTURES.iter().map(|s| s.to_string()).collect();
    expected.sort();

    assert_eq!(
        present, expected,
        "corpus directory and fixture list have drifted apart"
    );
}
