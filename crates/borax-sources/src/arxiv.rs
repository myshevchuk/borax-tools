//! Reading arXiv Atom feed responses.

use std::fmt;

use borax_core::identifier::{ArxivId, Doi, Identifier};
use borax_core::record::{EntryType, Name, Record, Source};
use quick_xml::escape::unescape;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::QName;
use quick_xml::{Reader, XmlVersion};
use serde_json::Value;

use crate::http::{HttpRequest, Politeness, Transport};
use crate::openalex::{split_display_name, ymd_date};
use crate::source::{ParseError, SourceError, SourceName, attribute};

/// What the feed's first `entry` element said, before any of it is
/// interpreted.
#[derive(Default)]
struct Entry {
    id: Option<String>,
    title: Option<String>,
    published: Option<String>,
    authors: Vec<Name>,
    doi: Option<String>,
    journal_ref: Option<String>,
    primary_category: Option<String>,
    categories: Vec<String>,
}

/// Report a reader or decoding failure as an unreadable body.
fn malformed<E: fmt::Display>(error: E) -> ParseError {
    ParseError::Malformed {
        message: error.to_string(),
    }
}

/// The text of the element `reader` has just entered, up to its `name`
/// end tag, with entities decoded.
fn element_text(reader: &mut Reader<&[u8]>, name: QName<'_>) -> Result<String, ParseError> {
    let raw = reader.read_text(name).map_err(malformed)?;
    let decoded = raw.decode().map_err(malformed)?;
    Ok(unescape(&decoded).map_err(malformed)?.into_owned())
}

/// The `term` attribute of a category element.
///
/// Normalization assumes a UTF-8 document, which the reader guarantees:
/// it was built from a `&str`.
fn term(element: &BytesStart<'_>) -> Option<String> {
    let attribute = element.try_get_attribute("term").ok()??;
    Some(
        attribute
            .normalized_value(XmlVersion::Explicit1_0)
            .ok()?
            .into_owned(),
    )
}

/// The `name` of the author element `reader` has just entered.
fn author_name(reader: &mut Reader<&[u8]>) -> Result<Option<String>, ParseError> {
    loop {
        match reader.read_event().map_err(malformed)? {
            Event::Start(element) if element.local_name().as_ref() == b"name" => {
                return element_text(reader, element.name()).map(Some);
            }
            Event::End(element) if element.local_name().as_ref() == b"author" => return Ok(None),
            Event::Eof => return Ok(None),
            _ => {}
        }
    }
}

/// Read the entry `reader` has just entered, up to its end tag.
fn read_entry(reader: &mut Reader<&[u8]>) -> Result<Entry, ParseError> {
    let mut entry = Entry::default();

    loop {
        match reader.read_event().map_err(malformed)? {
            Event::Start(element) => match element.local_name().as_ref() {
                b"id" if entry.id.is_none() => {
                    entry.id = Some(element_text(reader, element.name())?);
                }
                b"title" if entry.title.is_none() => {
                    entry.title = Some(element_text(reader, element.name())?);
                }
                b"published" if entry.published.is_none() => {
                    entry.published = Some(element_text(reader, element.name())?);
                }
                b"doi" if entry.doi.is_none() => {
                    entry.doi = Some(element_text(reader, element.name())?);
                }
                b"journal_ref" if entry.journal_ref.is_none() => {
                    entry.journal_ref = Some(element_text(reader, element.name())?);
                }
                b"author" => {
                    if let Some(name) = author_name(reader)? {
                        entry.authors.push(split_display_name(&name));
                    }
                }
                // Everything else is skipped whole, so a nested element
                // sharing a name with one of the above cannot be read as
                // the entry's own.
                _ => {
                    reader.read_to_end(element.name()).map_err(malformed)?;
                }
            },
            Event::Empty(element) => match element.local_name().as_ref() {
                b"primary_category" => entry.primary_category = term(&element),
                b"category" => entry.categories.extend(term(&element)),
                _ => {}
            },
            Event::End(element) if element.local_name().as_ref() == b"entry" => return Ok(entry),
            Event::Eof => return Ok(entry),
            _ => {}
        }
    }
}

