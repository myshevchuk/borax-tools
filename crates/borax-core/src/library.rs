//! The two records a library is made of, as values.
//!
//! A library states what it holds in two kinds of file. An *item* is the
//! canonical record of a work, kept under a minted item identity. An
//! *artifact record* is authoritative state about one file: the item it
//! is linked to when it has one, the hash history that says which run
//! recorded which bytes, and the library-relative path, size and
//! modification time last seen.
//!
//! This module is that half of the library which is values: the two
//! identities, the two records, the TOML text each is written as, and
//! the predicates validation is made of. Reading a file, writing one,
//! minting an identity, and deciding what a finding means belong to the
//! caller — nothing here performs I/O.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::content::ContentHash;
use crate::record::Record;

/// The UUID `text` spells, or `None` unless `text` is the canonical
/// spelling of one: 32 lowercase hex digits in five hyphenated groups.
///
/// A braced, `urn:uuid:`-prefixed, uppercase or unhyphenated spelling of
/// a real UUID is refused, so an identity read from text renders back as
/// the text it was read from.
fn canonical_uuid(text: &str) -> Option<Uuid> {
    let uuid = Uuid::try_parse(text).ok()?;
    (uuid.hyphenated().to_string() == text).then_some(uuid)
}

/// The identity of an item: the work a library keeps a record of.
///
/// Stable for the life of the item. It is minted once, by a caller with
/// a clock and entropy, and this crate only ever parses one or wraps one
/// already minted.
///
/// Rendered, compared and sorted as its canonical UUID text, and
/// serialized as that bare string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ItemId(Uuid);

impl ItemId {
    /// The identity `text` spells, or `None` when `text` is not a
    /// canonical UUID.
    ///
    /// Only the canonical spelling is accepted — lowercase, hyphenated —
    /// so [`ItemId::to_string`] reproduces the accepted text byte for
    /// byte.
    pub fn parse(text: &str) -> Option<ItemId> {
        canonical_uuid(text).map(ItemId)
    }

    /// The identity of `uuid`, whatever spelling it came from.
    ///
    /// This is the seam a caller mints through: minting needs a clock
    /// and entropy, which this crate has neither of.
    pub fn from_uuid(uuid: Uuid) -> ItemId {
        ItemId(uuid)
    }

    /// The UUID the identity is.
    pub fn uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.hyphenated())
    }
}

impl TryFrom<String> for ItemId {
    type Error = Malformed;

    fn try_from(text: String) -> Result<ItemId, Malformed> {
        ItemId::parse(&text)
            .ok_or_else(|| Malformed::Toml(format!("not a canonical item uuid: {text:?}")))
    }
}

impl From<ItemId> for String {
    fn from(id: ItemId) -> String {
        id.to_string()
    }
}

/// The identity of an artifact: one file the library holds.
///
/// Stable across both a move of the file and a change to its bytes, so
/// it is neither the path nor a content hash. Minted once by a caller,
/// like an [`ItemId`], and rendered, compared, sorted and serialized the
/// same way.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ArtifactId(Uuid);

impl ArtifactId {
    /// The identity `text` spells, or `None` when `text` is not a
    /// canonical UUID.
    ///
    /// Only the canonical spelling is accepted — lowercase, hyphenated —
    /// so [`ArtifactId::to_string`] reproduces the accepted text byte
    /// for byte.
    pub fn parse(text: &str) -> Option<ArtifactId> {
        canonical_uuid(text).map(ArtifactId)
    }

    /// The identity of `uuid`, whatever spelling it came from.
    pub fn from_uuid(uuid: Uuid) -> ArtifactId {
        ArtifactId(uuid)
    }

    /// The UUID the identity is.
    pub fn uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.hyphenated())
    }
}

impl TryFrom<String> for ArtifactId {
    type Error = Malformed;

    fn try_from(text: String) -> Result<ArtifactId, Malformed> {
        ArtifactId::parse(&text)
            .ok_or_else(|| Malformed::Toml(format!("not a canonical artifact uuid: {text:?}")))
    }
}

impl From<ArtifactId> for String {
    fn from(id: ArtifactId) -> String {
        id.to_string()
    }
}

/// Which run recorded something.
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

    /// The identifier as it appears in a record.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why an incoming file is one the library already holds.
///
/// The vocabulary a duplicate is reported in, at the two levels it can
/// be recognised at: the two are kept apart because what an operator
/// may do about them differs — the same bytes are nothing to file
/// twice, and a second file of one work is a second artifact of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DuplicateReason {
    /// The same bytes are archived: the file's hash is in some artifact
    /// record's history.
    Content,
    /// The same work is archived as a different file: no record holds
    /// the hash, but an item carrying one of the file's identifiers has
    /// an artifact whose path still holds a file.
    Work,
}

