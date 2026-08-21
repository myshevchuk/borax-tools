//! The record of what a collection has already admitted.
//!
//! The ledger is one JSON Lines file per collection, one object per
//! admitted file, appended to only by applied runs. It answers a single
//! question — "have I archived this before?" — at two levels: the same
//! bytes (content hash) and the same work (a normalized identifier).
//!
//! Everything here is values in, values out: the text of a ledger and
//! the entries it holds. Reading it from disk, appending to it, and
//! deciding what to do about a duplicate belong to the caller.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::content::ContentHash;
use crate::identifier::{ArxivId, Doi, Identifier, Isbn, Pmid};
use crate::record::EntryType;

/// Which run admitted an entry.
///
/// Opaque and only ever compared, never parsed: uniqueness is the
/// caller's to guarantee.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(String);

impl RunId {
    /// A run identifier reading exactly as `value`.
    pub fn new(value: impl Into<String>) -> RunId {
        RunId(value.into())
    }

    /// The identifier as it appears in the ledger.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One admitted file.
///
/// The identifiers are held as discrete optional fields, mirroring
/// [`Record`](crate::record::Record), so a ledger line reads the way a
/// sidecar does; [`Entry::identifiers`] assembles them into the form
/// the duplicate check takes.
///
/// `path` is relative to the collection root and `/`-separated, so an
/// entry means the same thing on the machine that wrote it and the one
/// that reads it back. `timestamp` and `tool_version` are rendered by
/// the caller and stored verbatim: nothing here parses or reformats
/// them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The file's content hash at the time it was admitted.
    pub hash: ContentHash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<Doi>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arxiv: Option<ArxivId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pmid: Option<Pmid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isbn: Option<Isbn>,
    /// Where the file sits, relative to the collection root.
    pub path: String,
    pub entry_type: EntryType,
    pub run: RunId,
    /// When the file was admitted, as the caller chose to render it.
    pub timestamp: String,
    /// The version of borax that admitted it.
    pub tool_version: String,
}

impl Entry {
    /// Every identifier the entry carries, in the field order DOI,
    /// arXiv, PMID, ISBN. Empty when the entry has none.
    pub fn identifiers(&self) -> Vec<Identifier> {
        let mut identifiers = Vec::new();
        if let Some(doi) = &self.doi {
            identifiers.push(Identifier::Doi(doi.clone()));
        }
        if let Some(arxiv) = &self.arxiv {
            identifiers.push(Identifier::Arxiv(arxiv.clone()));
        }
        if let Some(pmid) = &self.pmid {
            identifiers.push(Identifier::Pmid(*pmid));
        }
        if let Some(isbn) = &self.isbn {
            identifiers.push(Identifier::Isbn(isbn.clone()));
        }
        identifiers
    }
}

/// Something survivable that a ledger's text revealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// The final line was cut off mid-append and was skipped.
    TornTrailingLine,
}

/// A ledger line that is neither a valid entry nor a torn final line.
///
/// `line` is 1-based, counting every line of the text, so it names the
/// line a text editor would put the cursor on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unparsable {
    pub line: usize,
}

impl fmt::Display for Unparsable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {} is not a valid ledger entry", self.line)
    }
}

impl std::error::Error for Unparsable {}

/// The result of reading a ledger's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    /// The entries, in file order.
    pub entries: Vec<Entry>,
    pub warning: Option<Warning>,
}

/// Read the entries out of a ledger's text.
///
/// Returns them in file order, oldest first. A malformed final line
/// that is *not* newline-terminated is an interrupted append: it is
/// skipped and reported as [`Warning::TornTrailingLine`], because the
/// writer never finished it. A malformed line that is terminated —
/// anywhere, last line included — is damage to a line the writer did
/// finish, and yields [`Unparsable`] naming the first such line. Empty
/// text yields no entries and no warning.
pub fn parse_jsonl(text: &str) -> Result<Parsed, Unparsable> {
    if text.is_empty() {
        return Ok(Parsed {
            entries: Vec::new(),
            warning: None,
        });
    }

    // Everything up to the last newline was completely written; what
    // follows it, if anything, is a line the writer may have been cut
    // off in the middle of.
    let (complete, unterminated) = match text.strip_suffix('\n') {
        Some(body) => (body, None),
        None => match text.rsplit_once('\n') {
            Some((body, last)) => (body, Some(last)),
            None => ("", Some(text)),
        },
    };

    let mut entries = Vec::new();
    for (index, line) in complete.lines().enumerate() {
        match serde_json::from_str(line) {
            Ok(entry) => entries.push(entry),
            Err(_) => return Err(Unparsable { line: index + 1 }),
        }
    }

    let mut warning = None;
    if let Some(last) = unterminated {
        match serde_json::from_str(last) {
            Ok(entry) => entries.push(entry),
            Err(_) => warning = Some(Warning::TornTrailingLine),
        }
    }

    Ok(Parsed { entries, warning })
}

