//! Reading OpenAlex work responses.

use borax_core::identifier::{Doi, Identifier};
use borax_core::record::{DateParts, EntryType, Name, Record, Source};
use serde_json::Value;

use crate::http::{HttpRequest, Politeness, Transport};
use crate::source::{ParseError, SourceError, SourceName, attribute};

/// Map an OpenAlex `type` to the record model.
///
/// `article` → `Article`, `preprint` → `Preprint`, `book` → `Book`,
/// `book-chapter` → `Chapter`, `dissertation` → `Thesis`, `report` →
/// `Report`, `standard` → `Standard`, `patent` → `Patent`; anything
/// else → `Article`, for the same reason as the Crossref mapping.
pub fn entry_type(openalex_type: &str) -> EntryType {
    match openalex_type {
        "preprint" => EntryType::Preprint,
        "book" => EntryType::Book,
        "book-chapter" => EntryType::Chapter,
        "dissertation" => EntryType::Thesis,
        "report" => EntryType::Report,
        "standard" => EntryType::Standard,
        "patent" => EntryType::Patent,
        _ => EntryType::Article,
    }
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
    let trimmed = display_name.trim();
    match trimmed.rsplit_once(' ') {
        Some((given, family)) => Name {
            family: family.to_string(),
            given: Some(given.to_string()),
        },
        None => Name {
            family: trimmed.to_string(),
            given: None,
        },
    }
}

/// The date a `YYYY-MM-DD` prefix states, or `None` when the text does
/// not start with one.
pub(crate) fn ymd_date(text: &str) -> Option<DateParts> {
    let mut parts = text.get(..10)?.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;

    Some(DateParts {
        year,
        month: Some(month),
        day: Some(day),
    })
}

/// `work` at `path`, following one nested object key per step, when the
/// value there is a string.
fn nested_string(work: &Value, path: &[&str]) -> Option<String> {
    let mut value = work;
    for key in path {
        value = value.get(key)?;
    }
    Some(value.as_str()?.to_string())
}

/// The date `work` publishes, from `publication_date` when it states
/// one and from `publication_year` otherwise.
fn issued(work: &Value) -> Option<DateParts> {
    let date = work
        .get("publication_date")
        .and_then(Value::as_str)
        .and_then(ymd_date);
    if date.is_some() {
        return date;
    }

    let year = i32::try_from(work.get("publication_year")?.as_i64()?).ok()?;
    Some(DateParts {
        year,
        month: None,
        day: None,
    })
}

/// The page range `work` states, from the pages `biblio` holds.
fn pages(work: &Value) -> Option<String> {
    let first = nested_string(work, &["biblio", "first_page"])?;
    match nested_string(work, &["biblio", "last_page"]) {
        Some(last) => Some(format!("{first}-{last}")),
        None => Some(first),
    }
}

/// The DOI `work` states, absent when it holds none.
fn doi(work: &Value) -> Result<Option<Doi>, ParseError> {
    let value = match work.get("doi") {
        None | Some(Value::Null) => return Ok(None),
        Some(value) => value,
    };
    let invalid = |message: String| ParseError::Invalid {
        field: "doi",
        message,
    };

    let text = value
        .as_str()
        .ok_or_else(|| invalid(format!("not a string: {value}")))?;
    Doi::parse(text)
        .map(Some)
        .map_err(|error| invalid(error.to_string()))
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
    let work: Value = serde_json::from_str(body).map_err(|error| ParseError::Malformed {
        message: error.to_string(),
    })?;
    if !work.is_object() {
        return Err(ParseError::Malformed {
            message: "expected a JSON object".to_string(),
        });
    }

    let work_type = work
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ParseError::MissingField { field: "type" })?;

    let mut record = Record::new(entry_type(work_type));
    record.doi = doi(&work)?;
    record.title =
        nested_string(&work, &["title"]).or_else(|| nested_string(&work, &["display_name"]));
    record.authors = work
        .get("authorships")
        .and_then(Value::as_array)
        .map(|authorships| {
            authorships
                .iter()
                .filter_map(|authorship| {
                    nested_string(authorship, &["author", "display_name"])
                        .map(|name| split_display_name(&name))
                })
                .collect()
        })
        .unwrap_or_default();
    record.issued = issued(&work);
    record.container_title = nested_string(&work, &["primary_location", "source", "display_name"]);
    record.volume = nested_string(&work, &["biblio", "volume"]);
    record.issue = nested_string(&work, &["biblio", "issue"]);
    record.pages = pages(&work);

    if let Some(id) = nested_string(&work, &["id"]) {
        record
            .borax
            .source_fields
            .insert("openalex_id".to_string(), Value::String(id));
    }

    attribute(&mut record, Source::OpenAlex);
    Ok(record)
}

