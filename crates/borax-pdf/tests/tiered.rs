#![allow(clippy::unwrap_used)]

use std::cell::Cell;

use borax_core::identifier::Doi;
use borax_pdf::scan::FoundIdentifier;
use borax_pdf::source::{ExtractionError, InfoMetadata, PdfSource};
use borax_pdf::tiered::{DEFAULT_PAGE_LIMIT, Extracted, ExtractionConfig, Tier, extract};

/// A [`PdfSource`] fake driven entirely by data supplied through its
/// builder methods, with a read counter so tests can prove a page was
/// (or was not) touched — the returned [`Tier`] alone cannot show that.
struct FakePdf {
    pages: Vec<Result<String, ExtractionError>>,
    info: InfoMetadata,
    xmp: Option<String>,
    reads: Cell<usize>,
}

impl FakePdf {
    fn new() -> FakePdf {
        FakePdf {
            pages: Vec::new(),
            info: InfoMetadata::default(),
            xmp: None,
            reads: Cell::new(0),
        }
    }

    fn with_pages(mut self, pages: Vec<Result<String, ExtractionError>>) -> FakePdf {
        self.pages = pages;
        self
    }

    fn with_xmp(mut self, xmp: impl Into<String>) -> FakePdf {
        self.xmp = Some(xmp.into());
        self
    }

    fn with_info(mut self, info: InfoMetadata) -> FakePdf {
        self.info = info;
        self
    }

    /// Number of `page_text` calls made so far.
    fn reads(&self) -> usize {
        self.reads.get()
    }
}

impl PdfSource for FakePdf {
    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn info_metadata(&self) -> &InfoMetadata {
        &self.info
    }

    fn xmp(&self) -> Option<&str> {
        self.xmp.as_deref()
    }

    fn page_text(&self, index: usize) -> Result<String, ExtractionError> {
        self.reads.set(self.reads.get() + 1);
        self.pages[index].clone()
    }
}

fn ok_pages(texts: &[&str]) -> Vec<Result<String, ExtractionError>> {
    texts.iter().map(|t| Ok(t.to_string())).collect()
}

fn info_with_custom_doi(doi: &str) -> InfoMetadata {
    let mut info = InfoMetadata::default();
    info.custom.insert("doi".to_string(), doi.to_string());
    info
}

fn doi(s: &str) -> Doi {
    Doi::parse(s).unwrap()
}

// ---------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------

#[test]
fn default_page_limit_is_three() {
    assert_eq!(DEFAULT_PAGE_LIMIT, 3);
    assert_eq!(ExtractionConfig::default().page_limit, 3);
}

// ---------------------------------------------------------------------
// Embedded metadata short-circuits the text pass
// ---------------------------------------------------------------------

#[test]
fn embedded_doi_in_xmp_short_circuits_and_never_reads_pages() {
    let fake = FakePdf::new()
        .with_xmp("<prism:doi>10.1021/xmp-doi</prism:doi>")
        .with_pages(ok_pages(&["mentions 10.1021/page-doi somewhere"]));

    let result = extract(&fake, &ExtractionConfig::default());

    assert_eq!(
        result,
        Ok(Extracted {
            identifier: FoundIdentifier::Doi(doi("10.1021/xmp-doi")),
            tier: Tier::EmbeddedMetadata,
        })
    );
    assert_eq!(fake.reads(), 0);
}

#[test]
fn embedded_doi_in_info_short_circuits_and_never_reads_pages() {
    let fake = FakePdf::new()
        .with_info(info_with_custom_doi("10.1021/info-doi"))
        .with_pages(ok_pages(&["mentions 10.1021/page-doi somewhere"]));

    let result = extract(&fake, &ExtractionConfig::default());

    assert_eq!(
        result,
        Ok(Extracted {
            identifier: FoundIdentifier::Doi(doi("10.1021/info-doi")),
            tier: Tier::EmbeddedMetadata,
        })
    );
    assert_eq!(fake.reads(), 0);
}

#[test]
fn xmp_wins_over_info_when_both_hold_an_identifier() {
    let fake = FakePdf::new()
        .with_xmp("<prism:doi>10.1021/xmp-doi</prism:doi>")
        .with_info(info_with_custom_doi("10.1021/info-doi"));

    let result = extract(&fake, &ExtractionConfig::default()).unwrap();

    assert_eq!(
        result.identifier,
        FoundIdentifier::Doi(doi("10.1021/xmp-doi"))
    );
    assert_eq!(result.tier, Tier::EmbeddedMetadata);
}

// ---------------------------------------------------------------------
// Fallback to the text layer
// ---------------------------------------------------------------------

#[test]
fn falls_back_to_text_layer_when_no_metadata_identifier() {
    let fake = FakePdf::new().with_pages(ok_pages(&["found here: 10.1021/page-doi"]));

    let result = extract(&fake, &ExtractionConfig::default()).unwrap();

    assert_eq!(
        result.identifier,
        FoundIdentifier::Doi(doi("10.1021/page-doi"))
    );
    assert_eq!(result.tier, Tier::TextLayer);
}