/// Why a library's text could not be read as the record it claims to be.
///
/// One variant, because one thing has gone wrong: the file does not say
/// what a record is. That covers text that is not TOML at all and text
/// that is TOML but whose fields are absent or of the wrong type. An
/// artifact record with an empty history, one whose history entry names
/// no run, and an item whose source fields do not parse are none of
/// them: those are well-formed files the validator has findings about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Malformed {
    /// The text does not read as the record, with the reason as the
    /// parser gave it.
    Toml(String),
}

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Malformed::Toml(reason) => write!(f, "malformed toml: {reason}"),
        }
    }
}

impl std::error::Error for Malformed {}

/// A library's statement about one work.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: ItemId,
    pub record: Record,
}

/// The shape of an item file: the identity, then the record.
#[derive(Serialize, Deserialize)]
struct ItemFile {
    id: ItemId,
    record: toml::Value,
}

/// A source field that did not survive reading an item.
///
/// The field's stored text is not JSON, so there is no value to restore.
/// The field is dropped and named here; the rest of the item is intact,
/// because one unreadable provider field never costs the record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFieldFault {
    /// The source-field key whose text did not parse.
    pub key: String,
}

/// An item read back from text, with whatever reading it cost.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedItem {
    pub item: Item,
    /// The source fields dropped along the way, in key order. Empty for
    /// an item that read back whole.
    pub faults: Vec<SourceFieldFault>,
}

/// The record's map of verbatim source fields, when it has one.
fn source_fields(
    record: &mut serde_json::Value,
) -> Option<&mut serde_json::Map<String, serde_json::Value>> {
    record
        .get_mut("borax")?
        .get_mut("source_fields")?
        .as_object_mut()
}

/// Replace each source field's value with its JSON text.
///
/// Every value is encoded, not only the ones TOML cannot hold, so that
/// the string `"42"` and the number `42` stay distinguishable and the
/// decoder never has to guess which TOML string was meant as JSON.
fn encode_source_fields(record: &mut serde_json::Value) {
    let Some(fields) = source_fields(record) else {
        return;
    };
    for value in fields.values_mut() {
        let text = serde_json::to_string(value).unwrap_or_default();
        *value = serde_json::Value::String(text);
    }
}

/// Undo [`encode_source_fields`], returning the fields that could not be
/// undone.
///
/// A value that is not a string, or is a string that does not parse as
/// JSON, is dropped from the map and reported: there is no value to
/// restore and no way to tell what was meant.
fn decode_source_fields(record: &mut serde_json::Value) -> Vec<SourceFieldFault> {
    let Some(fields) = source_fields(record) else {
        return Vec::new();
    };

    let mut decoded = serde_json::Map::new();
    let mut faults = Vec::new();
    for (key, value) in fields.iter() {
        match value
            .as_str()
            .and_then(|text| serde_json::from_str(text).ok())
        {
            Some(value) => {
                decoded.insert(key.clone(), value);
            }
            None => faults.push(SourceFieldFault { key: key.clone() }),
        }
    }

    *fields = decoded;
    faults
}

impl Item {
    /// The item as the text of an item file: `id` at the top, the
    /// canonical record under `[record]`.
    ///
    /// The record's verbatim source fields are written as their JSON
    /// text, one TOML string per key, which is the one place the file
    /// departs from the record's own shape: JSON has a null and TOML has
    /// none, and encoding every value keeps a string that looks like a
    /// number from coming back as one.
    ///
    /// Yields the empty string for a record TOML cannot hold at all,
    /// which nothing the pipeline produces is: the encoding exists to
    /// keep that from happening.
    pub fn to_toml(&self) -> String {
        let Ok(mut record) = serde_json::to_value(&self.record) else {
            return String::new();
        };
        encode_source_fields(&mut record);

        let Ok(record) = toml::Value::try_from(record) else {
            return String::new();
        };
        toml::to_string(&ItemFile {
            id: self.id.clone(),
            record,
        })
        .unwrap_or_default()
    }

