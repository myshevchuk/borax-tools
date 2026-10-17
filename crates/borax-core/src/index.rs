//! The content index's entries, as values.
//!
//! The content index recognises a file by its bytes. An entry, kept per
//! content hash, holds identification evidence as separate facts, each
//! its own file with its own writer: `extracted`, the identifier an
//! extraction found and a lookup confirmed, and `operator`, an
//! operator's identifier with origin `supplied` or `overridden`. No fact
//! holds a record.
//!
//! This module is the half of the index that is values: a fact's JSON
//! text, an entry's facts as read, and what an entry answers with.
//! Reading and writing the facts belongs to the caller.

use serde::{Deserialize, Serialize};

use crate::identifier::{self, Identifier};
use crate::library::{EvidenceOrigin, IdentificationEvidence, Malformed, evidence_from_text};

/// One fact of an entry, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fact<T> {
    /// The entry holds no such fact.
    Absent,
    /// The fact, parsed.
    Held(T),
    /// The fact could not be read, or did not parse, for the reason
    /// given.
    Unreadable(String),
}

/// Both facts of one content hash's entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    /// The identifier an extraction found and a lookup confirmed.
    pub extracted: Fact<Identifier>,
    /// An operator's identifier, with origin `supplied` or
    /// `overridden`.
    pub operator: Fact<IdentificationEvidence>,
}

impl IndexEntry {
    /// The evidence the entry answers with, or `None` when it answers
    /// with none.
    ///
    /// A held operator fact answers whatever the extracted fact is. An
    /// unreadable operator fact answers with nothing, since the
    /// extracted identifier beside it may be the one the operator
    /// corrected. With no operator fact, a held extracted identifier
    /// answers with origin `extracted`.
    pub fn answer(&self) -> Option<IdentificationEvidence> {
        match (&self.operator, &self.extracted) {
            (Fact::Held(evidence), _) => Some(evidence.clone()),
            (Fact::Unreadable(_), _) => None,
            (Fact::Absent, Fact::Held(identifier)) => Some(IdentificationEvidence {
                identifier: identifier.clone(),
                origin: EvidenceOrigin::Extracted,
            }),
            (Fact::Absent, _) => None,
        }
    }
}

/// The shape of a fact as JSON. `origin` is absent from an extracted
/// fact, whose file name is its origin. Unknown keys are ignored.
#[derive(Serialize, Deserialize)]
struct FactFile {
    identifier: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<String>,
}

/// `identifier` as the text of an extracted fact:
/// `{"identifier":"<identifier>"}`, the identifier as it displays.
pub fn extracted_to_json(identifier: &Identifier) -> String {
    fact_text(&FactFile {
        identifier: identifier.to_string(),
        origin: None,
    })
}

/// The identifier the text of an extracted fact states.
///
/// Fails with [`Malformed`] when `text` is not a JSON object, when its
/// `identifier` is absent or not a string, or when that string is not an
/// identifier's text. Any `origin` and any unknown key is ignored.
pub fn extracted_from_json(text: &str) -> Result<Identifier, Malformed> {
    let file = fact_file(text)?;
    identifier::supplied(&file.identifier)
        .map_err(|_| Malformed::Toml(format!("not an identifier: {:?}", file.identifier)))
}

/// `evidence` as the text of an operator fact:
/// `{"identifier":"<identifier>","origin":"<origin>"}`.
///
/// Renders whatever origin `evidence` carries; only `supplied` and
/// `overridden` read back.
pub fn operator_to_json(evidence: &IdentificationEvidence) -> String {
    fact_text(&FactFile {
        identifier: evidence.identifier.to_string(),
        origin: Some(evidence.origin.as_str().to_string()),
    })
}

/// The evidence the text of an operator fact states.
///
/// Fails with [`Malformed`] when `text` is not a JSON object, when
/// `identifier` or `origin` is absent or not a string, when the
/// identifier does not parse, or when the origin is anything but
/// `supplied` or `overridden`. Unknown keys are ignored.
pub fn operator_from_json(text: &str) -> Result<IdentificationEvidence, Malformed> {
    let file = fact_file(text)?;
    let origin = file
        .origin
        .ok_or_else(|| Malformed::Toml("an operator fact has no origin".to_string()))?;
    let evidence = evidence_from_text(&file.identifier, &origin)?;
    if !evidence.origin.is_operator() {
        return Err(Malformed::Toml(format!(
            "not an operator origin: {origin:?}"
        )));
    }
    Ok(evidence)
}

/// `file` as compact JSON.
fn fact_text(file: &FactFile) -> String {
    serde_json::to_string(file).unwrap_or_default()
}

/// The fact `text` states, before its strings are checked.
fn fact_file(text: &str) -> Result<FactFile, Malformed> {
    serde_json::from_str(text).map_err(|error| Malformed::Toml(error.to_string()))
}
