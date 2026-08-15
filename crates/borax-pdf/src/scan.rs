//! Finding identifiers in text, XMP packets, and Info dictionaries.
//!
//! All scanning is pure and offline. Candidates are validated through
//! the canonical parsers in [`borax_core::identifier`], so anything
//! these functions return is already normalized: a scan can report a
//! DOI only if that DOI is well-formed.

use borax_core::identifier::{ArxivId, Doi, Identifier};

use crate::source::InfoMetadata;

/// An identifier found by a scan.
///
/// Narrower than [`Identifier`] on purpose: a page scan can yield a DOI
/// or an arXiv identifier and nothing else, so the two kinds a scan
/// cannot produce are not representable here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FoundIdentifier {
    Doi(Doi),
    Arxiv(ArxivId),
}

impl From<FoundIdentifier> for Identifier {
    /// Widen a scan result to the identifier kind resolution speaks.
    fn from(found: FoundIdentifier) -> Identifier {
        let _ = found;
        todo!("widen the found identifier")
    }
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
    if let Some(doi) = scan_doi(text) {
        return Some(FoundIdentifier::Doi(doi));
    }
    scan_arxiv(text).map(FoundIdentifier::Arxiv)
}

/// The run of characters from byte offset `start` to the next ASCII
/// whitespace, or to the end of `text`. Empty when `start` is past the
/// end or not on a character boundary.
fn run_to_whitespace(text: &str, start: usize) -> &str {
    let Some(rest) = text.get(start..) else {
        return "";
    };
    match rest.split_once(|c: char| c.is_ascii_whitespace()) {
        Some((run, _)) => run,
        None => rest,
    }
}

/// The first candidate starting at `10.` that [`Doi::parse`] accepts.
fn scan_doi(text: &str) -> Option<Doi> {
    text.match_indices("10.")
        .find_map(|(start, _)| Doi::parse(run_to_whitespace(text, start)).ok())
}

/// The first explicitly marked candidate that [`ArxivId::parse`] accepts.
fn scan_arxiv(text: &str) -> Option<ArxivId> {
    // ASCII-lowercasing rewrites bytes in place, so every offset found
    // in `lowered` addresses the same character in `text`.
    let lowered = text.to_ascii_lowercase();
    lowered
        .match_indices("arxiv")
        .filter_map(|(start, _)| arxiv_candidate_start(&lowered, start))
        .find_map(|start| ArxivId::parse(run_to_whitespace(text, start)).ok())
}

/// Where the candidate begins for an arXiv marker at byte offset
/// `start`, or `None` when what stands there is not a marker.
fn arxiv_candidate_start(lowered: &str, start: usize) -> Option<usize> {
    let rest = lowered.get(start..)?;
    if let Some(after_colon) = rest.strip_prefix("arxiv:") {
        let trimmed = after_colon.trim_start_matches(|c: char| c.is_ascii_whitespace());
        Some(start + rest.len() - trimmed.len())
    } else if rest.starts_with("arxiv.org/abs/") {
        Some(start + "arxiv.org/abs/".len())
    } else {
        None
    }
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
    let mut rest = xmp;
    while let Some((_, after_open)) = rest.split_once('<') {
        // An unterminated tag ends what can be read.
        let (tag, after_tag) = after_open.split_once('>')?;
        rest = after_tag;

        // Closing tags, declarations and processing instructions carry
        // no values of their own.
        if tag.starts_with(['/', '!', '?']) {
            continue;
        }

        let name = tag
            .split(|c: char| c.is_ascii_whitespace() || c == '/')
            .next()
            .unwrap_or(tag);
        let attributes = tag.get(name.len()..).unwrap_or("");

        if let Some(found) = scan_doi_attributes(attributes) {
            return Some(found);
        }

        let local = local_name(name);
        let is_wanted =
            local.eq_ignore_ascii_case("doi") || local.eq_ignore_ascii_case("identifier");
        if !is_wanted || attributes.trim_end().ends_with('/') {
            continue;
        }

        // A missing close tag yields the rest of the packet.
        let value = match after_tag.split_once("</") {
            Some((value, _)) => value,
            None => after_tag,
        };
        if let Some(found) = scan_text(value) {
            return Some(found);
        }
    }
    None
}

/// The part of an XML name after any `prefix:`.
fn local_name(name: &str) -> &str {
    match name.rsplit_once(':') {
        Some((_, local)) => local,
        None => name,
    }
}

/// Scan the attribute list of a single tag, in order, for a value under
/// an attribute whose local name is `doi`.
fn scan_doi_attributes(attributes: &str) -> Option<FoundIdentifier> {
    let mut rest = attributes;
    while let Some((before, after)) = rest.split_once('=') {
        let name = before
            .rsplit(|c: char| c.is_ascii_whitespace())
            .next()
            .unwrap_or(before);
        let unquoted = after.trim_start_matches(|c: char| c.is_ascii_whitespace());
        let Some(quote) = unquoted.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            // Not a quoted value; resume after the `=` rather than
            // mistaking the remaining text for one.
            rest = after;
            continue;
        };
        // An unterminated value ends what can be read.
        let (value, tail) = unquoted.get(quote.len_utf8()..)?.split_once(quote)?;
        if local_name(name).eq_ignore_ascii_case("doi") {
            if let Some(found) = scan_text(value) {
                return Some(found);
            }
        }
        rest = tail;
    }
    None
}

/// Scan the document-information dictionary.
///
/// Values are examined in this order, each under the [`scan_text`]
/// rules, and the first hit wins: every `custom` value in key order,
/// then `subject`, `keywords`, `title`, `author`. Custom keys come
/// first because that is where publishers put a DOI deliberately; the
/// standard fields may merely quote one.
pub fn scan_info(info: &InfoMetadata) -> Option<FoundIdentifier> {
    let standard = [&info.subject, &info.keywords, &info.title, &info.author];
    info.custom
        .values()
        .chain(standard.into_iter().flatten())
        .find_map(|value| scan_text(value))
}