    /// The item `text` states, with the source fields it cost.
    ///
    /// Fails with [`Malformed`] when `text` is not TOML, when it names
    /// no canonical `id`, or when `[record]` is absent or is not a
    /// record. A source field whose text is not JSON is a
    /// [`SourceFieldFault`] rather than a failure: the field is dropped
    /// and the item is returned.
    pub fn from_toml(text: &str) -> Result<ParsedItem, Malformed> {
        let file: ItemFile =
            toml::from_str(text).map_err(|error| Malformed::Toml(error.to_string()))?;

        let mut record = serde_json::to_value(&file.record)
            .map_err(|error| Malformed::Toml(error.to_string()))?;
        let faults = decode_source_fields(&mut record);
        let record =
            serde_json::from_value(record).map_err(|error| Malformed::Toml(error.to_string()))?;

        Ok(ParsedItem {
            item: Item {
                id: file.id,
                record,
            },
            faults,
        })
    }
}

/// One moment in an artifact's hash history: the bytes, and who
/// recorded them when.
///
/// `timestamp` and `tool_version` are rendered by the caller and stored
/// verbatim; nothing here parses or reformats them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashEntry {
    /// The artifact's content hash as this run found it.
    pub hash: ContentHash,
    pub run: RunId,
    pub timestamp: String,
    /// The version of borax that recorded the hash.
    pub tool_version: String,
}

/// Authoritative state about one artifact, under the artifact's
/// identity.
///
/// The record outlives the file: deleting the artifact leaves the fact
/// that the library once held it, and every hash it has ever had.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub id: ArtifactId,
    /// The item this artifact is a file of, when it is linked to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<ItemId>,
    /// Where the artifact was last seen, relative to the library root
    /// and `/`-separated, so a record means the same thing on the
    /// machine that wrote it and the one that reads it back.
    pub path: String,
    /// The artifact's size in bytes, as last seen.
    pub size: u64,
    /// The artifact's modification time in milliseconds since the Unix
    /// epoch, as last seen. With `size`, this is what reconciliation
    /// compares before it hashes anything.
    pub modified_millis: i64,
    /// Every hash the artifact has had, oldest first. Retained rather
    /// than replaced, so an edited artifact is still recognisable by the
    /// content identities it used to have.
    #[serde(default)]
    pub history: Vec<HashEntry>,
}

impl ArtifactRecord {
    /// The record as the text of an artifact-record file.
    ///
    /// Yields the empty string for a record TOML cannot hold — a size
    /// past [`i64::MAX`], which no file has.
    pub fn to_toml(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }

    /// The record `text` states.
    ///
    /// Fails with [`Malformed`] when `text` is not TOML or does not say
    /// what an artifact record is. An empty history, and a history entry
    /// naming no run, are well-formed text the validator has findings
    /// about, and parse.
    pub fn from_toml(text: &str) -> Result<ArtifactRecord, Malformed> {
        toml::from_str(text).map_err(|error| Malformed::Toml(error.to_string()))
    }

    /// The newest hash recorded, or `None` for a record whose history is
    /// empty.
    pub fn current_hash(&self) -> Option<&ContentHash> {
        self.history.last().map(|entry| &entry.hash)
    }

    /// Whether `hash` appears anywhere in the history — the artifact's
    /// bytes now, or bytes it had before it was edited.
    pub fn holds(&self, hash: &ContentHash) -> bool {
        self.history.iter().any(|entry| &entry.hash == hash)
    }
}

/// The identity a library file's name claims, or `None` when it claims
/// none.
///
/// The claim is the last `.`-separated component before the `.toml`
/// extension, so `milner1978.<uuid>.toml` and a bare `<uuid>.toml` both
/// name one. It must be a canonical UUID, the spelling
/// [`ItemId::parse`] accepts.
///
/// Comparing this against the identity inside the file is a validator's
/// business: a name and a record are free to disagree, and the
/// disagreement is the finding.
pub fn name_uuid(file_name: &str) -> Option<Uuid> {
    let stem = file_name.strip_suffix(".toml")?;
    canonical_uuid(stem.rsplit('.').next()?)
}

/// Whether `path` is a library-relative path: a non-empty, `/`-separated
/// sequence of non-empty segments, none of them `.` or `..`, with no
/// backslash anywhere.
///
/// An absolute path has an empty leading segment and is refused by that,
/// as a trailing slash is refused by its empty trailing one. The rule is
/// lexical: it says nothing about whether the path exists or where a
/// symlink along it leads.
pub fn is_library_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

/// Whether `hash` has the shape [`hash_bytes`](crate::content::hash_bytes)
/// produces: `sha256-` and 64 lowercase hex digits.
///
/// A hash is stored as text and read back without re-validation, so a
/// mangled file yields a value of the right type and the wrong shape.
/// This is what names one.
pub fn is_well_formed_hash(hash: &ContentHash) -> bool {
    match hash.as_str().strip_prefix("sha256-") {
        Some(hex) => {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        }
        None => false,
    }
}
