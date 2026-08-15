#![allow(clippy::unwrap_used)]

use borax_sources::http::{Politeness, TransportError, VERSION, classify};
use borax_sources::source::SourceError;

// --- Politeness::user_agent ---

#[test]
fn user_agent_without_mailto_is_bare_version_string() {
    let politeness = Politeness::default();
    assert_eq!(politeness.user_agent(), format!("borax/{VERSION}"));
}

#[test]
fn user_agent_with_mailto_appends_contact_address() {
    let politeness = Politeness {
        mailto: Some("a@b.example".to_string()),
    };
    assert_eq!(
        politeness.user_agent(),
        format!("borax/{VERSION} (mailto:a@b.example)")
    );
}

// --- classify: success ---

#[test]
fn classify_2xx_success_is_none() {
    for status in [200, 204, 299] {
        assert_eq!(classify(status), None, "status {status}");
    }
}

// --- classify: not found ---

#[test]
fn classify_404_and_410_are_not_found() {
    for status in [404, 410] {
        assert_eq!(
            classify(status),
            Some(SourceError::NotFound),
            "status {status}"
        );
    }
}

// --- classify: rate limited ---

#[test]
fn classify_429_is_rate_limited() {
    assert_eq!(classify(429), Some(SourceError::RateLimited));
}

// --- classify: unavailable ---

#[test]
fn classify_5xx_is_unavailable() {
    for status in [500, 502, 503, 599] {
        assert!(
            matches!(classify(status), Some(SourceError::Unavailable { .. })),
            "status {status}"
        );
    }
}

#[test]
fn classify_400_is_unavailable_naming_the_status() {
    match classify(400) {
        Some(SourceError::Unavailable { message }) => {
            assert!(message.contains("400"), "message was {message:?}");
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn classify_403_is_unavailable() {
    assert!(matches!(
        classify(403),
        Some(SourceError::Unavailable { .. })
    ));
}

#[test]
fn classify_301_is_unavailable() {
    assert!(matches!(
        classify(301),
        Some(SourceError::Unavailable { .. })
    ));
}

// --- From<TransportError> for SourceError ---

#[test]
fn timeout_becomes_unavailable_mentioning_timing_out() {
    let error: SourceError = TransportError::Timeout.into();
    match error {
        SourceError::Unavailable { message } => {
            assert!(message.contains("timed out"), "message was {message:?}");
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

#[test]
fn network_error_becomes_unavailable_carrying_the_message() {
    let error: SourceError = TransportError::Network {
        message: "dns".to_string(),
    }
    .into();
    match error {
        SourceError::Unavailable { message } => {
            assert!(message.contains("dns"), "message was {message:?}");
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
}
