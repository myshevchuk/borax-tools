//! Reading Crossref work responses.

use borax_core::record::{EntryType, Record};

use crate::source::ParseError;

/// Map a Crossref `type` to the record model.
///
/// `journal-article` → `Article`, `posted-content` → `Preprint`,
/// `book`/`monograph`/`edited-book`/`reference-book` → `Book`,
/// `book-chapter`/`book-part`/`book-section` → `Chapter`,
/// `dissertation` → `Thesis`, `report`/`report-component` → `Report`,
/// `standard` → `Standard`.
///
/// An unrecognized type maps to `Article`: Crossref adds types over
/// time, and treating an unknown work as an article keeps a new one
/// resolvable instead of failing the file.
pub fn entry_type(crossref_type: &str) -> EntryType {
    let _ = crossref_type;
    todo!("map a Crossref type")
}

/// Parse a Crossref `/works/{doi}` response body into a record.
///
/// The body is the full envelope: the work lives under `message`.
/// A tolerant reader — unknown fields are ignored, and every optional
/// field absent from the response is simply absent from the record.
///
/// Required: `message` ([`ParseError::MissingField`] otherwise),
/// `message.DOI` (missing → `MissingField`, unparsable →
/// [`ParseError::Invalid`]), and `message.type` (missing →
/// `MissingField`).
///
/// Mapping: `title[0]` → title, `container-title[0]` → container
/// title, `volume`/`issue`/`page` → the same, `publisher` →
/// publisher, `ISBN[0]` → ISBN when it parses (ignored when it does
/// not — a bad ISBN must not cost the whole record), and `issued`,
/// else `published`, → the issued date via its `date-parts[0]`
/// (year, then optional month and day; a malformed shape leaves the
/// date absent).
///
/// Authors come from `author[]`, keeping entries that have `family`
/// and dropping the rest (Crossref represents consortia with a `name`
/// field the record model has no place for yet). `given` is carried
/// when present.
///
/// Every field the response supplied is attributed to
/// [`borax_core::record::Source::Crossref`] in the record's
/// provenance, under the field's CSL name. Confidence is left unset:
/// a record fetched by its own identifier was not scored. The
/// response's `subject` array, which CSL has no field for, is
/// preserved under `subject` in the extension's source fields — but
/// only when it holds something: Crossref sends `[]` for the majority
/// of works, and an empty array is not data worth carrying into every
/// record and sidecar.
pub fn parse(body: &str) -> Result<Record, ParseError> {
    let _ = body;
    todo!("parse a Crossref work")
}
