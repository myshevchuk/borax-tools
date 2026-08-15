//! Reading arXiv Atom feed responses.

use borax_core::record::Record;

use crate::source::ParseError;

/// Parse an arXiv API Atom feed into a record.
///
/// The feed carries zero or more `entry` elements; the first is used
/// and any others ignored. **An empty feed is
/// [`ParseError::NotFound`]**, not a malformed body: the arXiv API
/// answers `200 OK` with an entry-less feed when it does not know the
/// identifier, and the dispatcher must read that as "this source does
/// not have it".
///
/// The resulting record is always [`borax_core::record::EntryType::Preprint`].
///
/// Required: well-formed XML ([`ParseError::Malformed`] otherwise) and
/// `entry/id`, which holds an abstract URL whose last path segment is
/// the arXiv identifier (missing → [`ParseError::MissingField`],
/// unparsable → [`ParseError::Invalid`]; both name the field `id`,
/// after the element). The identifier keeps the
/// version the feed reports and is stored in the extension's `arxiv`
/// field.
///
/// Mapping: `entry/title` → title, with internal whitespace runs
/// (arXiv wraps long titles across lines) collapsed to single spaces
/// and the result trimmed; `entry/author/name` → authors, split by
/// [`crate::openalex::split_display_name`], in feed order;
/// `entry/published` → the issued date, from the `YYYY-MM-DD` prefix
/// of its timestamp; `arxiv:doi` → the DOI when present and parsable
/// (ignored when not — a preprint's record is still useful without
/// it); `arxiv:journal_ref` → container title, which is how a
/// published preprint names its journal.
///
/// Provenance attributes every supplied field to
/// [`borax_core::record::Source::Arxiv`]; confidence is left unset.
/// The primary category and the full category list, which CSL has no
/// field for, are preserved in the extension's source fields under
/// `primary_category` and `categories` (the latter a JSON array of
/// term strings, in feed order).
///
/// XML entities are decoded, so a title containing `&amp;` yields
/// `&`.
pub fn parse(body: &str) -> Result<Record, ParseError> {
    let _ = body;
    todo!("parse an arXiv feed")
}