/// Render `entries` as ledger text, sorted by path, every line
/// newline-terminated. An empty slice yields an empty string.
///
/// The output depends only on the set of entries, not on the order they
/// arrive in, so two rebuilds of an unchanged collection are byte
/// identical and a rebuild diffs against the file it replaces. An entry
/// that cannot be serialized is omitted rather than written malformed.
pub fn serialize_jsonl(entries: &[Entry]) -> String {
    let mut lines: Vec<(&str, String)> = entries
        .iter()
        .filter_map(|entry| Some((entry.path.as_str(), serde_json::to_string(entry).ok()?)))
        .collect();
    lines.sort();

    let mut text = String::new();
    for (_, line) in lines {
        text.push_str(&line);
        text.push('\n');
    }
    text
}

/// Why an incoming file is already in the collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DuplicateReason {
    /// The same bytes are archived: the file's hash has an entry.
    Content,
    /// The same work is archived as a different file: the hash is
    /// unknown but one of the file's identifiers has an entry.
    Work,
}

/// An incoming file's match against the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    pub reason: DuplicateReason,
    /// The collection-relative path recorded for the matching entry.
    pub existing_path: String,
}

/// A ledger's entries, keyed for lookup by hash and by identifier.
///
/// Where two entries collide on a hash or an identifier the later one
/// answers, matching the append-only file: a fresh admission supersedes
/// what the same key recorded before.
#[derive(Debug, Clone)]
pub struct Index {
    entries: Vec<Entry>,
    by_hash: HashMap<ContentHash, usize>,
    by_identifier: HashMap<Identifier, usize>,
}

impl Index {
    /// Index `entries`, which are taken in file order (oldest first).
    pub fn build(entries: &[Entry]) -> Index {
        let mut by_hash = HashMap::new();
        let mut by_identifier = HashMap::new();

        for (position, entry) in entries.iter().enumerate() {
            by_hash.insert(entry.hash.clone(), position);
            for identifier in entry.identifiers() {
                by_identifier.insert(identifier, position);
            }
        }

        Index {
            entries: entries.to_vec(),
            by_hash,
            by_identifier,
        }
    }

    /// The entry recorded for `hash`, if any.
    pub fn by_hash(&self, hash: &ContentHash) -> Option<&Entry> {
        self.entries.get(*self.by_hash.get(hash)?)
    }

    /// The entry recorded for `identifier`, if any.
    pub fn by_identifier(&self, identifier: &Identifier) -> Option<&Entry> {
        self.entries.get(*self.by_identifier.get(identifier)?)
    }

    /// Whether a file hashing to `hash` is already archived.
    ///
    /// Answerable straight after hashing, before any identifier is
    /// resolved, so a re-downloaded file costs no network access.
    pub fn content_duplicate(&self, hash: &ContentHash) -> Option<Duplicate> {
        self.by_hash(hash).map(|entry| Duplicate {
            reason: DuplicateReason::Content,
            existing_path: entry.path.clone(),
        })
    }

    /// Whether any of `identifiers` names a work already archived.
    ///
    /// The first identifier with an entry decides, so a caller ordering
    /// them by confidence gets its preferred match. `None` for an empty
    /// slice: a file with no identifiers can only ever be a content
    /// duplicate.
    pub fn work_duplicate(&self, identifiers: &[Identifier]) -> Option<Duplicate> {
        identifiers
            .iter()
            .find_map(|identifier| self.by_identifier(identifier))
            .map(|entry| Duplicate {
                reason: DuplicateReason::Work,
                existing_path: entry.path.clone(),
            })
    }
}
