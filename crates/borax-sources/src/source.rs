//! The resolver's seam: what a bibliographic source is, and how it
//! fails.
//!
//! A [`Source`] answers one question — "what record does this
//! identifier name?" — and every implementation is an adapter over an
//! HTTP API. Dispatch, priority, and failure handling live above this
//! trait and are tested with fakes.

use std::fmt;

use borax_core::identifier::Identifier;
use borax_core::record::Record;

/// Which service answered (or was asked).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceName {
    Crossref,
    OpenAlex,
    Arxiv,
    DataCite,
    PubMed,
}

impl SourceName {
    /// The name as it appears in output and configuration.
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceName::Crossref => "crossref",
            SourceName::OpenAlex => "openalex",
            SourceName::Arxiv => "arxiv",
            SourceName::DataCite => "datacite",
            SourceName::PubMed => "pubmed",
        }
    }

    /// The source `name` denotes, matching [`as_str`] exactly.
    ///
    /// The match is case-sensitive and admits no aliases, so a
    /// configuration file naming `Crossref` or `arXiv` is a typo the
    /// caller reports rather than a spelling borax quietly accepts.
    ///
    /// [`as_str`]: SourceName::as_str
    pub fn parse(name: &str) -> Option<SourceName> {
        let _ = name;
        todo!("match the name against the known sources")
    }

    /// Every source, in declaration order.
    pub const ALL: [SourceName; 5] = [
        SourceName::Crossref,
        SourceName::OpenAlex,
        SourceName::Arxiv,
        SourceName::DataCite,
        SourceName::PubMed,
    ];
}

impl fmt::Display for SourceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a source did not return a record.
///
/// The distinction that matters to dispatch: [`SourceError::NotFound`]
/// is an answer (this service does not have it), while the others are
/// non-answers (ask again, or ask elsewhere).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// The service does not hold this identifier.
    NotFound,
    /// The service could not be reached or returned a server error.
    Unavailable { message: String },
    /// The service asked us to slow down.
    RateLimited,
    /// A response arrived but could not be read as a record.
    Malformed { message: String },
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::NotFound => f.write_str("not found"),
            SourceError::Unavailable { message } => write!(f, "unavailable: {message}"),
            SourceError::RateLimited => f.write_str("rate limited"),
            SourceError::Malformed { message } => write!(f, "malformed response: {message}"),
        }
    }
}

impl std::error::Error for SourceError {}

/// Why a response body could not be turned into a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The body is not well-formed in the format the source speaks.
    Malformed { message: String },
    /// The body is well-formed but describes no record — an arXiv feed
    /// with no entries, for example, which is how that API reports an
    /// unknown identifier while still answering `200`.
    NotFound,
    /// A field the record cannot do without is missing.
    MissingField { field: &'static str },
    /// A field is present but unusable (an unparsable DOI, a date that
    /// is not a date).
    Invalid {
        field: &'static str,
        message: String,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Malformed { message } => write!(f, "malformed body: {message}"),
            ParseError::NotFound => f.write_str("body describes no record"),
            ParseError::MissingField { field } => write!(f, "missing field {field:?}"),
            ParseError::Invalid { field, message } => {
                write!(f, "invalid field {field:?}: {message}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl From<ParseError> for SourceError {
    /// A body that describes no record is a `NotFound` answer;
    /// everything else is a malformed response.
    fn from(error: ParseError) -> SourceError {
        match error {
            ParseError::NotFound => SourceError::NotFound,
            other => SourceError::Malformed {
                message: other.to_string(),
            },
        }
    }
}

/// Attribute every field `record` carries to `source`, under the
/// field's CSL name.
///
/// Called by a reader once it has finished building a record, so a
/// field is attributed exactly when the response supplied it.
pub(crate) fn attribute(record: &mut Record, source: borax_core::record::Source) {
    let supplied = [
        ("title", record.title.is_some()),
        ("author", !record.authors.is_empty()),
        ("issued", record.issued.is_some()),
        ("container-title", record.container_title.is_some()),
        ("volume", record.volume.is_some()),
        ("issue", record.issue.is_some()),
        ("page", record.pages.is_some()),
        ("publisher", record.publisher.is_some()),
        ("DOI", record.doi.is_some()),
        ("ISBN", record.isbn.is_some()),
    ];

    for (field, present) in supplied {
        if present {
            record.borax.provenance.insert(field.to_string(), source);
        }
    }
}

/// A bibliographic service borax can ask about an identifier.
pub trait Source {
    /// Which service this is.
    fn name(&self) -> SourceName;

    /// Whether this source can be asked about `identifier` at all —
    /// the arXiv API knows nothing about ISBNs.
    fn supports(&self, identifier: &Identifier) -> bool;

    /// Fetch the record `identifier` names.
    fn fetch(&self, identifier: &Identifier) -> Result<Record, SourceError>;
}
