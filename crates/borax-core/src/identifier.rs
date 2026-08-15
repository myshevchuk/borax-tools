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

/// Any one of the identifiers borax can resolve a record from.
///
/// The pipeline carries this between stages: extraction produces one,
/// the resolver dispatches on its kind, and the record records it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Identifier {
    Doi(Doi),
    Arxiv(ArxivId),
    Pmid(Pmid),
    Isbn(Isbn),
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Identifier::Doi(id) => write!(f, "doi:{id}"),
            Identifier::Arxiv(id) => write!(f, "arXiv:{id}"),
            Identifier::Pmid(id) => write!(f, "pmid:{id}"),
            Identifier::Isbn(id) => write!(f, "isbn:{id}"),
        }
    }
}

/// Strip `prefix` from the front of `s`, comparing ASCII-case-insensitively.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    match s.get(..prefix.len()) {
        Some(head) if head.eq_ignore_ascii_case(prefix) => s.get(prefix.len()..),
        _ => None,
    }
}

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
        let invalid = || IdentifierError::Invalid {
            kind: "DOI",
            input: input.to_string(),
        };

        let mut body = input.trim();
        for prefix in [
            "https://doi.org/",
            "http://doi.org/",
            "https://dx.doi.org/",
            "http://dx.doi.org/",
            "doi:",
        ] {
            if let Some(rest) = strip_prefix_ci(body, prefix) {
                body = rest;
                break;
            }
        }
        let normalized = body
            .trim()
            .trim_end_matches(['.', ',', ';', ':', ')', ']', '}', '"', '\''])
            .to_ascii_lowercase();

        let registrant_and_suffix = normalized.strip_prefix("10.").ok_or_else(invalid)?;
        let (registrant, suffix) = registrant_and_suffix.split_once('/').ok_or_else(invalid)?;
        if !(4..=9).contains(&registrant.len())
            || !registrant.chars().all(|c| c.is_ascii_digit())
            || suffix.is_empty()
        {
            return Err(invalid());
        }

        Ok(Doi(normalized))
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

/// Split a trailing `vN` off `s`, yielding the base and the version.
/// Returns `None` when `s` has no such suffix.
fn split_version(s: &str) -> Option<(&str, u32)> {
    let marker = s.rfind(['v', 'V'])?;
    let (base, rest) = s.split_at(marker);
    let digits = rest.get(1..)?;
    if base.is_empty() || digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().map(|version| (base, version))
}

fn is_arxiv_id(s: &str) -> bool {
    is_new_style_arxiv_id(s) || is_old_style_arxiv_id(s)
}

fn is_new_style_arxiv_id(s: &str) -> bool {
    let Some((yymm, number)) = s.split_once('.') else {
        return false;
    };
    yymm.len() == 4
        && yymm.chars().all(|c| c.is_ascii_digit())
        && (4..=5).contains(&number.len())
        && number.chars().all(|c| c.is_ascii_digit())
}

fn is_old_style_arxiv_id(s: &str) -> bool {
    let Some((archive, number)) = s.split_once('/') else {
        return false;
    };
    if number.len() != 7 || !number.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let (main, subject) = match archive.split_once('.') {
        Some((main, subject)) => (main, Some(subject)),
        None => (archive, None),
    };
    !main.is_empty()
        && main.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
        && subject.is_none_or(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic()))
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
        let invalid = || IdentifierError::Invalid {
            kind: "arXiv id",
            input: input.to_string(),
        };

        let mut body = input.trim();
        if let Some(rest) = strip_prefix_ci(body, "arxiv:") {
            body = rest.trim_start();
        }

        if is_arxiv_id(body) {
            return Ok(ArxivId {
                id: body.to_string(),
                version: None,
            });
        }

        let (id, version) = split_version(body).ok_or_else(invalid)?;
        if !is_arxiv_id(id) {
            return Err(invalid());
        }
        Ok(ArxivId {
            id: id.to_string(),
            version: Some(version),
        })
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
        let invalid = || IdentifierError::Invalid {
            kind: "PMID",
            input: input.to_string(),
        };

        let mut body = input.trim();
        if let Some(rest) = strip_prefix_ci(body, "pmid:") {
            body = rest.trim_start();
        }
        if body.is_empty() || !body.chars().all(|c| c.is_ascii_digit()) {
            return Err(invalid());
        }

        match body.parse::<u64>() {
            Ok(0) | Err(_) => Err(invalid()),
            Ok(value) => Ok(Pmid(value)),
        }
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

/// Whether `digits` is a 10-character ISBN body: nine digits followed by
/// a digit or `X`.
fn isbn10_is_well_formed(digits: &[char]) -> bool {
    digits.len() == 10
        && digits[..9].iter().all(char::is_ascii_digit)
        && (digits[9].is_ascii_digit() || digits[9] == 'X')
}

fn isbn10_check_digit_holds(digits: &[char]) -> bool {
    let mut sum = 0u32;
    for (index, c) in digits.iter().enumerate() {
        let value = match c {
            'X' => 10,
            _ => match c.to_digit(10) {
                Some(value) => value,
                None => return false,
            },
        };
        sum += value * (10 - index as u32);
    }
    sum % 11 == 0
}

/// Whether `digits` is a 13-character ISBN body: thirteen digits.
fn isbn13_is_well_formed(digits: &[char]) -> bool {
    digits.len() == 13 && digits.iter().all(char::is_ascii_digit)
}

fn isbn13_check_digit_holds(digits: &[char]) -> bool {
    let mut sum = 0u32;
    for (index, c) in digits.iter().enumerate() {
        let Some(value) = c.to_digit(10) else {
            return false;
        };
        sum += value * if index % 2 == 0 { 1 } else { 3 };
    }
    sum % 10 == 0
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
        let invalid = || IdentifierError::Invalid {
            kind: "ISBN",
            input: input.to_string(),
        };
        let checksum = || IdentifierError::Checksum {
            kind: "ISBN",
            input: input.to_string(),
        };

        let mut body = input.trim();
        for prefix in ["isbn-13:", "isbn-10:", "isbn:"] {
            if let Some(rest) = strip_prefix_ci(body, prefix) {
                body = rest;
                break;
            }
        }
        let digits: Vec<char> = body
            .chars()
            .filter(|c| *c != '-' && !c.is_whitespace())
            .map(|c| c.to_ascii_uppercase())
            .collect();

        match digits.len() {
            10 => {
                if !isbn10_is_well_formed(&digits) {
                    return Err(invalid());
                }
                if !isbn10_check_digit_holds(&digits) {
                    return Err(checksum());
                }
            }
            13 => {
                if !isbn13_is_well_formed(&digits) {
                    return Err(invalid());
                }
                if !isbn13_check_digit_holds(&digits) {
                    return Err(checksum());
                }
            }
            _ => return Err(invalid()),
        }

        Ok(Isbn(digits.into_iter().collect()))
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