#[test]
fn text_pass_reads_pages_in_order_and_stops_at_first_hit() {
    let fake = FakePdf::new().with_pages(ok_pages(&[
        "no identifier on this page",
        "here it is: 10.1021/page-one",
        "different one: 10.1021/page-two",
    ]));

    let result = extract(&fake, &ExtractionConfig::default()).unwrap();

    assert_eq!(
        result.identifier,
        FoundIdentifier::Doi(doi("10.1021/page-one"))
    );
    assert_eq!(result.tier, Tier::TextLayer);
    assert_eq!(fake.reads(), 2);
}

#[test]
fn identifier_beyond_the_scanned_range_is_not_found() {
    let fake = FakePdf::new().with_pages(ok_pages(&[
        "no identifier here",
        "still nothing useful",
        "and nothing on this page either",
        "not scanned, no identifier anyway",
        "not scanned: 10.1021/too-late",
    ]));

    let result = extract(&fake, &ExtractionConfig::default());

    assert_eq!(result, Err(ExtractionError::NoIdentifierFound));
    assert_eq!(fake.reads(), 3);
}

#[test]
fn page_limit_is_clamped_to_the_actual_page_count() {
    let fake = FakePdf::new().with_pages(ok_pages(&["no identifier", "still none"]));

    let result = extract(&fake, &ExtractionConfig { page_limit: 3 });

    assert_eq!(result, Err(ExtractionError::NoIdentifierFound));
    assert_eq!(fake.reads(), 2);
}

#[test]
fn zero_page_limit_disables_the_text_pass() {
    let fake = FakePdf::new().with_pages(ok_pages(&["10.1021/never-read"]));

    let result = extract(&fake, &ExtractionConfig { page_limit: 0 });

    assert_eq!(result, Err(ExtractionError::NoTextLayer));
    assert_eq!(fake.reads(), 0);
}

#[test]
fn zero_pages_and_no_metadata_yields_no_text_layer() {
    let fake = FakePdf::new();

    let result = extract(&fake, &ExtractionConfig::default());

    assert_eq!(result, Err(ExtractionError::NoTextLayer));
}

#[test]
fn all_scanned_pages_whitespace_only_yields_no_text_layer() {
    let fake = FakePdf::new().with_pages(ok_pages(&["", "   \n\t"]));

    let result = extract(&fake, &ExtractionConfig::default());

    assert_eq!(result, Err(ExtractionError::NoTextLayer));
}

#[test]
fn text_present_but_no_identifier_yields_no_identifier_found() {
    let fake = FakePdf::new().with_pages(ok_pages(&["just some prose, no identifiers"]));

    let result = extract(&fake, &ExtractionConfig::default());

    assert_eq!(result, Err(ExtractionError::NoIdentifierFound));
}

#[test]
fn mixed_blank_and_non_blank_pages_yield_no_identifier_found() {
    let fake = FakePdf::new().with_pages(ok_pages(&["", "some text but no identifier"]));

    let result = extract(&fake, &ExtractionConfig::default());

    assert_eq!(result, Err(ExtractionError::NoIdentifierFound));
}

// ---------------------------------------------------------------------
// Error propagation
// ---------------------------------------------------------------------

#[test]
fn page_text_error_propagates_and_stops_the_run() {
    let fake = FakePdf::new().with_pages(vec![
        Err(ExtractionError::Unreadable {
            message: "corrupt stream".to_string(),
        }),
        Ok("10.1021/never-reached".to_string()),
    ]);

    let result = extract(&fake, &ExtractionConfig::default());

    assert_eq!(
        result,
        Err(ExtractionError::Unreadable {
            message: "corrupt stream".to_string(),
        })
    );
    assert_eq!(fake.reads(), 1);
}

#[test]
fn encrypted_page_error_propagates_unchanged() {
    let fake = FakePdf::new().with_pages(vec![Err(ExtractionError::Encrypted)]);

    let result = extract(&fake, &ExtractionConfig::default());

    assert_eq!(result, Err(ExtractionError::Encrypted));
}

// ---------------------------------------------------------------------
// arXiv end-to-end
// ---------------------------------------------------------------------

#[test]
fn arxiv_id_recognized_end_to_end() {
    let fake = FakePdf::new().with_pages(ok_pages(&["arXiv:2401.12345v2"]));

    let result = extract(&fake, &ExtractionConfig::default()).unwrap();

    assert_eq!(result.tier, Tier::TextLayer);
    let FoundIdentifier::Arxiv(id) = result.identifier else {
        panic!("expected an arXiv identifier, got {:?}", result.identifier);
    };
    assert_eq!(id.id(), "2401.12345");
    assert_eq!(id.version(), Some(2));
}

// ---------------------------------------------------------------------
// Determinism and offline behavior
// ---------------------------------------------------------------------

#[test]
fn extraction_is_deterministic_for_the_same_source_and_config() {
    let fake = FakePdf::new().with_pages(ok_pages(&["10.1021/repeatable"]));
    let config = ExtractionConfig::default();

    let first = extract(&fake, &config);
    let second = extract(&fake, &config);

    assert_eq!(first, second);
}

#[test]
fn extraction_succeeds_purely_from_the_fake_with_no_network_access() {
    // extract() performs no I/O of its own; a fully in-memory fake with
    // no filesystem or network access behind it is enough to produce a
    // result, which is itself evidence extraction stayed offline.
    let fake = FakePdf::new().with_pages(ok_pages(&["10.1021/offline-doi"]));

    let result = extract(&fake, &ExtractionConfig::default());

    assert!(result.is_ok());
}
