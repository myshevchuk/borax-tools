//! The real [`Transport`], over ureq.
//!
//! The only part of this crate that opens a socket, and the only part
//! its tests cannot cover: everything a client decides is settled
//! before the request is handed here, and everything it concludes is
//! settled after the response comes back. Verified by running it
//! against the live services.
//!
//! ureq is synchronous, so resolution is threads over blocking calls
//! rather than an async runtime — a short-lived CLI making a few dozen
//! requests has no use for one. Its rustls backend needs no system TLS
//! library, which keeps Windows builds ordinary.

use std::time::Duration;

use crate::http::{HttpRequest, HttpResponse, Transport, TransportError};

/// How long a single request may take, end to end.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Largest response body to read. Bibliographic records are kilobytes;
/// anything approaching this is a misrouted request, and reading it
/// would be the only unbounded allocation in the pipeline.
pub const MAX_BODY_BYTES: u64 = 8 * 1024 * 1024;

/// A [`Transport`] backed by a ureq agent.
#[derive(Debug, Clone)]
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    /// A transport whose requests time out after `timeout`.
    ///
    /// Status codes are not treated as errors: a `404` is an answer the
    /// caller must classify, not a transport failure.
    pub fn new(timeout: Duration) -> UreqTransport {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build();
        UreqTransport {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

impl Default for UreqTransport {
    fn default() -> UreqTransport {
        UreqTransport::new(DEFAULT_TIMEOUT)
    }
}

impl Transport for UreqTransport {
    fn get(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        let mut call = self.agent.get(&request.url);
        for (name, value) in &request.headers {
            call = call.header(name, value);
        }

        let mut response = call.call().map_err(transport_error)?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY_BYTES)
            .read_to_string()
            .map_err(transport_error)?;

        Ok(HttpResponse { status, body })
    }
}

/// Read a ureq failure as a transport failure.
fn transport_error(error: ureq::Error) -> TransportError {
    match error {
        ureq::Error::Timeout(_) => TransportError::Timeout,
        other => TransportError::Network {
            message: other.to_string(),
        },
    }
}
