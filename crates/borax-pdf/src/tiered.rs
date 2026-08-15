//! Tiered extraction: cheap passes first, stop at the first hit.

use crate::scan::{FoundIdentifier, scan_info, scan_text, scan_xmp};
use crate::source::{ExtractionError, PdfSource};

/// Which pass produced an identifier — recorded so the resolved record
/// can attribute the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// The XMP packet or the Info dictionary.
    EmbeddedMetadata,
    /// The page text layer.
    TextLayer,
}

impl Tier {
    /// The name as it appears in output, in the kebab-case the event
    /// schema uses throughout.
    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::EmbeddedMetadata => "embedded-metadata",
            Tier::TextLayer => "text-layer",
        }
    }
}

/// What extraction found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    pub identifier: FoundIdentifier,
    pub tier: Tier,
}

/// How far the text pass reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractionConfig {
    /// Pages the text pass may read, counted from the front. Zero
    /// disables the text pass entirely.
    pub page_limit: usize,
}

/// Three pages: publishers put the DOI on the first page, and a cover
/// sheet or abstract page rarely pushes it past the third.
pub const DEFAULT_PAGE_LIMIT: usize = 3;

impl Default for ExtractionConfig {
    fn default() -> ExtractionConfig {
        ExtractionConfig {
            page_limit: DEFAULT_PAGE_LIMIT,
        }
    }
}

/// Run the tiered extraction over `source`.
///
/// Passes run in this order and the first identifier found ends the
/// run — later passes are never executed:
///
/// 1. **Embedded metadata**: the XMP packet
///    ([`crate::scan::scan_xmp`]), then the Info dictionary
///    ([`crate::scan::scan_info`]). Reports [`Tier::EmbeddedMetadata`].
/// 2. **Text layer**: pages `0..min(page_count, page_limit)` in order,
///    each scanned with [`crate::scan::scan_text`]. Reports
///    [`Tier::TextLayer`].
///
/// Failure is reported when no pass produced an identifier:
/// [`ExtractionError::NoTextLayer`] when every page the text pass read
/// was blank (only ASCII whitespace) — including when the document has
/// no pages or the limit is zero — and
/// [`ExtractionError::NoIdentifierFound`] when text was present but
/// held nothing usable. A [`PdfSource::page_text`] error propagates
/// unchanged and ends the run.
///
/// Performs no network access and no file I/O of its own.
pub fn extract(
    source: &dyn PdfSource,
    config: &ExtractionConfig,
) -> Result<Extracted, ExtractionError> {
    if let Some(identifier) = source.xmp().and_then(scan_xmp) {
        return Ok(Extracted {
            identifier,
            tier: Tier::EmbeddedMetadata,
        });
    }
    if let Some(identifier) = scan_info(source.info_metadata()) {
        return Ok(Extracted {
            identifier,
            tier: Tier::EmbeddedMetadata,
        });
    }

    let mut saw_text = false;
    for index in 0..source.page_count().min(config.page_limit) {
        let text = source.page_text(index)?;
        saw_text |= !text.chars().all(|c| c.is_ascii_whitespace());
        if let Some(identifier) = scan_text(&text) {
            return Ok(Extracted {
                identifier,
                tier: Tier::TextLayer,
            });
        }
    }

    if saw_text {
        Err(ExtractionError::NoIdentifierFound)
    } else {
        Err(ExtractionError::NoTextLayer)
    }
}