/// The arXiv identifier an abstract URL names: everything after
/// `/abs/`.
///
/// Not the last path segment. An identifier issued before April 2007 is
/// `archive.subject/YYMMNNN`, and the `/` in it belongs to the
/// identifier — `http://arxiv.org/abs/math.GT/0309136v1` names
/// `math.GT/0309136`, where the last segment alone is a bare number
/// that is not an arXiv identifier at all.
///
/// A value carrying no `/abs/` is parsed whole, so a feed that gives
/// the identifier plainly is read rather than refused.
fn arxiv_id(id: &str) -> Result<ArxivId, ParseError> {
    let identifier = id.rsplit_once("/abs/").map_or(id, |(_, rest)| rest);
    ArxivId::parse(identifier).map_err(|error| ParseError::Invalid {
        field: "id",
        message: error.to_string(),
    })
}

/// `text` with every run of whitespace reduced to one space, trimmed.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

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
/// `entry/id`, which holds an abstract URL whose part after `/abs/` is
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
/// term strings, in feed order). An empty category list is dropped
/// rather than stored, as with the Crossref reader's `subject`.
///
/// XML entities are decoded, so a title containing `&amp;` yields
/// `&`.
pub fn parse(body: &str) -> Result<Record, ParseError> {
    let mut reader = Reader::from_str(body);
    let entry = loop {
        match reader.read_event().map_err(malformed)? {
            Event::Start(element) if element.local_name().as_ref() == b"entry" => {
                break read_entry(&mut reader)?;
            }
            Event::Eof => return Err(ParseError::NotFound),
            _ => {}
        }
    };

    let id = entry.id.ok_or(ParseError::MissingField { field: "id" })?;

    let mut record = Record::new(EntryType::Preprint);
    record.borax.arxiv = Some(arxiv_id(&id)?);
    record.title = entry.title.as_deref().map(collapse_whitespace);
    record.authors = entry.authors;
    record.issued = entry.published.as_deref().and_then(ymd_date);
    record.container_title = entry.journal_ref;
    record.doi = entry.doi.and_then(|doi| Doi::parse(&doi).ok());

    if let Some(primary_category) = entry.primary_category {
        record.borax.source_fields.insert(
            "primary_category".to_string(),
            Value::String(primary_category),
        );
    }
    if !entry.categories.is_empty() {
        let categories = entry.categories.into_iter().map(Value::String).collect();
        record
            .borax
            .source_fields
            .insert("categories".to_string(), Value::Array(categories));
    }

    attribute(&mut record, Source::Arxiv);
    Ok(record)
}

/// The arXiv API, over a [`Transport`].
#[derive(Debug, Clone)]
pub struct ArxivClient<T> {
    transport: T,
    politeness: Politeness,
}

impl<T> ArxivClient<T> {
    /// A client that queries arXiv through `transport`, identifying
    /// itself with `politeness`.
    pub fn new(transport: T, politeness: Politeness) -> ArxivClient<T> {
        ArxivClient {
            transport,
            politeness,
        }
    }

    /// The request that would ask arXiv about `identifier`, or `None`
    /// for anything that is not an arXiv id.
    ///
    /// The URL is
    /// `https://export.arxiv.org/api/query?id_list=<id>`, built from
    /// the bare identifier without its version, so the query returns
    /// whatever version is current rather than the one a file happened
    /// to carry. Only `User-Agent` is sent: arXiv runs no polite pool,
    /// so a contact address would be data sent for no purpose.
    pub fn request(&self, identifier: &Identifier) -> Option<HttpRequest> {
        let Identifier::Arxiv(arxiv) = identifier else {
            return None;
        };

        Some(HttpRequest {
            url: format!("https://export.arxiv.org/api/query?id_list={}", arxiv.id()),
            headers: vec![("User-Agent".to_string(), self.politeness.user_agent())],
        })
    }
}

impl<T: Transport> crate::source::Source for ArxivClient<T> {
    fn name(&self) -> SourceName {
        SourceName::Arxiv
    }

    fn supports(&self, identifier: &Identifier) -> bool {
        matches!(identifier, Identifier::Arxiv(_))
    }

    /// As [`crate::source::Source::fetch`] on
    /// [`crate::crossref::CrossrefClient`], with
    /// [`parse`].
    /// An unsupported identifier sends no request and reports
    /// [`SourceError::Unavailable`], as there.
    /// Note that arXiv reports an unknown identifier as an entry-less
    /// feed at `200`, which [`parse`] turns into
    /// [`crate::source::ParseError::NotFound`] and this maps to
    /// [`SourceError::NotFound`].
    fn fetch(&self, identifier: &Identifier) -> Result<Record, SourceError> {
        crate::http::fetch(
            &self.transport,
            self.request(identifier),
            SourceName::Arxiv,
            parse,
        )
    }
}
