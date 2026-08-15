//! Finding identifiers in text, XMP packets, and Info dictionaries.
//!
//! All scanning is pure and offline. Candidates are validated through
//! the canonical parsers in [`borax_core::identifier`], so anything
//! these functions return is already normalized: a scan can report a
//! DOI only if that DOI is well-formed.

use borax_core::identifier::{ArxivId, Doi};

use crate::source::InfoMetadata;

/// An identifier found by a scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FoundIdentifier {
    Doi(Doi),
    Arxiv(ArxivId),
}

/// Scan free text for the first usable identifier.
///
/// A **DOI candidate** starts at each occurrence of `10.` and runs to
/// the next ASCII whitespace; candidates are tried in order of
/// appearance and parsed with [`Doi::parse`], which strips resolver
/// prefixes and trailing punctuation. The first candidate that parses
/// wins.
///
/// An **arXiv candidate** requires an explicit marker — `arXiv:`
/// (ASCII-case-insensitive, optional whitespace after the colon) or an
/// `arxiv.org/abs/` URL — and runs to the next ASCII whitespace. A bare
/// `2401.12345` is never treated as an identifier: unmarked numbers of
/// that shape occur in figure and equation numbering, and a wrong
/// identifier is worse than none.
///
/// When both kinds appear, the DOI wins regardless of position: it
/// identifies the published version, which is what the pipeline
/// prefers to resolve.
pub fn scan_text(text: &str) -> Option<FoundIdentifier> {
    let _ = text;
    todo!("scan free text for an identifier")
}

/// Scan an XMP packet.
///
/// Tolerant, not a real XML parse: the value of every element whose
/// local name (after any `prefix:`) is `doi` or `identifier`, and of
/// every attribute whose local name is `doi`, is collected in document
/// order and passed through the [`scan_text`] rules. Malformed XML
/// yields whatever well-formed fragments it contains rather than an
/// error.
pub fn scan_xmp(xmp: &str) -> Option<FoundIdentifier> {
    let _ = xmp;
    todo!("scan an XMP packet for an identifier")
}

/// Scan the document-information dictionary.
///
/// Values are examined in this order, each under the [`scan_text`]
/// rules, and the first hit wins: every `custom` value in key order,
/// then `subject`, `keywords`, `title`, `author`. Custom keys come
/// first because that is where publishers put a DOI deliberately; the
/// standard fields may merely quote one.
pub fn scan_info(info: &InfoMetadata) -> Option<FoundIdentifier> {
    let _ = info;
    todo!("scan Info metadata for an identifier")
}
