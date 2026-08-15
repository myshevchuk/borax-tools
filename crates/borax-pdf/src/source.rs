//! What the extraction pipeline needs from a PDF, and how it fails.
//!
//! [`PdfSource`] is the seam between the pure extraction logic and the
//! native PDF engine: the pipeline reads metadata and page text through
//! this trait, so every rule above it is testable without pdfium.

use std::collections::BTreeMap;
use std::fmt;

/// Why extraction could not produce an identifier.
///
/// Every variant is per-file and non-fatal: a batch reports it and
/// carries on with the other files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionError {
    /// The file could not be opened or parsed as a PDF.
    Unreadable { message: String },
    /// The document is encrypted and cannot be read without a password.
    Encrypted,
    /// The document has no extractable text (a scan without OCR): every
    /// page in the scanned range yielded only whitespace.
    NoTextLayer,
    /// Text and metadata were readable, but held no identifier.
    NoIdentifierFound,
}

impl fmt::Display for ExtractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtractionError::Unreadable { message } => write!(f, "unreadable PDF: {message}"),
            ExtractionError::Encrypted => f.write_str("PDF is encrypted"),
            ExtractionError::NoTextLayer => f.write_str("no text layer"),
            ExtractionError::NoIdentifierFound => f.write_str("no identifier found"),
        }
    }
}

impl std::error::Error for ExtractionError {}

/// The PDF document-information dictionary, as far as extraction cares.
///
/// `custom` holds the non-standard keys publishers use to stash
/// identifiers (`doi`, `WPS-ARTICLEDOI`, …), keyed by the name as it
/// appears in the file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InfoMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub custom: BTreeMap<String, String>,
}

/// A PDF the extraction pipeline can read.
///
/// Implementations are adapters over a real engine; tests use fakes.
/// Opening the file (and thus reporting [`ExtractionError::Unreadable`]
/// or [`ExtractionError::Encrypted`]) happens before a `PdfSource`
/// exists.
pub trait PdfSource {
    /// Number of pages in the document.
    fn page_count(&self) -> usize;

    /// The document-information dictionary.
    fn info_metadata(&self) -> &InfoMetadata;

    /// The raw XMP metadata packet, when the document carries one.
    fn xmp(&self) -> Option<&str>;

    /// The text of page `index` (zero-based), in reading order as the
    /// engine reports it. Callers only pass indices below
    /// [`PdfSource::page_count`].
    fn page_text(&self, index: usize) -> Result<String, ExtractionError>;
}
