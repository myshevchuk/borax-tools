//! Normalized bibliographic identifiers.
//!
//! Each type validates on construction: a value of one of these types is
//! always in its normalized form, so equality comparisons are meaningful
//! and serialization is canonical. All types serialize as plain JSON
//! strings and re-validate on deserialization.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Why an input string is not a valid identifier of the attempted kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentifierError {
    /// The input does not match the identifier's syntax.
    Invalid { kind: &'static str, input: String },
    /// The input matches the syntax but fails its checksum (ISBN).
    Checksum { kind: &'static str, input: String },
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdentifierError::Invalid { kind, input } => {
                write!(f, "not a valid {kind}: {input:?}")
            }
            IdentifierError::Checksum { kind, input } => {
                write!(f, "{kind} checksum failed: {input:?}")
            }
        }
    }
}

impl std::error::Error for IdentifierError {}

/// A DOI in normalized form: lowercase, no resolver prefix, no
/// surrounding punctuation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Doi(String);

impl Doi {
    /// Parse and normalize a DOI candidate.
    ///
    /// Accepts the bare form, a `doi:` scheme prefix, or a resolver URL
    /// prefix (`https?://doi.org/`, `https?://dx.doi.org/`), all
    /// case-insensitively. Strips surrounding whitespace and trailing
    /// punctuation (`.,;:)]}"'`) that text extraction commonly attaches.
    /// The stored form is lowercase (DOIs are case-insensitive) and must
    /// match `10.<4-9 digits>/<suffix>`.
    pub fn parse(input: &str) -> Result<Doi, IdentifierError> {
        let _ = input;
        todo!("normalize and validate a DOI")
    }

    /// The normalized DOI, e.g. `10.1021/jacs.4c01234`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Doi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Doi {
    type Error = IdentifierError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Doi::parse(&s)
    }
}

impl From<Doi> for String {
    fn from(d: Doi) -> String {
        d.0
    }
}

/// An arXiv identifier: the bare id plus, separately, the version the
/// input carried (if any).
///
/// Both arXiv schemes are accepted: post-2007 (`2401.12345`, four id
/// digits before April 2015, five after) and pre-2007
/// (`math.GT/0309136`). The version never participates in equality of
/// the id itself; two inputs differing only in `vN` compare equal on
/// [`ArxivId::id`] but not on the whole value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ArxivId {
    id: String,
    version: Option<u32>,
}

impl ArxivId {
    /// Parse an arXiv identifier, with or without an `arXiv:` prefix
    /// (case-insensitive) and with or without a trailing `vN` version.
    pub fn parse(input: &str) -> Result<ArxivId, IdentifierError> {
        let _ = input;
        todo!("parse an arXiv identifier")
    }

    /// The identifier without its version, e.g. `2401.12345` or
    /// `math.GT/0309136`.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The version the input carried (`2` for `...v2`), if any.
    pub fn version(&self) -> Option<u32> {
        self.version
    }
}

/// Renders the full form, with the version when present:
/// `2401.12345v2`.
impl fmt::Display for ArxivId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.version {
            Some(v) => write!(f, "{}v{}", self.id, v),
            None => f.write_str(&self.id),
        }
    }
}

impl TryFrom<String> for ArxivId {
    type Error = IdentifierError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        ArxivId::parse(&s)
    }
}

impl From<ArxivId> for String {
    fn from(a: ArxivId) -> String {
        a.to_string()
    }
}

/// A PubMed identifier: a positive integer, serialized as a string (the
/// CSL-JSON convention for `PMID`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Pmid(u64);

impl Pmid {
    /// Parse a PMID, with or without a `PMID:` prefix (case-insensitive,
    /// optional whitespace after the colon). Zero is invalid.
    pub fn parse(input: &str) -> Result<Pmid, IdentifierError> {
        let _ = input;
        todo!("parse a PMID")
    }

    /// The numeric value.
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for Pmid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for Pmid {
    type Error = IdentifierError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Pmid::parse(&s)
    }
}

impl From<Pmid> for String {
    fn from(p: Pmid) -> String {
        p.to_string()
    }
}

/// An ISBN in compact form: separators stripped, 10 or 13 characters,
/// check digit verified. A trailing check character `x` is stored
/// uppercase.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Isbn(String);

impl Isbn {
    /// Parse an ISBN-10 or ISBN-13, ignoring hyphens and spaces and an
    /// `ISBN:`/`ISBN-10:`/`ISBN-13:` prefix (case-insensitive). The
    /// check digit is verified; [`IdentifierError::Checksum`] reports a
    /// well-formed input that fails it. The stored form preserves the
    /// input's length (a 10 is not converted to a 13).
    pub fn parse(input: &str) -> Result<Isbn, IdentifierError> {
        let _ = input;
        todo!("parse and verify an ISBN")
    }

    /// The compact normalized form, e.g. `9781593278281` or
    /// `080442957X`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Isbn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Isbn {
    type Error = IdentifierError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Isbn::parse(&s)
    }
}

impl From<Isbn> for String {
    fn from(i: Isbn) -> String {
        i.0
    }
}
