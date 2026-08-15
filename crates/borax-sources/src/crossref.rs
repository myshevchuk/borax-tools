//! Reading Crossref work responses.

use borax_core::identifier::{Doi, Identifier, Isbn};
use borax_core::record::{DateParts, EntryType, Name, Record, Source};
use serde_json::Value;

use crate::http::{HttpRequest, Politeness, Transport};
use crate::source::{ParseError, SourceError, SourceName, attribute};

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
    match crossref_type {
        "posted-content" => EntryType::Preprint,
        "book" | "monograph" | "edited-book" | "reference-book" => EntryType::Book,
        "book-chapter" | "book-part" | "book-section" => EntryType::Chapter,
        "dissertation" => EntryType::Thesis,
        "report" | "report-component" => EntryType::Report,
        "standard" => EntryType::Standard,
        _ => EntryType::Article,
    }
}

/// `work[key]` when it is a string.
fn string(work: &Value, key: &str) -> Option<String> {
    Some(work.get(key)?.as_str()?.to_string())
}

/// The first string in `work[key]`, which Crossref sends as an array
/// even for fields that hold at most one value.
fn first_string(work: &Value, key: &str) -> Option<String> {
    Some(work.get(key)?.as_array()?.first()?.as_str()?.to_string())
}

/// The date `work[key]` states, read from the head of its `date-parts`.
///
/// Any shape other than `[year]`, `[year, month]`, or
/// `[year, month, day]` yields no date.
fn date(work: &Value, key: &str) -> Option<DateParts> {
    let parts = work
        .get(key)?
        .get("date-parts")?
        .as_array()?
        .first()?
        .as_array()?;
    let part = |index: usize| -> Option<u8> { u8::try_from(parts.get(index)?.as_i64()?).ok() };
    let year = i32::try_from(parts.first()?.as_i64()?).ok()?;

    match parts.len() {
        1 => Some(DateParts {
            year,
            month: None,
            day: None,
        }),
        2 => Some(DateParts {
            year,
            month: Some(part(1)?),
            day: None,
        }),
        3 => Some(DateParts {
            year,
            month: Some(part(1)?),
            day: Some(part(2)?),
        }),
        _ => None,
    }
}

/// The authors of `work`, dropping the entries with no `family`.
fn authors(work: &Value) -> Vec<Name> {
    let Some(entries) = work.get("author").and_then(Value::as_array) else {
        return Vec::new();
    };

    entries
        .iter()
        .filter_map(|entry| {
            Some(Name {
                family: entry.get("family")?.as_str()?.to_string(),
                given: entry
                    .get("given")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
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
/// (year, then optional month and day). "Else" means whichever first
/// yields a usable date, so a malformed `issued` falls through to a
/// sound `published`; when neither yields one the date is absent, and
/// no date shape ever fails the record.
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
    let envelope: Value = serde_json::from_str(body).map_err(|error| ParseError::Malformed {
        message: error.to_string(),
    })?;
    let work = envelope
        .get("message")
        .ok_or(ParseError::MissingField { field: "message" })?;

    let doi = string(work, "DOI").ok_or(ParseError::MissingField { field: "DOI" })?;
    let doi = Doi::parse(&doi).map_err(|error| ParseError::Invalid {
        field: "DOI",
        message: error.to_string(),
    })?;
    let work_type = string(work, "type").ok_or(ParseError::MissingField { field: "type" })?;

    let mut record = Record::new(entry_type(&work_type));
    record.doi = Some(doi);
    record.title = first_string(work, "title");
    record.authors = authors(work);
    record.issued = date(work, "issued").or_else(|| date(work, "published"));
    record.container_title = first_string(work, "container-title");
    record.volume = string(work, "volume");
    record.issue = string(work, "issue");
    record.pages = string(work, "page");
    record.publisher = string(work, "publisher");
    record.isbn = first_string(work, "ISBN").and_then(|isbn| Isbn::parse(&isbn).ok());

    let subject = work
        .get("subject")
        .and_then(Value::as_array)
        .filter(|subject| !subject.is_empty());
    if let Some(subject) = subject {
        record
            .borax
            .source_fields
            .insert("subject".to_string(), Value::Array(subject.clone()));
    }

    attribute(&mut record, Source::Crossref);
    Ok(record)
}

/// The Crossref REST API, over a [`Transport`].
#[derive(Debug, Clone)]
pub struct CrossrefClient<T> {
    transport: T,
    politeness: Politeness,
}

impl<T> CrossrefClient<T> {
    /// A client that queries Crossref through `transport`, identifying
    /// itself with `politeness`.
    pub fn new(transport: T, politeness: Politeness) -> CrossrefClient<T> {
        CrossrefClient {
            transport,
            politeness,
        }
    }

    /// The request that would ask Crossref about `identifier`, or
    /// `None` when Crossref cannot be asked about it.
    ///
    /// Crossref answers about DOIs only. The URL is
    /// `https://api.crossref.org/works/<doi>` with the DOI appended in
    /// its normalized form and not percent-encoded — Crossref matches
    /// on the raw suffix, slashes included. Headers: `User-Agent` from
    /// [`Politeness::user_agent`], and `Accept: application/json`.
    pub fn request(&self, identifier: &Identifier) -> Option<HttpRequest> {
        let _ = (identifier, &self.politeness);
        todo!("build a Crossref request")
    }
}

impl<T: Transport> crate::source::Source for CrossrefClient<T> {
    fn name(&self) -> SourceName {
        SourceName::Crossref
    }

    fn supports(&self, identifier: &Identifier) -> bool {
        matches!(identifier, Identifier::Doi(_))
    }

    /// Perform the request, read the status with
    /// [`crate::http::classify`], and parse the body with [`parse`].
    /// A transport failure is [`SourceError::Unavailable`]; a body that
    /// will not parse is [`SourceError::Malformed`].
    ///
    /// An identifier this source does not support sends no request and
    /// reports [`SourceError::Unavailable`]: the question was never
    /// asked, so the answer is unknown — reporting `NotFound` would
    /// claim knowledge the source never had.
    fn fetch(&self, identifier: &Identifier) -> Result<Record, SourceError> {
        let _ = (identifier, &self.transport);
        todo!("fetch from Crossref")
    }
}
