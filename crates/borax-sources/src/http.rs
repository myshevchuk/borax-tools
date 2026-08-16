//! The network seam: what a request looks like, what a response looks
//! like, and how a status code is read.
//!
//! Clients build requests and interpret responses without performing
//! them; a [`Transport`] performs them. Everything a client decides is
//! therefore testable offline against recorded bodies, and the only
//! part that needs a socket is the transport itself.

use std::fmt;

use borax_core::record::Record;

use crate::source::{ParseError, SourceError, SourceName};

/// The version borax identifies itself as.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A GET request: a URL and the headers to send with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
    /// Headers in the order the client set them.
    pub headers: Vec<(String, String)>,
}

/// A response body and the status that came with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// Why a request could not be completed at all — no status ever
/// arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// DNS, connection, TLS, or a truncated read.
    Network { message: String },
    /// The request outlived its deadline.
    Timeout,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Network { message } => write!(f, "network error: {message}"),
            TransportError::Timeout => f.write_str("request timed out"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<TransportError> for SourceError {
    /// A request that never completed is a non-answer: the service may
    /// well hold the record, so this must never read as `NotFound`.
    fn from(error: TransportError) -> SourceError {
        SourceError::Unavailable {
            message: error.to_string(),
        }
    }
}

/// Something that can perform a GET.
///
/// The one trait in this crate that touches the network. Tests
/// implement it over recorded bodies; production uses
/// [`crate::transport::UreqTransport`].
///
/// `Sync` because a [`crate::source::Source`] holds one and resolution
/// shares that source across a pool of threads.
pub trait Transport: Sync {
    fn get(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError>;
}

/// How borax identifies itself to the services it queries.
///
/// Crossref and OpenAlex run "polite pools" — faster, more reliable
/// service for clients that say who they are and how to reach them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Politeness {
    /// A contact address, from configuration. Never defaulted: an
    /// address the user did not supply is not ours to send.
    pub mailto: Option<String>,
}

impl Politeness {
    /// The `User-Agent` to send: `borax/<version>` alone, or
    /// `borax/<version> (mailto:<address>)` when an address is
    /// configured.
    pub fn user_agent(&self) -> String {
        match &self.mailto {
            Some(mailto) => format!("borax/{VERSION} (mailto:{mailto})"),
            None => format!("borax/{VERSION}"),
        }
    }
}

/// Read a status code as an outcome.
///
/// `None` means the body is worth reading. Otherwise: `404` and `410`
/// are [`SourceError::NotFound`] (the service looked and does not have
/// it); `429` is [`SourceError::RateLimited`]; `5xx` is
/// [`SourceError::Unavailable`]; any other non-2xx is `Unavailable`
/// too, naming the status — a `400` means borax built a bad request,
/// which is a defect to surface, not an answer about the record.
pub fn classify(status: u16) -> Option<SourceError> {
    match status {
        200..=299 => None,
        404 | 410 => Some(SourceError::NotFound),
        429 => Some(SourceError::RateLimited),
        _ => Some(SourceError::Unavailable {
            message: format!("HTTP status {status}"),
        }),
    }
}

/// Perform `request` and read its body as a record.
///
/// The four steps every client's `fetch` shares: a `None` request is a
/// question that was never asked, the transport's failure is a
/// non-answer, the status decides whether the body is worth reading,
/// and `parse` turns the body into a record.
pub(crate) fn fetch<T: Transport>(
    transport: &T,
    request: Option<HttpRequest>,
    source: SourceName,
    parse: fn(&str) -> Result<Record, ParseError>,
) -> Result<Record, SourceError> {
    let request = request.ok_or_else(|| SourceError::Unavailable {
        message: format!("{source} cannot be asked about this identifier"),
    })?;
    let response = transport.get(&request)?;
    if let Some(error) = classify(response.status) {
        return Err(error);
    }
    Ok(parse(&response.body)?)
}