/// `address` with the two characters that a query parameter cannot
/// carry literally escaped: `+`, which a reader would take for a
/// space, and `@`.
fn encode_mailto(address: &str) -> String {
    address
        .chars()
        .map(|character| match character {
            '+' => "%2B".to_string(),
            '@' => "%40".to_string(),
            other => other.to_string(),
        })
        .collect()
}

/// The OpenAlex API, over a [`Transport`].
#[derive(Debug, Clone)]
pub struct OpenAlexClient<T> {
    transport: T,
    politeness: Politeness,
}

impl<T> OpenAlexClient<T> {
    /// A client that queries OpenAlex through `transport`, identifying
    /// itself with `politeness`.
    pub fn new(transport: T, politeness: Politeness) -> OpenAlexClient<T> {
        OpenAlexClient {
            transport,
            politeness,
        }
    }

    /// The request that would ask OpenAlex about `identifier`, or
    /// `None` when OpenAlex cannot be asked about it.
    ///
    /// OpenAlex's `/works/{id}` accepts a namespaced identifier, so a
    /// DOI becomes `https://api.openalex.org/works/doi:<doi>` and a
    /// PMID `.../works/pmid:<number>`. arXiv ids and ISBNs have no such
    /// form and yield `None`.
    ///
    /// When a contact address is configured it is appended as a
    /// `?mailto=` query parameter — OpenAlex's documented polite-pool
    /// mechanism — percent-encoding `+` as `%2B` and `@` as `%40` so an
    /// address with a tag survives. `User-Agent` is sent as well.
    pub fn request(&self, identifier: &Identifier) -> Option<HttpRequest> {
        let namespaced = match identifier {
            Identifier::Doi(doi) => format!("doi:{}", doi.as_str()),
            Identifier::Pmid(pmid) => format!("pmid:{}", pmid.value()),
            Identifier::Arxiv(_) | Identifier::Isbn(_) => return None,
        };

        let mut url = format!("https://api.openalex.org/works/{namespaced}");
        if let Some(mailto) = &self.politeness.mailto {
            url.push_str("?mailto=");
            url.push_str(&encode_mailto(mailto));
        }

        Some(HttpRequest {
            url,
            headers: vec![("User-Agent".to_string(), self.politeness.user_agent())],
        })
    }
}

impl<T: Transport> crate::source::Source for OpenAlexClient<T> {
    fn name(&self) -> SourceName {
        SourceName::OpenAlex
    }

    fn supports(&self, identifier: &Identifier) -> bool {
        matches!(identifier, Identifier::Doi(_) | Identifier::Pmid(_))
    }

    /// As [`crate::crossref::CrossrefClient::fetch`], with [`parse`].
    /// An unsupported identifier sends no request and reports
    /// [`SourceError::Unavailable`], as there.
    fn fetch(&self, identifier: &Identifier) -> Result<Record, SourceError> {
        crate::http::fetch(
            &self.transport,
            self.request(identifier),
            SourceName::OpenAlex,
            parse,
        )
    }
}
