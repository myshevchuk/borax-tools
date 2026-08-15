//! Reading OpenAlex work responses.

use borax_core::record::{EntryType, Name, Record};

use crate::source::ParseError;

/// Map an OpenAlex `type` to the record model.
///
/// `article` → `Article`, `preprint` → `Preprint`, `book` → `Book`,
/// `book-chapter` → `Chapter`, `dissertation` → `Thesis`, `report` →
/// `Report`, `standard` → `Standard`, `patent` → `Patent`; anything
/// else → `Article`, for the same reason as the Crossref mapping.
pub fn entry_type(openalex_type: &str) -> EntryType {
    let _ = openalex_type;
    todo!("map an OpenAlex type")
}

/// Split a display name into CSL family and given parts.
///
/// OpenAlex gives one string per author ("James Dewey Watson"), so the
/// split is a heuristic: everything after the last space is the family
/// name, everything before it is `given`. A single word is a family
/// name with no given part, and surrounding whitespace is trimmed.
///
/// The heuristic is wrong for compound surnames ("van der Waals"), and
/// that is accepted: OpenAlex is the fallback source, consulted when
/// Crossref — which supplies the parts separately — had nothing.
pub fn split_display_name(display_name: &str) -> Name {
    let _ = display_name;
    todo!("split an OpenAlex display name")
}

/// Parse an OpenAlex work response body into a record.
///
/// Tolerant, like the Crossref reader: unknown fields ignored, absent
/// optional fields left absent.
///
/// Required: a JSON object ([`ParseError::Malformed`] otherwise) and
/// `type` ([`ParseError::MissingField`]). `doi` is optional — OpenAlex
/// holds works that have none — and when present it arrives as a
/// resolver URL, which [`borax_core::identifier::Doi::parse`] accepts;
/// a `doi` that will not parse is [`ParseError::Invalid`].
///
/// Mapping: `title`, else `display_name`, → title;
/// `primary_location.source.display_name` → container title;
/// `biblio.volume`/`issue` → the same; `biblio.first_page` and
/// `last_page` → pages, joined with `-` when both are present and the
/// first page alone when only it is; `authorships[].author.display_name`
/// → authors via [`split_display_name`], in response order.
///
/// The date comes from `publication_date` (`YYYY-MM-DD`) when it
/// parses, else from `publication_year` as a year alone.
///
/// Provenance attributes every supplied field to
/// [`borax_core::record::Source::OpenAlex`]; confidence is left unset.
/// The work's OpenAlex id is preserved under `openalex_id` in the
/// extension's source fields, so a later run can re-query it directly.
pub fn parse(body: &str) -> Result<Record, ParseError> {
    let _ = body;
    todo!("parse an OpenAlex work")
}
