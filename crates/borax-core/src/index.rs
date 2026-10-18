//! The content index's entries, as values.
//!
//! The content index recognises a file by its bytes. An entry, kept per
//! content hash, holds identification evidence as separate facts, each
//! its own file with its own writer: `extracted`, the identifier an
//! extraction found and a lookup confirmed, and `operator`, an
//! operator's identifier with origin `supplied` or `overridden`. Beside
//! the evidence, an entry holds references to libraries, each its own
//! file named for its key: a [`LibraryReference`] names a library, the
//! root it was last known at, the artifact holding the bytes there, and
//! the item that artifact answers with. No fact holds a record.
//!
//! This module is the half of the index that is values: a fact's JSON
//! text, a reference's JSON text and name, an entry's evidence as read,
//! and what that evidence answers with. Reading and writing the files
//! belongs to the caller.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::content::hash_bytes;
use crate::identifier::{self, Identifier};
use crate::library::{
    ArtifactId, EvidenceOrigin, IdentificationEvidence, ItemId, LibraryId, Malformed,
    evidence_from_text,
};

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

/// The evidence facts of one content hash's entry. Its references are
/// read on their own.
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

/// A content-index entry's reference to the library holding its bytes.
///
/// The library, the root and the artifact are the reference's key: one
/// key names one file of the entry. The item is what the reference
/// answers with, and replacing it keeps the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryReference {
    /// The identity of the library holding the bytes.
    pub library: LibraryId,
    /// The root the library was last known at, absolute and normalised.
    pub root: PathBuf,
    /// The artifact the library holds the bytes as.
    pub artifact: ArtifactId,
    /// The item the artifact answers with for these bytes.
    pub item: ItemId,
}

impl LibraryReference {
    /// The name of the file this reference's key names: `sha256-` and
    /// the lowercase hex SHA-256 of the library, the root and the
    /// artifact, as text joined by NUL.
    ///
    /// `None` when `root` is not UTF-8 text. The item takes no part, so
    /// two references differing only in their item share a name.
    pub fn name(&self) -> Option<String> {
        let root = self.root.to_str()?;
        let key = format!("{}\0{root}\0{}", self.library, self.artifact);
        Some(hash_bytes(key.as_bytes()).as_str().to_string())
    }

    /// Whether `other` names the same library, root and artifact,
    /// whatever its item.
    pub fn same_key(&self, other: &LibraryReference) -> bool {
        self.library == other.library && self.root == other.root && self.artifact == other.artifact
    }
}

/// The shape of a reference as JSON. Unknown keys are ignored.
#[derive(Serialize, Deserialize)]
struct ReferenceFile {
    library: String,
    root: String,
    artifact: String,
    item: String,
}

/// `reference` as the text of its file:
/// `{"library":…,"root":…,"artifact":…,"item":…}`, the identities as
/// canonical UUID text.
///
/// `None` when the root is not UTF-8 text, which JSON cannot hold.
pub fn reference_to_json(reference: &LibraryReference) -> Option<String> {
    let file = ReferenceFile {
        library: reference.library.to_string(),
        root: reference.root.to_str()?.to_string(),
        artifact: reference.artifact.to_string(),
        item: reference.item.to_string(),
    };
    serde_json::to_string(&file).ok()
}

/// The reference the text of a reference file states.
///
/// Fails with [`Malformed`] when `text` is not a JSON object, when any
/// of `library`, `root`, `artifact` and `item` is absent or not a
/// string, or when an identity is not canonical UUID text. Unknown keys
/// are ignored.
pub fn reference_from_json(text: &str) -> Result<LibraryReference, Malformed> {
    let file: ReferenceFile =
        serde_json::from_str(text).map_err(|error| Malformed::Toml(error.to_string()))?;
    Ok(LibraryReference {
        library: LibraryId::try_from(file.library)?,
        root: PathBuf::from(file.root),
        artifact: ArtifactId::try_from(file.artifact)?,
        item: ItemId::try_from(file.item)?,
    })
}
